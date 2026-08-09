//! image_to_icns MCP Server
//!
//! Exposes session lifecycle tools and calls the optional Cloudflare Session
//! Worker via HTTP.

use clap::Parser;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use std::io::{self, BufRead, Read, Write};
use std::time::Duration;

const MAX_STDIO_MESSAGE_BYTES: usize = 1024 * 1024;
const MAX_WORKER_RESPONSE_BYTES: usize = 1024 * 1024;
const WORKER_CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const WORKER_REQUEST_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Parser)]
#[command(name = "image_to_icns_mcp", about = "MCP Server for ICNS conversion")]
struct Cli {
    #[arg(
        long,
        default_value = "http://localhost:8787",
        value_parser = parse_worker_url
    )]
    worker_url: String,
}

#[derive(Deserialize)]
struct McpRequest {
    jsonrpc: String,
    id: Option<serde_json::Value>,
    method: String,
    params: Option<serde_json::Value>,
}

#[derive(Serialize)]
struct McpResponse {
    jsonrpc: String,
    id: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<McpError>,
}

#[derive(Serialize)]
struct McpError {
    code: i32,
    message: String,
}

const TOOL_CATALOG_JSON: &str = include_str!("../../../skills/mcp-tools.json");

fn parse_worker_url(value: &str) -> Result<String, String> {
    let url = reqwest::Url::parse(value).map_err(|error| format!("invalid Worker URL: {error}"))?;
    let hostname = url
        .host_str()
        .ok_or_else(|| "Worker URL must include a hostname".to_owned())?;
    let is_local = matches!(hostname, "localhost" | "127.0.0.1" | "::1");
    if url.scheme() != "https" && !(url.scheme() == "http" && is_local) {
        return Err("Worker URL must use HTTPS (or HTTP on localhost)".to_owned());
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err("Worker URL must not contain credentials".to_owned());
    }
    if url.query().is_some() || url.fragment().is_some() {
        return Err("Worker URL must not contain a query or fragment".to_owned());
    }
    if !matches!(url.path(), "" | "/") {
        return Err("Worker URL must be an origin without a path".to_owned());
    }
    Ok(url.origin().ascii_serialization())
}

fn listed_tools() -> serde_json::Value {
    let catalog: serde_json::Value =
        serde_json::from_str(TOOL_CATALOG_JSON).expect("embedded MCP tool catalog must be valid");
    catalog["tools"].clone()
}

fn make_response(id: Option<serde_json::Value>, result: serde_json::Value) -> McpResponse {
    McpResponse {
        jsonrpc: "2.0".into(),
        id,
        result: Some(result),
        error: None,
    }
}

fn make_error(id: Option<serde_json::Value>, code: i32, message: &str) -> McpResponse {
    McpResponse {
        jsonrpc: "2.0".into(),
        id,
        result: None,
        error: Some(McpError {
            code,
            message: message.into(),
        }),
    }
}

fn build_worker_client(connect_timeout: Duration, request_timeout: Duration) -> reqwest::Client {
    reqwest::Client::builder()
        .connect_timeout(connect_timeout)
        .timeout(request_timeout)
        .build()
        .expect("build Worker HTTP client")
}

#[derive(Debug, Eq, PartialEq)]
enum BoundedLine {
    Message(Vec<u8>),
    TooLarge,
    Eof,
}

fn discard_to_line_end<R: BufRead>(reader: &mut R) -> io::Result<()> {
    loop {
        let buffer = reader.fill_buf()?;
        if buffer.is_empty() {
            return Ok(());
        }
        let (consumed, found_newline) = match buffer.iter().position(|byte| *byte == b'\n') {
            Some(index) => (index + 1, true),
            None => (buffer.len(), false),
        };
        reader.consume(consumed);
        if found_newline {
            return Ok(());
        }
    }
}

fn read_bounded_line<R: BufRead>(reader: &mut R, max_bytes: usize) -> io::Result<BoundedLine> {
    let mut message = Vec::with_capacity(max_bytes.min(8 * 1024));
    let read_limit = (max_bytes as u64).saturating_add(2);
    reader.take(read_limit).read_until(b'\n', &mut message)?;

    if message.is_empty() {
        return Ok(BoundedLine::Eof);
    }

    let ended_with_newline = message.last() == Some(&b'\n');
    if ended_with_newline {
        message.pop();
        if message.last() == Some(&b'\r') {
            message.pop();
        }
    }
    if message.len() <= max_bytes {
        return Ok(BoundedLine::Message(message));
    }

    if !ended_with_newline {
        discard_to_line_end(reader)?;
    }
    Ok(BoundedLine::TooLarge)
}

async fn serve_stdio<R: BufRead, W: Write>(
    reader: &mut R,
    writer: &mut W,
    worker_client: &reqwest::Client,
    worker_url: &str,
) -> io::Result<()> {
    loop {
        let response = match read_bounded_line(reader, MAX_STDIO_MESSAGE_BYTES)? {
            BoundedLine::Eof => return Ok(()),
            BoundedLine::TooLarge => serde_json::to_string(&make_error(
                None,
                -32700,
                "Parse error: message exceeds 1048576-byte limit",
            ))
            .expect("serialize fixed MCP size error"),
            BoundedLine::Message(message) => {
                let message = match std::str::from_utf8(&message) {
                    Ok(message) => message,
                    Err(_) => {
                        let response = serde_json::to_string(&make_error(
                            None,
                            -32700,
                            "Parse error: message is not valid UTF-8",
                        ))
                        .expect("serialize fixed MCP encoding error");
                        writeln!(writer, "{response}")?;
                        writer.flush()?;
                        continue;
                    }
                };
                if message.trim().is_empty() {
                    continue;
                }
                handle_message(message, worker_client, worker_url).await
            }
        };

        if response.is_empty() {
            continue;
        }
        writeln!(writer, "{response}")?;
        writer.flush()?;
    }
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    let worker_client = build_worker_client(WORKER_CONNECT_TIMEOUT, WORKER_REQUEST_TIMEOUT);
    eprintln!(
        "image_to_icns MCP Server starting (stdio mode, worker={})",
        cli.worker_url
    );

    let stdin = io::stdin();
    let mut stdin = stdin.lock();
    let mut stdout = io::stdout();
    serve_stdio(&mut stdin, &mut stdout, &worker_client, &cli.worker_url)
        .await
        .expect("serve MCP stdio");
}

async fn handle_message(msg: &str, worker_client: &reqwest::Client, worker_url: &str) -> String {
    let value: serde_json::Value = match serde_json::from_str(msg) {
        Ok(value) => value,
        Err(e) => {
            return serde_json::to_string(&make_error(None, -32700, &format!("Parse error: {e}")))
                .unwrap();
        }
    };
    let response_id = value
        .get("id")
        .filter(|id| id.is_string() || id.is_number())
        .cloned();
    if value
        .get("id")
        .is_some_and(|id| !id.is_string() && !id.is_number())
    {
        return serde_json::to_string(&make_error(None, -32600, "Invalid Request: invalid id"))
            .unwrap();
    }
    let req: McpRequest = match serde_json::from_value(value) {
        Ok(request) => request,
        Err(error) => {
            return serde_json::to_string(&make_error(
                response_id,
                -32600,
                &format!("Invalid Request: {error}"),
            ))
            .unwrap();
        }
    };

    if req.jsonrpc != "2.0" {
        return serde_json::to_string(&make_error(
            req.id,
            -32600,
            "Invalid Request: jsonrpc must be 2.0",
        ))
        .unwrap();
    }

    if req.id.is_none() {
        return String::new();
    }

    let resp = match req.method.as_str() {
        "initialize" => make_response(
            req.id,
            serde_json::json!({
                "protocolVersion": "2024-11-05",
                "capabilities": { "tools": {} },
                "serverInfo": {
                    "name": "image_to_icns",
                    "version": "0.1.0"
                }
            }),
        ),
        "ping" => make_response(req.id, serde_json::json!({})),
        "tools/list" => make_response(
            req.id,
            serde_json::json!({
                "tools": listed_tools()
            }),
        ),
        "tools/call" => {
            let params = match req.params.as_ref().and_then(serde_json::Value::as_object) {
                Some(params) => params,
                None => {
                    return serde_json::to_string(&make_error(
                        req.id,
                        -32602,
                        "tools/call params must be an object",
                    ))
                    .unwrap();
                }
            };
            let tool_name = match params.get("name").and_then(serde_json::Value::as_str) {
                Some(name) if !name.is_empty() => name,
                _ => {
                    return serde_json::to_string(&make_error(req.id, -32602, "Missing tool name"))
                        .unwrap();
                }
            };
            let args = params
                .get("arguments")
                .cloned()
                .unwrap_or(serde_json::json!({}));
            if !args.is_object() {
                return serde_json::to_string(&make_error(
                    req.id,
                    -32602,
                    "Tool arguments must be an object",
                ))
                .unwrap();
            }

            match tool_name {
                "create_icns_session" => {
                    call_create_session(req.id, worker_client, worker_url, &args).await
                }
                "query_icns_session" => {
                    call_query_session(req.id, worker_client, worker_url, &args).await
                }
                "cancel_icns_session" => {
                    call_cancel_session(req.id, worker_client, worker_url, &args).await
                }
                _ => make_error(req.id, -32602, &format!("Unknown tool: {tool_name}")),
            }
        }
        _ => make_error(req.id, -32601, &format!("Unknown method: {}", req.method)),
    };

    serde_json::to_string(&resp).unwrap()
}

fn make_tool_result(
    id: Option<serde_json::Value>,
    text: impl Into<String>,
    is_error: bool,
) -> McpResponse {
    let mut result = serde_json::json!({
        "content": [{
            "type": "text",
            "text": text.into()
        }]
    });
    if is_error {
        result["isError"] = serde_json::Value::Bool(true);
    }
    make_response(id, result)
}

fn is_hex_identifier(value: &str, expected_len: usize) -> bool {
    value.len() == expected_len && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

async fn send_worker_request(
    request: reqwest::RequestBuilder,
    operation: &str,
) -> Result<serde_json::Value, String> {
    let response = request.send().await.map_err(|error| {
        if error.is_timeout() {
            format!("Worker request timed out during {operation}")
        } else {
            format!("Worker unreachable during {operation}")
        }
    })?;
    let status = response.status();
    if response
        .content_length()
        .is_some_and(|length| length > MAX_WORKER_RESPONSE_BYTES as u64)
    {
        return Err(format!(
            "Worker response exceeded {MAX_WORKER_RESPONSE_BYTES}-byte limit during {operation}"
        ));
    }

    let mut body = Vec::with_capacity(
        response
            .content_length()
            .unwrap_or(0)
            .min(MAX_WORKER_RESPONSE_BYTES as u64) as usize,
    );
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|error| {
            if error.is_timeout() {
                format!("Worker request timed out during {operation}")
            } else {
                format!("Worker response unreadable during {operation}")
            }
        })?;
        if chunk.len() > MAX_WORKER_RESPONSE_BYTES - body.len() {
            return Err(format!(
                "Worker response exceeded {MAX_WORKER_RESPONSE_BYTES}-byte limit during {operation}"
            ));
        }
        body.extend_from_slice(&chunk);
    }

    if !status.is_success() {
        return Err(format!("Worker returned {status} during {operation}"));
    }

    serde_json::from_slice(&body)
        .map_err(|_| format!("Worker returned invalid JSON during {operation}"))
}

async fn call_create_session(
    id: Option<serde_json::Value>,
    worker_client: &reqwest::Client,
    worker_url: &str,
    args: &serde_json::Value,
) -> McpResponse {
    let source_format = match args.get("source_format") {
        None => None,
        Some(value) => match value.as_str() {
            Some(format @ ("png" | "jpeg" | "svg")) => Some(format),
            _ => return make_error(id, -32602, "Invalid source_format"),
        },
    };
    let body = serde_json::json!({
        "source_format": source_format
    });

    let request = worker_client
        .post(format!("{worker_url}/sessions"))
        .json(&body);
    match send_worker_request(request, "session creation").await {
        Ok(session) => make_tool_result(id, session.to_string(), false),
        Err(error) => make_tool_result(id, error, true),
    }
}

async fn call_query_session(
    id: Option<serde_json::Value>,
    worker_client: &reqwest::Client,
    worker_url: &str,
    args: &serde_json::Value,
) -> McpResponse {
    let session_id = match args.get("session_id").and_then(|v| v.as_str()) {
        Some(session_id) if is_hex_identifier(session_id, 64) => session_id,
        None => {
            return make_error(id, -32602, "Missing session_id");
        }
        Some(_) => return make_error(id, -32602, "Invalid session_id"),
    };

    let request = worker_client.get(format!("{worker_url}/sessions/{session_id}"));
    match send_worker_request(request, "session query").await {
        Ok(session) => make_tool_result(id, session.to_string(), false),
        Err(error) => make_tool_result(id, error, true),
    }
}

async fn call_cancel_session(
    id: Option<serde_json::Value>,
    worker_client: &reqwest::Client,
    worker_url: &str,
    args: &serde_json::Value,
) -> McpResponse {
    let session_id = match args.get("session_id").and_then(|v| v.as_str()) {
        Some(session_id) if is_hex_identifier(session_id, 64) => session_id,
        None => {
            return make_error(id, -32602, "Missing session_id");
        }
        Some(_) => return make_error(id, -32602, "Invalid session_id"),
    };
    let secret = match args.get("secret").and_then(|v| v.as_str()) {
        Some(secret) if is_hex_identifier(secret, 128) => secret,
        None => {
            return make_error(id, -32602, "Missing secret");
        }
        Some(_) => return make_error(id, -32602, "Invalid secret"),
    };

    let request = worker_client
        .delete(format!("{worker_url}/sessions/{session_id}"))
        .json(&serde_json::json!({ "secret": secret }));
    match send_worker_request(request, "session cancellation").await {
        Ok(session) => make_tool_result(id, session.to_string(), false),
        Err(error) => make_tool_result(id, error, true),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    async fn serve_once(status: &str, body: &str) -> (String, tokio::task::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let status = status.to_owned();
        let body = body.to_owned();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut request = vec![0; 8 * 1024];
            let _ = stream.read(&mut request).await.unwrap();
            let response = format!(
                "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(response.as_bytes()).await.unwrap();
        });
        (format!("http://{address}"), server)
    }

    fn parse_response(response: &str) -> serde_json::Value {
        serde_json::from_str(response).unwrap()
    }

    fn test_worker_client() -> reqwest::Client {
        build_worker_client(Duration::from_millis(50), Duration::from_millis(100))
    }

    async fn serve_stalled_request() -> (String, tokio::task::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut request = vec![0; 8 * 1024];
            let _ = stream.read(&mut request).await.unwrap();
            std::future::pending::<()>().await;
        });
        (format!("http://{address}"), server)
    }

    async fn serve_chunked(
        status: &str,
        chunks: Vec<Vec<u8>>,
    ) -> (String, tokio::task::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let status = status.to_owned();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut request = vec![0; 8 * 1024];
            let _ = stream.read(&mut request).await.unwrap();
            let headers = format!(
                "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n"
            );
            if stream.write_all(headers.as_bytes()).await.is_err() {
                return;
            }
            for chunk in chunks {
                let header = format!("{:x}\r\n", chunk.len());
                if stream.write_all(header.as_bytes()).await.is_err()
                    || stream.write_all(&chunk).await.is_err()
                    || stream.write_all(b"\r\n").await.is_err()
                {
                    return;
                }
            }
            let _ = stream.write_all(b"0\r\n\r\n").await;
        });
        (format!("http://{address}"), server)
    }

    async fn serve_oversized_content_length() -> (String, tokio::task::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut request = vec![0; 8 * 1024];
            let _ = stream.read(&mut request).await.unwrap();
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                MAX_WORKER_RESPONSE_BYTES + 1
            );
            stream.write_all(response.as_bytes()).await.unwrap();
            std::future::pending::<()>().await;
        });
        (format!("http://{address}"), server)
    }

    #[tokio::test]
    async fn oversized_stdio_message_is_rejected_and_eof_ping_still_runs() {
        let mut input = vec![b'x'; MAX_STDIO_MESSAGE_BYTES + 1];
        input.push(b'\n');
        input.extend_from_slice(br#"{"jsonrpc":"2.0","id":"after-oversize","method":"ping"}"#);
        let mut reader = Cursor::new(input);
        let mut output = Vec::new();
        let worker_client = test_worker_client();

        serve_stdio(
            &mut reader,
            &mut output,
            &worker_client,
            "http://127.0.0.1:1",
        )
        .await
        .unwrap();

        let responses = String::from_utf8(output).unwrap();
        let responses = responses.lines().map(parse_response).collect::<Vec<_>>();
        assert_eq!(responses.len(), 2);
        assert_eq!(responses[0]["error"]["code"], -32700);
        assert_eq!(
            responses[0]["error"]["message"],
            "Parse error: message exceeds 1048576-byte limit"
        );
        assert_eq!(responses[1]["id"], "after-oversize");
        assert_eq!(responses[1]["result"], serde_json::json!({}));
    }

    #[test]
    fn bounded_line_reader_accepts_exact_limit_without_newline() {
        let input = vec![b'x'; MAX_STDIO_MESSAGE_BYTES];
        let mut reader = Cursor::new(input.clone());

        assert_eq!(
            read_bounded_line(&mut reader, MAX_STDIO_MESSAGE_BYTES).unwrap(),
            BoundedLine::Message(input)
        );
        assert_eq!(
            read_bounded_line(&mut reader, MAX_STDIO_MESSAGE_BYTES).unwrap(),
            BoundedLine::Eof
        );
    }

    #[test]
    fn bounded_line_reader_excludes_crlf_from_the_limit() {
        let expected = vec![b'x'; MAX_STDIO_MESSAGE_BYTES];
        let mut input = expected.clone();
        input.extend_from_slice(b"\r\n");
        let mut reader = Cursor::new(input);

        assert_eq!(
            read_bounded_line(&mut reader, MAX_STDIO_MESSAGE_BYTES).unwrap(),
            BoundedLine::Message(expected)
        );
        assert_eq!(
            read_bounded_line(&mut reader, MAX_STDIO_MESSAGE_BYTES).unwrap(),
            BoundedLine::Eof
        );
    }

    #[tokio::test]
    async fn chunked_worker_response_over_limit_returns_tool_error_before_later_ping() {
        let chunks = vec![
            vec![b'x'; MAX_WORKER_RESPONSE_BYTES / 2],
            vec![b'y'; MAX_WORKER_RESPONSE_BYTES / 2],
            vec![b'z'; 1],
        ];
        let (worker_url, server) = serve_chunked("200 OK", chunks).await;
        let worker_client = test_worker_client();
        let response = handle_message(
            r#"{"jsonrpc":"2.0","id":"large","method":"tools/call","params":{"name":"create_icns_session","arguments":{}}}"#,
            &worker_client,
            &worker_url,
        )
        .await;
        server.await.unwrap();
        let response = parse_response(&response);

        assert_eq!(response["result"]["isError"], true);
        assert_eq!(
            response["result"]["content"][0]["text"],
            "Worker response exceeded 1048576-byte limit during session creation"
        );

        let ping_response = handle_message(
            r#"{"jsonrpc":"2.0","id":"after-large","method":"ping"}"#,
            &worker_client,
            &worker_url,
        )
        .await;
        assert_eq!(parse_response(&ping_response)["id"], "after-large");
    }

    #[tokio::test]
    async fn chunked_worker_error_response_over_limit_is_bounded_without_body_leak() {
        let chunks = vec![
            vec![b's'; MAX_WORKER_RESPONSE_BYTES],
            b"secret-upstream-body".to_vec(),
        ];
        let (worker_url, server) = serve_chunked("500 Internal Server Error", chunks).await;
        let worker_client = test_worker_client();
        let session_id = "a".repeat(64);
        let response = handle_message(
            &format!(
                r#"{{"jsonrpc":"2.0","id":"large-error","method":"tools/call","params":{{"name":"query_icns_session","arguments":{{"session_id":"{session_id}"}}}}}}"#
            ),
            &worker_client,
            &worker_url,
        )
        .await;
        server.await.unwrap();
        let response = parse_response(&response);
        let message = response["result"]["content"][0]["text"].as_str().unwrap();

        assert_eq!(response["result"]["isError"], true);
        assert_eq!(
            message,
            "Worker response exceeded 1048576-byte limit during session query"
        );
        assert!(!message.contains("secret-upstream-body"));
    }

    #[tokio::test]
    async fn oversized_content_length_is_rejected_before_body_read() {
        let (worker_url, server) = serve_oversized_content_length().await;
        let worker_client = test_worker_client();
        let response = handle_message(
            r#"{"jsonrpc":"2.0","id":"declared-large","method":"tools/call","params":{"name":"create_icns_session","arguments":{}}}"#,
            &worker_client,
            &worker_url,
        )
        .await;
        server.abort();
        let response = parse_response(&response);

        assert_eq!(response["result"]["isError"], true);
        assert_eq!(
            response["result"]["content"][0]["text"],
            "Worker response exceeded 1048576-byte limit during session creation"
        );
    }

    #[tokio::test]
    async fn stalled_worker_request_returns_a_tool_error_before_later_ping() {
        let (worker_url, server) = serve_stalled_request().await;
        let worker_client = test_worker_client();
        let tool_response = tokio::time::timeout(
            Duration::from_secs(1),
            handle_message(
                r#"{"jsonrpc":"2.0","id":"stalled","method":"tools/call","params":{"name":"create_icns_session","arguments":{}}}"#,
                &worker_client,
                &worker_url,
            ),
        )
        .await;
        server.abort();

        assert!(
            tool_response.is_ok(),
            "stalled Worker request blocked the MCP message loop"
        );
        let tool_response = parse_response(&tool_response.unwrap());
        assert_eq!(tool_response["id"], "stalled");
        assert_eq!(tool_response["result"]["isError"], true);
        assert_eq!(
            tool_response["result"]["content"][0]["text"],
            "Worker request timed out during session creation"
        );
        assert!(tool_response["error"].is_null());

        let ping_response = handle_message(
            r#"{"jsonrpc":"2.0","id":"after-timeout","method":"ping"}"#,
            &worker_client,
            &worker_url,
        )
        .await;
        let ping_response = parse_response(&ping_response);
        assert_eq!(ping_response["id"], "after-timeout");
        assert_eq!(ping_response["result"], serde_json::json!({}));
    }

    #[tokio::test]
    async fn tool_call_echoes_the_request_id() {
        let (worker_url, server) =
            serve_once("200 OK", r#"{"session_id":"session-1","state":"created"}"#).await;
        let worker_client = test_worker_client();
        let response = handle_message(
            r#"{"jsonrpc":"2.0","id":"call-42","method":"tools/call","params":{"name":"create_icns_session","arguments":{}}}"#,
            &worker_client,
            &worker_url,
        )
        .await;
        server.await.unwrap();

        assert_eq!(parse_response(&response)["id"], "call-42");
    }

    #[tokio::test]
    async fn worker_http_errors_are_returned_as_tool_errors() {
        let (worker_url, server) = serve_once(
            "404 Not Found",
            r#"{"error":"private upstream details: token=secret"}"#,
        )
        .await;
        let worker_client = test_worker_client();
        let session_id = "a".repeat(64);
        let response = handle_message(
            &format!(
                r#"{{"jsonrpc":"2.0","id":7,"method":"tools/call","params":{{"name":"query_icns_session","arguments":{{"session_id":"{session_id}"}}}}}}"#
            ),
            &worker_client,
            &worker_url,
        )
        .await;
        server.await.unwrap();
        let response = parse_response(&response);

        assert_eq!(response["id"], 7);
        assert_eq!(response["result"]["isError"], true);
        assert!(response["error"].is_null());
        assert_eq!(
            response["result"]["content"][0]["text"],
            "Worker returned 404 Not Found during session query"
        );
    }

    #[tokio::test]
    async fn notifications_do_not_receive_responses() {
        let worker_client = test_worker_client();
        let response = handle_message(
            r#"{"jsonrpc":"2.0","method":"notifications/example","params":{}}"#,
            &worker_client,
            "http://127.0.0.1:1",
        )
        .await;

        assert!(response.is_empty());
    }

    #[tokio::test]
    async fn ping_returns_an_empty_result_with_the_request_id() {
        let worker_client = test_worker_client();
        let response = handle_message(
            r#"{"jsonrpc":"2.0","id":"ping-1","method":"ping"}"#,
            &worker_client,
            "http://127.0.0.1:1",
        )
        .await;
        let response = parse_response(&response);

        assert_eq!(response["id"], "ping-1");
        assert_eq!(response["result"], serde_json::json!({}));
        assert!(response["error"].is_null());
    }

    #[tokio::test]
    async fn tools_list_contains_only_implemented_local_session_tools() {
        let worker_client = test_worker_client();
        let response = handle_message(
            r#"{"jsonrpc":"2.0","id":8,"method":"tools/list"}"#,
            &worker_client,
            "http://127.0.0.1:1",
        )
        .await;
        let response = parse_response(&response);
        let tools = response["result"]["tools"].as_array().unwrap();
        let names = tools
            .iter()
            .map(|tool| tool["name"].as_str().unwrap())
            .collect::<Vec<_>>();

        assert_eq!(
            names,
            vec![
                "create_icns_session",
                "query_icns_session",
                "cancel_icns_session"
            ]
        );
        assert!(tools[0]["inputSchema"]["properties"].get("mode").is_none());
        let catalog: serde_json::Value =
            serde_json::from_str(include_str!("../../../skills/mcp-tools.json")).unwrap();
        assert_eq!(response["result"]["tools"], catalog["tools"]);
    }

    #[tokio::test]
    async fn valid_json_without_a_method_is_an_invalid_request() {
        let worker_client = test_worker_client();
        let response = handle_message(
            r#"{"jsonrpc":"2.0","id":9}"#,
            &worker_client,
            "http://127.0.0.1:1",
        )
        .await;
        let response = parse_response(&response);

        assert_eq!(response["id"], 9);
        assert_eq!(response["error"]["code"], -32600);
    }

    #[tokio::test]
    async fn tool_arguments_must_be_an_object() {
        let worker_client = test_worker_client();
        let response = handle_message(
            r#"{"jsonrpc":"2.0","id":10,"method":"tools/call","params":{"name":"create_icns_session","arguments":"invalid"}}"#,
            &worker_client,
            "http://127.0.0.1:1",
        )
        .await;
        let response = parse_response(&response);

        assert_eq!(response["id"], 10);
        assert_eq!(response["error"]["code"], -32602);
    }

    #[tokio::test]
    async fn invalid_session_ids_are_rejected_before_http_requests() {
        let worker_client = test_worker_client();
        let response = handle_message(
            r#"{"jsonrpc":"2.0","id":11,"method":"tools/call","params":{"name":"query_icns_session","arguments":{"session_id":"../sessions"}}}"#,
            &worker_client,
            "http://127.0.0.1:1",
        )
        .await;
        let response = parse_response(&response);

        assert_eq!(response["id"], 11);
        assert_eq!(response["error"]["code"], -32602);
    }

    #[test]
    fn worker_urls_require_a_safe_origin() {
        assert_eq!(
            parse_worker_url("https://worker.example.com/").unwrap(),
            "https://worker.example.com"
        );
        assert_eq!(
            parse_worker_url("http://127.0.0.1:8787").unwrap(),
            "http://127.0.0.1:8787"
        );
        assert!(parse_worker_url("http://worker.example.com").is_err());
        assert!(parse_worker_url("https://user:secret@worker.example.com").is_err());
        assert!(parse_worker_url("https://worker.example.com?token=secret").is_err());
        assert!(parse_worker_url("https://worker.example.com/api").is_err());
    }
}
