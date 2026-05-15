//! Interactive prompts.
//!
//! The real implementation wraps `dialoguer`. Tests substitute [`MockPrompter`]
//! to feed canned answers in. Commands reach a prompter via [`Context::prompter`].
//!
//! Prompts must always have a flag equivalent — never let a non-interactive
//! invocation reach this code. Call sites should check `io.is_stdout_tty()` and
//! return [`CliError::Flag`] if a required flag was omitted in a script.

use std::collections::VecDeque;
use std::sync::Mutex;

use crate::error::CliError;

/// Pluggable prompter. Returns [`CliError::Cancel`] when the user aborts a
/// prompt (Ctrl-C / closed-editor).
pub trait Prompter: Send + Sync {
    fn input(&self, prompt: &str, default: Option<&str>) -> Result<String, CliError>;
    fn select(
        &self,
        prompt: &str,
        options: &[String],
        default_idx: usize,
    ) -> Result<usize, CliError>;
    fn multi_select(
        &self,
        prompt: &str,
        options: &[String],
        defaults: &[usize],
    ) -> Result<Vec<usize>, CliError>;
    fn confirm(&self, prompt: &str, default: bool) -> Result<bool, CliError>;
    fn password(&self, prompt: &str) -> Result<String, CliError>;
    fn editor(&self, prompt: &str, initial: &str, ext: &str) -> Result<String, CliError>;
}

/// The production prompter — talks to a real terminal via `dialoguer`.
#[derive(Debug, Default)]
pub struct DialoguerPrompter;

impl Prompter for DialoguerPrompter {
    fn input(&self, prompt: &str, default: Option<&str>) -> Result<String, CliError> {
        let i = dialoguer::Input::<String>::new().with_prompt(prompt);
        let i = match default {
            Some(d) => i.default(d.to_string()),
            None => i,
        };
        i.interact_text().map_err(map_err)
    }

    fn select(
        &self,
        prompt: &str,
        options: &[String],
        default_idx: usize,
    ) -> Result<usize, CliError> {
        dialoguer::Select::new()
            .with_prompt(prompt)
            .items(options)
            .default(default_idx)
            .interact()
            .map_err(map_err)
    }

    fn multi_select(
        &self,
        prompt: &str,
        options: &[String],
        defaults: &[usize],
    ) -> Result<Vec<usize>, CliError> {
        let flags: Vec<bool> = (0..options.len()).map(|i| defaults.contains(&i)).collect();
        dialoguer::MultiSelect::new()
            .with_prompt(prompt)
            .items(options)
            .defaults(&flags)
            .interact()
            .map_err(map_err)
    }

    fn confirm(&self, prompt: &str, default: bool) -> Result<bool, CliError> {
        dialoguer::Confirm::new()
            .with_prompt(prompt)
            .default(default)
            .interact()
            .map_err(map_err)
    }

    fn password(&self, prompt: &str) -> Result<String, CliError> {
        dialoguer::Password::new()
            .with_prompt(prompt)
            .interact()
            .map_err(map_err)
    }

    fn editor(&self, prompt: &str, initial: &str, ext: &str) -> Result<String, CliError> {
        let _ = prompt; // dialoguer's editor doesn't render a prompt; flag retained for parity.
        let mut e = dialoguer::Editor::new();
        if !ext.is_empty() {
            e.extension(ext);
        }
        match e.edit(initial).map_err(map_err)? {
            Some(s) => Ok(s),
            None => Err(CliError::Cancel),
        }
    }
}

fn map_err(e: dialoguer::Error) -> CliError {
    match e {
        dialoguer::Error::IO(io_err) => map_io_err(io_err),
    }
}

fn map_io_err(e: std::io::Error) -> CliError {
    if e.kind() == std::io::ErrorKind::Interrupted {
        CliError::Cancel
    } else {
        CliError::Other(anyhow::Error::from(e))
    }
}

/// Test-only prompter that returns canned answers in FIFO order.
///
/// Each method pops from its dedicated queue. If a queue is exhausted, the
/// method returns [`CliError::Cancel`] (so test failures surface as missing
/// answers rather than panics).
#[derive(Debug, Default)]
pub struct MockPrompter {
    pub inputs: Mutex<VecDeque<String>>,
    pub selects: Mutex<VecDeque<usize>>,
    pub multi_selects: Mutex<VecDeque<Vec<usize>>>,
    pub confirms: Mutex<VecDeque<bool>>,
    pub passwords: Mutex<VecDeque<String>>,
    pub editors: Mutex<VecDeque<String>>,
}

impl MockPrompter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push_input(&self, s: impl Into<String>) {
        self.inputs.lock().unwrap().push_back(s.into());
    }
    pub fn push_select(&self, idx: usize) {
        self.selects.lock().unwrap().push_back(idx);
    }
    pub fn push_multi_select(&self, indices: Vec<usize>) {
        self.multi_selects.lock().unwrap().push_back(indices);
    }
    pub fn push_confirm(&self, yes: bool) {
        self.confirms.lock().unwrap().push_back(yes);
    }
    pub fn push_password(&self, s: impl Into<String>) {
        self.passwords.lock().unwrap().push_back(s.into());
    }
    pub fn push_editor(&self, s: impl Into<String>) {
        self.editors.lock().unwrap().push_back(s.into());
    }
}

impl Prompter for MockPrompter {
    fn input(&self, _prompt: &str, default: Option<&str>) -> Result<String, CliError> {
        match self.inputs.lock().unwrap().pop_front() {
            Some(s) => Ok(s),
            None => default.map(str::to_string).ok_or(CliError::Cancel),
        }
    }

    fn select(
        &self,
        _prompt: &str,
        _options: &[String],
        default_idx: usize,
    ) -> Result<usize, CliError> {
        Ok(self
            .selects
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or(default_idx))
    }

    fn multi_select(
        &self,
        _prompt: &str,
        _options: &[String],
        defaults: &[usize],
    ) -> Result<Vec<usize>, CliError> {
        Ok(self
            .multi_selects
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or_else(|| defaults.to_vec()))
    }

    fn confirm(&self, _prompt: &str, default: bool) -> Result<bool, CliError> {
        Ok(self.confirms.lock().unwrap().pop_front().unwrap_or(default))
    }

    fn password(&self, _prompt: &str) -> Result<String, CliError> {
        self.passwords
            .lock()
            .unwrap()
            .pop_front()
            .ok_or(CliError::Cancel)
    }

    fn editor(&self, _prompt: &str, initial: &str, _ext: &str) -> Result<String, CliError> {
        Ok(self
            .editors
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or_else(|| initial.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mock_returns_pushed_values_in_order() {
        let p = MockPrompter::new();
        p.push_input("hbackman");
        p.push_confirm(true);
        p.push_select(2);

        assert_eq!(p.input("user?", None).unwrap(), "hbackman");
        assert!(p.confirm("ok?", false).unwrap());
        assert_eq!(
            p.select("pick", &["a".into(), "b".into(), "c".into()], 0)
                .unwrap(),
            2
        );
    }

    #[test]
    fn mock_input_falls_back_to_default() {
        let p = MockPrompter::new();
        assert_eq!(p.input("?", Some("dflt")).unwrap(), "dflt");
    }

    #[test]
    fn mock_input_cancels_without_default_or_value() {
        let p = MockPrompter::new();
        assert!(matches!(p.input("?", None), Err(CliError::Cancel)));
    }
}
