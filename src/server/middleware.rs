use crate::http::response::{Status, Response};
use std::error;
use async_std::fs;
use async_std::io::Write;
use crate::http::request::{Request, Method};
use crate::log;
use async_std::path::Path;
use crate::consts;
use crate::http::message::{MessageBuilder, Message};

pub enum MiddlewareOutput {
    Error(Status, bool),
    Status(Status, bool),
    Response(Response, bool),
    Terminate,
}

impl<T: error::Error> From<T> for MiddlewareOutput {
    fn from(_: T) -> Self {
        MiddlewareOutput::Terminate
    }
}

pub type MiddlewareResult<T> = Result<T, MiddlewareOutput>;

pub struct OutputProcessor<'a, 'b, 'c, W: Write + Unpin> {
    writer: &'a mut W,
    template_root: &'b str,
    request: Option<&'c Request>,
}

impl<'a, 'b, 'c, W: Write + Unpin> OutputProcessor<'a, 'b, 'c, W> {
    pub fn new(writer: &'a mut W, template_root: &'b str, request: Option<&'c Request>) -> Self {
        OutputProcessor { writer, template_root, request }
    }

    pub async fn process(&mut self, output: MiddlewareOutput) -> bool {
        match output {
            MiddlewareOutput::Error(status, close) => self.respond_error(status, close).await,
            MiddlewareOutput::Status(status, close) => self.respond_status(status, close).await,
            MiddlewareOutput::Response(response, close) => self.respond_response(response, close).await,
            _ => true,
        }
    }

    async fn respond_error(&mut self, status: Status, close: bool) -> bool {
        self.log_request(status);

        let error_file = format!("{}/error.html", self.template_root);
        let body = if !Path::new(&error_file).is_file().await {
            return true;
        } else {
            let status = status.to_string();
            match fs::read_to_string(&error_file).await {
                Ok(file) => file
                    .replace("{server}", consts::SERVER_NAME_VERSION)
                    .replace("{status}", &status)
                    .into_bytes(),
                _ => return true,
            }
        };

        let mut response = MessageBuilder::<Response>::new();
        if close {
            response.set_header(consts::H_CONNECTION, consts::H_CONN_CLOSE)
        }
        response
            .with_status(status)
            .with_header_multi(consts::H_ACCEPT, vec![&Method::Get.to_string(), &Method::Head.to_string()])
            .with_body(body, consts::H_MEDIA_HTML)
            .build()
            .send(self.writer)
            .await
            .is_err() || close
    }

    async fn respond_status(&mut self, status: Status, close: bool) -> bool {
        self.log_request(status);

        let mut response = MessageBuilder::<Response>::new();
        if close {
            response.set_header(consts::H_CONNECTION, consts::H_CONN_CLOSE);
        }
        response.with_status(status).build().send(self.writer).await.is_err() || close
    }

    async fn respond_response(&mut self, response: Response, close: bool) -> bool {
        response.send(self.writer).await.is_err() || close
    }

    fn log_request(&self, status: Status) {
        if status != Status::RequestTimeout {
            match self.request {
                Some(request) => log::info(format!("({}) {} {}", status, request.method, request.uri)),
                _ => log::info(format!("({})", status)),
            }
        }
    }
}
