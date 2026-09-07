pub(super) fn render_response(response: &CliResponse) {
    let value = match serde_json::to_value(response) {
        Ok(value) => value,
        Err(error) => {
            eprintln!("\x1b[1;31m✗\x1b[0m could not serialize response: {error}");
            return;
        }
    };
    if response.ok {
        println!("\x1b[1;32m● ok\x1b[0m");
        if let Some(result) = value.get("result") {
            print_colored_json(result, 0);
        }
    } else {
        let code = value
            .pointer("/error/code")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("error");
        let message = value
            .pointer("/error/message")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("");
        let retryable = value
            .pointer("/error/retryable")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false);
        println!(
            "\x1b[1;31m✗ {code}\x1b[0m \x1b[2m{}{}\x1b[0m",
            if retryable { "(retryable) " } else { "" },
            message,
        );
        if let Some(error) = value.get("error") {
            print_colored_json(error, 0);
        }
    }
}

fn print_colored_json(value: &serde_json::Value, indent: usize) {
    let pad = "  ".repeat(indent);
    match value {
        serde_json::Value::Null => println!("\x1b[2m{pad}null\x1b[0m"),
        serde_json::Value::Bool(b) => println!("\x1b[35m{pad}{b}\x1b[0m"),
        serde_json::Value::Number(n) => println!("\x1b[33m{pad}{n}\x1b[0m"),
        serde_json::Value::String(s) => {
            println!("\x1b[32m{pad}\"{}\"\x1b[0m", escape_json_string(s))
        }
        serde_json::Value::Array(items) => {
            if items.is_empty() {
                println!("\x1b[2m{pad}[]\x1b[0m");
                return;
            }
            println!("{pad}[");
            for item in items {
                print_colored_json(item, indent + 1);
            }
            println!("{pad}]");
        }
        serde_json::Value::Object(map) => {
            if map.is_empty() {
                println!("\x1b[2m{pad}{{}}\x1b[0m");
                return;
            }
            for (key, val) in map {
                match val {
                    serde_json::Value::Array(items) if !items.is_empty() => {
                        println!("\x1b[36m{pad}{key}\x1b[0m:");
                        print_colored_json(val, indent + 1);
                    }
                    serde_json::Value::Object(inner) if !inner.is_empty() => {
                        println!("\x1b[36m{pad}{key}\x1b[0m:");
                        print_colored_json(val, indent + 1);
                    }
                    _ => {
                        print!("\x1b[36m{pad}{key}\x1b[0m: ");
                        print_inline(val);
                        println!();
                    }
                }
            }
        }
    }
}

fn print_inline(value: &serde_json::Value) {
    match value {
        serde_json::Value::Null => print!("\x1b[2mnull\x1b[0m"),
        serde_json::Value::Bool(b) => print!("\x1b[35m{b}\x1b[0m"),
        serde_json::Value::Number(n) => print!("\x1b[33m{n}\x1b[0m"),
        serde_json::Value::String(s) => {
            print!("\x1b[32m\"{}\"\x1b[0m", escape_json_string(s))
        }
        serde_json::Value::Array(items) => {
            print!("\x1b[2m[\x1b[0m");
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    print!("\x1b[2m, \x1b[0m");
                }
                print_inline(item);
            }
            print!("\x1b[2m]\x1b[0m");
        }
        serde_json::Value::Object(map) => {
            print!("\x1b[2m{{\x1b[0m");
            for (i, (k, v)) in map.iter().enumerate() {
                if i > 0 {
                    print!("\x1b[2m, \x1b[0m");
                }
                print!("\x1b[36m{k}\x1b[0m: ");
                print_inline(v);
            }
            print!("\x1b[2m}}\x1b[0m");
        }
    }
}

fn escape_json_string(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

pub(super) fn print(text: &str) {
    use std::io::Write as _;
    let mut stdout = std::io::stdout().lock();
    let _ = stdout.write_all(text.as_bytes());
    let _ = stdout.flush();
}
use crate::protocol::CliResponse;
