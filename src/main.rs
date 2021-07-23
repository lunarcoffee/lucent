#![feature(box_syntax)]
#![feature(is_symlink)]
#![feature(slice_as_chunks)]
#![feature(slice_group_by)]

use std::env;

use async_std::process;
use async_std::sync::Arc;

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
        Err(FileServerStartError::CannotBindAddress) => "Cannot bind to that address!",
        Err(FileServerStartError::TlsCertNotFound) => "Cannot find TLS certificate file!",
        Err(FileServerStartError::TlsKeyNotFound) => "Cannot find RSA private key file!",
        Err(FileServerStartError::TlsInvalidCert) => "That TLS certificate is invalid!",
        _ => "That RSA private key is invalid!",
    });
}
