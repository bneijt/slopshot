mod config;
mod git;
mod snapshot;

use std::path::PathBuf;

use anyhow::{Context, Result, bail};

pub use config::CONFIG_FILE_NAME;
use config::Config;
use git::Git;
use snapshot::Snapshot;

#[derive(Debug, Default)]
pub struct Options {
    /// Use this config file instead of `<repo>/slopshot.toml`.
    pub config_path: Option<PathBuf>,
    /// Report what would happen without creating or pushing anything.
    pub dry_run: bool,
    /// Replace the remote branch with a fresh `Snapshot #1` root commit.
    pub restart: bool,
    /// Print every git command.
    pub verbose: bool,
}

pub fn execute(options: &Options) -> Result<()> {
    let git = Git::discover(options.verbose)?;

    let config_path = options
        .config_path
        .clone()
        .unwrap_or_else(|| git.root().join(CONFIG_FILE_NAME));
    if !config_path.exists() {
        bail!(
            "no config file at '{}'; create one or pass --config <FILE>",
            config_path.display()
        );
    }
    let config = Config::load(&config_path)?;
    let remote_branch = config.remote_branch().to_owned();

    git.validate_branch(&config.local_branch)
        .with_context(|| format!("invalid `local_branch` '{}'", config.local_branch))?;
    git.validate_branch(&remote_branch)
        .with_context(|| format!("invalid `branch` '{}'", remote_branch))?;

    if !git.remote_exists(&config.remote)? {
        bail!(
            "remote '{}' does not exist in '{}'; add it with `git remote add {} <url>`",
            config.remote,
            git.root().display(),
            config.remote
        );
    }
    if git.remote_is_mirror(&config.remote)? {
        bail!(
            "remote '{}' is configured as a mirror (remote.{}.mirror=true); use a normal \
             remote so that only the snapshot branch is published",
            config.remote,
            config.remote
        );
    }

    let local_ref = format!("refs/heads/{}", config.local_branch);
    let local_commit = git
        .rev_parse(&local_ref)
        .with_context(|| format!("local branch '{}' does not exist", config.local_branch))?;

    let remote_ref = format!("refs/heads/{remote_branch}");
    let remote_tip = if git.remote_branch(&config.remote, &remote_branch)?.is_some() {
        let tip = git
            .fetch_branch(&config.remote, &remote_branch)
            .with_context(|| {
                format!(
                    "could not fetch '{remote_ref}' from remote '{}'",
                    config.remote
                )
            })?;
        Some(tip)
    } else {
        None
    };

    let remote_snapshot = match (options.restart, remote_tip.as_deref()) {
        (true, _) => None,
        (false, Some(tip)) => {
            let message = git.commit_message(tip).with_context(|| {
                format!(
                    "could not read commit {} from remote '{}'",
                    short(tip),
                    config.remote
                )
            })?;
            match Snapshot::parse(&message) {
                Some(snapshot) => Some(snapshot),
                None => {
                    let subject = message.lines().next().unwrap_or("").trim();
                    bail!(
                        "the tip of '{remote_ref}' on remote '{}' is not a slopshot snapshot \
                         (commit {}, message '{subject}'); use --restart to replace it with \
                         Snapshot #1",
                        config.remote,
                        short(tip)
                    );
                }
            }
        }
        (false, None) => None,
    };

    let (number, parent) = if options.restart {
        (1, None)
    } else if let Some(snapshot) = &remote_snapshot {
        if snapshot.local_commit == local_commit {
            println!(
                "Already up to date: {}/{} is Snapshot #{} of {} ({}).",
                config.remote,
                remote_branch,
                snapshot.number,
                config.local_branch,
                short(&local_commit)
            );
            return Ok(());
        }
        (snapshot.next_number()?, remote_tip.clone())
    } else {
        (1, None)
    };

    let snapshot = Snapshot::new(number, local_commit.clone());
    let message = snapshot.message();
    let lease = if options.restart {
        remote_tip.as_deref().map(|tip| (remote_ref.as_str(), tip))
    } else {
        None
    };
    let parent_ref = parent.as_deref().map(short);

    if options.dry_run {
        match parent_ref {
            Some(parent) => println!("Dry run: would create \"{message}\" on top of {parent}."),
            None => println!("Dry run: would create \"{message}\" as a root commit."),
        }
        if options.restart && remote_tip.is_some() {
            println!(
                "Dry run: would force-replace {}/{} (currently {}).",
                config.remote,
                remote_branch,
                short(remote_tip.as_deref().unwrap_or(""))
            );
        } else {
            println!(
                "Dry run: would push it to {}/{}.",
                config.remote, remote_branch
            );
        }
        return Ok(());
    }

    let tree = git
        .tree_of(&local_commit)
        .with_context(|| format!("could not read the tree of {}", short(&local_commit)))?;
    let snapshot_commit = git
        .commit_tree(&tree, parent.as_deref(), &message)
        .context(
            "could not create the snapshot commit (is git user.name/user.email configured?)",
        )?;

    let refspec = format!("{snapshot_commit}:{remote_ref}");
    git.push(&config.remote, &refspec, lease)
        .with_context(|| format!("could not push to remote '{}'", config.remote))?;

    if options.restart && remote_tip.is_some() {
        println!(
            "Replaced {}/{} with a fresh snapshot chain.",
            config.remote, remote_branch
        );
    }
    println!("Snapshot #{} created: {}", number, snapshot_commit);
    println!(
        "Published to {}/{} for local {} at {}.",
        config.remote, remote_branch, config.local_branch, local_commit
    );
    Ok(())
}

fn short(sha: &str) -> &str {
    sha.get(..7).unwrap_or(sha)
}
