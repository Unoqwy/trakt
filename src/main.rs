use std::{path::PathBuf, str::FromStr, sync::Arc};

use clap::Parser;
use config::ConfigProvider;
use log::LevelFilter;
use proxy::RaknetProxy;
use simple_logger::SimpleLogger;
use tokio::io::AsyncBufReadExt;

mod config;
mod health;
mod load_balancer;
mod motd;
mod proxy;
mod raknet;
mod scheduler;

#[derive(Parser)]
#[command(version, about)]
struct Args {
    /// Configuration file.
    #[arg(short, long, value_name = "FILE")]
    config: Option<PathBuf>,
    /// Verbose level.
    #[arg(short, long, action = clap::ArgAction::Count)]
    verbose: u8,
    /// Disable reading from standard input for commands.
    #[arg(long)]
    ignore_stdin: bool,
    /// Disables colors from output.
    #[arg(long)]
    no_color: bool,
    /// Raises the maximum number of open files allowed to avoid issues.
    ///
    /// Not enabled by default as it may not work in all environments.
    #[arg(long)]
    raise_ulimit: bool,
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

    let config_file = args
        .config
        .as_ref()
        .map(PathBuf::clone)
        .unwrap_or_else(|| PathBuf::from_str("config.toml").unwrap());

    let config_provider = match config::read_config(config_file.clone()) {
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
    run(config_provider, args);
}

#[tokio::main]
async fn run(config_provider: ConfigProvider, args: Args) {
    let bind_address = {
        let config = config_provider.read().await;
        log::debug!("Parsed configuration: {:#?}", config);
        config.bind_address.clone()
    };
    let config_provider = Arc::new(config_provider);
    if !args.ignore_stdin {
        tokio::spawn({
            let config_provider = config_provider.clone();
            async move {
                log::info!("Console commands enabled");
                run_stdin_handler(config_provider).await;
            }
        });
    }
    let proxy = RaknetProxy::bind(bind_address, config_provider.clone())
        .await
        .unwrap();
    tokio::spawn({
        let proxy = proxy.clone();
        async move {
            loop {
                config_provider.wait_reload().await;
                proxy.reload_config().await;
            }
        }
    });
    if let Err(err) = proxy.clone().run().await {
        log::error!("{}", err);
    }
    proxy.cleanup().await;
}

async fn run_stdin_handler(config_provider: Arc<ConfigProvider>) {
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
            _ => log::warn!("Unknown command '{}'", line),
        }
    }
}
