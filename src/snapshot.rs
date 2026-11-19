use anyhow::{Result, bail};

/// Metadata carried in a published snapshot commit.
///
/// The subject is `Snapshot <number>`; the local commit it came from is
/// recorded in the body as `ref <full hash>`. Snapshots created by older
/// versions, with `#` before the number or `Local commit:` in the body, are
/// still recognized.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Snapshot {
    pub number: u64,
    pub local_commit: String,
}

impl Snapshot {
    pub fn new(number: u64, local_commit: impl Into<String>) -> Self {
        Self {
            number,
            local_commit: local_commit.into(),
        }
    }

    pub fn message(&self) -> String {
        format!("Snapshot {}\n\nref {}", self.number, self.local_commit)
    }

    /// Parse a snapshot commit message.
    ///
    /// The current format is a `Snapshot <digits>` subject followed by a body
    /// line `ref <40- or 64-char hex hash>`. Older formats are also accepted:
    /// `Snapshot #<digits>` with a `Local commit:` body, and the original
    /// `Snapshot #<digits> <hash>` subject.
    pub fn parse(message: &str) -> Option<Self> {
        let mut lines = message.lines();
        let subject = lines.next()?.trim();
        let rest = subject.strip_prefix("Snapshot ")?;
        let rest = rest.strip_prefix('#').unwrap_or(rest);

        let mut parts = rest.split_whitespace();
        let number = parts.next()?;
        if number.is_empty() || !number.bytes().all(|byte| byte.is_ascii_digit()) {
            return None;
        }
        let number = number.parse().ok()?;

        // Original format: the source hash was in the subject.
        //   Snapshot #7 0123...
        if let Some(local_commit) = parts.next() {
            if parts.next().is_some() || !is_full_hash(local_commit) {
                return None;
            }
            return Some(Self {
                number,
                local_commit: local_commit.to_ascii_lowercase(),
            });
        }

        // Current format: the source hash is in the body.
        //   Snapshot 8
        //
        //   ref 0123...
        //
        // The previous `Local commit:` body label is also accepted.
        let local_commit = lines
            .filter_map(|line| {
                let line = line.trim();
                line.strip_prefix("ref ")
                    .or_else(|| line.strip_prefix("Local commit:"))
                    .map(str::trim)
            })
            .find(|value| is_full_hash(value))?;

        Some(Self {
            number,
            local_commit: local_commit.to_ascii_lowercase(),
        })
    }

    pub fn next_number(&self) -> Result<u64> {
        match self.number.checked_add(1) {
            Some(number) => Ok(number),
            None => bail!("snapshot number {} is too large to increment", self.number),
        }
    }
}

fn is_full_hash(value: &str) -> bool {
    matches!(value.len(), 40 | 64) && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hash(byte: char) -> String {
        std::iter::repeat_n(byte, 40).collect()
    }

    #[test]
    fn formats_subject_and_body() {
        let snapshot = Snapshot::new(3, hash('a'));
        assert_eq!(
            snapshot.message(),
            format!("Snapshot 3\n\nref {}", hash('a'))
        );
    }

    #[test]
    fn parses_sha1_and_sha256_hashes_from_body() {
        let sha1 = hash('a');
        let parsed = Snapshot::parse(&format!("Snapshot 12\n\nref {sha1}\n")).unwrap();
        assert_eq!(parsed.number, 12);
        assert_eq!(parsed.local_commit, sha1);

        let sha256 = "b".repeat(64);
        let parsed = Snapshot::parse(&format!("Snapshot 1\n\nNote here\nref {sha256}\n")).unwrap();
        assert_eq!(parsed.number, 1);
        assert_eq!(parsed.local_commit, sha256);
    }

    #[test]
    fn parses_previous_body_label() {
        let sha = hash('a');
        let parsed = Snapshot::parse(&format!("Snapshot #2\n\nLocal commit: {sha}")).unwrap();
        assert_eq!(parsed.number, 2);
        assert_eq!(parsed.local_commit, sha);
    }

    #[test]
    fn parses_legacy_hash_from_subject() {
        let sha = hash('a');
        let parsed = Snapshot::parse(&format!("Snapshot #7 {sha}\n\nbody\n")).unwrap();
        assert_eq!(parsed.number, 7);
        assert_eq!(parsed.local_commit, sha);
    }

    #[test]
    fn parses_uppercase_hash_as_lowercase() {
        let parsed = Snapshot::parse(&format!("Snapshot 2\n\nref {}", hash('A'))).unwrap();
        assert_eq!(parsed.local_commit, hash('a'));
    }

    #[test]
    fn rejects_other_messages() {
        let sha = hash('a');

        assert!(Snapshot::parse("Snapshot 1").is_none());
        assert!(Snapshot::parse("snapshot 1").is_none());
        assert!(Snapshot::parse(&format!("Snapshot x\n\nref {sha}")).is_none());
        assert!(Snapshot::parse(&format!("Snapshot 1 {sha} extra")).is_none());
        assert!(Snapshot::parse(&format!("Snapshot 1\n\nref {sha} extra")).is_none());
        assert!(Snapshot::parse("Snapshot 1\n\nref abc123").is_none());
        assert!(Snapshot::parse(&format!("Snapshot 1\n\nNo source: {sha}")).is_none());
        assert!(Snapshot::parse("Merge branch 'main'").is_none());
    }

    #[test]
    fn increments_number() {
        assert_eq!(Snapshot::new(1, hash('a')).next_number().unwrap(), 2);
        let error = Snapshot::new(u64::MAX, hash('a'))
            .next_number()
            .unwrap_err();
        assert!(error.to_string().contains("too large"));
    }
}
