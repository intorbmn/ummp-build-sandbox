use std::io::{self, BufRead};

fn escape(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
}

fn main() {
    let stdin = io::stdin();
    let mut request = String::new();
    stdin.lock().read_line(&mut request).unwrap();

    if request.contains("\"method\":\"hello\"") {
        println!("{}", r#"{"id":"demo","ummp_api":1}"#);
    } else if request.contains("\"method\":\"findings\"") {
        println!("[]");
    } else if request.contains("\"method\":\"edits\"") {
        println!(
            "{}",
            r#"[{"path":"src/Main.java","original":"class DemoWidget {}","updated":"class DemoGadget {}"}]"#
        );
    } else if request.contains("\"method\":\"env\"") {
        let fields = std::env::vars()
            .map(|(key, value)| format!("\"{}\":\"{}\"", escape(&key), escape(&value)))
            .collect::<Vec<_>>()
            .join(",");
        println!("{{{fields}}}");
    }
}
