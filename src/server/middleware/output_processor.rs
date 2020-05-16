use crate::http::response::{Status, Response};
use async_std::io::{self, Write};
use crate::http::request::{Request, Method};
use crate::log;
use crate::consts;
use crate::http::message::MessageBuilder;
use crate::server::template::templates::Templates;
use crate::server::template::{TemplateSubstitution, SubstitutionMap};
use crate::server::middleware::MiddlewareOutput;
use async_std::io::prelude::WriteExt;

pub struct OutputProcessor<'a, 'b, 'c, W: Write + Unpin> {
    writer: &'a mut W,
    templates: &'b Templates,
    request: Option<&'c Request>,
}

impl<'a, 'b, 'c, W: Write + Unpin> OutputProcessor<'a, 'b, 'c, W> {
    pub fn new(writer: &'a mut W, templates: &'b Templates, request: Option<&'c Request>) -> Self {
        OutputProcessor { writer, templates, request }
    }

    pub async fn process(&mut self, output: MiddlewareOutput) -> bool {
        match output {
            MiddlewareOutput::Error(status, close) => self.respond_error(status, close).await,
            MiddlewareOutput::Status(status, close) => self.respond_status(status, close).await,
            MiddlewareOutput::Response(response, close) => self.respond_response(response, close).await,
            MiddlewareOutput::Bytes(bytes, close) => self.respond_bytes(bytes, close).await,
            _ => true,
        }
    }

    async fn respond_error(&mut self, status: Status, close: bool) -> bool {
        self.log_request(Some(status));

        let mut sub = SubstitutionMap::new();
        sub.insert("server".to_string(), TemplateSubstitution::Single(consts::SERVER_NAME_VERSION.to_string()));
        sub.insert("status".to_string(), TemplateSubstitution::Single(status.to_string()));
        let body = self.templates.error.substitute(&sub).unwrap().into_bytes();

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
        self.log_request(Some(status));

        let mut response = MessageBuilder::<Response>::new();
        if close {
            response.set_header(consts::H_CONNECTION, consts::H_CONN_CLOSE);
        }
        response.with_status(status).build().send(self.writer).await.is_err() || close
    }

    async fn respond_response(&mut self, response: Response, close: bool) -> bool {
        response.send(self.writer).await.is_err() || close
    }

    async fn respond_bytes(&mut self, bytes: Vec<u8>, close: bool) -> bool {
        self.log_request(None);

        io::timeout(consts::MAX_WRITE_TIMEOUT, async {
            self.writer.write_all(&bytes).await?;
            self.writer.flush().await
        }).await.is_err() || close
    }

    fn log_request(&self, status: Option<Status>) {
        let status = match status {
            Some(status) if status == Status::RequestTimeout => return,
            Some(status) => status.to_string(),
            _ => " - ".to_string(),
        };

        match self.request {
            Some(request) => log::info(format!("({}) {} {}", status, request.method, request.uri)),
            _ => log::info(format!("({})", status)),
        }
    }
}
