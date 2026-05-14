use clap::Args;

use crate::config::{self, Config};
use crate::context::Context;
use crate::error::CliError;

#[derive(Args, Debug)]
pub struct GetArgs {
    /// Bitbucket host to scope this read to (reads from `hosts.yml`).
    #[arg(long, value_name = "HOST")]
    host: Option<String>,

    /// The config key to read.
    key: String,
}

pub async fn run(args: GetArgs, ctx: &mut Context) -> Result<(), CliError> {
    let cfg = Config::load().await.map_err(CliError::Other)?;

    let value = if let Some(host) = args.host.as_deref() {
        // hosts.yml is owned by the auth slice; we accept any key here.
        let hosts = cfg.hosts();
        let hosts = hosts.read().await;
        hosts.get(host, &args.key).unwrap_or_default()
    } else {
        if !config::is_known_key(&args.key) && cfg.get(&args.key).is_none() {
            writeln!(ctx.io.err(), "unknown key '{}'", args.key)
                .map_err(|e| CliError::Other(e.into()))?;
            return Err(CliError::Silent);
        }
        cfg.get_or_default(&args.key)
    };

    writeln!(ctx.io.out(), "{value}").map_err(|e| CliError::Other(e.into()))
}
