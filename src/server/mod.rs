pub mod file_server;
pub mod templates;
pub mod config_loader;

mod middleware;

pub trait Server {
    fn start(&self);
    fn stop(&self);
}
