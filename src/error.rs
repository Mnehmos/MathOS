use std::fmt::{Display, Formatter};

use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct AppError {
    pub code: &'static str,
    pub message: String,
    pub retryable: bool,
    pub corrective_action: String,
}

impl AppError {
    pub fn new(
        code: &'static str,
        message: impl Into<String>,
        retryable: bool,
        corrective_action: impl Into<String>,
    ) -> Self {
        Self {
            code,
            message: message.into(),
            retryable,
            corrective_action: corrective_action.into(),
        }
    }

    pub fn io(context: &str, error: std::io::Error) -> Self {
        Self::new(
            "MCL_IO_ERROR",
            format!("{context}: {error}"),
            false,
            "Check the configured root, permissions, and available disk space.",
        )
    }

    pub fn database(context: &str, error: rusqlite::Error) -> Self {
        Self::new(
            "MCL_DATABASE_ERROR",
            format!("{context}: {error}"),
            error.sqlite_error().is_some(),
            "Run `mcl doctor` and restore from a verified backup if integrity failed.",
        )
    }
}

impl Display for AppError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for AppError {}
