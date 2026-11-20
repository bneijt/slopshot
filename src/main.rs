use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;
use slopshot::Options;

#[derive(Debug, Parser)]
#[command(
    name = "slopshot",
    version,
    about = "Publish squashed snapshot commits of your private git history",
    long_about = "Read slopshot.toml (or use --remote), compare the configured remote \
                  branch with the local branch, and publish the current local tree as the \
                  next 'Snapshot N' commit. The local commit hash is recorded in the commit \
                  body. Development history stays local; the remote only ever sees the \
                  snapshot chain."
)]
struct Cli {
    /// Path to the configuration file (default: <repo>/slopshot.toml)
    #[arg(short, long, value_name = "FILE")]
    config: Option<PathBuf>,

    /// Remote name to publish to; makes slopshot.toml optional
    #[arg(short = 'r', long, value_name = "NAME")]
    remote: Option<String>,

    /// Show what would be done, without creating a snapshot or pushing
    #[arg(long)]
    dry_run: bool,

    /// Replace the remote branch with a fresh Snapshot 1 chain (force push)
    #[arg(long)]
    restart: bool,

    /// Print every git command
    #[arg(short, long)]
    verbose: bool,
}

fn main() -> ExitCode {
    #[cfg(unix)]
    restore_default_sigpipe();

    let cli = Cli::parse();
    let options = Options {
        config_path: cli.config,
        remote: cli.remote,
        dry_run: cli.dry_run,
        restart: cli.restart,
        verbose: cli.verbose,
    };

    match slopshot::execute(&options) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("slopshot: {error:#}");
            ExitCode::FAILURE
        }
    }
}

/// Rust ignores `SIGPIPE` by default, which turns `slopshot | head` into a
/// panic on the first broken write. Restore the usual Unix behavior.
#[cfg(unix)]
fn restore_default_sigpipe() {
    // SAFETY: setting a signal disposition to the OS default is always safe.
    unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_DFL);
    }
}
