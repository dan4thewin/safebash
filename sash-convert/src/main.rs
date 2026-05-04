use std::io::{self, Read, Write};
use std::process::Command;

mod convert;
mod known_externals;

fn main() {
    let args: Vec<String> = std::env::args().collect();

    let mut input_file: Option<&str> = None;
    let mut output_file: Option<&str> = None;
    let mut annotate = false;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--annotate" => annotate = true,
            "-o" => {
                i += 1;
                if i < args.len() {
                    output_file = Some(&args[i]);
                } else {
                    eprintln!("sash-convert: -o requires an argument");
                    std::process::exit(1);
                }
            }
            "-h" | "--help" => {
                eprintln!("Usage: sash-convert [--annotate] [-o output] [input.sh]");
                eprintln!("  Converts bash scripts to sash-native source.");
                eprintln!("  Reads stdin if no input file given.");
                std::process::exit(0);
            }
            arg if arg.starts_with('-') => {
                eprintln!("sash-convert: unknown flag: {}", arg);
                std::process::exit(1);
            }
            _ => input_file = Some(&args[i]),
        }
        i += 1;
    }

    // Read input
    let source = if let Some(path) = input_file {
        std::fs::read_to_string(path).unwrap_or_else(|e| {
            eprintln!("sash-convert: {}: {}", path, e);
            std::process::exit(1);
        })
    } else {
        let mut buf = String::new();
        io::stdin().read_to_string(&mut buf).unwrap_or_else(|e| {
            eprintln!("sash-convert: stdin: {}", e);
            std::process::exit(1);
        });
        buf
    };

    // Run shellcheck for analysis
    let sc_findings = run_shellcheck(input_file, &source);

    // Convert
    let output = convert::convert(&source, &sc_findings, annotate);

    // Write output
    if let Some(path) = output_file {
        std::fs::write(path, &output).unwrap_or_else(|e| {
            eprintln!("sash-convert: {}: {}", path, e);
            std::process::exit(1);
        });
    } else {
        io::stdout().write_all(output.as_bytes()).unwrap_or_else(|e| {
            eprintln!("sash-convert: stdout: {}", e);
            std::process::exit(1);
        });
    }
}

/// A single shellcheck finding.
#[derive(Debug)]
pub struct ScFinding {
    pub line: usize,
    pub column: usize,
    pub end_line: usize,
    pub end_column: usize,
    pub code: u32,
    pub message: String,
}

/// Run shellcheck on the source and parse JSON output.
fn run_shellcheck(path: Option<&str>, source: &str) -> Vec<ScFinding> {
    let mut cmd = Command::new("shellcheck");
    cmd.args(["-f", "json", "-o", "all", "-s", "bash"]);

    // Always pipe source via stdin to avoid file-path issues
    cmd.arg("-");
    cmd.stdin(std::process::Stdio::piped())
       .stdout(std::process::Stdio::piped())
       .stderr(std::process::Stdio::null());

    let _ = path; // not used — we always pipe

    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("sash-convert: warning: shellcheck not available: {}", e);
            return Vec::new();
        }
    };

    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(source.as_bytes()).ok();
    }

    let output = match child.wait_with_output() {
        Ok(o) => o,
        Err(_) => return Vec::new(),
    };

    parse_shellcheck_json(&output.stdout)
}

fn parse_shellcheck_json(data: &[u8]) -> Vec<ScFinding> {
    // Minimal JSON parsing — shellcheck outputs an array of objects
    // Each has: line, column, endLine, endColumn, code, message
    let text = std::str::from_utf8(data).unwrap_or("[]");
    let mut findings = Vec::new();

    // Simple state-machine parser for shellcheck JSON
    // Format: [{"file":"...","line":N,"endLine":N,"column":N,"endColumn":N,"level":"...","code":N,"message":"..."},...]
    if text.trim() == "[]" || text.trim().is_empty() {
        return findings;
    }

    // Split by },{ to get individual objects
    let trimmed = text.trim().trim_start_matches('[').trim_end_matches(']');
    for obj in split_json_objects(trimmed) {
        let line = extract_json_uint(&obj, "\"line\":");
        let column = extract_json_uint(&obj, "\"column\":");
        let end_line = extract_json_uint(&obj, "\"endLine\":");
        let end_column = extract_json_uint(&obj, "\"endColumn\":");
        let code = extract_json_uint(&obj, "\"code\":");
        let message = extract_json_string(&obj, "\"message\":");

        if code > 0 {
            findings.push(ScFinding {
                line: line as usize,
                column: column as usize,
                end_line: end_line as usize,
                end_column: end_column as usize,
                code: code as u32,
                message,
            });
        }
    }

    findings
}

fn split_json_objects(s: &str) -> Vec<String> {
    let mut objects = Vec::new();
    let mut depth = 0;
    let mut start = 0;
    let mut in_string = false;
    let mut escape = false;

    for (i, c) in s.char_indices() {
        if escape {
            escape = false;
            continue;
        }
        match c {
            '\\' if in_string => escape = true,
            '"' => in_string = !in_string,
            '{' if !in_string => {
                if depth == 0 { start = i; }
                depth += 1;
            }
            '}' if !in_string => {
                depth -= 1;
                if depth == 0 {
                    objects.push(s[start..=i].to_string());
                }
            }
            _ => {}
        }
    }
    objects
}

fn extract_json_uint(obj: &str, key: &str) -> u64 {
    if let Some(pos) = obj.find(key) {
        let rest = &obj[pos + key.len()..];
        let num: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
        num.parse().unwrap_or(0)
    } else {
        0
    }
}

fn extract_json_string(obj: &str, key: &str) -> String {
    if let Some(pos) = obj.find(key) {
        let rest = &obj[pos + key.len()..];
        if let Some(start) = rest.find('"') {
            let inner = &rest[start + 1..];
            let mut result = String::new();
            let mut escape = false;
            for c in inner.chars() {
                if escape {
                    result.push(c);
                    escape = false;
                } else if c == '\\' {
                    escape = true;
                } else if c == '"' {
                    break;
                } else {
                    result.push(c);
                }
            }
            return result;
        }
    }
    String::new()
}
