#![feature(is_symlink)]
#![feature(slice_as_chunks)]
#![feature(slice_group_by)]

use std::env;

use async_std::{process, sync::Arc};

use crate::server::{
    config::Config,
    file_server::{FileServer, FileServerStartError::*},
    Server,
};

mod consts;
mod http;
mod log;
mod server;
mod util;

#[async_std::main]
async fn main() {
    // The arguments taken are the paths to the config files (at least one is required).
    let mut args = env::args();
    if args.len() < 2 {
        println!("usage: {} <config paths...>", args.next().unwrap());
        process::exit(1);
    }

    log::info(format!("lucent v{}", consts::SERVER_VERSION));

    // Load all configs concurrently, stopping if any fail to be loaded.
    let config_futures = args.skip(1).into_iter().map(|path| Config::load(path));
    let configs = futures::future::join_all(config_futures)
        .await
        .into_iter()
        .collect::<Option<_>>()
        .unwrap_or_else(|| log::fatal("a configuration file was invalid or omitted required options"));

    log::fatal(match FileServer::new(configs).await {
        // Register a signal handler for graceful shutdowns and start the server.
        Ok(server) => {
            let server = Arc::new(server);
            let server_clone = Arc::clone(&server);
            if let Err(_) = ctrlc::set_handler(move || server_clone.stop()) {
                log::warn("failed to attach signal handler for graceful shutdown");
            }
            return server.start();
        }
        // Initialization failed, here's why.
        Err(InvalidFileRoot) => "file directory invalid",
        Err(InvalidTemplates) => "template directory invalid or missing files",
        Err(AddressInUse) => "that address is in use",
        Err(AddressUnavailable) => "that address is unavailable",
        Err(CannotBindAddress) => "cannot bind to that address",
        Err(TlsCertNotFound) => "cannot find TLS certificate file",
        Err(TlsKeyNotFound) => "cannot find RSA private key file",
        Err(TlsInvalidCert) => "that TLS certificate is invalid",
        _ => "that RSA private key is invalid",
    });
}
