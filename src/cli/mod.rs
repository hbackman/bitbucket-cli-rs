use clap::{Parser, Subcommand};

use crate::context::Context;
use crate::error::CliError;

pub mod api;
pub mod auth;
pub mod completion;
pub mod config;
pub mod jq;
pub mod json_flags;
pub mod messages;
pub mod pr;
pub mod prompter;
pub mod repo;
pub mod version;

#[derive(Parser, Debug)]
#[command(
    name = "bbk",
    bin_name = "bbk",
    version = concat!(
        env!("CARGO_PKG_VERSION"),
        " (commit ", env!("BB_BUILD_COMMIT"),
        ", built ",  env!("BB_BUILD_DATE"), ")"
    ),
    about = "Bitbucket Cloud command-line tool",
    long_about = "bbk is a Bitbucket Cloud CLI modeled on GitHub's gh.",
    propagate_version = false,
    disable_help_subcommand = true,
)]
pub struct Cli {
    /// Select a repository using the WORKSPACE/REPO format.
    #[arg(
        short = 'R',
        long = "repo",
        global = true,
        env = "BB_REPO",
        value_name = "WORKSPACE/REPO"
    )]
    pub repo: Option<String>,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Authenticate with Bitbucket.
    Auth(auth::AuthArgs),
    /// Manage repositories.
    Repo(repo::RepoArgs),
    /// Manage pull requests.
    Pr(pr::PrArgs),
    /// Make an authenticated request to the Bitbucket REST API.
    Api(api::ApiArgs),
    /// Manage configuration.
    Config(config::ConfigArgs),
    /// Print version information.
    Version(version::VersionArgs),
    /// Generate shell completion scripts.
    Completion(completion::CompletionArgs),
}

pub async fn run(mut ctx: Context) -> Result<(), CliError> {
    let cli = Cli::parse();
    let _ = ctx.repo_override.set(cli.repo);
    match cli.command {
        Command::Auth(a) => auth::run(a, &mut ctx).await,
        Command::Repo(a) => repo::run(a, &mut ctx).await,
        Command::Pr(a) => pr::run(a, &mut ctx).await,
        Command::Api(a) => api::run(a, &mut ctx).await,
        Command::Config(a) => config::run(a, &mut ctx).await,
        Command::Version(a) => version::run_with(a, &mut ctx).await,
        Command::Completion(a) => completion::run(a, &mut ctx).await,
    }
}
