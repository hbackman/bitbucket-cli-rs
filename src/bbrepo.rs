use anyhow::{anyhow, bail, Context as _, Result};
use url::Url;

pub const DEFAULT_HOST: &str = "bitbucket.org";

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct BbRepo {
    pub workspace: String,
    pub slug: String,
    pub host: String,
}

impl BbRepo {
    pub fn new(workspace: impl Into<String>, slug: impl Into<String>) -> Self {
        Self::with_host(workspace, slug, DEFAULT_HOST)
    }

    pub fn with_host(
        workspace: impl Into<String>,
        slug: impl Into<String>,
        host: impl Into<String>,
    ) -> Self {
        Self {
            workspace: workspace.into(),
            slug: slug.into(),
            host: host.into(),
        }
    }

    /// Parse a `workspace/slug` short form.
    pub fn from_full_name(s: &str) -> Result<Self> {
        let s = s.trim();
        let (ws, slug) = s
            .split_once('/')
            .ok_or_else(|| anyhow!("expected WORKSPACE/REPO, got {s:?}"))?;
        if ws.is_empty() || slug.is_empty() || slug.contains('/') {
            bail!("expected WORKSPACE/REPO, got {s:?}");
        }
        Ok(Self::new(ws, strip_dot_git(slug)))
    }

    /// Parse from a parsed `https://` URL.
    pub fn from_url(u: &Url) -> Result<Self> {
        let host = u
            .host_str()
            .ok_or_else(|| anyhow!("URL has no host: {u}"))?
            .to_string();
        let segments: Vec<&str> = u
            .path_segments()
            .ok_or_else(|| anyhow!("URL has no path: {u}"))?
            .filter(|s| !s.is_empty())
            .collect();
        if segments.len() < 2 {
            bail!("URL {u} does not look like a Bitbucket repo path");
        }
        let workspace = segments[0];
        let slug = strip_dot_git(segments[1]);
        Ok(Self::with_host(workspace, slug, host))
    }

    /// Parse any remote string (`https://`, `git@host:ws/repo.git`, `ssh://`).
    pub fn parse_remote(remote: &str) -> Result<Self> {
        let remote = remote.trim();

        // scp-like form: `git@host:workspace/repo.git`
        if let Some(rest) = remote.strip_prefix("git@") {
            if let Some((host, path)) = rest.split_once(':') {
                return parse_path_with_host(host, path);
            }
        }

        let url = Url::parse(remote).with_context(|| format!("parsing remote URL {remote:?}"))?;
        Self::from_url(&url)
    }
}

impl std::fmt::Display for BbRepo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}/{}", self.workspace, self.slug)
    }
}

fn parse_path_with_host(host: &str, path: &str) -> Result<BbRepo> {
    let path = path.trim_start_matches('/');
    let (ws, slug) = path
        .split_once('/')
        .ok_or_else(|| anyhow!("expected host:WORKSPACE/REPO, got {host}:{path}"))?;
    if ws.is_empty() || slug.is_empty() {
        bail!("expected host:WORKSPACE/REPO, got {host}:{path}");
    }
    Ok(BbRepo::with_host(ws, strip_dot_git(slug), host))
}

fn strip_dot_git(s: &str) -> &str {
    s.strip_suffix(".git").unwrap_or(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_full_name_ok() {
        let r = BbRepo::from_full_name("acme/widgets").unwrap();
        assert_eq!(r.workspace, "acme");
        assert_eq!(r.slug, "widgets");
        assert_eq!(r.host, DEFAULT_HOST);
        assert_eq!(r.to_string(), "acme/widgets");
    }

    #[test]
    fn from_full_name_strips_dot_git() {
        let r = BbRepo::from_full_name("acme/widgets.git").unwrap();
        assert_eq!(r.slug, "widgets");
    }

    #[test]
    fn from_full_name_rejects_garbage() {
        assert!(BbRepo::from_full_name("nope").is_err());
        assert!(BbRepo::from_full_name("/nope").is_err());
        assert!(BbRepo::from_full_name("nope/").is_err());
        assert!(BbRepo::from_full_name("a/b/c").is_err());
    }

    #[test]
    fn parse_https_remote() {
        let r = BbRepo::parse_remote("https://bitbucket.org/acme/widgets.git").unwrap();
        assert_eq!(r.workspace, "acme");
        assert_eq!(r.slug, "widgets");
        assert_eq!(r.host, "bitbucket.org");
    }

    #[test]
    fn parse_https_remote_without_git_suffix() {
        let r = BbRepo::parse_remote("https://bitbucket.org/acme/widgets").unwrap();
        assert_eq!(r.slug, "widgets");
    }

    #[test]
    fn parse_scp_like_ssh_remote() {
        let r = BbRepo::parse_remote("git@bitbucket.org:acme/widgets.git").unwrap();
        assert_eq!(r.workspace, "acme");
        assert_eq!(r.slug, "widgets");
        assert_eq!(r.host, "bitbucket.org");
    }

    #[test]
    fn parse_ssh_url_remote() {
        let r = BbRepo::parse_remote("ssh://git@bitbucket.org/acme/widgets.git").unwrap();
        assert_eq!(r.workspace, "acme");
        assert_eq!(r.slug, "widgets");
        assert_eq!(r.host, "bitbucket.org");
    }

    #[test]
    fn parse_remote_rejects_non_repo_url() {
        assert!(BbRepo::parse_remote("https://bitbucket.org/acme").is_err());
        assert!(BbRepo::parse_remote("https://bitbucket.org/").is_err());
    }
}
