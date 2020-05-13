use crate::http::request::{Request, Method};
use async_std::fs::File;
use crate::http::response::{Status, Response};
use crate::server::cond_checker::{ConditionalInformation, ConditionalChecker};
use crate::consts;
use crate::{util, log};
use crate::http::message::MessageBuilder;
use async_std::path::Path;
use async_std::fs;
use chrono::{DateTime, Utc};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use crate::server::middleware::{MiddlewareOutput, MiddlewareResult};
use crate::server::range_parser::{RangeParser, RangeBody};
use crate::server::dir_lister::DirectoryLister;
use crate::server::templates::template_container::TemplateContainer;
use crate::server::config_loader::Config;

pub struct ResponseGenerator<'a, 'b, 'c> {
    config: &'a Config,
    templates: &'b TemplateContainer,
    request: &'c Request,

    response: MessageBuilder<Response>,
    body: Vec<u8>,
    media_type: String,
}

impl<'a, 'b, 'c> ResponseGenerator<'a, 'b, 'c> {
    pub fn new(config: &'a Config, templates: &'b TemplateContainer, request: &'c Request) -> Self {
        ResponseGenerator {
            config,
            templates,
            request,
            response: MessageBuilder::<Response>::new(),
            body: vec![],
            media_type: consts::H_MEDIA_BINARY.to_string(),
        }
    }

    pub async fn get_response(mut self) -> MiddlewareResult<()> {
        let is_head = self.request.method == Method::Head;

        let raw_target = &self.request.uri.to_string();
        let replaced_target = if raw_target == "/" { self.config.route_empty_to.as_str() } else { raw_target };
        let target = format!("{}{}", &self.config.file_root, replaced_target);
        let file = match File::open(&target).await {
            Ok(file) => file,
            _ => return Err(MiddlewareOutput::Error(Status::NotFound, false)),
        };

        let metadata = file.metadata().await?;
        let last_modified = Some(metadata.modified()?.into());
        let etag = Some(Self::generate_etag(&last_modified.unwrap()));
        let info = ConditionalInformation::new(etag, last_modified);
        let can_send_range = match ConditionalChecker::new(&info, &self.request.headers).check() {
            Err(MiddlewareOutput::Status(Status::Ok, ..)) => false,
            Err(output) if !metadata.is_dir() => return Err(output),
            _ => true,
        };

        if metadata.is_dir() {
            let target_trimmed = raw_target.trim_end_matches('/').to_string();
            self.media_type = consts::H_MEDIA_HTML.to_string();
            self.body = DirectoryLister::new(&target_trimmed, &target, self.templates)
                .get_listing_body()
                .await?
                .into_bytes();
        } else {
            let file_ext = Path::new(&target).extension().and_then(|s| s.to_str()).unwrap_or("");
            if !is_head {
                self.body = fs::read(&target).await?
            }
            self.media_type = util::media_type_by_ext(file_ext).to_string();
            if can_send_range && !is_head {
                if let Some(output) = self.get_range_body() {
                    return Err(output);
                }
            }
        }

        let response = self
            .response
            .with_header(consts::H_ETAG, &info.etag.unwrap())
            .with_header(consts::H_LAST_MODIFIED, &util::format_time_imf(&info.last_modified.unwrap().into()))
            .with_body(self.body, &self.media_type)
            .build();

        log::info(format!("({}) {} {}", response.status, &self.request.method, &self.request.uri));
        Err(MiddlewareOutput::Response(response, false))
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
