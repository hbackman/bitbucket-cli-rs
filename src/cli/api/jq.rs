//! `--jq EXPR` filter for `bb api`. Uses `jaq`, the pure-Rust jq.
//!
//! Each match emitted by the filter is printed on its own line.

use anyhow::{anyhow, bail, Context, Result};
use jaq_interpret::{Ctx, FilterT, ParseCtx, RcIter, Val};
use serde_json::Value;

/// Apply `expr` to `input` and return the matching outputs (in order).
pub fn run(expr: &str, input: Value) -> Result<Vec<Value>> {
    let mut defs = ParseCtx::new(Vec::new());
    defs.insert_natives(jaq_core::core());
    defs.insert_defs(jaq_std::std());

    let (parsed, errs) = jaq_parse::parse(expr, jaq_parse::main());
    if !errs.is_empty() {
        let msg = errs
            .iter()
            .map(|e| e.to_string())
            .collect::<Vec<_>>()
            .join("; ");
        bail!("jq parse error: {msg}");
    }
    let filter = defs.compile(parsed.ok_or_else(|| anyhow!("empty jq expression"))?);
    if !defs.errs.is_empty() {
        let msg = defs
            .errs
            .iter()
            .map(|(e, _)| e.to_string())
            .collect::<Vec<_>>()
            .join("; ");
        bail!("jq compile error: {msg}");
    }

    let inputs = RcIter::new(core::iter::empty());
    let val = Val::from(input);
    let mut out = Vec::new();
    for r in filter.run((Ctx::new([], &inputs), val)) {
        let v = r.map_err(|e| anyhow!("jq runtime error: {e}"))?;
        out.push(Value::from(v));
    }
    Ok(out)
}

/// Render one filter output. Strings are printed raw (matching `jq -r` for the
/// common `--jq '.username'` case); everything else is pretty-printed JSON.
pub fn render(value: &Value) -> Result<String> {
    match value {
        Value::String(s) => Ok(s.clone()),
        other => serde_json::to_string_pretty(other).context("encoding jq output"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_field() {
        let input = serde_json::json!({"username": "hbackman"});
        let out = run(".username", input).unwrap();
        assert_eq!(out, vec![Value::String("hbackman".into())]);
        assert_eq!(render(&out[0]).unwrap(), "hbackman");
    }

    #[test]
    fn length_of_array() {
        let input = serde_json::json!([1, 2, 3]);
        let out = run("length", input).unwrap();
        assert_eq!(out, vec![Value::from(3)]);
    }

    #[test]
    fn rejects_bad_syntax() {
        let err = run("..invalid..", serde_json::json!({})).unwrap_err();
        assert!(err.to_string().to_lowercase().contains("jq"));
    }
}
