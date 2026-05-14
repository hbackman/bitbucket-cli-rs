use clap::Args;

use crate::config::Config;
use crate::context::Context;
use crate::error::CliError;

#[derive(Args, Debug)]
pub struct ListArgs {
    /// Bitbucket host whose `hosts.yml` block should be listed instead of `config.yml`.
    #[arg(long, value_name = "HOST")]
    host: Option<String>,
}

pub async fn run(args: ListArgs, ctx: &mut Context) -> Result<(), CliError> {
    let cfg = Config::load().await.map_err(CliError::Other)?;
    let yaml = if let Some(host) = args.host.as_deref() {
        let hosts = cfg.hosts();
        let hosts = hosts.read().await;
        hosts.host_yaml(host).map_err(CliError::Other)?
    } else {
        cfg.effective_yaml().map_err(CliError::Other)?
    };
    write!(ctx.io.out(), "{yaml}").map_err(|e| CliError::Other(e.into()))
}
