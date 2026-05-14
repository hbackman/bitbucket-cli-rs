use clap::{Parser, Subcommand};

use crate::context::Context;
use crate::error::CliError;

pub mod api;
pub mod auth;
pub mod browse;
pub mod config;
pub mod pr;
pub mod repo;
pub mod version;

#[derive(Parser, Debug)]
#[command(
    name = "bb",
    bin_name = "bb",
    version = concat!(
        env!("CARGO_PKG_VERSION"),
        " (commit ", env!("BB_BUILD_COMMIT"),
        ", built ",  env!("BB_BUILD_DATE"), ")"
    ),
    about = "Bitbucket Cloud command-line tool",
    long_about = "bb is a Bitbucket Cloud CLI modeled on GitHub's gh.",
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
    /// Open a Bitbucket page in the browser.
    Browse(browse::BrowseArgs),
    /// Manage configuration.
    Config(config::ConfigArgs),
    /// Print version information.
    Version,
}

pub async fn run(mut ctx: Context) -> Result<(), CliError> {
    let cli = Cli::parse();
    let _ = ctx.repo_override.set(cli.repo);
    match cli.command {
        Command::Auth(a) => auth::run(a, &mut ctx).await,
        Command::Repo(a) => repo::run(a, &mut ctx).await,
        Command::Pr(a) => pr::run(a, &mut ctx).await,
        Command::Api(a) => api::run(a, &mut ctx).await,
        Command::Browse(a) => browse::run(a, &mut ctx).await,
        Command::Config(a) => config::run(a, &mut ctx).await,
        Command::Version => version::run(&mut ctx).await,
    }
}
