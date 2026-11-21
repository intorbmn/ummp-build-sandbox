//! Stable CLI JSON envelopes. Human-readable command output stays on stderr in JSON mode.

use serde::Serialize;
use serde_json::{json, Value};
use std::cell::RefCell;
use std::sync::atomic::{AtomicBool, Ordering};

static JSON_MODE: AtomicBool = AtomicBool::new(false);

thread_local! {
    static RESULT: RefCell<Option<Value>> = const { RefCell::new(None) };
}

pub fn set_json_mode(enabled: bool) {
    JSON_MODE.store(enabled, Ordering::Relaxed);
}

#[must_use]
pub fn is_json_mode() -> bool {
    JSON_MODE.load(Ordering::Relaxed)
}

pub fn set_result<T: Serialize>(value: T) {
    RESULT.with(|result| *result.borrow_mut() = serde_json::to_value(value).ok());
}

fn success_envelope(command: &str, result: &Value) -> Value {
    json!({
        "schema": 1,
        "command": command,
        "result": result,
    })
}

fn failure_envelope(kind: &str, message: &str) -> Value {
    json!({
        "schema": 1,
        "error": {
            "kind": kind,
            "message": message,
        }
    })
}

pub fn success(command: &str, exit_code: i32) {
    let result = RESULT
        .with(|result| result.borrow_mut().take())
        .unwrap_or_else(|| json!({ "exit_code": exit_code }));
    println!("{}", success_envelope(command, &result));
}

pub fn failure(kind: &str, message: &str) {
    println!("{}", failure_envelope(kind, message));
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn success_envelope_matches_golden_fixture() {
        let golden: Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/golden/cli/success-envelope.json"
        ))
        .unwrap();
        assert_eq!(success_envelope("scan", &json!({})), golden);
    }

    #[test]
    fn failure_envelope_matches_golden_fixture() {
        let golden: Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/golden/cli/error-envelope.json"
        ))
        .unwrap();
        assert_eq!(failure_envelope("invalid_input", "bad path"), golden);
    }

    #[test]
    fn success_envelope_has_stable_top_level_keys() {
        let mut keys = success_envelope("scan", &json!({}))
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        keys.sort();
        assert_eq!(keys, vec!["command", "result", "schema"]);
    }

    #[test]
    fn failure_envelope_has_stable_top_level_keys() {
        let mut keys = failure_envelope("error", "failed")
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        keys.sort();
        assert_eq!(keys, vec!["error", "schema"]);
    }
}
