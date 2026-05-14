//! Browser launcher abstraction. Production code calls the `webbrowser` crate;
//! tests inject `RecordingBrowser` to capture the URL that would have been opened.

use std::sync::{Arc, Mutex};

use anyhow::{anyhow, Result};

/// Open a URL in the user's default browser.
pub trait Browser: Send + Sync {
    fn open(&self, url: &str) -> Result<()>;
}

/// Default implementation backed by the `webbrowser` crate.
#[derive(Debug, Default)]
pub struct DefaultBrowser;

impl Browser for DefaultBrowser {
    fn open(&self, url: &str) -> Result<()> {
        webbrowser::open(url).map_err(|e| anyhow!("opening browser: {e}"))?;
        Ok(())
    }
}

/// Test fake that records every URL passed to `open` and reports success.
#[derive(Debug, Default, Clone)]
pub struct RecordingBrowser {
    urls: Arc<Mutex<Vec<String>>>,
}

impl RecordingBrowser {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn urls(&self) -> Vec<String> {
        self.urls.lock().unwrap().clone()
    }
}

impl Browser for RecordingBrowser {
    fn open(&self, url: &str) -> Result<()> {
        self.urls.lock().unwrap().push(url.to_string());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recording_browser_captures_urls() {
        let b = RecordingBrowser::new();
        b.open("https://example.com/a").unwrap();
        b.open("https://example.com/b").unwrap();
        assert_eq!(
            b.urls(),
            vec![
                "https://example.com/a".to_string(),
                "https://example.com/b".to_string(),
            ]
        );
    }
}
