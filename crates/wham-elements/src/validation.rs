use regex::Regex;
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;

fn email_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^[^\s@]+@[^\s@]+\.[^\s@]+$").expect("valid email regex"))
}

use crate::form::{FieldValue, FormPath};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum ValidationRule {
    Required,
    Regex { pattern: String },
    NumberRange { min: Option<f64>, max: Option<f64> },
    Email,
    Custom { name: String },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ValidationError {
    pub path: FormPath,
    pub message: String,
}

pub fn validate_value(path: &FormPath, value: &FieldValue, rules: &[ValidationRule]) -> Vec<ValidationError> {
    let mut errors = Vec::new();
    for rule in rules {
        match rule {
            ValidationRule::Required => {
                let is_empty = match value {
                    FieldValue::Text(v) => v.trim().is_empty(),
                    FieldValue::Number(v) => v.is_nan(),
                    FieldValue::Bool(_) => false,
                    FieldValue::Selection(v) => v.is_empty(),
                    FieldValue::Group(_) => false,
                    FieldValue::GroupList(list) => list.is_empty(),
                };
                if is_empty {
                    errors.push(ValidationError {
                        path: path.clone(),
                        message: "This field is required.".to_string(),
                    });
                }
            }
            ValidationRule::Regex { pattern } => {
                if let FieldValue::Text(text) = value {
                    if let Ok(re) = Regex::new(pattern) {
                        if !re.is_match(text) {
                            errors.push(ValidationError {
                                path: path.clone(),
                                message: "Value does not match required format.".to_string(),
                            });
                        }
                    }
                }
            }
            ValidationRule::NumberRange { min, max } => {
                if let FieldValue::Number(num) = value {
                    if let Some(min) = min {
                        if *num < *min {
                            errors.push(ValidationError {
                                path: path.clone(),
                                message: format!("Value must be >= {}", min),
                            });
                        }
                    }
                    if let Some(max) = max {
                        if *num > *max {
                            errors.push(ValidationError {
                                path: path.clone(),
                                message: format!("Value must be <= {}", max),
                            });
                        }
                    }
                }
            }
            ValidationRule::Email => {
                if let FieldValue::Text(text) = value {
                    if !email_regex().is_match(text) {
                        errors.push(ValidationError {
                            path: path.clone(),
                            message: "Please enter a valid email address.".to_string(),
                        });
                    }
                }
            }
            ValidationRule::Custom { name: _ } => {}
        }
    }
    errors
}
