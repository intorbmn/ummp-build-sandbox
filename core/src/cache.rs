//! Stable cache identities shared by analysis and scanning.

use sha2::{Digest, Sha256};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CacheKind {
    Analysis,
    Scan,
}

impl CacheKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Analysis => "analysis",
            Self::Scan => "scan",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CacheKey {
    pub kind: CacheKind,
    pub content_sha256: String,
    pub ummp_version: String,
    pub rules_sha256: String,
}

impl CacheKey {
    #[must_use]
    pub fn new(kind: CacheKind, content: &[u8], rules_sha256: String) -> Self {
        Self {
            kind,
            content_sha256: sha256(content),
            ummp_version: env!("CARGO_PKG_VERSION").to_string(),
            rules_sha256,
        }
    }
}

#[must_use]
pub fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

#[must_use]
pub fn rules_sha256<'a>(
    behavior_id: &str,
    files: impl IntoIterator<Item = (&'a str, &'a [u8])>,
) -> String {
    let mut files: Vec<_> = files
        .into_iter()
        .map(|(path, bytes)| (path.replace('\\', "/"), bytes.to_vec()))
        .collect();
    files.sort_by(|a, b| a.0.cmp(&b.0));
    let mut hash = Sha256::new();
    hash.update(b"UMMP-RULES");
    hash.update(behavior_id.as_bytes());
    hash.update([0]);
    for (path, bytes) in files {
        hash.update((path.len() as u64).to_be_bytes());
        hash.update(path.as_bytes());
        hash.update((bytes.len() as u64).to_be_bytes());
        hash.update(bytes);
    }
    format!("{:x}", hash.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rules_hash_is_order_independent_and_path_normalized() {
        let a = rules_sha256(
            "scan-v1",
            [
                ("rules/a.toml", b"alpha".as_slice()),
                ("rules\\b.toml", b"beta".as_slice()),
            ],
        );
        let b = rules_sha256(
            "scan-v1",
            [
                ("rules/b.toml", b"beta".as_slice()),
                ("rules/a.toml", b"alpha".as_slice()),
            ],
        );
        assert_eq!(a, b);
    }
    #[test]
    fn cache_key_separates_content_and_kind() {
        let a = CacheKey::new(CacheKind::Analysis, b"jar", "rules".into());
        let b = CacheKey::new(CacheKind::Scan, b"jar", "rules".into());
        let c = CacheKey::new(CacheKind::Analysis, b"other", "rules".into());
        assert_ne!(a.kind, b.kind);
        assert_ne!(a.content_sha256, c.content_sha256);
        let old_version = CacheKey {
            ummp_version: "0.0.1".into(),
            ..a.clone()
        };
        assert_ne!(a.ummp_version, old_version.ummp_version);
    }
}
