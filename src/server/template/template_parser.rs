use crate::server::template::{Template, TemplatePart};

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

    pub fn parse(self) -> Option<Template> {
        let parts = self.parse_parts()?;
        Some(Template { parts })
    }

    fn parse_parts(mut self) -> Option<Vec<TemplatePart>> {
        let chars = self.file.chars().collect::<Vec<_>>();
        let mut pos = 0;

        while pos < chars.len() {
            self.parts.push(match chars[pos] {
                '[' => {
                    let end_index = chars[pos..].iter().position(|c| *c == ']')? + pos;
                    let name = chars[pos + 1..end_index].iter().collect();

                    pos = end_index + 1;
                    TemplatePart::Placeholder(name)
                }
                '*' => {
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
                '\\' => {
                    pos += 2;
                    TemplatePart::String(chars[pos - 1].to_string())
                }
                _ => {
                    let start_of_next_part = chars[pos..]
                        .iter()
                        .position(|c| "[*\\".contains(*c))
                        .unwrap_or(chars.len() - pos)
                        + pos;
                    let text = chars[pos..start_of_next_part].iter().collect();

                    pos = start_of_next_part;
                    TemplatePart::String(text)
                }
            });
        }
        Some(self.parts)
    }
}
