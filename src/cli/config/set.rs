use clap::Args;

use crate::config::Config;
use crate::context::Context;
use crate::error::CliError;

#[derive(Args, Debug)]
pub struct SetArgs {
    /// Bitbucket host to scope this write to (writes to `hosts.yml`).
    #[arg(long, value_name = "HOST")]
    host: Option<String>,

    /// The config key to write.
    key: String,
    /// The value to assign.
    value: String,
}

pub async fn run(args: SetArgs, _ctx: &mut Context) -> Result<(), CliError> {
    let mut cfg = Config::load().await.map_err(CliError::Other)?;
    if let Some(host) = args.host.as_deref() {
        let hosts = cfg.hosts();
        let mut hosts = hosts.write().await;
        hosts
            .set(host, &args.key, &args.value)
            .await
            .map_err(CliError::Other)?;
    } else {
        cfg.set(&args.key, &args.value)
            .await
            .map_err(CliError::Other)?;
    }
    Ok(())
}
