//! `--paginate` output handling for `bb api`.
//!
//! Without `--slurp`: print each page's JSON document, one per line. With
//! `--slurp`: concatenate the `values` arrays from every page into a single
//! JSON array — handy for piping to `jq '. | length'`.

use serde_json::Value;

use crate::api::errors::ResponseError;
use crate::api::{ApiError, Transport};

/// Collect `values` from every page reachable via `next`. The starting URL's
/// response is fetched first; if it has a `next`, that URL is followed; repeat.
pub async fn paginate_slurp(transport: &Transport, first_url: &str) -> Result<Value, ApiError> {
    let mut all = Vec::<Value>::new();
    let mut url: Option<String> = Some(first_url.to_string());
    while let Some(next) = url.take() {
        let req = transport
            .http
            .get(&next)
            .build()
            .map_err(ApiError::Network)?;
        let resp = transport.send(req).await?;
        let mut doc: Value = serde_json::from_slice(&resp.body).map_err(|e| {
            ApiError::Response(Box::new(ResponseError {
                status: resp.status,
                method: reqwest::Method::GET,
                url: resp.url.to_string(),
                message: format!("decoding page: {e}"),
                errors: vec![],
                raw: resp.body.clone(),
            }))
        })?;
        if let Some(values) = doc.get_mut("values").and_then(|v| v.as_array_mut()) {
            all.append(values);
        }
        url = doc
            .get("next")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .filter(|s| !s.is_empty());
    }
    Ok(Value::Array(all))
}

/// Stream pages one by one. Returns each page document so callers can print
/// them as they arrive.
pub async fn paginate_each<F>(
    transport: &Transport,
    first_url: &str,
    mut on_page: F,
) -> Result<(), ApiError>
where
    F: FnMut(Value) -> Result<(), ApiError>,
{
    let mut url: Option<String> = Some(first_url.to_string());
    while let Some(next) = url.take() {
        let req = transport
            .http
            .get(&next)
            .build()
            .map_err(ApiError::Network)?;
        let resp = transport.send(req).await?;
        let doc: Value = serde_json::from_slice(&resp.body).map_err(|e| {
            ApiError::Response(Box::new(ResponseError {
                status: resp.status,
                method: reqwest::Method::GET,
                url: resp.url.to_string(),
                message: format!("decoding page: {e}"),
                errors: vec![],
                raw: resp.body.clone(),
            }))
        })?;
        url = doc
            .get("next")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .filter(|s| !s.is_empty());
        on_page(doc)?;
    }
    Ok(())
}
