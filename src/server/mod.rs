pub mod file_server;
pub mod template;
pub mod config;

// Middleware components for servers.
mod middleware;

pub trait Server {
    fn start(&self);
    fn stop(&self);
}
