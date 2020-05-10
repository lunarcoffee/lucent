use crate::http::request::{Request, Method, HttpVersion};
use crate::http::response::{Response, Status};
use crate::http::headers::Headers;
use crate::http::consts;
use std::collections::HashMap;
use crate::http::uri::Uri;
use crate::util;

pub trait Message {
    fn get_headers_mut(&mut self) -> &mut Headers;
    fn get_body_mut(&mut self) -> &mut Option<Vec<u8>>;

    fn into_bytes(self) -> Vec<u8>;
}

pub struct MessageBuilder<M: Message> {
    message: M,
}

impl MessageBuilder<Request> {
    pub fn new() -> Self {
        let mut headers = Headers::from(HashMap::new());
        headers.set_one(consts::H_CONTENT_LENGTH, "0");

        MessageBuilder {
            message: Request {
                method: Method::Get,
                uri: Uri::AsteriskForm,
                http_version: HttpVersion::Http11,
                headers,
                body: None,
            }
        }
    }

    pub fn with_method(mut self, method: Method) -> Self {
        self.message.method = method;
        self
    }

    pub fn with_uri(mut self, uri: Uri) -> Self {
        self.message.uri = uri;
        self
    }
}

impl MessageBuilder<Response> {
    pub fn new() -> Self {
        let mut headers = Headers::from(HashMap::new());
        headers.set_one(consts::H_CONTENT_LENGTH, "0");
        headers.set_one(consts::H_SERVER, consts::SERVER_NAME_VERSION);
        headers.set_one(consts::H_DATE, &util::format_time_imf(&util::get_time_utc()));

        MessageBuilder {
            message: Response {
                http_version: HttpVersion::Http11,
                status: Status::Ok,
                headers,
                body: None,
            }
        }
    }

    pub fn with_status(mut self, status: Status) -> Self {
        self.message.status = status;
        if status == Status::NoContent || status < Status::Ok {
            self.message.headers.remove(consts::H_CONTENT_LENGTH);
        }
        self
    }
}

impl<M: Message> MessageBuilder<M> {
    pub fn with_header(mut self, name: &str, value: &str) -> Self {
        self.message.get_headers_mut().set_one(&name, value);
        self
    }

    pub fn with_header_multi(mut self, name: &str, value: Vec<&str>) -> Self {
        self.message.get_headers_mut().set(&name, value);
        self
    }

    pub fn with_body(mut self, body: Vec<u8>, media_type: &str) -> Self {
        self = self
            .with_header(consts::H_CONTENT_LENGTH, &body.len().to_string())
            .with_header(consts::H_CONTENT_TYPE, media_type);

        *self.message.get_body_mut() = Some(body);
        self
    }

    pub fn build(self) -> M {
        self.message
    }
}
