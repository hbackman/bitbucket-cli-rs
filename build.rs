use std::process::Command;

fn main() {
    let commit = Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .and_then(|o| {
            if o.status.success() {
                String::from_utf8(o.stdout).ok()
            } else {
                None
            }
        })
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "unknown".into());

    let date = Command::new("date")
        .args(["-u", "+%Y-%m-%d"])
        .output()
        .ok()
        .and_then(|o| {
            if o.status.success() {
                String::from_utf8(o.stdout).ok()
            } else {
                None
            }
        })
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "unknown".into());

    println!("cargo:rustc-env=BB_BUILD_COMMIT={commit}");
    println!("cargo:rustc-env=BB_BUILD_DATE={date}");
    println!("cargo:rerun-if-changed=.git/HEAD");
    println!("cargo:rerun-if-changed=.git/refs");

    // Embed the OAuth client_id / client_secret so release binaries can run
    // `bb auth login` without explicit flags. Release builds set these in CI;
    // local dev builds leave them empty and require --client-id / --client-secret
    // (or BB_OAUTH_CLIENT_ID / BB_OAUTH_CLIENT_SECRET).
    let client_id = std::env::var("BB_OAUTH_CLIENT_ID").unwrap_or_default();
    let client_secret = std::env::var("BB_OAUTH_CLIENT_SECRET").unwrap_or_default();
    println!("cargo:rustc-env=BB_EMBEDDED_OAUTH_CLIENT_ID={client_id}");
    println!("cargo:rustc-env=BB_EMBEDDED_OAUTH_CLIENT_SECRET={client_secret}");
    println!("cargo:rerun-if-env-changed=BB_OAUTH_CLIENT_ID");
    println!("cargo:rerun-if-env-changed=BB_OAUTH_CLIENT_SECRET");
}
