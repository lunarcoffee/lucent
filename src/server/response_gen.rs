use crate::http::request::{Request, Method};
use async_std::fs::File;
use crate::http::response::{Status, Response};
use crate::server::cond_checker::{ConditionalInformation, ConditionalChecker};
use crate::consts;
use crate::{util, log};
use crate::http::message::MessageBuilder;
use async_std::path::Path;
use async_std::{fs, io};
use chrono::{DateTime, Utc};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use crate::server::middleware::{MiddlewareOutput};
use crate::server::range_parser::{RangeParser, RangeBody};

pub struct ResponseGenerator<'a, 'b> {
    file_root: &'a str,
    request: &'b Request,

    response: MessageBuilder<Response>,
    body: Vec<u8>,
    media_type: String,
}

impl<'a, 'b> ResponseGenerator<'a, 'b> {
    pub fn new(file_root: &'a str, request: &'b Request) -> Self {
        ResponseGenerator {
            file_root,
            request,
            response: MessageBuilder::<Response>::new(),
            body: vec![],
            media_type: consts::H_MEDIA_BINARY.to_string(),
        }
    }

    pub async fn get_response(mut self) -> Result<MiddlewareOutput, io::Error> {
        let is_head = self.request.method == Method::Head;

        let target = &self.request.uri.to_string();
        let target = format!("{}{}", &self.file_root, if target == "/" { "/index.html" } else { target });
        let file = match File::open(&target).await {
            Ok(file) => file,
            _ => return Ok(MiddlewareOutput::Error(Status::NotFound, false)),
        };

        let last_modified = Some(file.metadata().await?.modified()?.into());
        let etag = Some(Self::generate_etag(&last_modified.unwrap()));
        let info = ConditionalInformation::new(etag, last_modified);
        let can_send_range = match ConditionalChecker::new(&info, &self.request.headers).check() {
            Err(MiddlewareOutput::Status(Status::Ok, ..)) => false,
            Err(output) => return Ok(output),
            _ => true,
        };

        if !is_head {
            self.body = fs::read(&target).await?
        }
        let file_ext = Path::new(&target).extension().and_then(|s| s.to_str()).unwrap_or("");
        self.media_type = util::media_type_by_ext(file_ext).to_string();
        if can_send_range && !is_head {
            if let Some(output) = self.get_range_body() {
                return Ok(output);
            }
        }

        let response = self
            .response
            .with_header(consts::H_ETAG, &info.etag.unwrap())
            .with_header(consts::H_LAST_MODIFIED, &util::format_time_imf(&info.last_modified.unwrap().into()))
            .with_body(self.body, &self.media_type)
            .build();

        log::info(format!("({}) {} {}", response.status, &self.request.method, &self.request.uri));
        Ok(MiddlewareOutput::Response(response, false))
    }

    fn get_range_body(&mut self) -> Option<MiddlewareOutput> {
        match RangeParser::new(&self.request.headers, &self.body, &self.media_type).get_body() {
            Err(output) => return Some(output),
            Ok(RangeBody::Range(body, content_range)) => {
                self.body = body;
                self.response.set_header(consts::H_CONTENT_RANGE, &content_range);
                self.response.set_status(Status::PartialContent);
            }
            Ok(RangeBody::MultipartRange(body, media_type)) => {
                self.body = body;
                self.media_type = media_type;
                self.response.set_status(Status::PartialContent);
            }
            _ => {}
        }
        None
    }

    fn generate_etag(modified: &DateTime<Utc>) -> String {
        let mut hasher = DefaultHasher::new();
        let time = util::format_time_imf(modified);
        time.hash(&mut hasher);

        let etag = format!("\"{:x}", hasher.finish());
        time.chars().into_iter().rev().collect::<String>().hash(&mut hasher);

        etag + &format!("{:x}\"", hasher.finish())
    }
}
