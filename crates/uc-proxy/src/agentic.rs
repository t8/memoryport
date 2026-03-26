use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use serde_json::Value;
use std::sync::Arc;
use tracing::{debug, warn};

use crate::routes::ProxyState;

const TOOL_PREFIX: &str = "memoryport_";
const MAX_TOOL_RESULT_CHARS: usize = 50_000;

// ---------------------------------------------------------------------------
// API format
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy)]
pub enum ApiFormat {
    Anthropic,
    OpenAi,
    Ollama,
}

// ---------------------------------------------------------------------------
// Internal types
// ---------------------------------------------------------------------------

struct RawToolCall {
    id: Option<String>,
    name: String,
    arguments: Value,
}

enum ResponseAction {
    /// No memoryport tool calls — return this response to the client.
    Final,
    /// One or more memoryport tool calls to execute. May also contain client tool calls.
    MemoryportCalls {
        our_calls: Vec<RawToolCall>,
        has_client_calls: bool,
    },
}

struct ToolResult {
    tool_call_id: Option<String>,
    content: String,
}

// ---------------------------------------------------------------------------
// Tool definitions
// ---------------------------------------------------------------------------

fn anthropic_tool_defs() -> Vec<Value> {
    serde_json::json!([
        {
            "name": "memoryport_search",
            "description": "Search the user's conversation memory for relevant context from previous conversations. Use this when the user references something from the past, or when additional context would improve your answer.",
            "input_schema": {
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "Describe what information you are looking for"
                    },
                    "max_results": {
                        "type": "integer",
                        "description": "Maximum results to return (default 20)"
                    }
                },
                "required": ["query"]
            }
        },
        {
            "name": "memoryport_get_session",
            "description": "Retrieve the full transcript of a specific conversation session by its ID.",
            "input_schema": {
                "type": "object",
                "properties": {
                    "session_id": {
                        "type": "string",
                        "description": "The session ID to retrieve"
                    }
                },
                "required": ["session_id"]
            }
        },
        {
            "name": "memoryport_list_sessions",
            "description": "List all available conversation sessions with metadata (ID, message count, date range).",
            "input_schema": {
                "type": "object",
                "properties": {}
            }
        }
    ])
    .as_array()
    .unwrap()
    .clone()
}

fn openai_tool_defs() -> Vec<Value> {
    serde_json::json!([
        {
            "type": "function",
            "function": {
                "name": "memoryport_search",
                "description": "Search the user's conversation memory for relevant context from previous conversations. Use this when the user references something from the past, or when additional context would improve your answer.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "query": {
                            "type": "string",
                            "description": "Describe what information you are looking for"
                        },
                        "max_results": {
                            "type": "integer",
                            "description": "Maximum results to return (default 20)"
                        }
                    },
                    "required": ["query"]
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "memoryport_get_session",
                "description": "Retrieve the full transcript of a specific conversation session by its ID.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "session_id": {
                            "type": "string",
                            "description": "The session ID to retrieve"
                        }
                    },
                    "required": ["session_id"]
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "memoryport_list_sessions",
                "description": "List all available conversation sessions with metadata (ID, message count, date range).",
                "parameters": {
                    "type": "object",
                    "properties": {}
                }
            }
        }
    ])
    .as_array()
    .unwrap()
    .clone()
}

// ---------------------------------------------------------------------------
// Tool injection
// ---------------------------------------------------------------------------

fn inject_tools(request: &mut Value, format: ApiFormat) {
    let defs = match format {
        ApiFormat::Anthropic => anthropic_tool_defs(),
        ApiFormat::OpenAi | ApiFormat::Ollama => openai_tool_defs(),
    };

    let tools = request
        .as_object_mut()
        .unwrap()
        .entry("tools")
        .or_insert_with(|| Value::Array(vec![]));

    if let Some(arr) = tools.as_array_mut() {
        arr.extend(defs);
    }
}

fn strip_injected_tools(request: &mut Value, format: ApiFormat) {
    let tools = match request.get_mut("tools").and_then(|t| t.as_array_mut()) {
        Some(t) => t,
        None => return,
    };

    tools.retain(|tool| {
        let name = match format {
            ApiFormat::Anthropic => tool.get("name").and_then(|n| n.as_str()),
            ApiFormat::OpenAi | ApiFormat::Ollama => {
                tool.get("function").and_then(|f| f.get("name")).and_then(|n| n.as_str())
            }
        };
        !name.map(|n| n.starts_with(TOOL_PREFIX)).unwrap_or(false)
    });

    // Remove tools array entirely if empty (cleaner for models that don't expect it)
    if tools.is_empty() {
        request.as_object_mut().unwrap().remove("tools");
    }
}

// ---------------------------------------------------------------------------
// Tool-call extraction
// ---------------------------------------------------------------------------

fn extract_tool_calls(response: &Value, format: ApiFormat) -> Vec<RawToolCall> {
    match format {
        ApiFormat::Anthropic => extract_anthropic_tool_calls(response),
        ApiFormat::OpenAi => extract_openai_tool_calls(response),
        ApiFormat::Ollama => extract_ollama_tool_calls(response),
    }
}

fn extract_anthropic_tool_calls(response: &Value) -> Vec<RawToolCall> {
    let content = match response.get("content").and_then(|c| c.as_array()) {
        Some(c) => c,
        None => return vec![],
    };

    content
        .iter()
        .filter_map(|block| {
            if block.get("type").and_then(|t| t.as_str()) != Some("tool_use") {
                return None;
            }
            let id = block.get("id").and_then(|i| i.as_str()).map(|s| s.to_string());
            let name = block.get("name").and_then(|n| n.as_str())?.to_string();
            let arguments = block.get("input").cloned().unwrap_or(Value::Object(Default::default()));
            Some(RawToolCall { id, name, arguments })
        })
        .collect()
}

fn extract_openai_tool_calls(response: &Value) -> Vec<RawToolCall> {
    let tool_calls = response
        .get("choices")
        .and_then(|c| c.get(0))
        .and_then(|c| c.get("message"))
        .and_then(|m| m.get("tool_calls"))
        .and_then(|t| t.as_array());

    let tool_calls = match tool_calls {
        Some(tc) => tc,
        None => return vec![],
    };

    tool_calls
        .iter()
        .filter_map(|tc| {
            let id = tc.get("id").and_then(|i| i.as_str()).map(|s| s.to_string());
            let func = tc.get("function")?;
            let name = func.get("name").and_then(|n| n.as_str())?.to_string();
            // OpenAI returns arguments as a JSON string
            let args_str = func.get("arguments").and_then(|a| a.as_str()).unwrap_or("{}");
            let arguments = serde_json::from_str(args_str).unwrap_or(Value::Object(Default::default()));
            Some(RawToolCall { id, name, arguments })
        })
        .collect()
}

fn extract_ollama_tool_calls(response: &Value) -> Vec<RawToolCall> {
    let tool_calls = response
        .get("message")
        .and_then(|m| m.get("tool_calls"))
        .and_then(|t| t.as_array());

    let tool_calls = match tool_calls {
        Some(tc) => tc,
        None => return vec![],
    };

    tool_calls
        .iter()
        .filter_map(|tc| {
            let func = tc.get("function")?;
            let name = func.get("name").and_then(|n| n.as_str())?.to_string();
            // Ollama returns arguments as a parsed object
            let arguments = func.get("arguments").cloned().unwrap_or(Value::Object(Default::default()));
            Some(RawToolCall {
                id: None,
                name,
                arguments,
            })
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Response classification
// ---------------------------------------------------------------------------

fn classify_response(response: &Value, format: ApiFormat) -> ResponseAction {
    let all_calls = extract_tool_calls(response, format);

    if all_calls.is_empty() {
        return ResponseAction::Final;
    }

    let (ours, theirs): (Vec<_>, Vec<_>) =
        all_calls.into_iter().partition(|c| c.name.starts_with(TOOL_PREFIX));

    if ours.is_empty() {
        ResponseAction::Final
    } else {
        ResponseAction::MemoryportCalls {
            our_calls: ours,
            has_client_calls: !theirs.is_empty(),
        }
    }
}

// ---------------------------------------------------------------------------
// Tool execution
// ---------------------------------------------------------------------------

async fn execute_tool(
    engine: &uc_core::Engine,
    user_id: &str,
    call: &RawToolCall,
) -> ToolResult {
    let content = match call.name.as_str() {
        "memoryport_search" => execute_search(engine, user_id, &call.arguments).await,
        "memoryport_get_session" => execute_get_session(engine, user_id, &call.arguments).await,
        "memoryport_list_sessions" => execute_list_sessions(engine, user_id).await,
        other => format!("Unknown tool: {other}"),
    };

    ToolResult {
        tool_call_id: call.id.clone(),
        content: truncate(&content, MAX_TOOL_RESULT_CHARS),
    }
}

async fn execute_search(engine: &uc_core::Engine, user_id: &str, args: &Value) -> String {
    let query = args.get("query").and_then(|q| q.as_str()).unwrap_or("");
    let max_results = args
        .get("max_results")
        .and_then(|m| m.as_u64())
        .unwrap_or(20) as u32;

    match engine.search(query, user_id, max_results as usize).await {
        Ok(results) if results.is_empty() => "No matching results found.".into(),
        Ok(results) => {
            let items: Vec<String> = results
                .iter()
                .enumerate()
                .map(|(i, r)| {
                    let ts = format_timestamp(r.timestamp);
                    let session = &r.session_id;
                    let role = r
                        .role
                        .as_ref()
                        .map(|r| format!("{r:?}"))
                        .unwrap_or_else(|| "unknown".into());
                    format!(
                        "{}. [{}] session={} role={} score={:.3}\n{}",
                        i + 1,
                        ts,
                        session,
                        role,
                        r.score,
                        r.content
                    )
                })
                .collect();
            format!("Found {} results:\n\n{}", results.len(), items.join("\n\n"))
        }
        Err(e) => format!("Search error: {e}"),
    }
}

async fn execute_get_session(engine: &uc_core::Engine, user_id: &str, args: &Value) -> String {
    let session_id = args
        .get("session_id")
        .and_then(|s| s.as_str())
        .unwrap_or("");

    if session_id.is_empty() {
        return "Error: session_id is required".into();
    }

    match engine.get_session(user_id, session_id).await {
        Ok(chunks) if chunks.is_empty() => format!("No data found for session '{session_id}'."),
        Ok(chunks) => {
            let turns: Vec<String> = chunks
                .iter()
                .map(|c| {
                    let ts = format_timestamp(c.timestamp);
                    let role = c
                        .role
                        .as_ref()
                        .map(|r| format!("{r:?}"))
                        .unwrap_or_else(|| "unknown".into());
                    format!("[{}] {}: {}", ts, role, c.content)
                })
                .collect();
            format!(
                "Session '{}' ({} messages):\n\n{}",
                session_id,
                turns.len(),
                turns.join("\n\n")
            )
        }
        Err(e) => format!("Error retrieving session: {e}"),
    }
}

async fn execute_list_sessions(engine: &uc_core::Engine, user_id: &str) -> String {
    match engine.list_sessions(user_id).await {
        Ok(sessions) if sessions.is_empty() => "No sessions found.".into(),
        Ok(sessions) => {
            let items: Vec<String> = sessions
                .iter()
                .map(|s| {
                    let first = format_timestamp_millis(s.first_timestamp);
                    let last = format_timestamp_millis(s.last_timestamp);
                    format!(
                        "- {} ({} messages, {} → {})",
                        s.session_id, s.chunk_count, first, last
                    )
                })
                .collect();
            format!("{} sessions:\n{}", sessions.len(), items.join("\n"))
        }
        Err(e) => format!("Error listing sessions: {e}"),
    }
}

// ---------------------------------------------------------------------------
// Append tool round to conversation history
// ---------------------------------------------------------------------------

fn append_round_to_messages(
    request: &mut Value,
    assistant_response: &Value,
    results: &[ToolResult],
    format: ApiFormat,
    strip_client_calls: bool,
) {
    let messages = match request.get_mut("messages").and_then(|m| m.as_array_mut()) {
        Some(m) => m,
        None => return,
    };

    match format {
        ApiFormat::Anthropic => {
            // Append the assistant message (content array)
            let mut assistant_content = assistant_response
                .get("content")
                .cloned()
                .unwrap_or(Value::Array(vec![]));

            // If stripping client calls, remove non-memoryport tool_use blocks
            if strip_client_calls {
                if let Some(arr) = assistant_content.as_array_mut() {
                    arr.retain(|block| {
                        let is_tool_use = block.get("type").and_then(|t| t.as_str()) == Some("tool_use");
                        if !is_tool_use {
                            return true;
                        }
                        let name = block.get("name").and_then(|n| n.as_str()).unwrap_or("");
                        name.starts_with(TOOL_PREFIX)
                    });
                }
            }

            messages.push(serde_json::json!({
                "role": "assistant",
                "content": assistant_content
            }));

            // Append tool results as a single user message with tool_result blocks
            let result_blocks: Vec<Value> = results
                .iter()
                .map(|r| {
                    serde_json::json!({
                        "type": "tool_result",
                        "tool_use_id": r.tool_call_id.as_deref().unwrap_or(""),
                        "content": r.content
                    })
                })
                .collect();

            messages.push(serde_json::json!({
                "role": "user",
                "content": result_blocks
            }));
        }
        ApiFormat::OpenAi => {
            // Append assistant message with tool_calls
            let mut assistant_msg = assistant_response
                .get("choices")
                .and_then(|c| c.get(0))
                .and_then(|c| c.get("message"))
                .cloned()
                .unwrap_or(serde_json::json!({"role": "assistant", "content": null}));

            // Strip client tool calls if needed
            if strip_client_calls {
                if let Some(tc) = assistant_msg.get_mut("tool_calls").and_then(|t| t.as_array_mut()) {
                    tc.retain(|call| {
                        let name = call
                            .get("function")
                            .and_then(|f| f.get("name"))
                            .and_then(|n| n.as_str())
                            .unwrap_or("");
                        name.starts_with(TOOL_PREFIX)
                    });
                }
            }

            messages.push(assistant_msg);

            // Append each tool result as a separate message
            for r in results {
                messages.push(serde_json::json!({
                    "role": "tool",
                    "tool_call_id": r.tool_call_id.as_deref().unwrap_or(""),
                    "content": r.content
                }));
            }
        }
        ApiFormat::Ollama => {
            // Append assistant message
            let mut assistant_msg = assistant_response
                .get("message")
                .cloned()
                .unwrap_or(serde_json::json!({"role": "assistant", "content": ""}));

            if strip_client_calls {
                if let Some(tc) = assistant_msg.get_mut("tool_calls").and_then(|t| t.as_array_mut()) {
                    tc.retain(|call| {
                        let name = call
                            .get("function")
                            .and_then(|f| f.get("name"))
                            .and_then(|n| n.as_str())
                            .unwrap_or("");
                        name.starts_with(TOOL_PREFIX)
                    });
                }
            }

            messages.push(assistant_msg);

            // Append tool results
            for r in results {
                messages.push(serde_json::json!({
                    "role": "tool",
                    "content": r.content
                }));
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Forwarding helpers
// ---------------------------------------------------------------------------

async fn forward_and_read(
    state: &ProxyState,
    headers: &HeaderMap,
    body: &[u8],
    upstream_url: &str,
    format: ApiFormat,
) -> Result<(Value, StatusCode, axum::http::HeaderMap), StatusCode> {
    let mut req_builder = state
        .http
        .post(upstream_url)
        .header("content-type", "application/json")
        .body(body.to_vec());

    // Forward auth headers
    for (name, value) in headers.iter() {
        let n = name.as_str();
        match format {
            ApiFormat::Anthropic => {
                if n == "authorization" || n == "x-api-key" || n.starts_with("anthropic-") {
                    req_builder = req_builder.header(name, value);
                }
            }
            ApiFormat::OpenAi | ApiFormat::Ollama => {
                if n == "authorization" || n == "x-api-key" {
                    req_builder = req_builder.header(name, value);
                }
            }
        }
    }

    let resp = req_builder.send().await.map_err(|e| {
        warn!(error = %e, "agentic loop: failed to forward to upstream");
        StatusCode::BAD_GATEWAY
    })?;

    let status = StatusCode::from_u16(resp.status().as_u16())
        .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    let resp_headers = resp.headers().clone();
    let body_bytes = resp.bytes().await.map_err(|e| {
        warn!(error = %e, "agentic loop: failed to read response");
        StatusCode::BAD_GATEWAY
    })?;

    // For Ollama, response may be NDJSON streaming — take the last complete JSON object
    let resp_json: Value = if matches!(format, ApiFormat::Ollama) {
        parse_ollama_response(&body_bytes)?
    } else {
        serde_json::from_slice(&body_bytes).map_err(|e| {
            warn!(error = %e, "agentic loop: failed to parse response JSON");
            StatusCode::BAD_GATEWAY
        })?
    };

    Ok((resp_json, status, resp_headers))
}

/// Parse Ollama NDJSON response — accumulate message content and take the final object's metadata.
fn parse_ollama_response(body: &[u8]) -> Result<Value, StatusCode> {
    let body_str = std::str::from_utf8(body).map_err(|_| StatusCode::BAD_GATEWAY)?;
    let mut last_obj: Option<Value> = None;
    let mut accumulated_content = String::new();
    let mut tool_calls: Option<Value> = None;

    for line in body_str.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Ok(obj) = serde_json::from_str::<Value>(line) {
            // Accumulate content
            if let Some(content) = obj
                .get("message")
                .and_then(|m| m.get("content"))
                .and_then(|c| c.as_str())
            {
                accumulated_content.push_str(content);
            }
            // Capture tool_calls if present
            if let Some(tc) = obj.get("message").and_then(|m| m.get("tool_calls")) {
                if tc.is_array() && !tc.as_array().unwrap().is_empty() {
                    tool_calls = Some(tc.clone());
                }
            }
            last_obj = Some(obj);
        }
    }

    let mut result = last_obj.unwrap_or(serde_json::json!({}));
    // Reconstruct the full message
    let message = result
        .as_object_mut()
        .unwrap()
        .entry("message")
        .or_insert_with(|| serde_json::json!({}));
    if let Some(msg) = message.as_object_mut() {
        msg.insert("role".into(), Value::String("assistant".into()));
        msg.insert("content".into(), Value::String(accumulated_content));
        if let Some(tc) = tool_calls {
            msg.insert("tool_calls".into(), tc);
        }
    }
    Ok(result)
}

// ---------------------------------------------------------------------------
// The main agentic loop
// ---------------------------------------------------------------------------

pub async fn run_agentic_loop(
    state: &Arc<ProxyState>,
    headers: &HeaderMap,
    request: &mut Value,
    format: ApiFormat,
    upstream_base: &str,
    session_source: &str,
) -> Result<Response, StatusCode> {
    let max_rounds = state.agentic_config.agentic().await.max_rounds;
    let upstream_url = match format {
        ApiFormat::Anthropic => format!("{upstream_base}/v1/messages"),
        ApiFormat::OpenAi => format!("{upstream_base}/v1/chat/completions"),
        ApiFormat::Ollama => format!("{upstream_base}/api/chat"),
    };

    // Save and override streaming — we need full responses to detect tool calls
    let original_stream = request.get("stream").and_then(|s| s.as_bool()).unwrap_or(false);
    request
        .as_object_mut()
        .unwrap()
        .insert("stream".into(), Value::Bool(false));

    // Inject our tool definitions
    inject_tools(request, format);

    eprintln!(
        "[memoryport-proxy] agentic loop: starting ({:?}, max_rounds={})",
        format, max_rounds
    );

    let mut final_response: Option<Value> = None;
    let mut final_status = StatusCode::OK;
    let mut _final_headers = HeaderMap::new();

    for round in 1..=max_rounds {
        let body = serde_json::to_vec(request).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        let (resp_json, status, resp_headers) =
            forward_and_read(state, headers, &body, &upstream_url, format).await?;

        if !status.is_success() {
            // Upstream error — return immediately
            eprintln!(
                "[memoryport-proxy] agentic loop: upstream error (round {}, status {})",
                round, status
            );
            let body_bytes = serde_json::to_vec(&resp_json).unwrap_or_default();
            let mut response = (status, body_bytes).into_response();
            if let Some(ct) = resp_headers.get("content-type") {
                response.headers_mut().insert("content-type", ct.clone());
            }
            return Ok(response);
        }

        match classify_response(&resp_json, format) {
            ResponseAction::Final => {
                eprintln!(
                    "[memoryport-proxy] agentic loop: final response (round {})",
                    round
                );
                final_response = Some(resp_json);
                final_status = status;
                _final_headers = resp_headers;
                break;
            }
            ResponseAction::MemoryportCalls {
                our_calls,
                has_client_calls,
            } => {
                eprintln!(
                    "[memoryport-proxy] agentic loop: round {} — {} memoryport tool calls{}",
                    round,
                    our_calls.len(),
                    if has_client_calls { " (+ client calls)" } else { "" }
                );

                // Execute our tool calls
                let mut results = Vec::with_capacity(our_calls.len());
                for call in &our_calls {
                    debug!(tool = %call.name, "executing memoryport tool");
                    let result = execute_tool(&state.engine, &state.user_id, call).await;
                    eprintln!(
                        "[memoryport-proxy]   {} → {} chars",
                        call.name,
                        result.content.len()
                    );
                    results.push(result);
                }

                // Append this round to conversation history
                append_round_to_messages(
                    request,
                    &resp_json,
                    &results,
                    format,
                    has_client_calls, // strip client calls so the model re-issues them
                );

                // If this was the last allowed round, use the response as-is
                if round == max_rounds {
                    warn!("agentic loop: max rounds ({}) reached", max_rounds);
                    final_response = Some(resp_json);
                    final_status = status;
                    _final_headers = resp_headers;
                }
            }
        }
    }

    // If we somehow have no response (shouldn't happen), error out
    let resp_json = final_response.ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;

    // Store user message + assistant response (same as non-agentic path)
    store_conversation(state, request, &resp_json, format, session_source).await;

    // Strip our injected tools from the request (not needed for response, but clean up)
    strip_injected_tools(request, format);

    // Build the response to return to the client
    let body_bytes = serde_json::to_vec(&resp_json).unwrap_or_default();

    // If client originally requested streaming, wrap as SSE/NDJSON
    let (body_bytes, content_type) = if original_stream {
        wrap_as_stream(&resp_json, format)
    } else {
        (body_bytes.into(), "application/json")
    };

    let mut response = (final_status, body_bytes).into_response();
    response
        .headers_mut()
        .insert("content-type", content_type.parse().unwrap());

    Ok(response)
}

// ---------------------------------------------------------------------------
// Stream wrapping (for clients that requested stream=true)
// ---------------------------------------------------------------------------

fn wrap_as_stream(response: &Value, format: ApiFormat) -> (axum::body::Bytes, &'static str) {
    match format {
        ApiFormat::Anthropic => wrap_anthropic_sse(response),
        ApiFormat::OpenAi => wrap_openai_sse(response),
        ApiFormat::Ollama => wrap_ollama_ndjson(response),
    }
}

fn wrap_anthropic_sse(response: &Value) -> (axum::body::Bytes, &'static str) {
    // Build minimal SSE event sequence that Anthropic clients expect
    let mut sse = String::new();

    // message_start
    let msg_start = serde_json::json!({
        "type": "message_start",
        "message": {
            "id": response.get("id").cloned().unwrap_or(Value::String("msg_agentic".into())),
            "type": "message",
            "role": "assistant",
            "content": [],
            "model": response.get("model").cloned().unwrap_or(Value::String("unknown".into())),
            "stop_reason": null,
            "usage": response.get("usage").cloned().unwrap_or(Value::Null)
        }
    });
    sse.push_str(&format!("event: message_start\ndata: {}\n\n", msg_start));

    // Content blocks
    if let Some(content) = response.get("content").and_then(|c| c.as_array()) {
        for (i, block) in content.iter().enumerate() {
            if block.get("type").and_then(|t| t.as_str()) == Some("text") {
                let text = block.get("text").and_then(|t| t.as_str()).unwrap_or("");

                // content_block_start
                let start = serde_json::json!({
                    "type": "content_block_start",
                    "index": i,
                    "content_block": {"type": "text", "text": ""}
                });
                sse.push_str(&format!("event: content_block_start\ndata: {}\n\n", start));

                // content_block_delta (single delta with all text)
                let delta = serde_json::json!({
                    "type": "content_block_delta",
                    "index": i,
                    "delta": {"type": "text_delta", "text": text}
                });
                sse.push_str(&format!("event: content_block_delta\ndata: {}\n\n", delta));

                // content_block_stop
                let stop = serde_json::json!({"type": "content_block_stop", "index": i});
                sse.push_str(&format!("event: content_block_stop\ndata: {}\n\n", stop));
            }
        }
    }

    // message_delta + message_stop
    let msg_delta = serde_json::json!({
        "type": "message_delta",
        "delta": {"stop_reason": response.get("stop_reason").cloned().unwrap_or(Value::String("end_turn".into()))},
        "usage": response.get("usage").cloned().unwrap_or(Value::Null)
    });
    sse.push_str(&format!("event: message_delta\ndata: {}\n\n", msg_delta));
    sse.push_str("event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n");

    (sse.into(), "text/event-stream")
}

fn wrap_openai_sse(response: &Value) -> (axum::body::Bytes, &'static str) {
    // Extract the text content
    let text = response
        .get("choices")
        .and_then(|c| c.get(0))
        .and_then(|c| c.get("message"))
        .and_then(|m| m.get("content"))
        .and_then(|c| c.as_str())
        .unwrap_or("");

    let model = response
        .get("model")
        .and_then(|m| m.as_str())
        .unwrap_or("unknown");
    let id = response
        .get("id")
        .and_then(|i| i.as_str())
        .unwrap_or("chatcmpl-agentic");

    let chunk = serde_json::json!({
        "id": id,
        "object": "chat.completion.chunk",
        "model": model,
        "choices": [{
            "index": 0,
            "delta": {"role": "assistant", "content": text},
            "finish_reason": "stop"
        }]
    });

    let mut sse = format!("data: {}\n\n", chunk);
    sse.push_str("data: [DONE]\n\n");

    (sse.into(), "text/event-stream")
}

fn wrap_ollama_ndjson(response: &Value) -> (axum::body::Bytes, &'static str) {
    // Return the response as a single NDJSON line with done=true
    let mut obj = response.clone();
    obj.as_object_mut()
        .unwrap()
        .insert("done".into(), Value::Bool(true));
    let line = format!("{}\n", serde_json::to_string(&obj).unwrap_or_default());
    (line.into(), "application/x-ndjson")
}

// ---------------------------------------------------------------------------
// Conversation storage (reuses existing patterns)
// ---------------------------------------------------------------------------

async fn store_conversation(
    state: &Arc<ProxyState>,
    request: &Value,
    response: &Value,
    format: ApiFormat,
    session_source: &str,
) {
    let user_msg = extract_original_user_text(request);
    let assistant_text = extract_final_assistant_text(response, format);
    let model = extract_model(request, response, format);
    let source_integration = match format {
        ApiFormat::Ollama => "proxy-ollama",
        _ => "proxy",
    };

    let user_msg = crate::anthropic::sanitize_for_storage_pub(&user_msg);
    let assistant_text = crate::anthropic::sanitize_for_storage_pub(&assistant_text);

    // Store user message
    if user_msg.len() >= 10 {
        let engine = state.engine.clone();
        let uid = state.user_id.clone();
        let sid = state.sessions.get_session(session_source).await;
        let m = model.clone();
        let si = source_integration.to_string();
        tokio::spawn(async move {
            let params = uc_core::models::StoreParams {
                user_id: uid,
                session_id: sid,
                chunk_type: uc_core::models::ChunkType::Conversation,
                role: Some(uc_core::models::Role::User),
                source_integration: Some(si),
                source_model: Some(m),
            };
            let _ = engine.store(&user_msg, params).await;
            let _ = engine.flush().await;
        });
    }

    // Store assistant response
    if assistant_text.len() >= 10 {
        let engine = state.engine.clone();
        let uid = state.user_id.clone();
        let sid = state.sessions.get_session(session_source).await;
        let si = source_integration.to_string();
        tokio::spawn(async move {
            let params = uc_core::models::StoreParams {
                user_id: uid,
                session_id: sid,
                chunk_type: uc_core::models::ChunkType::Conversation,
                role: Some(uc_core::models::Role::Assistant),
                source_integration: Some(si),
                source_model: Some(model),
            };
            let _ = engine.store(&assistant_text, params).await;
            let _ = engine.flush().await;
        });
    }
}

/// Extract the original user text (first user message, before tool rounds were appended).
fn extract_original_user_text(request: &Value) -> String {
    let messages = match request.get("messages").and_then(|m| m.as_array()) {
        Some(m) => m,
        None => return String::new(),
    };

    // Find the last user message that is plain text (not tool_result)
    for msg in messages.iter().rev() {
        if msg.get("role").and_then(|r| r.as_str()) != Some("user") {
            continue;
        }
        // Skip tool_result messages (Anthropic format)
        if let Some(content) = msg.get("content").and_then(|c| c.as_array()) {
            if content
                .iter()
                .any(|b| b.get("type").and_then(|t| t.as_str()) == Some("tool_result"))
            {
                continue;
            }
        }
        // String content
        if let Some(s) = msg.get("content").and_then(|c| c.as_str()) {
            return s.to_string();
        }
        // Array content blocks — extract text
        if let Some(blocks) = msg.get("content").and_then(|c| c.as_array()) {
            let texts: Vec<&str> = blocks
                .iter()
                .filter_map(|b| {
                    if b.get("type").and_then(|t| t.as_str()) == Some("text") {
                        b.get("text").and_then(|t| t.as_str())
                    } else {
                        None
                    }
                })
                .collect();
            if !texts.is_empty() {
                return texts.join("\n");
            }
        }
    }
    String::new()
}

fn extract_final_assistant_text(response: &Value, format: ApiFormat) -> String {
    match format {
        ApiFormat::Anthropic => {
            response
                .get("content")
                .and_then(|c| c.as_array())
                .map(|blocks| {
                    blocks
                        .iter()
                        .filter_map(|b| {
                            if b.get("type").and_then(|t| t.as_str()) == Some("text") {
                                b.get("text").and_then(|t| t.as_str())
                            } else {
                                None
                            }
                        })
                        .collect::<Vec<_>>()
                        .join("\n")
                })
                .unwrap_or_default()
        }
        ApiFormat::OpenAi => response
            .get("choices")
            .and_then(|c| c.get(0))
            .and_then(|c| c.get("message"))
            .and_then(|m| m.get("content"))
            .and_then(|c| c.as_str())
            .unwrap_or("")
            .to_string(),
        ApiFormat::Ollama => response
            .get("message")
            .and_then(|m| m.get("content"))
            .and_then(|c| c.as_str())
            .unwrap_or("")
            .to_string(),
    }
}

fn extract_model(request: &Value, response: &Value, format: ApiFormat) -> String {
    // Prefer model from response, fall back to request
    let from_response = match format {
        ApiFormat::Anthropic | ApiFormat::OpenAi => {
            response.get("model").and_then(|m| m.as_str())
        }
        ApiFormat::Ollama => response.get("model").and_then(|m| m.as_str()),
    };
    from_response
        .or_else(|| request.get("model").and_then(|m| m.as_str()))
        .unwrap_or("unknown")
        .to_string()
}

// ---------------------------------------------------------------------------
// Per-request disable check
// ---------------------------------------------------------------------------

pub fn is_disabled_by_header(headers: &HeaderMap) -> bool {
    headers
        .get("x-memoryport-agentic")
        .and_then(|v| v.to_str().ok())
        .map(|v| v.eq_ignore_ascii_case("false") || v == "0")
        .unwrap_or(false)
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn truncate(s: &str, max_chars: usize) -> String {
    if s.len() <= max_chars {
        s.to_string()
    } else {
        format!(
            "{}\n\n[Truncated — showing first {} of {} characters]",
            &s[..max_chars],
            max_chars,
            s.len()
        )
    }
}

fn format_timestamp(ts: i64) -> String {
    chrono::DateTime::from_timestamp(ts, 0)
        .map(|dt| dt.format("%Y-%m-%d %H:%M").to_string())
        .unwrap_or_else(|| ts.to_string())
}

fn format_timestamp_millis(ts: i64) -> String {
    format_timestamp(ts / 1000)
}
