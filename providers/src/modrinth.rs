//! Read-only Modrinth lookups for modpack migration (Phase 10, ADR 0013). Used only with
//! `--online`; never downloads a JAR, only metadata (URL, hashes, size).

use serde_json::Value;
use std::time::Duration;
use ummp_core::metadata::Loader;
use ummp_core::modpack::plan::{loader_slug, ModIndex, RemoteVersion};
use ummp_core::modpack::{Env, PackError};

const DEFAULT_BASE: &str = "https://api.modrinth.com";

pub struct ModrinthIndex {
    base: String,
    agent: ureq::Agent,
    retry_delay: Duration,
}

impl Default for ModrinthIndex {
    fn default() -> Self {
        Self::with_base(DEFAULT_BASE)
    }
}

impl ModrinthIndex {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// A client for another API root (tests use a local fake server).
    #[must_use]
    pub fn with_base(base: &str) -> Self {
        Self {
            base: base.trim_end_matches('/').to_string(),
            agent: ureq::AgentBuilder::new()
                .timeout(Duration::from_secs(20))
                .user_agent(concat!(
                    "intorbmn/universal-minecraft-mod-porter/",
                    env!("CARGO_PKG_VERSION")
                ))
                .build(),
            retry_delay: Duration::from_secs(1),
        }
    }

    /// How long to wait before the single retry after an HTTP 429.
    #[must_use]
    pub const fn with_retry_delay(mut self, d: Duration) -> Self {
        self.retry_delay = d;
        self
    }

    /// GET with one retry on 429. `Ok(None)` for 404.
    fn get(&self, path: &str, query: &[(&str, &str)]) -> Result<Option<Value>, PackError> {
        let url = format!("{}{path}", self.base);
        for attempt in 0..2 {
            let mut req = self.agent.get(&url);
            for (k, v) in query {
                req = req.query(k, v);
            }
            match req.call() {
                Ok(resp) => {
                    let text = resp
                        .into_string()
                        .map_err(|e| PackError::Index(format!("reading response: {e}")))?;
                    return serde_json::from_str(&text)
                        .map(Some)
                        .map_err(|e| PackError::Index(format!("invalid JSON from Modrinth: {e}")));
                }
                Err(ureq::Error::Status(404, _)) => return Ok(None),
                Err(ureq::Error::Status(429, _)) if attempt == 0 => {
                    std::thread::sleep(self.retry_delay);
                }
                Err(ureq::Error::Status(code, _)) => {
                    return Err(PackError::Index(format!("Modrinth answered HTTP {code}")));
                }
                Err(e) => return Err(PackError::Index(format!("request failed: {e}"))),
            }
        }
        Err(PackError::Index("Modrinth answered HTTP 429".into()))
    }
}

/// Percent-encodes everything but unreserved characters, so an id taken from an untrusted
/// descriptor cannot change the request path.
fn path_segment(s: &str) -> String {
    use std::fmt::Write as _;
    s.bytes().fold(String::new(), |mut out, b| {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') && s != ".." {
            out.push(char::from(b));
        } else {
            let _ = write!(out, "%{b:02X}");
        }
        out
    })
}

fn parse_version(v: &Value) -> Result<RemoteVersion, PackError> {
    let bad =
        |what: &str| PackError::Index(format!("unexpected Modrinth response: missing {what}"));
    let text = |k: &str| v[k].as_str().map(str::to_string).ok_or_else(|| bad(k));
    let files = v["files"].as_array().ok_or_else(|| bad("files"))?;
    let file = files
        .iter()
        .find(|f| f["primary"].as_bool() == Some(true))
        .or_else(|| files.first())
        .ok_or_else(|| bad("files[0]"))?;
    let hash = |k: &str| {
        file["hashes"][k]
            .as_str()
            .map(str::to_string)
            .ok_or_else(|| bad(k))
    };
    let required_deps = v["dependencies"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter(|d| d["dependency_type"].as_str() == Some("required"))
                .filter_map(|d| d["project_id"].as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    Ok(RemoteVersion {
        project: text("project_id")?,
        version: text("version_number")?,
        file_name: file["filename"]
            .as_str()
            .map(str::to_string)
            .ok_or_else(|| bad("filename"))?,
        url: file["url"]
            .as_str()
            .map(str::to_string)
            .ok_or_else(|| bad("url"))?,
        sha1: hash("sha1")?,
        sha512: hash("sha512")?,
        size: file["size"].as_u64().ok_or_else(|| bad("size"))?,
        required_deps,
        env: Env::default(),
    })
}

impl ModIndex for ModrinthIndex {
    fn versions(
        &self,
        project: &str,
        mc: &str,
        loader: &Loader,
    ) -> Result<Vec<RemoteVersion>, PackError> {
        let loaders = format!("[\"{}\"]", loader_slug(loader));
        let games = format!("[\"{mc}\"]");
        let Some(body) = self.get(
            &format!("/v2/project/{}/version", path_segment(project)),
            &[("loaders", &loaders), ("game_versions", &games)],
        )?
        else {
            return Ok(Vec::new());
        };
        body.as_array()
            .ok_or_else(|| PackError::Index("unexpected Modrinth response: not a list".into()))?
            .iter()
            .map(parse_version)
            .collect()
    }

    fn by_hash(&self, sha1: &str) -> Result<Option<RemoteVersion>, PackError> {
        self.get(
            &format!("/v2/version_file/{}", path_segment(sha1)),
            &[("algorithm", "sha1")],
        )?
        .as_ref()
        .map(parse_version)
        .transpose()
    }
}
