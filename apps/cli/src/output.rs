//! Human and JSON rendering of command results.
//!
//! With `--json`, stdout carries exactly one JSON document, the envelope
//! described by `schemas/glint-cli-output-v1.schema.json`, for success and
//! failure alike, and stderr stays empty. Without it, results go to stdout
//! and errors to stderr as plain text.

use serde_json::{Map, Value, json};

use crate::error::{CliError, EXIT_OK};

/// Version of the JSON envelope. Bumped only on an incompatible change.
pub const OUTPUT_SCHEMA_VERSION: u32 = 1;

/// A successful command result.
#[derive(Debug)]
pub struct Success {
    /// Machine-readable payload (the envelope's `data`).
    pub data: Value,
    /// Human-readable rendering.
    pub text: String,
}

/// What to print and which exit code to return.
#[derive(Debug)]
pub struct Rendered {
    /// Text for stdout (may be empty).
    pub stdout: String,
    /// Text for stderr (may be empty).
    pub stderr: String,
    /// Process exit code.
    pub exit_code: u8,
}

/// Render a result in the requested mode.
pub fn render(json: bool, command: Option<&str>, result: Result<Success, CliError>) -> Rendered {
    match (json, result) {
        (true, result) => {
            let exit_code = match &result {
                Ok(_) => EXIT_OK,
                Err(error) => error.exit_code(),
            };
            let mut envelope = Map::new();
            envelope.insert("schemaVersion".into(), json!(OUTPUT_SCHEMA_VERSION));
            envelope.insert("glintVersion".into(), json!(env!("CARGO_PKG_VERSION")));
            envelope.insert("command".into(), json!(command));
            envelope.insert("ok".into(), json!(result.is_ok()));
            envelope.insert("exitCode".into(), json!(exit_code));
            match result {
                Ok(success) => {
                    envelope.insert("data".into(), success.data);
                }
                Err(error) => {
                    envelope.insert(
                        "error".into(),
                        json!({
                            "kind": error.kind.as_str(),
                            "code": error.code,
                            "message": error.message,
                            "details": Value::Object(error.details),
                        }),
                    );
                }
            }
            let mut stdout = serde_json::to_string_pretty(&Value::Object(envelope))
                .unwrap_or_else(|_| String::from("{}"));
            stdout.push('\n');
            Rendered {
                stdout,
                stderr: String::new(),
                exit_code,
            }
        }
        (false, Ok(success)) => Rendered {
            stdout: with_newline(success.text),
            stderr: String::new(),
            exit_code: EXIT_OK,
        },
        (false, Err(error)) => Rendered {
            stdout: String::new(),
            stderr: format!(
                "error[{}]: {} ({})\n",
                error.kind.as_str(),
                error.message,
                error.code
            ),
            exit_code: error.exit_code(),
        },
    }
}

fn with_newline(mut text: String) -> String {
    if !text.is_empty() && !text.ends_with('\n') {
        text.push('\n');
    }
    text
}
