use std::error;

use crate::http::response::{Response, Status};

// Used to process a `MiddlewareOutput`.
pub mod output_processor;

// Parses a request and converts errors to an HTTP status.
pub mod request_verifier;

// Generates a response for a request.
pub mod response_gen;

// Handles range requests (see RFC 7233).
pub mod range_parser;

// Checks conditional headers (modify time/ETag; see RFC 7232).
pub mod cond_checker;

// Generates a response with a directory listing.
pub mod dir_lister;

// Executes CGI scripts, returning their output (after validation). Also executes NPH scripts.
pub mod cgi_runner;

// Handles request authentication using HTTP basic authentication.
pub mod basic_auth;

// Indicates that this request is finished being processed, and that something should be done with the client, such as
// sending a response, an error page, or simply terminating the connection. If the boolean field is true, the client
// connection will be closed after responding.
pub enum MiddlewareOutput {
    // Respond with a non-OK status.
    Status(Status, bool),

    // Like `Status`, but sends a formatted page (for statuses which should be seen by the client user, such as a 404).
    // This is generated with the error template.
    Error(Status, bool),

    // Respond with the given `Response`.
    Response(Response, bool),

    // Respond with the given bytes. This is used when the server itself does not generate the content of the response,
    // such as when an NPH script is executed.
    Bytes(Vec<u8>, bool),

    // Just close the connection.
    Terminate,
}

// The structure of this module is loosely based around passing a request through a chain of 'middleware', until it
// passes through the last middleware, or until an intermediate middleware returns an `Err`. The implementation is
// very messy, though... I should refactor it sometime.
pub type MiddlewareResult<T> = Result<T, MiddlewareOutput>;

impl<T: error::Error> From<T> for MiddlewareOutput {
    fn from(_: T) -> Self {
        MiddlewareOutput::Terminate
    }
}
