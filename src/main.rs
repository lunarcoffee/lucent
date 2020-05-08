use std::env;

use crate::server::{FileServer, FileServerStartError, Server};

mod server;
mod log;
mod http;

#[async_std::main]
async fn main() {
    let args = env::args().collect::<Vec<String>>();
    if args.len() != 3 && args.len() != 4 {
        println!("usage: {} <file root> <template root> [host]", args[0]);
        return;
    }

    let fallback_address = &"0.0.0.0:80".to_string();
    let address = args.get(3).unwrap_or(fallback_address);

    match FileServer::new(&args[1], &args[2], address).await {
        Ok(server) => server.start(),
        Err(FileServerStartError::FileRootInvalid) => log::fatal("File root invalid or not a directory!"),
        Err(FileServerStartError::TemplateRootInvalid) => log::fatal("Template root invalid or not a directory!"),
        _ => log::fatal("Could not bind to that address!"),
    }
}
