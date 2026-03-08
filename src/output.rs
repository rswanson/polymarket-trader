use comfy_table::{ContentArrangement, Table};
use serde::Serialize;

pub fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let end = s
            .char_indices()
            .nth(max.saturating_sub(3))
            .map(|(i, _)| i)
            .unwrap_or(s.len());
        format!("{}...", &s[..end])
    }
}

pub fn print_output<T: Serialize>(
    json_mode: bool,
    headers: &[&str],
    rows: Vec<Vec<String>>,
    data: &T,
) {
    if json_mode {
        match serde_json::to_string_pretty(data) {
            Ok(s) => println!("{s}"),
            Err(e) => eprintln!("Error serializing output: {e}"),
        }
    } else {
        let mut table = Table::new();
        table.set_content_arrangement(ContentArrangement::Dynamic);
        table.set_header(headers);
        for row in rows {
            table.add_row(row);
        }
        println!("{table}");
    }
}

pub fn print_error(json_mode: bool, msg: &str) {
    if json_mode {
        let err = serde_json::json!({"error": msg});
        match serde_json::to_string_pretty(&err) {
            Ok(s) => println!("{s}"),
            Err(e) => eprintln!("Error: {msg} (serialization failed: {e})"),
        }
    } else {
        eprintln!("Error: {msg}");
    }
}
