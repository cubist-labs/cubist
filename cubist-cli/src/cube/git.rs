//! Support for creating new projects from git-repo templates.
use cubist_config::Config;
use eyre::{bail, Result, WrapErr};
use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::{Command, Stdio};

/// ULRs for git repos
pub type GitUrl = git_url::GitUrl;

/// Cube for git repositories
#[derive(Debug, Clone)]
pub struct Git {
    dir: PathBuf,
    url: GitUrl,
}

impl Git {
    /// Create new cube which clones the project at the `url`
    pub fn new(dir: PathBuf, url: &GitUrl) -> Self {
        Git {
            dir,
            url: url.clone(),
        }
    }
}

impl Git {
    pub fn new_project(&self, force: bool) -> Result<()> {
        if self.dir.exists() {
            bail!("Will not clone repo. {} exists", self.dir.display())
        }
        // clone repo
        let status = Command::new("git")
            .args([
                "clone",
                &self.url.to_string(),
                self.dir.to_str().unwrap(),
                "--depth",
                "1",
            ])
            // skip hooks unless explicitly set
            .env(
                "SKIP_POST_CHECKOUT",
                env::var("SKIP_POST_CHECKOUT").unwrap_or_else(|_| "1".to_string()),
            )
            .stderr(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stdin(Stdio::inherit())
            .status()
            .wrap_err("Failed to execute 'git clone'")?;
        if !status.success() {
            bail!(
                "git clone {} {} exited with {}",
                &self.url.to_string(),
                self.dir.to_str().unwrap(),
                status
            );
        }
        // remove the .git directory (ignore if this fails)
        let _ = fs::remove_dir_all(self.dir.join(".git"));

        // sanity check the cloned thing unless `force` is true
        if !force {
            Config::from_dir(&self.dir)?;
        }

        Ok(())
    }
}

/// This module exports a GitUrl type. It's similar to the git_url crate, but simpler for our use
/// case: we just care about the URL being a valid git URL (to avoid calling `git clone` with a bad
/// URL), not the different URL components.
pub mod git_url {
    use std::fmt;
    use std::str::FromStr;

    use thiserror::Error;
    use url::Url;

    /// Git URLs implemented as a light wrapper around strings (to ensure the string is a valid URL).
    /// We follow the syntax from <https://git-scm.com/docs/git-clone#URLS>
    #[derive(Debug, Eq, PartialEq, Clone)]
    pub struct GitUrl {
        url: String,
    }

    /// GitUrl parse errors
    #[derive(Debug, Error)]
    pub enum ParseError {
        /// Invalid git URL
        #[error("Invalid git URL {url:?}")]
        InvalidGitUrl { url: String },
        /// Unexpected git URL scheme
        #[error("Unexpected git URL scheme {scheme:?}")]
        UnexpectedGitUrlScheme { scheme: String },
    }

    impl fmt::Display for GitUrl {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "{}", self.url)
        }
    }

    impl FromStr for GitUrl {
        type Err = ParseError;

        fn from_str(s: &str) -> Result<Self, Self::Err> {
            let ok_res = Ok(GitUrl { url: s.to_string() });
            let err_res = Err(ParseError::InvalidGitUrl { url: s.to_string() });
            // Try parsing as URL
            if s.contains("://") {
                if let Ok(url) = Url::parse(s) {
                    let schemes = vec![
                        "git", "ssh", "git+ssh", "http", "https", "ftp", "ftps", "file",
                    ];
                    if schemes.contains(&url.scheme()) {
                        return ok_res;
                    } else {
                        return Err(ParseError::UnexpectedGitUrlScheme {
                            scheme: url.scheme().to_string(),
                        });
                    }
                }
            }
            if s.find(':') < s.find('/') {
                // Try SCP style: [user@]host.xz:path/to/repo.git/
                if Url::parse(&format!("ssh://{}", s.replacen(':', "/", 1))).is_ok() {
                    return ok_res;
                }
            }
            // Try file
            if Url::parse(&format!("file://{}", s)).is_ok() {
                return ok_res;
            }
            err_res
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use std::str::FromStr;

        #[test]
        fn parse_git_url() {
            // From <https://git-scm.com/docs/git-clone#URLS>:
            // ssh://[user@]host.xz[:port]/path/to/repo.git/
            assert!(GitUrl::from_str("ssh://user@host.xz:1337/path/to/repo.git/").is_ok());
            assert!(GitUrl::from_str("ssh://host.xz:1337/path/to/repo.git/").is_ok());
            assert!(GitUrl::from_str("ssh://host.xz/path/to/repo.git/").is_ok());
            assert!(GitUrl::from_str("ssh://user@host.xz/path/to/repo.git/").is_ok());
            assert!(GitUrl::from_str("ssh://user@host.xz:1337/path/to/repo.git/").is_ok());
            // git://host.xz[:port]/path/to/repo.git/
            assert!(GitUrl::from_str("git://host.xz:2022/path/to/repo.git/").is_ok());
            assert!(GitUrl::from_str("git://host.xz/path/to/repo.git/").is_ok());
            // http[s]://host.xz[:port]/path/to/repo.git/
            assert!(GitUrl::from_str("https://host.xz:1337/path/to/repo.git/").is_ok());
            assert!(GitUrl::from_str("https://host.xz/path/to/repo.git/").is_ok());
            assert!(GitUrl::from_str("http://host.xz:1337/path/to/repo.git/").is_ok());
            assert!(GitUrl::from_str("http://host.xz/path/to/repo.git/").is_ok());
            // ftp[s]://host.xz[:port]/path/to/repo.git/
            assert!(GitUrl::from_str("ftps://host.xz:1337/path/to/repo.git/").is_ok());
            assert!(GitUrl::from_str("ftps://host.xz/path/to/repo.git/").is_ok());
            assert!(GitUrl::from_str("ftp://host.xz:1337/path/to/repo.git/").is_ok());
            assert!(GitUrl::from_str("ftp://host.xz/path/to/repo.git/").is_ok());
            // [user@]host.xz:path/to/repo.git/
            assert!(GitUrl::from_str("user@host.xz:path/to/repo.git/").is_ok());
            assert!(GitUrl::from_str("host.xz:path/to/repo.git/").is_ok());
            assert!(GitUrl::from_str("git@github.com:user/repo.git").is_ok());
            // ssh://[user@]host.xz[:port]/~[user]/path/to/repo.git/
            assert!(GitUrl::from_str("ssh://user@host.xz:1337/~user/path/to/repo.git/").is_ok());
            assert!(GitUrl::from_str("ssh://host.xz:1337/~user/path/to/repo.git/").is_ok());
            // git://host.xz[:port]/~[user]/path/to/repo.git/
            assert!(GitUrl::from_str("git://host.xz:1337/~user/path/to/repo.git/").is_ok());
            assert!(GitUrl::from_str("git://host.xz/~user/path/to/repo.git/").is_ok());
            // [user@]host.xz:/~[user]/path/to/repo.git/
            assert!(GitUrl::from_str("user@host.xz:/~user/path/to/repo.git/").is_ok());
            assert!(GitUrl::from_str("host.xz:/~user/path/to/repo.git/").is_ok());
            // /path/to/repo.git/
            assert!(GitUrl::from_str("/path/to/repo.git/").is_ok());
            // file:///path/to/repo.git/
            assert!(GitUrl::from_str("file:///path/to/repo.git/").is_ok());
            // Additional:
            assert!(GitUrl::from_str("git+ssh://user@host.xz/~/repo.git").is_ok());
            assert!(GitUrl::from_str("git+ssh://host.xz/~user/repo.git").is_ok());
        }
    }
}
