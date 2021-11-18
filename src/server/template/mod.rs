use std::collections::HashMap;

use crate::server::template::template_parser::TemplateParser;

// Container for the templates used by `FileServer`.
pub mod templates;

mod template_parser;

// The name of a template variable (placeholder).
pub type PlaceholderName = String;

// See the comment on `Template`.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum TemplatePart {
    // Regular text, no special processing.
    String(String),

    // A placeholder which can take a single value. The syntax is the placeholder's name in square brackets (i.e.
    // 'welcome back, [user_name]').
    Placeholder(PlaceholderName),

    // A placeholder which can take many values, each fitting the `Template` (which is substituted once for each
    // value). Since each value is itself a template, it is possible to have arbitrarily deep templates. The syntax
    // for this is '*', followed by the placeholder's name, then square brackets. Within the brackets is the
    // template each value will be substituted into. See '/resources/templates/dir_listing.html' for an example.
    MultiplePlaceholder(PlaceholderName, Template),
}

// Mapping placeholders to their values, used when calling `substitute` on a template.
pub type SubstitutionMap = HashMap<PlaceholderName, TemplateSubstitution>;

// The value to substitute for a placeholder.
#[derive(Debug)]
pub enum TemplateSubstitution {
    // Just a string, used for the single placeholders.
    Single(String),

    // Used for multi-value placeholders. Each of the values is itself a template (hence the use of
    // `SubstitutionMap`).
    Multiple(Vec<SubstitutionMap>),
}

// A template is basically many parts concatenated. See any of the files in '/resources/templates' for examples.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct Template {
    pub parts: Vec<TemplatePart>,
}

impl Template {
    // Attempts to parse a template from a file.
    pub fn new(file: String) -> Option<Self> { TemplateParser::new(file).parse() }

    pub fn new_empty() -> Self { Template { parts: vec![] } }

    // Attempts to substitute values from `placeholders` into this template.
    pub fn substitute(&self, placeholders: &SubstitutionMap) -> Option<String> {
        let mut output = String::new();

        // Build up the `output` by processing each part.
        for part in &self.parts {
            match part {
                // Don't do anything special with string parts.
                TemplatePart::String(value) => output.push_str(value),
                // Substitute a single value only if a placeholder with that name exists in the template, and if it is
                // a single-value placeholder.
                TemplatePart::Placeholder(name) => match placeholders.get(name) {
                    Some(TemplateSubstitution::Single(output_part)) => output.push_str(output_part),
                    _ => return None,
                },
                // Substitute multiple values (recursively) only if a placeholder with that name exists in the
                // template, and if it is a multi-value placeholder.
                TemplatePart::MultiplePlaceholder(name, template) => match placeholders.get(name) {
                    Some(TemplateSubstitution::Multiple(maps)) => {
                        for map in maps {
                            output.push_str(&template.substitute(map)?);
                        }
                    }
                    _ => return None,
                },
            };
        }
        Some(output)
    }
}
