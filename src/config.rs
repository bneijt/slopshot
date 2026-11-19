use std::fs;
use std::path::Path;

use anyhow::{Context, Result, bail};
use serde::Deserialize;

pub const CONFIG_FILE_NAME: &str = "slopshot.toml";

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    /// Name of the git remote to publish snapshots to.
    pub remote: String,

    /// Local branch whose committed state is snapshotted.
    #[serde(default = "default_branch")]
    pub local_branch: String,

    /// Branch on the remote. Defaults to `local_branch`.
    #[serde(default)]
    pub branch: Option<String>,
}

fn default_branch() -> String {
    "main".to_owned()
}

impl Config {
    pub fn load(path: &Path) -> Result<Self> {
        let text = fs::read_to_string(path)
            .with_context(|| format!("could not read config file '{}'", path.display()))?;
        let config: Self = toml::from_str(&text)
            .with_context(|| format!("could not parse config file '{}'", path.display()))?;
        config
            .validate()
            .with_context(|| format!("invalid config file '{}'", path.display()))?;
        Ok(config)
    }

    pub fn remote_branch(&self) -> &str {
        self.branch.as_deref().unwrap_or(&self.local_branch)
    }

    fn validate(&self) -> Result<()> {
        if self.remote.is_empty() {
            bail!("`remote` must not be empty");
        }
        if self.remote.starts_with('-') {
            bail!("`remote` must not start with '-'");
        }
        if self
            .remote
            .chars()
            .any(|c| c.is_whitespace() || c.is_control())
        {
            bail!("`remote` must not contain whitespace or control characters");
        }
        if self.local_branch.is_empty() {
            bail!("`local_branch` must not be empty");
        }
        if self.remote_branch().is_empty() {
            bail!("`branch` must not be empty");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_both_branches_to_main() {
        let config: Config = toml::from_str(r#"remote = "github""#).unwrap();
        assert_eq!(config.local_branch, "main");
        assert_eq!(config.remote_branch(), "main");
    }

    #[test]
    fn remote_branch_defaults_to_local_branch() {
        let config: Config = toml::from_str(
            r#"
remote = "github"
local_branch = "trunk"
"#,
        )
        .unwrap();
        assert_eq!(config.local_branch, "trunk");
        assert_eq!(config.remote_branch(), "trunk");
    }

    #[test]
    fn explicit_remote_branch_wins() {
        let config: Config = toml::from_str(
            r#"
remote = "github"
local_branch = "work"
branch = "main"
"#,
        )
        .unwrap();
        assert_eq!(config.remote_branch(), "main");
    }

    #[test]
    fn rejects_unknown_fields() {
        let error = toml::from_str::<Config>(
            r#"
remote = "github"
remot = "typo"
"#,
        )
        .unwrap_err();
        assert!(error.to_string().contains("unknown field"));
    }

    #[test]
    fn rejects_empty_remote() {
        let config: Config = toml::from_str(r#"remote = """#).unwrap();
        let error = config.validate().unwrap_err();
        assert!(error.to_string().contains("must not be empty"));
    }

    #[test]
    fn rejects_option_like_remote() {
        let config: Config = toml::from_str(r#"remote = "-oops""#).unwrap();
        let error = config.validate().unwrap_err();
        assert!(error.to_string().contains("must not start with '-'"));
    }

    #[test]
    fn rejects_whitespace_in_remote() {
        let config: Config = toml::from_str(r#"remote = "git hub""#).unwrap();
        let error = config.validate().unwrap_err();
        assert!(error.to_string().contains("whitespace"));
    }
}
