use std::{path::PathBuf, process::exit, str::FromStr, sync::Arc};

use clap::Parser;
use log::LevelFilter;
use simple_logger::SimpleLogger;
use tokio::io::AsyncBufReadExt;
use trakt_core::{
    api::SingleProxyApi,
    bedrock::{snapshot::RaknetProxySnapshot, RaknetProxyServer},
    config::{LoadBalanceMethod, RuntimeConfig, RuntimeConfigProvider},
    snapshot::{self, RecoverableProxyServer},
    Backend, DefaultLoadBalancer, Proxy,
};

mod config;

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

#[tokio::main]
async fn main() {
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
    let snapshot =
        match snapshot::read_snapshot_file::<_, RaknetProxySnapshot>(&recovery_snapshot_file) {
            Ok(Some(snapshot)) if snapshot.has_expired() => {
                log::warn!(
                "Recovery snapshot file exists but dates back from more than 10 seconds. Ignoring."
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
    let (config, runtime_config, bind_address) = if let Some(snapshot) = &snapshot {
        (
            None,
            snapshot.config.clone(),
            snapshot.player_proxy_bind.clone(),
        )
    } else {
        let config = match config::read_config(config_file.clone()).await {
            Ok(config) => config,
            Err(err) => {
                log::error!(
                    "Could not read configuration file ({}): {}",
                    config_file.to_string_lossy(),
                    err
                );
                return;
            }
        };
        log::debug!("Parsed configuration: {:#?}", config);
        let runtime_config = RuntimeConfig {
            proxy_bind: config.proxy_bind.clone(),
            health_check_rate: config.health_check_rate,
            motd_refresh_rate: config.motd_refresh_rate,
        };
        let bind_address = config.bind_address.clone();
        (Some(config), runtime_config, bind_address)
    };
    let config_provider = Arc::new(RuntimeConfigProvider::new(runtime_config));
    let (backend, load_result) = Backend::new_bedrock(
        "default".to_owned(),
        |state| {
            Box::new(DefaultLoadBalancer::init(
                state,
                LoadBalanceMethod::RoundRobin,
            ))
        },
        config_provider.clone(),
        config.as_ref().map(|config| &config.backend),
    )
    .await;
    log::info!("Loaded {} backend servers", load_result.server_count);
    let proxy_server = RaknetProxyServer::bind(
        bind_address,
        config_provider.clone(),
        Some(Arc::new(backend)),
    )
    .await
    .unwrap();
    let proxy_server = Arc::new(proxy_server);

    if let Some(snapshot) = snapshot {
        proxy_server.recover_from_snapshot(snapshot).await;
        tokio::spawn({
            let proxy_server = proxy_server.clone();
            let config_file = config_file.clone();
            async move {
                config::reload_bedrock_proxy(&proxy_server, config_file).await;
            }
        });
    }
    let proxy = Proxy::new(proxy_server, config_provider, Some(recovery_snapshot_file));
    let proxy = Arc::new(proxy);
    if !args.ignore_stdin {
        tokio::spawn({
            let proxy = proxy.clone();
            async move {
                log::info!("Console commands enabled");
                run_stdin_handler(proxy, config_file).await;
            }
        });
    }

    #[derive(Debug, Clone)]
    struct DoubleError;

    impl std::error::Error for DoubleError {}

    impl std::fmt::Display for DoubleError {
        fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            write!(f, "invalid first item to double")
        }
    }

    tokio::spawn({
        let proxy = proxy.clone();
        async move {
            let api = SingleProxyApi::new("node1", proxy.server.clone());
            trakt_dashboard::start("0.0.0.0:8081", Box::new(api))
                .await
                .unwrap();
        }
    });

    tokio::spawn({
        let proxy = proxy.clone();
        async move {
            let api = SingleProxyApi::new("node1", proxy.server.clone());
            trakt_webapi::start("0.0.0.0:8084", Box::new(api))
                .await
                .unwrap();
        }
    });

    tokio::spawn({
        let proxy = proxy.clone();
        async move {
            let mut shutdown_requests = 0;
            loop {
                tokio::select! {
                    _ = tokio::signal::ctrl_c() => {
                        shutdown_requests += 1;
                        if shutdown_requests >= 2 {
                            exit(1);
                        }
                        log::info!("Shutdown requested... CTRL C to force");
                        match proxy.take_and_write_snapshot().await {
                            Ok(_) => exit(0),
                            Err(err) => {
                                log::error!("Failed to take snapshot: {:?}", err)
                            }
                        }
                    }
                    _ = proxy.config_provider.wait_reload() => {
                        proxy.reload_config().await;
                    }
                }
            }
        }
    });
    if let Err(err) = proxy.run().await {
        log::error!("{}", err);
    }
}

async fn run_stdin_handler(proxy: Arc<Proxy<RaknetProxyServer>>, config_file: PathBuf) {
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
            "reload" => {
                if config::reload_bedrock_proxy(&proxy.server, &config_file).await {
                    proxy.reload_config().await;
                }
            }
            // "list" | "load" => {
            //     let overview = proxy.load_overview().await;
            //     log::info!(
            //         "There are {} online players ({} active clients). Breakdown: {:?}",
            //         overview.connected_count,
            //         overview.client_count,
            //         overview.per_server
            //     )
            // }
            _ => log::warn!("Unknown command '{}'", line),
        }
    }
}
