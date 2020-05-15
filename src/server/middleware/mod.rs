use crate::http::response::{Status, Response};
use std::error;

pub mod output_processor;
pub mod request_verifier;
pub mod response_gen;
pub mod range_parser;
pub mod cond_checker;
pub mod dir_lister;
pub mod cgi_runner;

pub enum MiddlewareOutput {
    Error(Status, bool),
    Status(Status, bool),
    Response(Response, bool),
    Bytes(Vec<u8>, bool),
    Terminate,
}

pub type MiddlewareResult<T> = Result<T, MiddlewareOutput>;

impl<T: error::Error> From<T> for MiddlewareOutput {
    fn from(_: T) -> Self {
        MiddlewareOutput::Terminate
    }
}
