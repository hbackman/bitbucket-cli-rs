//! Parser for `bbk api -F/--field` (typed) and `bbk api -f/--raw-field` (string)
//! flag values.
//!
//! Typed fields coerce `true`/`false`/`null` and decimal numbers; everything
//! else stays a JSON string. Raw fields are always strings, even if they
//! happen to look like a number.

use anyhow::{anyhow, Result};
use serde_json::Value;

#[derive(Debug, Clone)]
pub enum Field {
    Typed { key: String, value: Value },
    Raw { key: String, value: String },
}

impl Field {
    pub fn key(&self) -> &str {
        match self {
            Field::Typed { key, .. } | Field::Raw { key, .. } => key,
        }
    }
}

pub fn parse_typed(spec: &str) -> Result<Field> {
    let (key, raw) = split_kv(spec)?;
    let value = coerce(raw);
    Ok(Field::Typed {
        key: key.to_string(),
        value,
    })
}

pub fn parse_raw(spec: &str) -> Result<Field> {
    let (key, value) = split_kv(spec)?;
    Ok(Field::Raw {
        key: key.to_string(),
        value: value.to_string(),
    })
}

fn split_kv(spec: &str) -> Result<(&str, &str)> {
    spec.split_once('=')
        .filter(|(k, _)| !k.is_empty())
        .ok_or_else(|| anyhow!("expected KEY=VALUE, got {spec:?}"))
}

/// Coerce `--field` value strings: `true|false|null`, decimal numbers,
/// otherwise pass through as a JSON string.
fn coerce(raw: &str) -> Value {
    match raw {
        "true" => Value::Bool(true),
        "false" => Value::Bool(false),
        "null" => Value::Null,
        _ => {
            if let Ok(i) = raw.parse::<i64>() {
                Value::from(i)
            } else if let Ok(f) = raw.parse::<f64>() {
                Value::from(f)
            } else {
                Value::String(raw.to_string())
            }
        }
    }
}

/// Walk a dotted key (`source.branch.name`) into a nested object. Builds a fresh
/// object on the way down and inserts the value at the leaf.
pub fn assign_dotted(target: &mut serde_json::Map<String, Value>, key: &str, value: Value) {
    let parts: Vec<&str> = key.split('.').collect();
    if parts.len() == 1 {
        target.insert(parts[0].to_string(), value);
        return;
    }
    let mut current = target;
    for part in &parts[..parts.len() - 1] {
        let next = current
            .entry((*part).to_string())
            .or_insert_with(|| Value::Object(serde_json::Map::new()));
        if !next.is_object() {
            *next = Value::Object(serde_json::Map::new());
        }
        current = next.as_object_mut().expect("just ensured");
    }
    current.insert(parts[parts.len() - 1].to_string(), value);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typed_coerces_scalars() {
        match parse_typed("a=1").unwrap() {
            Field::Typed { value, .. } => assert_eq!(value, Value::from(1)),
            _ => panic!(),
        }
        match parse_typed("a=true").unwrap() {
            Field::Typed { value, .. } => assert_eq!(value, Value::Bool(true)),
            _ => panic!(),
        }
        match parse_typed("a=null").unwrap() {
            Field::Typed { value, .. } => assert_eq!(value, Value::Null),
            _ => panic!(),
        }
        match parse_typed("a=foo").unwrap() {
            Field::Typed { value, .. } => assert_eq!(value, Value::String("foo".into())),
            _ => panic!(),
        }
    }

    #[test]
    fn raw_keeps_string_form() {
        match parse_raw("a=1").unwrap() {
            Field::Raw { value, .. } => assert_eq!(value, "1"),
            _ => panic!(),
        }
    }

    #[test]
    fn dotted_keys_build_nested_objects() {
        let mut m = serde_json::Map::new();
        assign_dotted(&mut m, "source.branch.name", Value::String("x".into()));
        let json = Value::Object(m);
        assert_eq!(
            json,
            serde_json::json!({"source": {"branch": {"name": "x"}}})
        );
    }

    #[test]
    fn rejects_missing_equals() {
        assert!(parse_typed("nokv").is_err());
        assert!(parse_typed("=novalue").is_err());
    }
}
