use crate::http::request::{Request, Method};
use async_std::fs::File;
use crate::http::response::{Status, Response};
use crate::server::cond_checker::{ConditionalInformation, ConditionalChecker};
use crate::http::consts;
use crate::{util, log};
use crate::http::message::MessageBuilder;
use async_std::path::Path;
use async_std::{fs, io};
use chrono::{DateTime, Utc};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use crate::server::middleware::MiddlewareOutput;

pub struct ResponseGenerator<'a, 'b> {
    file_root: &'a str,
    request: &'b Request,
}

impl<'a, 'b> ResponseGenerator<'a, 'b> {
    pub fn new(file_root: &'a str, request: &'b Request) -> Self {
        ResponseGenerator { file_root, request }
    }

    pub async fn get_response(&mut self) -> Result<MiddlewareOutput, io::Error> {
        let target = &self.request.uri.to_string();
        let target = format!("{}{}", &self.file_root, if target == "/" { "/index.html" } else { target });
        let file = match File::open(&target).await {
            Ok(file) => file,
            _ => return Ok(MiddlewareOutput::Error(Status::NotFound, false)),
        };

        let last_modified = Some(file.metadata().await?.modified()?.into());
        let etag = Some(Self::generate_etag(&last_modified.unwrap()));
        let info = ConditionalInformation::new(etag, last_modified);
        match ConditionalChecker::new(&info, &self.request.headers).check() {
            Err(output) => return Ok(output),
            _ => {}
        };

        let body = fs::read(&target).await?;
        let file_ext = Path::new(&target).extension().and_then(|s| s.to_str()).unwrap_or("");
        let media_type = util::media_type_by_ext(file_ext);
        let body = if self.request.method == Method::Head { vec![] } else { body };

        log::info(format!("({}) {} {}", Status::Ok, &self.request.method, &self.request.uri));
        let response = MessageBuilder::<Response>::new()
            .with_header(consts::H_ETAG, &info.etag.unwrap())
            .with_header(consts::H_LAST_MODIFIED, &util::format_time_imf(&info.last_modified.unwrap().into()))
            .with_body(body, media_type)
            .build();
        Ok(MiddlewareOutput::Response(response, false))
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
