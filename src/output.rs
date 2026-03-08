use comfy_table::{ContentArrangement, Table};
use serde::Serialize;

pub fn print_output<T: Serialize>(json_mode: bool, headers: &[&str], rows: Vec<Vec<String>>, data: &T) {
    if json_mode {
        println!("{}", serde_json::to_string_pretty(data).unwrap());
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

pub fn print_json<T: Serialize>(data: &T) {
    println!("{}", serde_json::to_string_pretty(data).unwrap());
}

pub fn print_error(json_mode: bool, msg: &str) {
    if json_mode {
        println!(r#"{{"error": "{}"}}"#, msg.replace('"', r#"\""#));
    } else {
        eprintln!("Error: {msg}");
    }
}
