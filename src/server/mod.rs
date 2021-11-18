pub mod config;
pub mod file_server;
pub mod template;

// Middleware components for servers.
mod middleware;

pub trait Server {
    fn start(&self);
    fn stop(&self);
}
