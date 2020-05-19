pub mod file_server;
pub mod template;
pub mod config;

mod middleware;

pub trait Server {
    fn start(&self);
    fn stop(&self);
}
