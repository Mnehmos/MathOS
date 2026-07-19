use std::fmt::{Display, Formatter};
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::AppError;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RecordKind {
    Source,
    Concept,
    Claim,
    Formalization,
    LearningUnit,
}

impl RecordKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Source => "source",
            Self::Concept => "concept",
            Self::Claim => "claim",
            Self::Formalization => "formalization",
            Self::LearningUnit => "learning_unit",
        }
    }
}

impl Display for RecordKind {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for RecordKind {
    type Err = AppError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "source" => Ok(Self::Source),
            "concept" => Ok(Self::Concept),
            "claim" => Ok(Self::Claim),
            "formalization" => Ok(Self::Formalization),
            "learning_unit" => Ok(Self::LearningUnit),
            _ => Err(AppError::new(
                "MCL_RECORD_KIND_UNKNOWN",
                format!("unknown canonical record kind `{value}`"),
                false,
                "Use a record kind declared by the committed domain model.",
            )),
        }
    }
}

#[derive(Clone, Debug)]
pub struct RecordDraft {
    pub kind: RecordKind,
    pub schema_version: String,
    pub payload: Value,
    pub searchable_text: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RecordSnapshot {
    pub object_id: String,
    pub kind: RecordKind,
    pub version_hash: String,
    pub schema_version: String,
    pub payload: Value,
    pub predecessor_hash: Option<String>,
    pub created_at: i64,
    pub created_by: String,
}
