use std::env;

use async_std::process;
use async_std::sync::Arc;
use futures::TryFutureExt;

use crate::server::config::Config;
use crate::server::file_server::{FileServer, FileServerStartError};
use crate::server::Server;

mod server;
mod log;
mod http;
mod util;
mod consts;

#[async_std::main]
async fn main() {
    let mut args = env::args();
    if args.len() != 2 {
        println!("usage: {} <config path>", args.next().unwrap());
        process::exit(1);
    }

    let config = Config::load(&args.nth(1).unwrap()).await
        .unwrap_or_else(|| log::fatal("Configuration file invalid or missing required settings!"));

    log::fatal(match FileServer::new(config).await {
        Ok(server) => {
            let server = Arc::new(server);
            let server_clone = Arc::clone(&server);
            let _ = ctrlc::set_handler(move || server_clone.stop());
            return server.start();
        }
        Err(FileServerStartError::InvalidFileRoot) => "File directory invalid!",
        Err(FileServerStartError::InvalidTemplates) => "Template directory invalid or incomplete!",
        Err(FileServerStartError::AddressInUse) => "That address is in use!",
        Err(FileServerStartError::AddressUnavailable) => "That address is unavailable!",
        _ => "Cannot bind to that address!",
    });
}
