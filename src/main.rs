//! MCP server for polyhedral computations.
//!
//! Implements the Model Context Protocol (JSON-RPC 2.0 over stdio) and
//! exposes polyhedral tools from the polyhedral_common C++ library.
//!
//! # Current tools
//! - `dual_description`: compute the dual description of a polyhedral cone.
//!
//! # Adding new tools
//! 1. Add a new constant for the tool name.
//! 2. Register the tool in `tools_list()`.
//! 3. Add a match arm in `dispatch_tool()`.
//! 4. Implement the handler function.

use serde_json::{json, Value};
use std::io::Write as _;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;

// ---------------------------------------------------------------------------
// Server metadata
// ---------------------------------------------------------------------------

const PROTOCOL_VERSION: &str = "2024-11-05";
const SERVER_NAME: &str = "mcp-polyhedral";
const SERVER_VERSION: &str = "0.1.0";

// ---------------------------------------------------------------------------
// Tool name constants  (add new ones here when extending the server)
// ---------------------------------------------------------------------------

const TOOL_DUAL_DESCRIPTION: &str = "dual_description";

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

#[tokio::main]
async fn main() {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let mut reader = BufReader::new(stdin);
    let mut writer = tokio::io::BufWriter::new(stdout);

    let mut line = String::new();
    loop {
        line.clear();
        match reader.read_line(&mut line).await {
            Ok(0) => break, // EOF
            Ok(_) => {}
            Err(e) => {
                eprintln!("[mcp-polyhedral] stdin read error: {e}");
                break;
            }
        }

        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let msg: Value = match serde_json::from_str(trimmed) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("[mcp-polyhedral] JSON parse error: {e}");
                // Send a parse-error response with null id as per JSON-RPC spec.
                let resp = jsonrpc_error(Value::Null, -32700, "Parse error");
                send_response(&mut writer, &resp).await;
                continue;
            }
        };

        if let Some(resp) = handle_message(&msg).await {
            send_response(&mut writer, &resp).await;
        }
    }
}

async fn send_response(writer: &mut tokio::io::BufWriter<tokio::io::Stdout>, resp: &Value) {
    let bytes = serde_json::to_string(resp).unwrap();
    if let Err(e) = writer.write_all(bytes.as_bytes()).await {
        eprintln!("[mcp-polyhedral] write error: {e}");
    }
    if let Err(e) = writer.write_all(b"\n").await {
        eprintln!("[mcp-polyhedral] write error: {e}");
    }
    if let Err(e) = writer.flush().await {
        eprintln!("[mcp-polyhedral] flush error: {e}");
    }
}

// ---------------------------------------------------------------------------
// JSON-RPC message dispatch
// ---------------------------------------------------------------------------

async fn handle_message(msg: &Value) -> Option<Value> {
    let method = msg.get("method")?.as_str()?;
    let id = msg.get("id");
    let is_notification = id.is_none();

    let result: Option<Value> = match method {
        // Standard MCP lifecycle
        "initialize" => Some(handle_initialize()),
        "initialized" => None, // notification – no response

        // Keep-alive
        "ping" => Some(json!({})),

        // Tool discovery and invocation
        "tools/list" => Some(handle_tools_list()),
        "tools/call" => Some(handle_tools_call(msg).await),

        _ => {
            if is_notification {
                None
            } else {
                Some(json!({
                    "error": {
                        "code": -32601,
                        "message": format!("Method not found: {method}")
                    }
                }))
            }
        }
    };

    if is_notification {
        return None;
    }

    result.map(|r| {
        let id_val = id.cloned().unwrap_or(Value::Null);
        if let Some(err) = r.get("error") {
            jsonrpc_error(id_val, err["code"].as_i64().unwrap_or(-32603), err["message"].as_str().unwrap_or("Internal error"))
        } else {
            jsonrpc_ok(id_val, r)
        }
    })
}

// ---------------------------------------------------------------------------
// MCP handlers
// ---------------------------------------------------------------------------

fn handle_initialize() -> Value {
    json!({
        "protocolVersion": PROTOCOL_VERSION,
        "serverInfo": {
            "name": SERVER_NAME,
            "version": SERVER_VERSION
        },
        "capabilities": {
            "tools": {}
        }
    })
}

/// Return the list of all available tools.
///
/// When adding a new tool, append its descriptor to the `tools` array below.
fn handle_tools_list() -> Value {
    json!({
        "tools": [
            tool_dual_description_descriptor(),
            // INSERT NEW TOOL DESCRIPTORS HERE
        ]
    })
}

async fn handle_tools_call(msg: &Value) -> Value {
    let params = match msg.get("params") {
        Some(p) => p,
        None => return tool_error("Missing 'params' field"),
    };

    let name = match params.get("name").and_then(|n| n.as_str()) {
        Some(n) => n,
        None => return tool_error("Missing 'name' field in params"),
    };

    let args = params.get("arguments").unwrap_or(&Value::Null);

    dispatch_tool(name, args).await
}

/// Route a tool call to its handler.
///
/// Add new match arms here when extending the server.
async fn dispatch_tool(name: &str, args: &Value) -> Value {
    match name {
        TOOL_DUAL_DESCRIPTION => run_dual_description(args).await,
        // INSERT NEW TOOL HANDLERS HERE
        _ => tool_error(&format!("Unknown tool: {name}")),
    }
}

// ---------------------------------------------------------------------------
// Tool: dual_description
// ---------------------------------------------------------------------------

fn tool_dual_description_descriptor() -> Value {
    json!({
        "name": TOOL_DUAL_DESCRIPTION,
        "description": "Compute the dual description of a polyhedral cone. \
            Given an H-representation (rows are integer inequalities a·x ≥ 0), \
            returns the V-representation (rows are extreme rays), or vice versa. \
            Uses the POLY_dual_description program from polyhedral_common.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "matrix": {
                    "type": "array",
                    "description": "The input matrix (list of rows, each row is a list of integers).",
                    "items": {
                        "type": "array",
                        "items": { "type": "integer" }
                    }
                },
                "arithmetic": {
                    "type": "string",
                    "description": "Arithmetic type. Defaults to 'safe_rational'.",
                    "enum": ["safe_rational", "rational", "cpp_rational", "mpq_rational"],
                    "default": "safe_rational"
                },
                "backend": {
                    "type": "string",
                    "description": "Dual-description backend. Defaults to 'cdd'.",
                    "enum": ["cdd", "lrs", "ppl_ext", "cdd_ext", "normaliz", "glrs"],
                    "default": "cdd"
                }
            },
            "required": ["matrix"]
        }
    })
}

async fn run_dual_description(args: &Value) -> Value {
    // --- parse arguments ---------------------------------------------------

    let matrix = match args.get("matrix").and_then(|m| m.as_array()) {
        Some(m) => m,
        None => return tool_error("'matrix' must be a non-null array"),
    };

    if matrix.is_empty() {
        return tool_error("'matrix' must have at least one row");
    }

    let nb_rows = matrix.len();
    let nb_cols = match matrix[0].as_array() {
        Some(row) => row.len(),
        None => return tool_error("Each matrix row must be an array"),
    };

    if nb_cols == 0 {
        return tool_error("Matrix rows must be non-empty");
    }

    let arithmetic = args
        .get("arithmetic")
        .and_then(|a| a.as_str())
        .unwrap_or("safe_rational");

    let backend = args
        .get("backend")
        .and_then(|b| b.as_str())
        .unwrap_or("cdd");

    // --- build the input file ----------------------------------------------

    let mut input_text = format!("{nb_rows} {nb_cols}\n");
    for (i, row_val) in matrix.iter().enumerate() {
        let row = match row_val.as_array() {
            Some(r) => r,
            None => return tool_error(&format!("Row {i} is not an array")),
        };
        if row.len() != nb_cols {
            return tool_error(&format!(
                "Row {i} has {} columns, expected {nb_cols}",
                row.len()
            ));
        }
        let parts: Vec<String> = row
            .iter()
            .map(|v| {
                if let Some(n) = v.as_i64() {
                    n.to_string()
                } else {
                    // Fallback: keep raw JSON representation (e.g. floats)
                    v.to_string()
                }
            })
            .collect();
        input_text.push_str(&parts.join(" "));
        input_text.push('\n');
    }

    // --- write temp files --------------------------------------------------

    let mut input_file = match tempfile::NamedTempFile::new() {
        Ok(f) => f,
        Err(e) => return tool_error(&format!("Cannot create input temp file: {e}")),
    };
    if let Err(e) = input_file.write_all(input_text.as_bytes()) {
        return tool_error(&format!("Cannot write input temp file: {e}"));
    }
    // Flush so the C++ program sees the content.
    if let Err(e) = input_file.flush() {
        return tool_error(&format!("Cannot flush input temp file: {e}"));
    }

    let output_file = match tempfile::NamedTempFile::new() {
        Ok(f) => f,
        Err(e) => return tool_error(&format!("Cannot create output temp file: {e}")),
    };

    let input_path = input_file.path().to_string_lossy().into_owned();
    let output_path = output_file.path().to_string_lossy().into_owned();

    // --- invoke POLY_dual_description --------------------------------------

    let binary = find_polyhedral_binary("POLY_dual_description");

    let child_output = match Command::new(&binary)
        .args([arithmetic, backend, "CPP", &input_path, &output_path])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
    {
        Ok(o) => o,
        Err(e) => {
            return tool_error(&format!(
                "Failed to spawn '{binary}': {e}. \
                 Is POLY_dual_description installed and in PATH?"
            ))
        }
    };

    if !child_output.status.success() {
        let stderr = String::from_utf8_lossy(&child_output.stderr);
        return tool_error(&format!(
            "POLY_dual_description exited with error:\n{stderr}"
        ));
    }

    // --- read and parse the output -----------------------------------------

    let output_text = match tokio::fs::read_to_string(&output_path).await {
        Ok(t) => t,
        Err(e) => return tool_error(&format!("Cannot read output temp file: {e}")),
    };

    match parse_integer_matrix(&output_text) {
        Ok(result_matrix) => tool_ok(json!({ "matrix": result_matrix })),
        Err(e) => tool_error(&format!("Failed to parse program output: {e}")),
    }
}

// ---------------------------------------------------------------------------
// Matrix parsing (standard polyhedral_common text format)
//
// Format:
//   <nbRows> <nbCols>
//   <e00> <e01> ... <e0(nbCols-1)>
//   ...
// ---------------------------------------------------------------------------

fn parse_integer_matrix(text: &str) -> Result<Vec<Vec<i64>>, String> {
    let mut lines = text.lines().filter(|l| !l.trim().is_empty());

    let header = lines.next().ok_or("Output is empty")?;
    let mut dims = header.split_whitespace();

    let nb_rows: usize = dims
        .next()
        .ok_or("Header missing row count")?
        .parse()
        .map_err(|e| format!("Invalid row count: {e}"))?;

    let nb_cols: usize = dims
        .next()
        .ok_or("Header missing column count")?
        .parse()
        .map_err(|e| format!("Invalid column count: {e}"))?;

    let mut matrix = Vec::with_capacity(nb_rows);
    for i in 0..nb_rows {
        let line = lines
            .next()
            .ok_or_else(|| format!("Missing row {i} in output"))?;

        let row: Vec<i64> = line
            .split_whitespace()
            .map(|tok| {
                tok.parse::<i64>()
                    .map_err(|e| format!("Row {i}: invalid integer '{tok}': {e}"))
            })
            .collect::<Result<_, _>>()?;

        if row.len() != nb_cols {
            return Err(format!(
                "Row {i} has {} elements, expected {nb_cols}",
                row.len()
            ));
        }
        matrix.push(row);
    }

    Ok(matrix)
}

// ---------------------------------------------------------------------------
// Binary location helper
// ---------------------------------------------------------------------------

/// Return the path to a polyhedral_common binary.
///
/// Searches well-known installation locations first, then falls back to PATH.
fn find_polyhedral_binary(name: &str) -> String {
    // Locations where polyhedral_common typically installs its binaries.
    let candidates = [
        // Docker image built from the standard Dockerfile
        format!("/GIT/polyhedral_common/src_poly/{name}"),
        // Possible user-level installation
        format!("/usr/local/bin/{name}"),
        format!("/opt/polyhedral_common/bin/{name}"),
    ];

    for path in &candidates {
        if std::path::Path::new(path).exists() {
            return path.clone();
        }
    }

    // Fall back to searching PATH (works in development environments).
    name.to_string()
}

// ---------------------------------------------------------------------------
// JSON-RPC / MCP response helpers
// ---------------------------------------------------------------------------

/// Wrap a successful result in a JSON-RPC 2.0 response object.
fn jsonrpc_ok(id: Value, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

/// Wrap an error in a JSON-RPC 2.0 error response object.
fn jsonrpc_error(id: Value, code: i64, message: &str) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": { "code": code, "message": message }
    })
}

/// Build a successful MCP tool result carrying JSON content.
fn tool_ok(payload: Value) -> Value {
    json!({
        "content": [{
            "type": "text",
            "text": serde_json::to_string(&payload).unwrap()
        }]
    })
}

/// Build an MCP tool error result.
fn tool_error(message: &str) -> Value {
    json!({
        "isError": true,
        "content": [{
            "type": "text",
            "text": message
        }]
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // -----------------------------------------------------------------------
    // parse_integer_matrix
    // -----------------------------------------------------------------------

    #[test]
    fn parse_valid_2x3() {
        let m = parse_integer_matrix("2 3\n1 2 3\n4 5 6\n").unwrap();
        assert_eq!(m, vec![vec![1, 2, 3], vec![4, 5, 6]]);
    }

    #[test]
    fn parse_valid_1x1() {
        let m = parse_integer_matrix("1 1\n42\n").unwrap();
        assert_eq!(m, vec![vec![42]]);
    }

    #[test]
    fn parse_zero_rows() {
        let m = parse_integer_matrix("0 3\n").unwrap();
        assert!(m.is_empty());
    }

    #[test]
    fn parse_negative_values() {
        let m = parse_integer_matrix("1 2\n-3 7\n").unwrap();
        assert_eq!(m, vec![vec![-3, 7]]);
    }

    #[test]
    fn parse_extra_blank_lines() {
        let m = parse_integer_matrix("\n\n2 2\n\n1 0\n0 1\n\n").unwrap();
        assert_eq!(m, vec![vec![1, 0], vec![0, 1]]);
    }

    #[test]
    fn parse_multiple_spaces_in_row() {
        let m = parse_integer_matrix("1 3\n1  0  0\n").unwrap();
        assert_eq!(m, vec![vec![1, 0, 0]]);
    }

    #[test]
    fn parse_empty_string_errors() {
        assert!(parse_integer_matrix("").is_err());
    }

    #[test]
    fn parse_missing_row_errors() {
        // Header claims 2 rows but only 1 is present.
        assert!(parse_integer_matrix("2 2\n1 0\n").is_err());
    }

    #[test]
    fn parse_wrong_col_count_errors() {
        // Header says 3 cols but row only has 2.
        assert!(parse_integer_matrix("1 3\n1 2\n").is_err());
    }

    #[test]
    fn parse_non_integer_token_errors() {
        assert!(parse_integer_matrix("1 2\n1 abc\n").is_err());
    }

    #[test]
    fn parse_missing_col_count_errors() {
        // Header has only one number (no column count).
        assert!(parse_integer_matrix("2\n1 0\n0 1\n").is_err());
    }

    // -----------------------------------------------------------------------
    // JSON-RPC / MCP response helpers
    // -----------------------------------------------------------------------

    #[test]
    fn jsonrpc_ok_structure() {
        let resp = jsonrpc_ok(json!(7), json!({"x": 1}));
        assert_eq!(resp["jsonrpc"], "2.0");
        assert_eq!(resp["id"], 7);
        assert_eq!(resp["result"]["x"], 1);
        assert!(resp.get("error").is_none());
    }

    #[test]
    fn jsonrpc_ok_null_id() {
        let resp = jsonrpc_ok(Value::Null, json!({}));
        assert_eq!(resp["id"], Value::Null);
    }

    #[test]
    fn jsonrpc_error_structure() {
        let resp = jsonrpc_error(json!(3), -32601, "Method not found");
        assert_eq!(resp["jsonrpc"], "2.0");
        assert_eq!(resp["id"], 3);
        assert_eq!(resp["error"]["code"], -32601);
        assert_eq!(resp["error"]["message"], "Method not found");
    }

    #[test]
    fn tool_ok_structure() {
        let resp = tool_ok(json!({"matrix": [[1, 0]]}));
        assert!(resp.get("isError").is_none());
        let content = resp["content"].as_array().unwrap();
        assert!(!content.is_empty());
        assert_eq!(content[0]["type"], "text");
        let text = content[0]["text"].as_str().unwrap();
        let parsed: Value = serde_json::from_str(text).unwrap();
        assert!(parsed["matrix"].is_array());
    }

    #[test]
    fn tool_error_structure() {
        let resp = tool_error("oops");
        assert_eq!(resp["isError"], true);
        let content = resp["content"].as_array().unwrap();
        assert_eq!(content[0]["text"], "oops");
    }

    // -----------------------------------------------------------------------
    // MCP handlers (synchronous)
    // -----------------------------------------------------------------------

    #[test]
    fn initialize_fields() {
        let r = handle_initialize();
        assert_eq!(r["protocolVersion"], PROTOCOL_VERSION);
        assert_eq!(r["serverInfo"]["name"], SERVER_NAME);
        assert_eq!(r["serverInfo"]["version"], SERVER_VERSION);
        assert!(r["capabilities"]["tools"].is_object());
    }

    #[test]
    fn tools_list_contains_dual_description() {
        let r = handle_tools_list();
        let tools = r["tools"].as_array().unwrap();
        let dual = tools.iter().find(|t| t["name"] == TOOL_DUAL_DESCRIPTION);
        assert!(dual.is_some(), "dual_description not in tool list");
        let d = dual.unwrap();
        assert!(!d["description"].as_str().unwrap().is_empty());
        assert_eq!(d["inputSchema"]["required"][0], "matrix");
    }

    // -----------------------------------------------------------------------
    // Message dispatch (asynchronous)
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn handle_message_initialize() {
        let msg = json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{}});
        let resp = handle_message(&msg).await.unwrap();
        assert_eq!(resp["jsonrpc"], "2.0");
        assert_eq!(resp["id"], 1);
        assert!(resp["result"]["protocolVersion"].is_string());
    }

    #[tokio::test]
    async fn handle_message_ping() {
        let msg = json!({"jsonrpc":"2.0","id":2,"method":"ping"});
        let resp = handle_message(&msg).await.unwrap();
        assert_eq!(resp["id"], 2);
        assert!(resp["result"].is_object());
    }

    #[tokio::test]
    async fn handle_message_tools_list() {
        let msg = json!({"jsonrpc":"2.0","id":3,"method":"tools/list"});
        let resp = handle_message(&msg).await.unwrap();
        assert_eq!(resp["id"], 3);
        assert!(resp["result"]["tools"].is_array());
    }

    #[tokio::test]
    async fn handle_message_unknown_method_returns_error() {
        let msg = json!({"jsonrpc":"2.0","id":4,"method":"no_such_method"});
        let resp = handle_message(&msg).await.unwrap();
        assert_eq!(resp["id"], 4);
        assert_eq!(resp["error"]["code"], -32601);
    }

    #[tokio::test]
    async fn handle_message_initialized_notification_returns_none() {
        // "initialized" is a notification (no "id") – must produce no response.
        let msg = json!({"jsonrpc":"2.0","method":"initialized"});
        assert!(handle_message(&msg).await.is_none());
    }

    #[tokio::test]
    async fn handle_message_unknown_notification_returns_none() {
        let msg = json!({"jsonrpc":"2.0","method":"unknown_notification"});
        assert!(handle_message(&msg).await.is_none());
    }

    #[tokio::test]
    async fn dispatch_unknown_tool_returns_error() {
        let result = dispatch_tool("no_such_tool", &json!({})).await;
        assert_eq!(result["isError"], true);
    }

    #[tokio::test]
    async fn handle_message_tools_call_unknown_tool() {
        let msg = json!({
            "jsonrpc": "2.0", "id": 11,
            "method": "tools/call",
            "params": { "name": "nonexistent_tool", "arguments": {} }
        });
        let resp = handle_message(&msg).await.unwrap();
        assert_eq!(resp["id"], 11);
        // MCP tool error is surfaced in result, not as a JSON-RPC error.
        assert_eq!(resp["result"]["isError"], true);
    }

    #[tokio::test]
    async fn handle_message_tools_call_missing_params() {
        let msg = json!({"jsonrpc":"2.0","id":12,"method":"tools/call"});
        let resp = handle_message(&msg).await.unwrap();
        assert_eq!(resp["id"], 12);
        assert_eq!(resp["result"]["isError"], true);
    }

    // -----------------------------------------------------------------------
    // run_dual_description – argument validation (no binary required)
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn dual_description_missing_matrix_key() {
        let result = run_dual_description(&json!({})).await;
        assert_eq!(result["isError"], true);
    }

    #[tokio::test]
    async fn dual_description_null_matrix() {
        let result = run_dual_description(&json!({"matrix": null})).await;
        assert_eq!(result["isError"], true);
    }

    #[tokio::test]
    async fn dual_description_empty_matrix() {
        let result = run_dual_description(&json!({"matrix": []})).await;
        assert_eq!(result["isError"], true);
    }

    #[tokio::test]
    async fn dual_description_empty_row() {
        let result = run_dual_description(&json!({"matrix": [[]]})).await;
        assert_eq!(result["isError"], true);
    }

    #[tokio::test]
    async fn dual_description_inconsistent_row_length() {
        // Row 0 has 2 elements, row 1 has 1.
        let result = run_dual_description(&json!({"matrix": [[1,0],[1]]})).await;
        assert_eq!(result["isError"], true);
    }

    #[tokio::test]
    async fn dual_description_non_array_row() {
        let result = run_dual_description(&json!({"matrix": [42]})).await;
        assert_eq!(result["isError"], true);
    }

    // -----------------------------------------------------------------------
    // Integration tests – require POLY_dual_description in PATH
    // -----------------------------------------------------------------------

    fn binary_available() -> bool {
        std::process::Command::new("sh")
            .args(["-c", "command -v POLY_dual_description"])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    /// The positive orthant in R^3 is self-dual: H-rep and V-rep are both
    /// the three standard basis vectors (3 rows × 3 cols).
    #[tokio::test]
    async fn integration_positive_orthant_r3() {
        if !binary_available() {
            eprintln!("SKIP: POLY_dual_description not found in PATH");
            return;
        }
        let args = json!({"matrix": [[1,0,0],[0,1,0],[0,0,1]]});
        let result = run_dual_description(&args).await;
        assert!(
            result.get("isError").is_none() || result["isError"] != true,
            "unexpected error: {result:?}"
        );
        let text = result["content"][0]["text"].as_str().unwrap();
        let parsed: Value = serde_json::from_str(text).unwrap();
        let rows = parsed["matrix"].as_array().unwrap();
        assert_eq!(rows.len(), 3, "expected 3 extreme rays, got {}", rows.len());
        for row in rows {
            assert_eq!(row.as_array().unwrap().len(), 3);
        }
    }

    /// The positive quadrant in R^2 (2 inequalities → 2 extreme rays).
    #[tokio::test]
    async fn integration_positive_quadrant_r2() {
        if !binary_available() {
            eprintln!("SKIP: POLY_dual_description not found in PATH");
            return;
        }
        let args = json!({"matrix": [[1,0],[0,1]]});
        let result = run_dual_description(&args).await;
        assert!(
            result.get("isError").is_none() || result["isError"] != true,
            "unexpected error: {result:?}"
        );
        let text = result["content"][0]["text"].as_str().unwrap();
        let parsed: Value = serde_json::from_str(text).unwrap();
        let rows = parsed["matrix"].as_array().unwrap();
        assert_eq!(rows.len(), 2, "expected 2 extreme rays, got {}", rows.len());
        for row in rows {
            assert_eq!(row.as_array().unwrap().len(), 2);
        }
    }

    /// End-to-end test via handle_message for a tools/call request.
    #[tokio::test]
    async fn integration_handle_message_tools_call() {
        if !binary_available() {
            eprintln!("SKIP: POLY_dual_description not found in PATH");
            return;
        }
        let msg = json!({
            "jsonrpc": "2.0", "id": 10,
            "method": "tools/call",
            "params": {
                "name": "dual_description",
                "arguments": { "matrix": [[1,0],[0,1]] }
            }
        });
        let resp = handle_message(&msg).await.unwrap();
        assert_eq!(resp["id"], 10);
        let result = &resp["result"];
        assert!(result.get("isError").is_none() || result["isError"] != true);
        let text = result["content"][0]["text"].as_str().unwrap();
        let parsed: Value = serde_json::from_str(text).unwrap();
        assert!(parsed["matrix"].is_array());
    }

    /// Test with the lrs backend (accepted even if lrs is not installed).
    #[tokio::test]
    async fn integration_lrs_backend() {
        if !binary_available() {
            eprintln!("SKIP: POLY_dual_description not found in PATH");
            return;
        }
        let args = json!({
            "matrix": [[1,0,0],[0,1,0],[0,0,1]],
            "backend": "lrs"
        });
        let result = run_dual_description(&args).await;
        if result.get("isError").map(|v| v == true).unwrap_or(false) {
            let msg = result["content"][0]["text"].as_str().unwrap_or("");
            eprintln!("lrs backend not available or failed: {msg}");
        } else {
            let text = result["content"][0]["text"].as_str().unwrap();
            let parsed: Value = serde_json::from_str(text).unwrap();
            assert!(parsed["matrix"].is_array());
        }
    }

    /// Test with an explicit arithmetic type.
    #[tokio::test]
    async fn integration_rational_arithmetic() {
        if !binary_available() {
            eprintln!("SKIP: POLY_dual_description not found in PATH");
            return;
        }
        let args = json!({
            "matrix": [[1,0,0],[0,1,0],[0,0,1]],
            "arithmetic": "rational"
        });
        let result = run_dual_description(&args).await;
        assert!(
            result.get("isError").is_none() || result["isError"] != true,
            "unexpected error: {result:?}"
        );
        let text = result["content"][0]["text"].as_str().unwrap();
        let parsed: Value = serde_json::from_str(text).unwrap();
        assert_eq!(parsed["matrix"].as_array().unwrap().len(), 3);
    }
}
