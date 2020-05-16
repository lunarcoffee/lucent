use crate::server::template::template_parser::TemplateParser;
use std::collections::HashMap;

pub mod templates;

mod template_parser;

pub type PlaceholderName = String;

#[derive(Clone, Eq, Hash, PartialEq)]
pub enum TemplatePart {
    String(String),
    Placeholder(PlaceholderName),
    MultiplePlaceholder(PlaceholderName, Template),
}

pub type SubstitutionMap = HashMap<PlaceholderName, TemplateSubstitution>;

pub enum TemplateSubstitution {
    Single(String),
    Multiple(Vec<SubstitutionMap>),
}

#[derive(Clone, Eq, Hash, PartialEq)]
pub struct Template {
    pub parts: Vec<TemplatePart>
}

impl Template {
    pub fn new(file: String) -> Option<Self> {
        TemplateParser::new(file).parse()
    }

    pub fn substitute(&self, placeholders: &SubstitutionMap) -> Option<String> {
        let mut output = String::new();
        for part in &self.parts {
            match part {
                TemplatePart::String(value) => output.push_str(value),
                TemplatePart::Placeholder(name) => match placeholders.get(name) {
                    Some(TemplateSubstitution::Single(output_part)) => output.push_str(output_part),
                    _ => return None,
                },
                TemplatePart::MultiplePlaceholder(name, template) => match placeholders.get(name) {
                    Some(TemplateSubstitution::Multiple(maps)) => for map in maps {
                        output.push_str(&template.substitute(map)?);
                    },
                    _ => return None,
                },
            };
        }
        Some(output)
    }
}
