pub mod file_server;

mod conditionals;

pub trait Server {
    fn start(&self);
    fn stop(&self);
}
