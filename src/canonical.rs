use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::error::AppError;

pub const MAX_SAFE_INTEGER: u64 = 9_007_199_254_740_991;

pub fn canonical_json(value: &Value) -> Result<Vec<u8>, AppError> {
    validate_numbers(value)?;
    serde_json_canonicalizer::to_vec(value).map_err(|error| {
        AppError::new(
            "MCL_CANONICAL_JSON_INVALID",
            error.to_string(),
            false,
            "Use valid I-JSON values and encode integers outside the safe range as strings.",
        )
    })
}

pub fn record_version_hash(schema_version: &str, payload: &Value) -> Result<String, AppError> {
    if schema_version.trim().is_empty() || schema_version.as_bytes().contains(&0) {
        return Err(AppError::new(
            "MCL_SCHEMA_VERSION_INVALID",
            "schema version must be nonempty and cannot contain NUL",
            false,
            "Use the committed schema identifier for this record type.",
        ));
    }
    let canonical = canonical_json(payload)?;
    let mut digest = Sha256::new();
    digest.update(schema_version.as_bytes());
    digest.update([0]);
    digest.update(canonical);
    Ok(format!("{:x}", digest.finalize()))
}

pub fn value_hash(value: &Value) -> Result<String, AppError> {
    Ok(format!("{:x}", Sha256::digest(canonical_json(value)?)))
}

fn validate_numbers(value: &Value) -> Result<(), AppError> {
    match value {
        Value::Array(items) => {
            for item in items {
                validate_numbers(item)?;
            }
        }
        Value::Object(items) => {
            for item in items.values() {
                validate_numbers(item)?;
            }
        }
        Value::Number(number) => {
            let safe = if let Some(value) = number.as_u64() {
                value <= MAX_SAFE_INTEGER
            } else if let Some(value) = number.as_i64() {
                value >= -(MAX_SAFE_INTEGER as i64) && value <= MAX_SAFE_INTEGER as i64
            } else {
                number.as_f64().is_some_and(f64::is_finite)
            };
            if !safe {
                return Err(AppError::new(
                    "MCL_CANONICAL_NUMBER_UNSAFE",
                    format!("number {number} cannot be represented safely by RFC 8785"),
                    false,
                    "Encode exact integers outside the IEEE-754 safe range as JSON strings.",
                ));
            }
        }
        Value::Null | Value::Bool(_) | Value::String(_) => {}
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use serde_json::{Map, Number, json};

    use super::*;

    #[test]
    fn object_order_and_whitespace_do_not_change_identity() {
        let left: Value =
            serde_json::from_str(r#"{ "z": 2, "a": {"y": false, "x": 1} }"#).expect("left JSON");
        let right: Value =
            serde_json::from_str(r#"{"a":{"x":1,"y":false},"z":2}"#).expect("right JSON");
        assert_eq!(
            canonical_json(&left).expect("left"),
            canonical_json(&right).expect("right")
        );
        assert_eq!(
            record_version_hash("claim/1", &left).expect("left hash"),
            record_version_hash("claim/1", &right).expect("right hash")
        );
    }

    #[test]
    fn golden_canonical_json_vector_is_stable() {
        let value = json!({
            "z": 1.0,
            "é": "math",
            "a": [true, null, 0.002]
        });
        assert_eq!(
            canonical_json(&value).expect("canonical JSON"),
            "{\"a\":[true,null,0.002],\"z\":1,\"é\":\"math\"}".as_bytes()
        );
        assert_eq!(
            record_version_hash("canonical/1", &value).expect("golden hash"),
            "d7e1de32a2eebabc9ab7895e25ad56e969116cb287cbe2d5c806fa250c04918a"
        );
    }

    #[test]
    fn integers_outside_binary64_safe_range_fail_closed() {
        let value = Value::Number(Number::from(MAX_SAFE_INTEGER + 1));
        assert_eq!(
            canonical_json(&value).expect_err("unsafe integer").code,
            "MCL_CANONICAL_NUMBER_UNSAFE"
        );
    }

    #[test]
    fn unicode_is_preserved_without_hidden_normalization() {
        let mut composed = Map::new();
        composed.insert("term".to_owned(), Value::String("é".to_owned()));
        let mut decomposed = Map::new();
        decomposed.insert("term".to_owned(), Value::String("e\u{301}".to_owned()));
        assert_ne!(
            record_version_hash("concept/1", &Value::Object(composed)).expect("composed"),
            record_version_hash("concept/1", &Value::Object(decomposed)).expect("decomposed")
        );
    }
}
