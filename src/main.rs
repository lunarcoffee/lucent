use std::{env, fs};

use crate::server::file_server::{FileServer, FileServerStartError};
use crate::server::Server;
use crate::server::templates::{Template, TemplateSubstitution};
use std::collections::HashMap;

mod server;
mod log;
mod http;
mod util;
mod consts;

#[async_std::main]
async fn main() {
    let args = env::args().collect::<Vec<_>>();
    if args.len() != 3 && args.len() != 4 {
        println!("usage: {} <file root> <template root> [host]", args[0]);
        return;
    }

    let fallback_address = &"0.0.0.0:80".to_string();
    let address = args.get(3).unwrap_or(fallback_address);

    match FileServer::new(&args[1], &args[2], address).await {
        Ok(server) => server.start(),
        Err(FileServerStartError::InvalidTemplates) =>
            log::fatal("Either the template root is invalid, or it is missing some templates!"),
        Err(FileServerStartError::FileRootInvalid) => log::fatal("The file root is invalid!"),
        _ => log::fatal("Cannot not bind to that address!"),
    }
}
