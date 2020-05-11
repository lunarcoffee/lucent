use crate::http::request::{Request, Method, HttpVersion};
use crate::http::response::{Response, Status};
use crate::http::headers::Headers;
use std::collections::HashMap;
use crate::http::uri::Uri;
use crate::{util, consts};

pub trait Message {
    fn get_headers_mut(&mut self) -> &mut Headers;
    fn get_body_mut(&mut self) -> &mut Option<Vec<u8>>;
    fn set_chunked(&mut self);

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
                chunked: false,
            }
        }
    }

    pub fn set_method(&mut self, method: Method) {
        self.message.method = method;
    }

    pub fn with_method(mut self, method: Method) -> Self {
        self.set_method(method);
        self
    }

    pub fn set_uri(&mut self, uri: Uri) {
        self.message.uri = uri;
    }

    pub fn with_uri(mut self, uri: Uri) -> Self {
        self.set_uri(uri);
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
                chunked: false,
            }
        }
    }

    pub fn set_status(&mut self, status: Status) {
        self.message.status = status;
        if status == Status::NoContent || status < Status::Ok {
            self.message.headers.remove(consts::H_CONTENT_LENGTH);
        }
    }

    pub fn with_status(mut self, status: Status) -> Self {
        self.set_status(status);
        self
    }
}

impl<M: Message> MessageBuilder<M> {
    pub fn set_header(&mut self, name: &str, value: &str) {
        self.message.get_headers_mut().set_one(&name, value);
    }

    pub fn with_header(mut self, name: &str, value: &str) -> Self {
        self.set_header(name, value);
        self
    }

    pub fn unset_header(&mut self, name: &str) {
        self.message.get_headers_mut().remove(name);
    }

    pub fn without_header(mut self, name: &str) -> Self {
        self.unset_header(name);
        self
    }

    pub fn set_header_multi(&mut self, name: &str, value: Vec<&str>) {
        self.message.get_headers_mut().set(&name, value);
    }

    pub fn with_header_multi(mut self, name: &str, value: Vec<&str>) -> Self {
        self.set_header_multi(name, value);
        self
    }

    pub fn with_body(mut self, body: Vec<u8>, media_type: &str) -> Self {
        if body.len() > consts::MAX_BODY_LENGTH {
            self.message.set_chunked();
            self = self
                .with_header(consts::H_TRANSFER_ENCODING, consts::H_T_ENC_CHUNKED)
                .without_header(consts::H_CONTENT_LENGTH);
        } else {
            self.set_header(consts::H_CONTENT_LENGTH, &body.len().to_string());
        }

        *self.message.get_body_mut() = Some(body);
        self.with_header(consts::H_CONTENT_TYPE, media_type)
    }

    pub fn build(self) -> M {
        self.message
    }
}
