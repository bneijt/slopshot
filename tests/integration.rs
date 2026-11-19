use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use tempfile::TempDir;

struct Fixture {
    _temp: TempDir,
    root: PathBuf,
    home: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let temp = TempDir::new().expect("create temp dir");
        let root = temp.path().to_path_buf();
        let home = root.join("home");
        fs::create_dir(&home).expect("create fake home");
        Self {
            _temp: temp,
            root,
            home,
        }
    }

    fn path(&self, name: &str) -> PathBuf {
        self.root.join(name)
    }

    fn git(&self, dir: &Path, args: &[&str]) -> Output {
        Command::new("git")
            .current_dir(dir)
            .args(args)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("HOME", &self.home)
            .env("XDG_CONFIG_HOME", self.home.join(".config"))
            .env("GIT_AUTHOR_NAME", "Slopshot Test")
            .env("GIT_AUTHOR_EMAIL", "slopshot@example.com")
            .env("GIT_COMMITTER_NAME", "Slopshot Test")
            .env("GIT_COMMITTER_EMAIL", "slopshot@example.com")
            .env("GIT_TERMINAL_PROMPT", "0")
            .output()
            .expect("run git")
    }

    fn git_ok(&self, dir: &Path, args: &[&str]) -> String {
        let output = self.git(dir, args);
        assert!(
            output.status.success(),
            "git {args:?} failed\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).trim().to_owned()
    }

    fn slopshot(&self, dir: &Path, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_slopshot"))
            .current_dir(dir)
            .args(args)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("HOME", &self.home)
            .env("XDG_CONFIG_HOME", self.home.join(".config"))
            .env("GIT_AUTHOR_NAME", "Slopshot Test")
            .env("GIT_AUTHOR_EMAIL", "slopshot@example.com")
            .env("GIT_COMMITTER_NAME", "Slopshot Test")
            .env("GIT_COMMITTER_EMAIL", "slopshot@example.com")
            .env("GIT_TERMINAL_PROMPT", "0")
            .output()
            .expect("run slopshot")
    }

    fn init_remote(&self) -> PathBuf {
        let remote = self.path("github.git");
        fs::create_dir(&remote).expect("create remote dir");
        self.git_ok(
            &remote,
            &["init", "--quiet", "--bare", "--initial-branch=main"],
        );
        remote
    }

    fn init_work(&self) -> PathBuf {
        let work = self.path("work");
        fs::create_dir(&work).expect("create work dir");
        self.git_ok(&work, &["init", "--quiet", "--initial-branch=main"]);
        self.configure_repo(&work);
        work
    }

    fn configure_repo(&self, repo: &Path) {
        self.git_ok(repo, &["config", "user.name", "Slopshot Test"]);
        self.git_ok(repo, &["config", "user.email", "slopshot@example.com"]);
        self.git_ok(repo, &["config", "commit.gpgsign", "false"]);
    }

    fn add_remote(&self, work: &Path, remote: &Path) {
        self.git_ok(work, &["remote", "add", "github", remote.to_str().unwrap()]);
    }

    fn write_config(&self, work: &Path, contents: &str) {
        fs::write(work.join("slopshot.toml"), contents).expect("write config");
    }

    fn commit_file(&self, work: &Path, name: &str, contents: &str, message: &str) -> String {
        fs::write(work.join(name), contents).expect("write tracked file");
        self.git_ok(work, &["add", "-A"]);
        self.git_ok(work, &["commit", "--quiet", "-m", message]);
        self.git_ok(work, &["rev-parse", "HEAD"])
    }

    fn remote_head(&self, remote: &Path) -> String {
        self.git_ok(remote, &["rev-parse", "refs/heads/main"])
    }

    fn remote_subjects(&self, remote: &Path, branch: &str) -> Vec<String> {
        let log = self.git_ok(
            remote,
            &["log", "--format=%s", &format!("refs/heads/{branch}")],
        );
        log.lines().map(str::to_owned).collect()
    }

    fn message_at(&self, remote: &Path, revision: &str) -> String {
        self.git_ok(remote, &["log", "-1", "--format=%B", revision])
    }

    fn remote_message(&self, remote: &Path, branch: &str) -> String {
        self.message_at(remote, &format!("refs/heads/{branch}"))
    }

    fn assert_snapshot_message(
        &self,
        remote: &Path,
        branch: &str,
        number: u64,
        local_commit: &str,
    ) {
        let subject = self.git_ok(
            remote,
            &["log", "-1", "--format=%s", &format!("refs/heads/{branch}")],
        );
        assert_eq!(subject, format!("Snapshot {number}"));
        let message = self.remote_message(remote, branch);
        assert!(
            message.contains(&format!("ref {local_commit}")),
            "snapshot body does not contain ref {local_commit}:\n{message}"
        );
    }
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn assert_success(output: &Output) {
    assert!(
        output.status.success(),
        "expected success, got {:?}\nstdout:\n{}\nstderr:\n{}",
        output.status,
        stdout(output),
        stderr(output)
    );
}

fn assert_failure(output: &Output) {
    assert!(
        !output.status.success(),
        "expected failure\nstdout:\n{}\nstderr:\n{}",
        stdout(output),
        stderr(output)
    );
}

#[test]
fn publishes_initial_snapshot_and_is_idempotent() {
    let fixture = Fixture::new();
    let remote = fixture.init_remote();
    let work = fixture.init_work();
    fixture.add_remote(&work, &remote);
    fixture.write_config(&work, r#"remote = "github""#);
    let head = fixture.commit_file(&work, "hello.txt", "hello\n", "initial commit");

    let first = fixture.slopshot(&work, &[]);
    assert_success(&first);
    assert!(stdout(&first).contains("Snapshot 1"));
    assert!(stdout(&first).contains(&head));

    let subjects = fixture.remote_subjects(&remote, "main");
    assert_eq!(subjects, vec!["Snapshot 1".to_owned()]);
    fixture.assert_snapshot_message(&remote, "main", 1, &head);

    // The published tree is exactly the local tree.
    let local_tree = fixture.git_ok(&work, &["rev-parse", "HEAD^{tree}"]);
    let remote_tree = fixture.git_ok(&remote, &["rev-parse", "refs/heads/main^{tree}"]);
    assert_eq!(local_tree, remote_tree);

    // The first snapshot is a root commit.
    let parents = fixture.git_ok(
        &remote,
        &["rev-list", "--parents", "-n", "1", "refs/heads/main"],
    );
    assert_eq!(parents.split_whitespace().count(), 1);

    let snapshot_head = fixture.remote_head(&remote);
    let second = fixture.slopshot(&work, &[]);
    assert_success(&second);
    assert!(stdout(&second).contains("Already up to date"));
    assert_eq!(fixture.remote_head(&remote), snapshot_head);
}

#[test]
fn chains_a_new_snapshot_for_a_new_local_commit() {
    let fixture = Fixture::new();
    let remote = fixture.init_remote();
    let work = fixture.init_work();
    fixture.add_remote(&work, &remote);
    fixture.write_config(&work, r#"remote = "github""#);

    let first_local = fixture.commit_file(&work, "hello.txt", "hello\n", "initial commit");
    let first = fixture.slopshot(&work, &[]);
    assert_success(&first);
    let first_snapshot = fixture.remote_head(&remote);
    assert_eq!(
        fixture.remote_subjects(&remote, "main"),
        vec!["Snapshot 1".to_owned()]
    );
    fixture.assert_snapshot_message(&remote, "main", 1, &first_local);

    let second_local = fixture.commit_file(
        &work,
        "hello.txt",
        "hello\nworld\n",
        "this development commit must not be published",
    );
    let second = fixture.slopshot(&work, &[]);
    assert_success(&second);
    assert!(stdout(&second).contains("Snapshot 2"));

    assert_eq!(
        fixture.remote_subjects(&remote, "main"),
        vec!["Snapshot 2".to_owned(), "Snapshot 1".to_owned()]
    );
    fixture.assert_snapshot_message(&remote, "main", 2, &second_local);
    let first_message = fixture.message_at(&remote, "refs/heads/main^");
    assert!(first_message.contains(&format!("ref {first_local}")));

    // Snapshot 2 has Snapshot 1 as its parent and the current local tree.
    let parent = fixture.git_ok(&remote, &["rev-parse", "refs/heads/main^"]);
    assert_eq!(parent, first_snapshot);
    let local_tree = fixture.git_ok(&work, &["rev-parse", "HEAD^{tree}"]);
    let remote_tree = fixture.git_ok(&remote, &["rev-parse", "refs/heads/main^{tree}"]);
    assert_eq!(local_tree, remote_tree);

    // No trace of the local development commit message on the published side.
    let full_log = fixture.git_ok(&remote, &["log", "--format=%B", "refs/heads/main"]);
    assert!(!full_log.contains("development commit"));
}

#[test]
fn refuses_foreign_history_and_restart_replaces_it() {
    let fixture = Fixture::new();
    let remote = fixture.init_remote();

    let seed = fixture.path("seed");
    fs::create_dir(&seed).expect("create seed dir");
    fixture.git_ok(&seed, &["init", "--quiet", "--initial-branch=main"]);
    fixture.configure_repo(&seed);
    fixture.commit_file(&seed, "old.txt", "old\n", "foreign history");
    fixture.git_ok(
        &seed,
        &["push", "--quiet", remote.to_str().unwrap(), "main"],
    );
    assert_eq!(
        fixture.remote_subjects(&remote, "main"),
        vec!["foreign history".to_owned()]
    );

    let work = fixture.init_work();
    fixture.add_remote(&work, &remote);
    fixture.write_config(&work, r#"remote = "github""#);
    let head = fixture.commit_file(&work, "new.txt", "new\n", "local work");

    let refused = fixture.slopshot(&work, &[]);
    assert_failure(&refused);
    assert!(stderr(&refused).contains("not a slopshot snapshot"));
    assert!(stderr(&refused).contains("--restart"));
    assert_eq!(
        fixture.remote_subjects(&remote, "main"),
        vec!["foreign history".to_owned()]
    );

    let restarted = fixture.slopshot(&work, &["--restart"]);
    assert_success(&restarted);
    assert_eq!(
        fixture.remote_subjects(&remote, "main"),
        vec!["Snapshot 1".to_owned()]
    );
    fixture.assert_snapshot_message(&remote, "main", 1, &head);

    // The foreign commit is gone entirely, not just orphaned on top of.
    let count = fixture.git_ok(&remote, &["rev-list", "--count", "refs/heads/main"]);
    assert_eq!(count, "1");
}

#[test]
fn dry_run_publishes_nothing() {
    let fixture = Fixture::new();
    let remote = fixture.init_remote();
    let work = fixture.init_work();
    fixture.add_remote(&work, &remote);
    fixture.write_config(&work, r#"remote = "github""#);
    let head = fixture.commit_file(&work, "hello.txt", "hello\n", "initial commit");

    let output = fixture.slopshot(&work, &["--dry-run"]);
    assert_success(&output);
    assert!(stdout(&output).contains("Dry run"));
    assert!(stdout(&output).contains("Snapshot 1"));
    assert!(stdout(&output).contains(&head));
    assert!(stdout(&output).contains(&format!("ref {head}")));

    let remote_tip = fixture.git(&remote, &["rev-parse", "--verify", "refs/heads/main"]);
    assert!(!remote_tip.status.success());
}

#[test]
fn reports_a_missing_config_file() {
    let fixture = Fixture::new();
    let work = fixture.init_work();

    let output = fixture.slopshot(&work, &[]);
    assert_failure(&output);
    assert!(stderr(&output).contains("no config file"));
    assert!(stderr(&output).contains("--config"));
}

#[test]
fn supports_custom_branch_names() {
    let fixture = Fixture::new();
    let remote = fixture.init_remote();
    let work = fixture.init_work();
    fixture.add_remote(&work, &remote);
    fixture.write_config(
        &work,
        r#"
remote = "github"
local_branch = "dev"
branch = "public"
"#,
    );
    fixture.git_ok(&work, &["checkout", "--quiet", "-b", "dev"]);
    let head = fixture.commit_file(&work, "hello.txt", "hello\n", "initial commit");

    let output = fixture.slopshot(&work, &[]);
    assert_success(&output);
    assert_eq!(
        fixture.remote_subjects(&remote, "public"),
        vec!["Snapshot 1".to_owned()]
    );
    fixture.assert_snapshot_message(&remote, "public", 1, &head);
}

#[test]
fn rejects_an_invalid_local_branch_name() {
    let fixture = Fixture::new();
    let remote = fixture.init_remote();
    let work = fixture.init_work();
    fixture.add_remote(&work, &remote);
    fixture.write_config(
        &work,
        r#"
remote = "github"
local_branch = "bad..branch"
"#,
    );
    fixture.commit_file(&work, "hello.txt", "hello\n", "initial commit");

    let output = fixture.slopshot(&work, &[]);
    assert_failure(&output);
    assert!(stderr(&output).contains("not a valid git branch name"));
}

#[test]
fn runs_from_a_subdirectory_using_the_repo_root_config() {
    let fixture = Fixture::new();
    let remote = fixture.init_remote();
    let work = fixture.init_work();
    fixture.add_remote(&work, &remote);
    fixture.write_config(&work, r#"remote = "github""#);
    let head = fixture.commit_file(&work, "hello.txt", "hello\n", "initial commit");

    let subdir = work.join("src/deep");
    fs::create_dir_all(&subdir).expect("create subdirectory");

    let output = fixture.slopshot(&subdir, &[]);
    assert_success(&output);
    assert_eq!(
        fixture.remote_subjects(&remote, "main"),
        vec!["Snapshot 1".to_owned()]
    );
    fixture.assert_snapshot_message(&remote, "main", 1, &head);
}

#[test]
fn reports_a_missing_remote() {
    let fixture = Fixture::new();
    let work = fixture.init_work();
    fixture.write_config(&work, r#"remote = "does-not-exist""#);
    fixture.commit_file(&work, "hello.txt", "hello\n", "initial commit");

    let output = fixture.slopshot(&work, &[]);
    assert_failure(&output);
    assert!(stderr(&output).contains("does not exist"));
    assert!(stderr(&output).contains("git remote add"));
}

#[test]
fn dry_run_reports_the_next_snapshot_without_pushing() {
    let fixture = Fixture::new();
    let remote = fixture.init_remote();
    let work = fixture.init_work();
    fixture.add_remote(&work, &remote);
    fixture.write_config(&work, r#"remote = "github""#);

    let first_local = fixture.commit_file(&work, "hello.txt", "hello\n", "initial commit");
    assert_success(&fixture.slopshot(&work, &[]));
    let first_snapshot = fixture.remote_head(&remote);

    let second_local = fixture.commit_file(&work, "hello.txt", "hello\nworld\n", "next commit");
    let output = fixture.slopshot(&work, &["--dry-run"]);
    assert_success(&output);
    assert!(stdout(&output).contains("Snapshot 2"));
    assert!(stdout(&output).contains(&second_local));
    assert_eq!(fixture.remote_head(&remote), first_snapshot);
    assert_eq!(
        fixture.remote_subjects(&remote, "main"),
        vec!["Snapshot 1".to_owned()]
    );
    fixture.assert_snapshot_message(&remote, "main", 1, &first_local);
}

#[test]
fn restart_resets_an_existing_snapshot_chain() {
    let fixture = Fixture::new();
    let remote = fixture.init_remote();
    let work = fixture.init_work();
    fixture.add_remote(&work, &remote);
    fixture.write_config(&work, r#"remote = "github""#);

    fixture.commit_file(&work, "hello.txt", "hello\n", "initial commit");
    assert_success(&fixture.slopshot(&work, &[]));
    let second_local = fixture.commit_file(&work, "hello.txt", "hello\nworld\n", "next commit");
    assert_success(&fixture.slopshot(&work, &[]));
    assert_eq!(fixture.remote_subjects(&remote, "main").len(), 2);

    let output = fixture.slopshot(&work, &["--restart"]);
    assert_success(&output);
    assert_eq!(
        fixture.remote_subjects(&remote, "main"),
        vec!["Snapshot 1".to_owned()]
    );
    fixture.assert_snapshot_message(&remote, "main", 1, &second_local);
    let count = fixture.git_ok(&remote, &["rev-list", "--count", "refs/heads/main"]);
    assert_eq!(count, "1");
}

#[test]
fn does_not_push_tags_even_with_follow_tags_configured() {
    let fixture = Fixture::new();
    let remote = fixture.init_remote();
    let work = fixture.init_work();
    fixture.add_remote(&work, &remote);
    fixture.write_config(&work, r#"remote = "github""#);
    let head = fixture.commit_file(&work, "hello.txt", "hello\n", "initial commit");
    fixture.git_ok(&work, &["tag", "v1.0.0"]);
    fixture.git_ok(&work, &["config", "push.followTags", "true"]);

    let output = fixture.slopshot(&work, &[]);
    assert_success(&output);
    assert_eq!(
        fixture.remote_subjects(&remote, "main"),
        vec!["Snapshot 1".to_owned()]
    );
    fixture.assert_snapshot_message(&remote, "main", 1, &head);
    let tags = fixture.git_ok(
        &remote,
        &["for-each-ref", "--format=%(refname)", "refs/tags"],
    );
    assert!(tags.is_empty(), "unexpected tags on remote: {tags}");
}

#[test]
fn refuses_a_mirror_remote() {
    let fixture = Fixture::new();
    let remote = fixture.init_remote();
    let work = fixture.init_work();
    fixture.add_remote(&work, &remote);
    fixture.write_config(&work, r#"remote = "github""#);
    fixture.commit_file(&work, "hello.txt", "hello\n", "initial commit");
    fixture.git_ok(&work, &["config", "remote.github.mirror", "true"]);

    let output = fixture.slopshot(&work, &[]);
    assert_failure(&output);
    assert!(stderr(&output).contains("mirror"));
    let refs = fixture.git_ok(&remote, &["for-each-ref", "--format=%(refname)"]);
    assert!(refs.is_empty(), "remote was modified: {refs}");
}

#[test]
fn supports_a_custom_config_path() {
    let fixture = Fixture::new();
    let remote = fixture.init_remote();
    let work = fixture.init_work();
    fixture.add_remote(&work, &remote);
    let config_dir = work.join("configs");
    fs::create_dir(&config_dir).expect("create config dir");
    fs::write(config_dir.join("publish.toml"), r#"remote = "github""#)
        .expect("write custom config");
    let head = fixture.commit_file(&work, "hello.txt", "hello\n", "initial commit");

    let output = fixture.slopshot(&work, &["--config", "configs/publish.toml"]);
    assert_success(&output);
    assert_eq!(
        fixture.remote_subjects(&remote, "main"),
        vec!["Snapshot 1".to_owned()]
    );
    fixture.assert_snapshot_message(&remote, "main", 1, &head);
}

#[test]
fn reports_when_run_outside_a_git_work_tree() {
    let fixture = Fixture::new();
    let plain = fixture.path("plain");
    fs::create_dir(&plain).expect("create plain dir");
    fs::write(plain.join("slopshot.toml"), r#"remote = "github""#).expect("write config");

    let output = fixture.slopshot(&plain, &[]);
    assert_failure(&output);
    assert!(stderr(&output).contains("not inside a git work tree"));
}

#[test]
fn recognizes_legacy_snapshots_with_the_hash_in_the_subject() {
    let fixture = Fixture::new();
    let remote = fixture.init_remote();
    let work = fixture.init_work();
    fixture.add_remote(&work, &remote);
    fixture.write_config(&work, r#"remote = "github""#);

    let first_local = fixture.commit_file(&work, "hello.txt", "hello\n", "initial commit");
    let tree = fixture.git_ok(&work, &["rev-parse", "HEAD^{tree}"]);
    let legacy_snapshot = fixture.git_ok(
        &work,
        &[
            "commit-tree",
            &tree,
            "-m",
            &format!("Snapshot #1 {first_local}"),
        ],
    );
    fixture.git_ok(
        &work,
        &[
            "push",
            "--quiet",
            remote.to_str().unwrap(),
            &format!("{legacy_snapshot}:refs/heads/main"),
        ],
    );

    let no_op = fixture.slopshot(&work, &[]);
    assert_success(&no_op);
    assert!(stdout(&no_op).contains("Already up to date"));

    let second_local = fixture.commit_file(&work, "hello.txt", "hello\nworld\n", "next commit");
    let output = fixture.slopshot(&work, &[]);
    assert_success(&output);
    assert_eq!(
        fixture.remote_subjects(&remote, "main"),
        vec![
            "Snapshot 2".to_owned(),
            format!("Snapshot #1 {first_local}"),
        ]
    );
    fixture.assert_snapshot_message(&remote, "main", 2, &second_local);
    let parent = fixture.git_ok(&remote, &["rev-parse", "refs/heads/main^"]);
    assert_eq!(parent, legacy_snapshot);
}
