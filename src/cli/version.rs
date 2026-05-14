use crate::context::Context;
use crate::error::CliError;

pub async fn run(ctx: &mut Context) -> Result<(), CliError> {
    let line = format!(
        "bb {} (commit {}, built {})",
        ctx.build.version, ctx.build.commit, ctx.build.date,
    );
    writeln!(ctx.io.out(), "{line}").map_err(|e| CliError::Other(e.into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn prints_version_line() {
        let (mut ctx, bufs) = Context::test();
        run(&mut ctx).await.unwrap();
        let out = bufs.stdout_string();
        assert!(out.starts_with("bb "));
        assert!(out.contains("commit"));
        assert!(out.contains("built"));
    }
}
