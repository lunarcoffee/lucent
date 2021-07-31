use async_std::io::{self, prelude::WriteExt, Write};

use crate::{
    consts,
    http::{message::{Body, MessageBuilder}, request::{Method, Request}, response::{Response, Status}},
    log,
    server::{
        middleware::MiddlewareOutput,
        template::{SubstitutionMap, templates::Templates, TemplateSubstitution},
    },
};

// Processor for any `Err(MiddlewareOutput)` results from middleware, writing the appropriate response to `writer`,
// using `templates` if necessary (i.e. for error pages). `request` is used to log the method and target.
pub struct OutputProcessor<'a, W: Write + Unpin> {
    writer: &'a mut W,
    templates: &'a Templates,
    request: Option<&'a Request>,
}

impl<'a, W: Write + Unpin> OutputProcessor<'a, W> {
    pub fn new(writer: &'a mut W, templates: &'a Templates, request: Option<&'a Request>) -> Self {
        OutputProcessor { writer, templates, request }
    }

    // Send the response specified by `output` to the client, returning whether the connection should be closed (true
    // means it should be closed).
    pub async fn process(&mut self, output: MiddlewareOutput) -> bool {
        match output {
            MiddlewareOutput::Error(status, close) => self.respond_error(status, close).await,
            MiddlewareOutput::Status(status, close) => self.respond_status(status, close).await,
            MiddlewareOutput::Response(response, close) => self.respond_response(response, close).await,
            MiddlewareOutput::Bytes(bytes, close) => self.respond_bytes(bytes, close).await,
            _ => true,
        }
    }

    // Generates and sends a templated error page with the given `status`.
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
            .with_body(Body::Bytes(body), consts::H_MEDIA_HTML)
            .build()
            .send(self.writer)
            .await
            .is_err() || close
    }

    // Responds with a request of the given `status` with no body.
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

    // Logs the request status, along with the request's method and target if available.
    fn log_request(&self, status: Option<Status>) {
        let status = match status {
            Some(status) if status == Status::RequestTimeout => return,
            Some(status) => status.to_string(),
            // No status is available; this usually comes from an NPH script, as their output is not parsed.
            _ => " - ".to_string(),
        };

        match self.request {
            Some(request) => log::info(format!("({}) {} {}", status, request.method, request.uri)),
            _ => log::info(format!("({})", status)),
        }
    }
}
