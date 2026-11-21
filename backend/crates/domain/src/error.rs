use thiserror::Error;

use crate::validation::ValidationIssue;

/// Base error for domain rule violations.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum DomainError {
    /// An input value violates a domain rule.
    #[error("validation failed on `{}`: {}", .0.field, .0.issue.as_str())]
    Validation(ValidationIssue),
}
