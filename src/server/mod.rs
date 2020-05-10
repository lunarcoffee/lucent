pub mod file_server;

mod request_verifier;
mod response_gen;
mod cond_checker;
mod middleware;

pub trait Server {
    fn start(&self);
    fn stop(&self);
}
