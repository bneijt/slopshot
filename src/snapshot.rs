use anyhow::{Result, bail};

/// Metadata carried in the subject of every published snapshot commit.
///
/// The commit subject is exactly `Snapshot #<number> <local commit hash>`, for
/// example `Snapshot #3 4f3c1e2...`. The hash lets the next run recognize
/// whether the remote already represents the current local branch tip.
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
        format!("Snapshot #{} {}", self.number, self.local_commit)
    }

    /// Parse the first line of a commit message.
    ///
    /// Accepts nothing but `Snapshot #<digits> <40- or 64-char hex hash>`.
    pub fn parse(message: &str) -> Option<Self> {
        let line = message.lines().next()?.trim();
        let rest = line.strip_prefix("Snapshot #")?;

        let mut parts = rest.split_whitespace();
        let number = parts.next()?;
        let local_commit = parts.next()?;
        if parts.next().is_some() {
            return None;
        }
        if number.is_empty() || !number.bytes().all(|byte| byte.is_ascii_digit()) {
            return None;
        }
        if !is_full_hash(local_commit) {
            return None;
        }

        Some(Self {
            number: number.parse().ok()?,
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
    fn formats_expected_subject() {
        let snapshot = Snapshot::new(3, hash('a'));
        assert_eq!(snapshot.message(), format!("Snapshot #3 {}", hash('a')));
    }

    #[test]
    fn parses_sha1_and_sha256_hashes() {
        let sha1 = hash('a');
        let parsed = Snapshot::parse(&format!("Snapshot #12 {sha1}\n\nbody\n")).unwrap();
        assert_eq!(parsed.number, 12);
        assert_eq!(parsed.local_commit, sha1);

        let sha256 = "b".repeat(64);
        let parsed = Snapshot::parse(&format!("Snapshot #1 {sha256}")).unwrap();
        assert_eq!(parsed.local_commit, sha256);
    }

    #[test]
    fn parses_uppercase_hash_as_lowercase() {
        let parsed = Snapshot::parse(&format!("Snapshot #2 {}", hash('A'))).unwrap();
        assert_eq!(parsed.local_commit, hash('a'));
    }

    #[test]
    fn rejects_other_messages() {
        assert!(Snapshot::parse("Snapshot 1").is_none());
        assert!(Snapshot::parse("snapshot #1 0123456789").is_none());
        assert!(Snapshot::parse(&format!("Snapshot #x {}", hash('a'))).is_none());
        assert!(Snapshot::parse(&format!("Snapshot #1 {} extra", hash('a'))).is_none());
        assert!(Snapshot::parse("Snapshot #1 abc123").is_none());
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
