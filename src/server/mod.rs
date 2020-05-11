pub mod file_server;

mod middleware;
mod request_verifier;
mod response_gen;
mod range_parser;
mod cond_checker;

pub trait Server {
    fn start(&self);
    fn stop(&self);
}
