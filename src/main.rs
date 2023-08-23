use std::{path::PathBuf, str::FromStr, sync::Arc};

use clap::Parser;
use config::{ConfigProvider, RootConfig};
use log::LevelFilter;
use proxy::RaknetProxy;
use simple_logger::SimpleLogger;

mod config;
mod load_balancer;
mod motd;
mod proxy;
mod raknet;

#[derive(Parser)]
#[command(version, about)]
struct Args {
    /// Configuration file.
    #[arg(short, long, value_name = "FILE")]
    config: Option<PathBuf>,
    /// Verbose level.
    #[arg(short, long, action = clap::ArgAction::Count)]
    verbose: u8,
}

fn main() {
    let args = Args::parse();

    let config_file = args
        .config
        .unwrap_or_else(|| PathBuf::from_str("config.toml").unwrap());
    let log_level = match args.verbose {
        0 => LevelFilter::Info,
        1 => LevelFilter::Debug,
        _ => LevelFilter::Trace,
    };
    SimpleLogger::new().with_level(log_level).init().unwrap();

    let config = match config::read_config(config_file.clone()) {
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
    run(config);
}

#[tokio::main]
async fn run(config: RootConfig) {
    let bind_address = config.bind_address.clone();
    let config_provider = Arc::new(ConfigProvider::new(config));
    let proxy = RaknetProxy::bind(bind_address, config_provider)
        .await
        .unwrap();
    if let Err(err) = proxy.clone().run().await {
        log::error!("{}", err);
    }
    proxy.cleanup().await;
}
