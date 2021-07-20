use crate::server::template::{Template, TemplatePart};

// Parser for reading template files into a sequence of their parts.
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

    // Attempts to parse the file into a sequence of template parts.
    fn parse_parts(mut self) -> Option<Vec<TemplatePart>> {
        let chars = self.file.chars().collect::<Vec<_>>();
        let mut pos = 0;

        while pos < chars.len() {
            self.parts.push(match chars[pos] {
                // Beginning a single-value placeholder.
                '[' => {
                    // Find the end of the placeholder and extract its name.
                    let end_index = chars[pos..].iter().position(|c| *c == ']')? + pos;
                    let name = chars[pos + 1..end_index].iter().collect();

                    pos = end_index + 1;
                    TemplatePart::Placeholder(name)
                }
                // Beginning a multi-value placeholder.
                '*' => {
                    // Find the start of this placeholder.
                    let start_index = chars[pos..].iter().position(|c| *c == '[')? + pos;

                    // Find the end.
                    let mut depth = 0;
                    let end_index = chars[start_index + 1..].iter().position(|c| {
                        // Adjust the depth when encountering the start or end of a placeholder.
                        depth += "] [".find(|ch| ch == *c).unwrap_or(1) as i32 - 1;

                        // If we've hit the end of a placeholder and the depth has gone negative, that means we've
                        // exited the current placeholder, so this is the index of its end.
                        *c == ']' && depth < 0
                    })? + start_index + 1;

                    // Extract the template for the values of this placeholder and try parsing it.
                    let sub_template = chars[start_index + 1..end_index].iter().collect();
                    let parts = TemplateParser::new(sub_template).parse()?;

                    let name = chars[pos + 1..start_index].iter().collect();

                    pos = end_index + 1;
                    TemplatePart::MultiplePlaceholder(name, parts)
                }
                // Skip the character following any '\'.
                '\\' => {
                    pos += 2;
                    TemplatePart::String(chars[pos - 1].to_string())
                }
                // Nothing special, extract everything until the next special character.
                _ => {
                    let start_of_next_part = chars[pos..]
                        .iter()
                        .position(|c| "[*\\".contains(*c))
                        .unwrap_or(chars.len() - pos)
                        + pos;
                    let text = chars[pos..start_of_next_part].iter().collect();

                    // Jump to the next special character.
                    pos = start_of_next_part;
                    TemplatePart::String(text)
                }
            });
        }
        Some(self.parts)
    }
}
