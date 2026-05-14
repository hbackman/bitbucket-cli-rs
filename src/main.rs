use std::process::ExitCode;

use bb::{cli, context::Context, error};

#[tokio::main]
async fn main() -> ExitCode {
    let ctx = Context::from_env();
    match cli::run(ctx).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => error::report(err),
    }
}
