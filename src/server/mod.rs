pub mod file_server;
pub mod template;
pub mod config_loader;

mod middleware;

pub trait Server {
    fn start(&self);
    fn stop(&self);
}
