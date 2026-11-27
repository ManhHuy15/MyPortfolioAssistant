use domain::errors::DomainError;
use thiserror::Error;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum AppError {
    #[error(transparent)]
    Domain(#[from] DomainError),
}
