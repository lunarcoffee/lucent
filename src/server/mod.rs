pub mod file_server;

pub trait Server {
    fn start(&self);
    fn stop(&self);
}
