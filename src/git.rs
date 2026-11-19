use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use anyhow::{Context, Result, bail};

/// Thin wrapper around the git CLI.
///
/// Shelling out (instead of linking libgit2) keeps credential helpers, SSH
/// configuration, and git's own transport behavior available for pushes.
#[derive(Debug)]
pub struct Git {
    root: PathBuf,
    verbose: bool,
}

impl Git {
    pub fn discover(verbose: bool) -> Result<Self> {
        let output = Command::new("git")
            .args(["rev-parse", "--show-toplevel"])
            .output()
            .context("could not run `git`; is git installed and on PATH?")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("not inside a git work tree: {}", stderr.trim());
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let root = stdout.trim_end_matches(['\r', '\n']);
        if root.is_empty() {
            bail!("git did not report a work tree root");
        }
        Ok(Self {
            root: PathBuf::from(root),
            verbose,
        })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn remote_exists(&self, remote: &str) -> Result<bool> {
        let remotes = self.checked(&["remote"])?;
        Ok(remotes.lines().any(|line| line.trim() == remote))
    }

    pub fn remote_is_mirror(&self, remote: &str) -> Result<bool> {
        let key = format!("remote.{remote}.mirror");
        let args = ["config", "--bool", "--get", &key];
        let output = self.run(&args)?;
        match output.status.code() {
            Some(0) => Ok(String::from_utf8_lossy(&output.stdout).trim() == "true"),
            Some(1) => Ok(false),
            _ => Err(git_error(&args, &output)),
        }
    }

    pub fn validate_branch(&self, branch: &str) -> Result<()> {
        let refname = format!("refs/heads/{branch}");
        let output = self.run(&["check-ref-format", &refname])?;
        if !output.status.success() {
            bail!("'{branch}' is not a valid git branch name");
        }
        Ok(())
    }

    pub fn rev_parse(&self, revision: &str) -> Result<String> {
        let output = self.checked(&["rev-parse", "--verify", revision])?;
        let sha = output.lines().next().unwrap_or("").trim();
        if sha.is_empty() {
            bail!("git rev-parse returned no object for '{revision}'");
        }
        Ok(sha.to_owned())
    }

    pub fn tree_of(&self, revision: &str) -> Result<String> {
        self.rev_parse(&format!("{revision}^{{tree}}"))
    }

    /// Report the current remote branch tip, if the branch exists.
    pub fn remote_branch(&self, remote: &str, branch: &str) -> Result<Option<String>> {
        let refname = format!("refs/heads/{branch}");
        let output = self.checked(&["ls-remote", "--heads", remote, &refname])?;

        let Some(line) = output.lines().find(|line| !line.trim().is_empty()) else {
            return Ok(None);
        };

        let mut fields = line.split_whitespace();
        let sha = fields.next().context("malformed `git ls-remote` output")?;
        let found_ref = fields.next().unwrap_or("");
        if found_ref != refname {
            bail!("`git ls-remote` returned unexpected ref '{found_ref}'");
        }
        Ok(Some(sha.to_owned()))
    }

    /// Fetch the remote branch and return the fetched tip commit.
    pub fn fetch_branch(&self, remote: &str, branch: &str) -> Result<String> {
        let refname = format!("refs/heads/{branch}");
        self.checked(&["fetch", "--quiet", "--no-tags", remote, &refname])?;
        self.rev_parse("FETCH_HEAD^{commit}")
    }

    pub fn commit_message(&self, revision: &str) -> Result<String> {
        self.checked(&["log", "-1", "--format=%B", revision])
    }

    pub fn commit_tree(&self, tree: &str, parent: Option<&str>, message: &str) -> Result<String> {
        let mut args = vec!["commit-tree", tree];
        if let Some(parent) = parent {
            args.push("-p");
            args.push(parent);
        }
        args.extend(["-m", message]);

        let output = self.checked(&args)?;
        let sha = output.lines().next().unwrap_or("").trim();
        if sha.is_empty() {
            bail!("git commit-tree returned no commit hash");
        }
        Ok(sha.to_owned())
    }

    /// Push `refspec`, optionally guarded by an explicit force-with-lease value.
    pub fn push(&self, remote: &str, refspec: &str, lease: Option<(&str, &str)>) -> Result<()> {
        // Do not let user/global push configuration add tags or mirror other
        // refs: the snapshot remote should only receive the snapshot branch.
        let mut args: Vec<String> = vec![
            "push".to_owned(),
            "--no-follow-tags".to_owned(),
            "--no-mirror".to_owned(),
        ];
        if let Some((refname, expected)) = lease {
            args.push(format!("--force-with-lease={refname}:{expected}"));
        }
        args.push(remote.to_owned());
        args.push(refspec.to_owned());

        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        self.checked(&args)?;
        Ok(())
    }

    fn run(&self, args: &[&str]) -> Result<Output> {
        if self.verbose {
            eprintln!("git {}", args.join(" "));
        }
        Command::new("git")
            .arg("-C")
            .arg(&self.root)
            .args(args)
            .output()
            .context("could not run `git`; is git installed and on PATH?")
    }

    fn checked(&self, args: &[&str]) -> Result<String> {
        let output = self.run(args)?;
        if !output.status.success() {
            return Err(git_error(args, &output));
        }
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
    }
}

fn git_error(args: &[&str], output: &Output) -> anyhow::Error {
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let detail = if stderr.trim().is_empty() {
        stdout.trim()
    } else {
        stderr.trim()
    };

    if detail.is_empty() {
        anyhow::anyhow!("git {} failed with {}", args.join(" "), output.status)
    } else {
        anyhow::anyhow!("git {} failed: {}", args.join(" "), detail)
    }
}
