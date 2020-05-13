use crate::server::templates::Template;
use async_std::fs;
use crate::consts;

#[derive(Clone)]
pub struct TemplateContainer {
    pub error: Template,
    pub dir_listing: Template,
}

impl TemplateContainer {
    pub async fn new(template_root: String) -> Option<Self> {
        let error_path = format!("{}/{}", template_root, consts::TEMPLATE_ERROR);
        let dir_listing_path = format!("{}/{}", template_root, consts::TEMPLATE_DIR_LISTING);

        let error_template = fs::read_to_string(error_path).await.ok()?;
        let dir_listing_template = fs::read_to_string(dir_listing_path).await.ok()?;

        let error = Template::new(error_template)?;
        let dir_listing = Template::new(dir_listing_template)?;
        Some(TemplateContainer { error, dir_listing })
    }
}
