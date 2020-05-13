use crate::server::templates::{TemplatePart, Template};
use std::ops::Index;

pub struct TemplateParser {
    file: String,
    parts: Vec<TemplatePart>,
}

impl TemplateParser {
    pub fn new(file: String) -> Self {
        TemplateParser {
            file,
            parts: vec![],
        }
    }

    pub fn parse(mut self) -> Option<Template> {
        let parts = self.parse_parts()?;
        Some(Template { parts })
    }

    fn parse_parts(mut self) -> Option<Vec<TemplatePart>> {
        let chars = self.file.chars().collect::<Vec<_>>();
        let mut pos = 0;

        while pos < chars.len() {
            let part = match chars[pos] {
                '[' => {
                    let end_index = chars[pos..].iter().position(|c| *c == ']')? + pos;
                    let name = chars[pos + 1..end_index].iter().collect();

                    pos = end_index + 1;
                    TemplatePart::Placeholder(name)
                }
                '~' => {
                    let start_index = chars[pos..].iter().position(|c| *c == '[')? + pos;
                    let mut depth = 0;
                    let end_index = chars[start_index + 1..].iter().position(|c| {
                        depth += "] [".find(|ch| ch == *c).unwrap_or(1) as i32 - 1;
                        *c == ']' && depth < 0
                    })? + start_index + 1;
                    let sub_template = chars[start_index + 1..end_index].iter().collect();

                    let name = chars[pos + 1..start_index].iter().collect();
                    let parts = TemplateParser::new(sub_template).parse()?;

                    pos = end_index + 1;
                    TemplatePart::MultiplePlaceholder(name, parts)
                }
                _ => {
                    let start_of_next_part = chars[pos..]
                        .iter()
                        .position(|c| *c == '[' || *c == '~')
                        .unwrap_or(chars.len() - pos)
                        + pos;
                    let text = chars[pos..start_of_next_part].iter().collect();

                    pos = start_of_next_part;
                    TemplatePart::String(text)
                }
            };
            self.parts.push(part);
        }
        Some(self.parts)
    }
}
