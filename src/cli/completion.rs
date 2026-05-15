//! `bb completion <shell>` — emit shell completion scripts via `clap_complete`.

use clap::{Args, CommandFactory, ValueEnum};
use clap_complete::{generate, Shell};

use crate::context::Context;
use crate::error::CliError;

#[derive(Args, Debug)]
pub struct CompletionArgs {
    /// Target shell.
    #[arg(value_enum)]
    pub shell: ShellKind,
}

#[derive(Copy, Clone, Debug, ValueEnum)]
pub enum ShellKind {
    Bash,
    Zsh,
    Fish,
    Powershell,
    Elvish,
}

impl From<ShellKind> for Shell {
    fn from(s: ShellKind) -> Self {
        match s {
            ShellKind::Bash => Shell::Bash,
            ShellKind::Zsh => Shell::Zsh,
            ShellKind::Fish => Shell::Fish,
            ShellKind::Powershell => Shell::PowerShell,
            ShellKind::Elvish => Shell::Elvish,
        }
    }
}

pub async fn run(args: CompletionArgs, ctx: &mut Context) -> Result<(), CliError> {
    let mut cmd = super::Cli::command();
    let shell: Shell = args.shell.into();
    let mut out: Vec<u8> = Vec::new();
    generate(shell, &mut cmd, "bb", &mut out);
    ctx.io
        .out()
        .write_all(&out)
        .map_err(|e| CliError::Other(e.into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn bash_completion_is_nonempty() {
        let (mut ctx, bufs) = Context::test();
        run(
            CompletionArgs {
                shell: ShellKind::Bash,
            },
            &mut ctx,
        )
        .await
        .unwrap();
        let s = bufs.stdout_string();
        assert!(!s.is_empty());
        assert!(s.contains("bb"));
    }
}
