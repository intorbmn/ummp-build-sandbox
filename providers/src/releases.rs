//! Read-only GitHub Releases update checks. This module never downloads binaries.

use serde_json::Value;
use std::cmp::Ordering;
use std::time::Duration;

const RELEASES_API: &str =
    "https://api.github.com/repos/intorbmn/Universal-Minecraft-Mod-Porter/releases/latest";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseUpdate {
    pub tag_name: String,
    pub html_url: String,
}

/// Query the latest stable GitHub release and return it only when newer than the running version.
pub fn check_for_update(current_version: &str) -> Result<Option<ReleaseUpdate>, String> {
    check_for_update_at(RELEASES_API, current_version)
}

/// Injectable endpoint for deterministic tests and compatible GitHub Enterprise instances.
pub fn check_for_update_at(
    endpoint: &str,
    current_version: &str,
) -> Result<Option<ReleaseUpdate>, String> {
    let agent = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(5))
        .build();
    let response = agent
        .get(endpoint)
        .set("Accept", "application/vnd.github+json")
        .set("User-Agent", "Universal-Minecraft-Mod-Porter")
        .call()
        .map_err(|error| format!("GitHub Releases request failed: {error}"))?;
    let body = response
        .into_string()
        .map_err(|error| format!("could not read GitHub Releases response: {error}"))?;
    let value: Value = serde_json::from_str(&body)
        .map_err(|error| format!("GitHub Releases returned invalid JSON: {error}"))?;
    let tag_name = value["tag_name"]
        .as_str()
        .filter(|tag| !tag.trim().is_empty())
        .ok_or_else(|| "GitHub Releases response is missing tag_name".to_string())?;
    let html_url = value["html_url"]
        .as_str()
        .filter(|url| url.starts_with("https://"))
        .ok_or_else(|| "GitHub Releases response is missing a valid html_url".to_string())?;

    if is_newer(tag_name, current_version) {
        Ok(Some(ReleaseUpdate {
            tag_name: tag_name.to_string(),
            html_url: html_url.to_string(),
        }))
    } else {
        Ok(None)
    }
}

fn parse_version(value: &str) -> Option<(Vec<u64>, Option<String>)> {
    let value = value.trim();
    let value = value.strip_prefix('v').unwrap_or(value);
    let (core, prerelease) = value
        .split_once('-')
        .map_or((value, None), |(core, pre)| (core, Some(pre.to_string())));
    let parts = core
        .split('.')
        .map(str::parse::<u64>)
        .collect::<Result<Vec<_>, _>>()
        .ok()?;
    (parts.len() == 3).then_some((parts, prerelease))
}

fn is_newer(candidate: &str, current: &str) -> bool {
    let (Some((candidate_core, candidate_pre)), Some((current_core, current_pre))) =
        (parse_version(candidate), parse_version(current))
    else {
        return false;
    };
    match candidate_core.cmp(&current_core) {
        Ordering::Greater => true,
        Ordering::Less => false,
        Ordering::Equal => match (candidate_pre, current_pre) {
            (None, Some(_)) => true,
            (Some(_) | None, None) => false,
            (Some(candidate), Some(current)) => candidate > current,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader, Write};
    use std::net::TcpListener;

    fn serve_once(body: &'static str) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut line = String::new();
            while reader.read_line(&mut line).unwrap() > 0 && line != "\r\n" {
                line.clear();
            }
            write!(
                stream,
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                body.len(),
                body
            )
            .unwrap();
        });
        format!("http://{address}/releases/latest")
    }

    #[test]
    fn newer_release_is_returned_from_fake_github_server() {
        let endpoint = serve_once(
            r#"{"tag_name":"v1.0.1","html_url":"https://github.com/intorbmn/Universal-Minecraft-Mod-Porter/releases/tag/v1.0.1"}"#,
        );
        let update = check_for_update_at(&endpoint, "1.0.0-rc.1")
            .unwrap()
            .unwrap();
        assert_eq!(update.tag_name, "v1.0.1");
        assert!(update.html_url.ends_with("v1.0.1"));
    }

    #[test]
    fn same_or_older_release_is_not_an_update() {
        let same = serve_once(
            r#"{"tag_name":"v1.0.0","html_url":"https://github.com/intorbmn/Universal-Minecraft-Mod-Porter/releases/tag/v1.0.0"}"#,
        );
        assert!(check_for_update_at(&same, "1.0.0").unwrap().is_none());

        let older = serve_once(
            r#"{"tag_name":"v0.9.9","html_url":"https://github.com/intorbmn/Universal-Minecraft-Mod-Porter/releases/tag/v0.9.9"}"#,
        );
        assert!(check_for_update_at(&older, "1.0.0").unwrap().is_none());
    }

    #[test]
    fn invalid_or_unreachable_endpoint_returns_an_error() {
        assert!(check_for_update_at("http://127.0.0.1:1/unreachable", "1.0.0").is_err());
    }

    #[test]
    fn stable_release_is_newer_than_its_release_candidate() {
        assert!(is_newer("1.0.0", "1.0.0-rc.1"));
        assert!(!is_newer("1.0.0-rc.1", "1.0.0"));
        assert!(is_newer("1.0.1", "1.0.0"));
    }
}
