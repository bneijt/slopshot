# slopshot

`slopshot` publishes the current state of a local branch as a minimal chain of
snapshot commits. Your real development history stays on your private remote;
the public remote only ever receives:

```text
Snapshot 1
Snapshot 2
Snapshot 3
```

Each snapshot commit contains the **complete tree** of your local branch at that
moment. Its parent is the previous snapshot, not the local development commit,
so the published history is a clean line with no development commits, merges,
or development commit messages in it. The body of each snapshot records the
local commit it came from:

```text
Snapshot 3

ref 9d1c2b3a4e5f60718293a4b5c6d7e8f9a0b1c2d3
```

```text
local main (private):  A -- B -- C -- D -- E
                                 \          \
published chain:        Snapshot 1 -------- Snapshot 2
                        (tree of C)          (tree of E)
```

## How it decides whether to publish

The local commit hash is recorded in the snapshot body as
`ref <hash>`. On every run `slopshot`:

1. Reads `slopshot.toml` and resolves the configured local branch tip.
2. Looks at the configured branch on the remote (fetching it if it exists).
3. Parses the remote tip's subject as `Snapshot N` and finds the `ref <hash>`
   line in its body. Snapshots made by older versions—with `#` before the
   number, the hash in the subject, or `Local commit:` in the body—are still
   recognized.
4. If that hash is the current local tip, it prints `Already up to date` and
   exits successfully without pushing anything.
5. Otherwise it creates a commit whose tree is the current local tree and whose
   parent is the remote snapshot tip, with the subject `Snapshot N+1` and the
   body `ref <hash>`, and pushes that commit.

There is no local state file: the remote snapshot commit itself records which
local commit it corresponds to. Any machine with the repository and config can
publish the next snapshot.

## Requirements

- `git` on `PATH` at runtime.
- `user.name` and `user.email` configured for the repository (used for the
  snapshot commit).
- A git remote that points at the public repository.
- Rust 1.85+ only if you build from source (edition 2024).

## Build and install

```sh
cargo build --release
./target/release/slopshot --help

# or install it into ~/.cargo/bin
cargo install --path .
```

## Setup

Add the public remote once (the name is up to you):

```sh
git remote add github git@github.com:you/project.git
```

Create `slopshot.toml` at the root of the repository (see
[`slopshot.toml.example`](slopshot.toml.example)):

```toml
# Required: name of the git remote to publish to.
remote = "github"

# Optional: local branch to snapshot. Defaults to "main".
# local_branch = "main"

# Optional: branch on the remote. Defaults to local_branch.
# branch = "main"
```

Then preview and publish:

```sh
slopshot --dry-run
slopshot
```

Example output:

```text
$ slopshot
Snapshot 3 created: 4b2f1c9a0d8e7f6a5b4c3d2e1f0a9b8c7d6e5f4a
Published to github/main for local main at 9d1c2b3a4e5f60718293a4b5c6d7e8f9a0b1c2d3.

$ slopshot
Already up to date: github/main is Snapshot 3 of main (9d1c2b3).
```

## Configuration

| Key            | Required | Default           | Meaning                              |
| -------------- | -------- | ----------------- | ------------------------------------ |
| `remote`       | yes      | —                 | Name of the local git remote to push to. |
| `local_branch` | no       | `main`            | Local branch whose committed tree is published. |
| `branch`       | no       | `local_branch`    | Branch to update on the remote.       |

Unknown keys are rejected so typos do not silently do the wrong thing.

The file is looked up at the repository root. Use `--config <FILE>` to point at
a different file; a relative path is resolved from the current directory.

## Command-line flags

| Flag              | Effect |
| ----------------- | ------ |
| `--dry-run`       | Show the snapshot number, source commit, and push target without creating a snapshot or pushing. It still fetches the remote branch to decide. |
| `--restart`       | Ignore the remote history and replace the remote branch with a fresh `Snapshot 1` root commit. |
| `-c`, `--config`  | Use a specific config file. |
| `-v`, `--verbose` | Print every git command that is run. |

## First publish into an existing repository

If the remote branch already contains ordinary commits, `slopshot` refuses to
mix them into the snapshot chain. This is deliberate: otherwise those old
commits would remain in the published history. Once you are sure you want to
replace that branch, run:

```sh
slopshot --restart
```

This creates a fresh `Snapshot 1` root commit and replaces the remote branch
with it. When the branch already exists, the replacement is guarded with
`--force-with-lease` against the tip that was just fetched, so a concurrent push
will make the command fail instead of being overwritten.

## Details and caveats

- `slopshot` never commits to, checks out, or rewrites your local branch; it
  only creates the snapshot commit object and pushes it.
- Only **committed** state is published. Commit or stash your edits first if
  you want them in the snapshot. Ignored and untracked files are not included
  unless they are committed.
- Only the configured remote branch is managed; other refs on the remote are
  left untouched.
- The configured remote branch is owned by `slopshot`; do not push normal
  development commits to it or merge other work into it. `--restart` is the
  recovery path if it gets into an unexpected state.
- Do not pull or merge the snapshot branch back into your development branch.
  The two histories are intentionally unrelated; the snapshot tree is the
  useful artifact, not its ancestry.
- Normal snapshots are fast-forward appends. Rewriting local history does not
  rewrite existing public snapshots; the next run just appends a snapshot of
  the new local state.
- Public history grows by one commit per publish. Git deduplicates unchanged
  files, but long-lived projects will accumulate public blob history over time.
  Anything that has ever appeared in a published snapshot remains in the public
  history, even if a later snapshot deletes it. Do not publish secrets.
- Works with SHA-1 and SHA-256 repositories; both the local and remote
  repository must use the same object format.
- Snapshot commit subjects are exactly `Snapshot N`; the full source hash
  lives in the body as `ref <hash>`. Older snapshots using `Snapshot #N`, a
  hash in the subject, or a `Local commit:` body are still understood.
- Snapshot commits are created directly with `git commit-tree` and are not
  added to any local branch, so local commit hooks do not run.
- Tags are not published, even when `push.followTags` is enabled; only the
  configured snapshot branch is written. Mirror remotes are rejected.
- The tool shells out to git, so SSH agents, credential helpers, hooks, and
  remote branch protection behave exactly as they do for a normal `git push`.
- If the remote changes between fetch and push, the push fails safely; re-run
  the command.
- Git LFS pointer files are published like any other file; whether the LFS
  objects are uploaded depends on the repository's git-lfs setup.

## Development

```sh
cargo test
cargo clippy --all-targets -- -D warnings
cargo fmt --check
```
