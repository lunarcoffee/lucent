pub mod file_server;
pub mod templates;

mod middleware;
mod request_verifier;
mod response_gen;
mod range_parser;
mod cond_checker;
mod dir_lister;

pub trait Server {
    fn start(&self);
    fn stop(&self);
}
