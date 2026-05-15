//! `bbk auth ...` — login, logout, status, token, refresh, switch, setup-git,
//! git-credential. Spec: `docs/specs/02-authentication.md`.

use clap::{Args, Subcommand};

use crate::context::Context;
use crate::error::CliError;

pub mod git_credential;
pub mod login;
pub mod logout;
pub mod setup_git;
pub mod status;
pub mod switch;
pub mod token;

#[derive(Args, Debug)]
pub struct AuthArgs {
    #[command(subcommand)]
    command: AuthCommand,
}

#[derive(Subcommand, Debug)]
enum AuthCommand {
    /// Authenticate `bbk` with a Bitbucket host.
    Login(login::LoginArgs),
    /// Log out of a Bitbucket host.
    Logout(logout::LogoutArgs),
    /// View authentication status.
    Status(status::StatusArgs),
    /// Print the auth token `bbk` is configured to use.
    Token(token::TokenArgs),
    /// Switch the active account on a Bitbucket host.
    Switch(switch::SwitchArgs),
    /// Configure `git` to use `bbk` as a credential helper.
    #[command(name = "setup-git")]
    SetupGit(setup_git::SetupGitArgs),
    /// Git credential helper backend. Invoked by `git`.
    #[command(name = "git-credential")]
    GitCredential(git_credential::GitCredentialArgs),
}

pub async fn run(args: AuthArgs, ctx: &mut Context) -> Result<(), CliError> {
    match args.command {
        AuthCommand::Login(a) => login::run(a, ctx).await,
        AuthCommand::Logout(a) => logout::run(a, ctx).await,
        AuthCommand::Status(a) => status::run(a, ctx).await,
        AuthCommand::Token(a) => token::run(a, ctx).await,
        AuthCommand::Switch(a) => switch::run(a, ctx).await,
        AuthCommand::SetupGit(a) => setup_git::run(a, ctx).await,
        AuthCommand::GitCredential(a) => git_credential::run(a, ctx).await,
    }
}
