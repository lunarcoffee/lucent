#![feature(str_strip)]

use std::env;

use crate::server::file_server::{FileServer, FileServerStartError};
use crate::server::Server;
use crate::server::config::Config;

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
        return;
    }

    let config = match Config::load(&args.nth(1).unwrap()).await {
        Some(config) => config,
        _ => log::fatal("Configuration file invalid, or missing settings!"),
    };

    match FileServer::new(config).await {
        Ok(server) => server.start(),
        Err(FileServerStartError::InvalidFileRoot) => log::fatal("File directory invalid!"),
        Err(FileServerStartError::InvalidTemplates) => log::fatal("Template directory invalid, or missing templates!"),
        Err(FileServerStartError::AddressInUse) => log::fatal("That address is in use!"),
        Err(FileServerStartError::AddressUnavailable) => log::fatal("That address is unavailable!"),
        _ => log::fatal("Cannot bind to that address!"),
    }
}
