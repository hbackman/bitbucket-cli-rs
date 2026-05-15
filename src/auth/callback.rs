//! Single-shot loopback HTTP server that catches the OAuth redirect from Bitbucket.
//!
//! Bitbucket lets you register `http://localhost` (no port) as the redirect URI and
//! ignores the port on the actual request, so we bind a random port and let it know
//! what we're using at authorize time.

use std::collections::HashMap;
use std::time::Duration;

use anyhow::{anyhow, bail, Result};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

pub const SUCCESS_HTML: &str = "<!doctype html><html><head><title>bbk — authenticated</title>\
    <style>body{font-family:system-ui,sans-serif;margin:4rem auto;max-width:32rem;text-align:center}</style>\
    </head><body><h1>You're all set.</h1>\
    <p>Authentication complete — you can close this window and return to your terminal.</p></body></html>";

/// Bind a random local port and return the listener + chosen port.
pub async fn bind_loopback() -> Result<(TcpListener, u16)> {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .map_err(|e| anyhow!("binding loopback for OAuth callback: {e}"))?;
    let port = listener.local_addr()?.port();
    Ok((listener, port))
}

/// Wait for a single GET to `/` carrying `code` and `state` query params. Returns
/// the code on success; surfaces `error=...` from the provider; times out after
/// `timeout`.
pub async fn await_callback(
    listener: TcpListener,
    expected_state: &str,
    timeout: Duration,
) -> Result<String> {
    let fut = async move {
        loop {
            let (mut socket, _peer) = listener
                .accept()
                .await
                .map_err(|e| anyhow!("accepting OAuth callback: {e}"))?;
            let req = read_request(&mut socket).await?;
            let (path, query) = parse_request_line(&req)?;
            match dispatch(&path, &query, expected_state) {
                Outcome::Success(code) => {
                    write_html(&mut socket, 200, SUCCESS_HTML).await?;
                    return Ok::<String, anyhow::Error>(code);
                }
                Outcome::ProviderError(err, desc) => {
                    let body = error_page(&err, desc.as_deref());
                    write_html(&mut socket, 400, &body).await?;
                    bail!("oauth callback error: {err}");
                }
                Outcome::BadState => {
                    write_html(&mut socket, 400, "Authentication failed: state mismatch.").await?;
                    bail!("oauth callback returned mismatched state token");
                }
                Outcome::NotFound => {
                    write_html(&mut socket, 404, "Not found.").await?;
                    // Keep listening — could be a stray request from a browser preconnect.
                    continue;
                }
            }
        }
    };
    tokio::time::timeout(timeout, fut)
        .await
        .map_err(|_| anyhow!("authentication timed out. Re-run `bbk auth login`."))?
}

enum Outcome {
    Success(String),
    ProviderError(String, Option<String>),
    BadState,
    NotFound,
}

fn dispatch(path: &str, query: &HashMap<String, String>, expected_state: &str) -> Outcome {
    if path != "/" {
        return Outcome::NotFound;
    }
    if let Some(err) = query.get("error") {
        return Outcome::ProviderError(err.clone(), query.get("error_description").cloned());
    }
    let code = match query.get("code") {
        Some(c) => c.clone(),
        None => return Outcome::NotFound,
    };
    match query.get("state") {
        Some(s) if s == expected_state => Outcome::Success(code),
        _ => Outcome::BadState,
    }
}

/// Read the full HTTP request headers (up to \r\n\r\n). Body is ignored — OAuth
/// callbacks are GETs with everything in the query string.
async fn read_request(socket: &mut tokio::net::TcpStream) -> Result<String> {
    let mut buf = Vec::with_capacity(2048);
    let mut tmp = [0u8; 1024];
    loop {
        let n = socket
            .read(&mut tmp)
            .await
            .map_err(|e| anyhow!("reading OAuth callback request: {e}"))?;
        if n == 0 {
            bail!("OAuth callback closed before sending a complete request");
        }
        buf.extend_from_slice(&tmp[..n]);
        if buf.windows(4).any(|w| w == b"\r\n\r\n") || buf.len() > 64 * 1024 {
            break;
        }
    }
    String::from_utf8(buf).map_err(|e| anyhow!("OAuth callback was not utf-8: {e}"))
}

/// Returns (path, query_params).
fn parse_request_line(req: &str) -> Result<(String, HashMap<String, String>)> {
    let first = req
        .lines()
        .next()
        .ok_or_else(|| anyhow!("OAuth callback had no request line"))?;
    let mut parts = first.split_whitespace();
    let _method = parts.next();
    let target = parts
        .next()
        .ok_or_else(|| anyhow!("OAuth callback request line missing target"))?;
    let (path, query) = match target.split_once('?') {
        Some((p, q)) => (p.to_string(), q.to_string()),
        None => (target.to_string(), String::new()),
    };
    let params: HashMap<String, String> = url::form_urlencoded::parse(query.as_bytes())
        .into_owned()
        .collect();
    Ok((path, params))
}

async fn write_html(socket: &mut tokio::net::TcpStream, status: u16, body: &str) -> Result<()> {
    let phrase = match status {
        200 => "OK",
        400 => "Bad Request",
        404 => "Not Found",
        _ => "Status",
    };
    let resp = format!(
        "HTTP/1.1 {status} {phrase}\r\n\
         Content-Type: text/html; charset=utf-8\r\n\
         Content-Length: {len}\r\n\
         Connection: close\r\n\
         \r\n\
         {body}",
        len = body.len(),
    );
    socket
        .write_all(resp.as_bytes())
        .await
        .map_err(|e| anyhow!("writing OAuth callback response: {e}"))?;
    let _ = socket.shutdown().await;
    Ok(())
}

fn error_page(err: &str, desc: Option<&str>) -> String {
    let desc = desc.unwrap_or("");
    format!(
        "<!doctype html><html><body><h1>Authentication failed</h1>\
         <p><code>{err}</code></p><p>{desc}</p></body></html>"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::AsyncWriteExt;
    use tokio::net::TcpStream;

    async fn send_request(port: u16, line: &str) -> String {
        let mut s = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
        let req = format!("{line} HTTP/1.1\r\nHost: localhost\r\n\r\n");
        s.write_all(req.as_bytes()).await.unwrap();
        let mut buf = String::new();
        tokio::io::AsyncReadExt::read_to_string(&mut s, &mut buf)
            .await
            .unwrap();
        buf
    }

    #[tokio::test]
    async fn success_path_returns_code() {
        let (listener, port) = bind_loopback().await.unwrap();
        let server = tokio::spawn(await_callback(
            listener,
            "the-state",
            Duration::from_secs(5),
        ));
        let resp = send_request(port, "GET /?code=abc&state=the-state").await;
        assert!(resp.contains("200 OK"), "unexpected response: {resp}");
        let code = server.await.unwrap().unwrap();
        assert_eq!(code, "abc");
    }

    #[tokio::test]
    async fn state_mismatch_is_rejected() {
        let (listener, port) = bind_loopback().await.unwrap();
        let server = tokio::spawn(await_callback(
            listener,
            "the-state",
            Duration::from_secs(5),
        ));
        let resp = send_request(port, "GET /?code=abc&state=wrong").await;
        assert!(resp.contains("400 Bad Request"), "got: {resp}");
        let err = server.await.unwrap().unwrap_err();
        assert!(err.to_string().contains("state"));
    }

    #[tokio::test]
    async fn provider_error_is_surfaced() {
        let (listener, port) = bind_loopback().await.unwrap();
        let server = tokio::spawn(await_callback(
            listener,
            "the-state",
            Duration::from_secs(5),
        ));
        let resp = send_request(port, "GET /?error=access_denied").await;
        assert!(resp.contains("400 Bad Request"));
        let err = server.await.unwrap().unwrap_err();
        assert!(err.to_string().contains("access_denied"));
    }

    #[tokio::test]
    async fn timeout_fires_when_no_request_arrives() {
        let (listener, _port) = bind_loopback().await.unwrap();
        let res = await_callback(listener, "x", Duration::from_millis(50)).await;
        assert!(res.is_err());
        assert!(res.unwrap_err().to_string().contains("timed out"));
    }
}
