use std::{path::PathBuf, process::exit, str::FromStr, sync::Arc, time::Duration};

use clap::Parser;
use config::ConfigProvider;
use log::LevelFilter;
use proxy::RaknetProxy;
use simple_logger::SimpleLogger;
use snapshot::RaknetProxySnapshot;
use tokio::io::AsyncBufReadExt;

mod config;
mod health;
mod load_balancer;
mod motd;
mod proxy;
mod raknet;
mod scheduler;
mod snapshot;

#[derive(Parser)]
#[command(version, about)]
struct Args {
    /// Configuration file.
    #[arg(short, long, value_name = "FILE", default_value = "config.toml")]
    config: Option<PathBuf>,
    /// Verbose level.
    #[arg(short, long, action = clap::ArgAction::Count)]
    verbose: u8,
    /// Disable reading from standard input for commands.
    #[arg(long)]
    ignore_stdin: bool,
    /// Disable colors from output.
    #[arg(long)]
    no_color: bool,
    /// Raise the maximum number of open files allowed to avoid issues.
    ///
    /// Not enabled by default as it may not work in all environments.
    #[arg(long)]
    raise_ulimit: bool,
    /// File to read & write the recovery snapshot to.
    #[arg(long, value_name = "FILE", default_value = ".trakt_recover")]
    recovery_snapshot_file: Option<PathBuf>,
}

fn main() {
    let args = Args::parse();
    let log_level = match args.verbose {
        0 => LevelFilter::Info,
        1 => LevelFilter::Debug,
        _ => LevelFilter::Trace,
    };
    SimpleLogger::new()
        .with_level(log_level)
        .with_colors(!args.no_color)
        .init()
        .unwrap();

    if args.raise_ulimit {
        let ulimit = fdlimit::raise_fd_limit().unwrap_or(0);
        log::info!("Raised ulimit to {}", ulimit);
    }

    let recovery_snapshot_file = args
        .recovery_snapshot_file
        .as_ref()
        .map(PathBuf::clone)
        .unwrap_or_else(|| PathBuf::from_str(".trakt_recover").unwrap());
    let snapshot = match snapshot::read_snapshot_file(&recovery_snapshot_file) {
        Ok(Some(snapshot))
            if snapshot
                .taken_at
                .elapsed()
                .map(|elapsed| elapsed >= Duration::from_secs(10))
                .unwrap_or(true) =>
        {
            log::warn!(
                "Recovery snapshot file exsits but dates back from more than 10 seconds. Ignoring."
            );
            None
        }
        Ok(snapshot) => {
            if snapshot.is_some() {
                log::info!("Recovering active connections from recovery snapshot.");
            }
            snapshot
        }
        Err(err) => {
            log::error!(
                "Could not read snapshot recovery file ({}): {}",
                recovery_snapshot_file.to_string_lossy(),
                err
            );
            None
        }
    };

    let config_file = args
        .config
        .as_ref()
        .map(PathBuf::clone)
        .unwrap_or_else(|| PathBuf::from_str("config.toml").unwrap());
    let config_provider = if let Some(snapshot) = &snapshot {
        ConfigProvider::new(config_file, snapshot.config.clone())
    } else {
        match config::read_config(config_file.clone()) {
            Ok(config) => config,
            Err(err) => {
                log::error!(
                    "Could not read configuration file ({}): {}",
                    config_file.to_string_lossy(),
                    err
                );
                return;
            }
        }
    };
    run(config_provider, args, recovery_snapshot_file, snapshot);
}

#[tokio::main]
async fn run(
    config_provider: ConfigProvider,
    args: Args,
    recovery_snapshot_file: PathBuf,
    snapshot: Option<RaknetProxySnapshot>,
) {
    let bind_address = if let Some(snapshot) = &snapshot {
        snapshot.player_proxy_bind.clone()
    } else {
        let config = config_provider.read().await;
        log::debug!("Parsed configuration: {:#?}", config);
        config.bind_address.clone()
    };
    let config_provider = Arc::new(config_provider);
    let proxy = RaknetProxy::bind(
        bind_address,
        config_provider.clone(),
        recovery_snapshot_file,
    )
    .await
    .unwrap();
    if let Some(snapshot) = snapshot {
        proxy.recover_from_snapshot(snapshot).await;
        tokio::spawn({
            let config_provider = config_provider.clone();
            async move {
                config_provider.reload().await;
            }
        });
    }
    if !args.ignore_stdin {
        tokio::spawn({
            let proxy = proxy.clone();
            let config_provider = config_provider.clone();
            async move {
                log::info!("Console commands enabled");
                run_stdin_handler(proxy, config_provider).await;
            }
        });
    }
    tokio::spawn({
        let proxy = proxy.clone();
        async move {
            let mut shutdown_requests = 0;
            loop {
                tokio::select! {
                    _ = tokio::signal::ctrl_c() => {
                        shutdown_requests += 1;
                        if shutdown_requests >= 3 {
                            exit(1);
                        }
                        log::info!("Shutdown requested...");
                        if proxy.take_and_write_snapshot().await {
                            exit(0);
                        }
                    }
                    _ = config_provider.wait_reload() => {
                        proxy.reload_config().await;
                    }
                }
            }
        }
    });
    if let Err(err) = proxy.clone().run().await {
        log::error!("{}", err);
    }
    proxy.cleanup().await;
}

async fn run_stdin_handler(proxy: Arc<RaknetProxy>, config_provider: Arc<ConfigProvider>) {
    let mut reader = tokio::io::BufReader::new(tokio::io::stdin());
    loop {
        let mut buf = String::new();
        let len = match reader.read_line(&mut buf).await {
            Ok(line) => line,
            Err(err) => {
                log::error!("Error reading user input: {:?}", err);
                continue;
            }
        };
        let line = &buf[0..len].trim();
        match line.to_lowercase().as_str() {
            "reload" => config_provider.reload().await,
            "list" | "load" => {
                let overview = proxy.load_overview().await;
                log::info!(
                    "There are {} online players ({} active clients). Breakdown: {:?}",
                    overview.connected_count,
                    overview.client_count,
                    overview.per_server
                )
            }
            "recover-able-shutdown" | "ras" => {
                proxy.take_and_write_snapshot().await;
            }
            _ => log::warn!("Unknown command '{}'", line),
        }
    }
}
