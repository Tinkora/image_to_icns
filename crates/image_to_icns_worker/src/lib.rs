//! image_to_icns Cloudflare Session Worker
//!
//! Lightweight HTTP API: create Sessions, query status, return editor URLs.
//! Does not receive source images, decode images, or generate ICNS.
//!
//! Security features:
//! - Rate limiting (IP-based + D1 counter)
//! - CSP / security response headers
//! - Expired session cleanup (Cron Trigger)

use futures_util::StreamExt;
use serde::de::DeserializeOwned;
use worker::*;

mod session;

use session::{SessionMutationError, SessionStore, is_valid_session_id, is_valid_session_secret};

/// Maximum requests per IP per minute.
const RATE_LIMIT_MAX: u32 = 30;
/// Rate limit window in seconds.
const RATE_LIMIT_WINDOW: u32 = 60;
/// Maximum accepted JSON request body size.
const MAX_JSON_BODY_BYTES: usize = 8 * 1024;

#[derive(Debug, PartialEq, Eq)]
enum JsonBodyError {
    TooLarge,
    InvalidJson,
}

struct BoundedBody {
    bytes: Vec<u8>,
    limit: usize,
}

impl BoundedBody {
    fn new(limit: usize) -> Self {
        Self {
            bytes: Vec::new(),
            limit,
        }
    }

    fn push(&mut self, chunk: &[u8]) -> std::result::Result<(), JsonBodyError> {
        if self
            .bytes
            .len()
            .checked_add(chunk.len())
            .is_none_or(|length| length > self.limit)
        {
            return Err(JsonBodyError::TooLarge);
        }
        self.bytes.extend_from_slice(chunk);
        Ok(())
    }

    fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }
}

async fn read_json_body<T: DeserializeOwned>(
    req: &mut Request,
) -> std::result::Result<T, JsonBodyError> {
    let mut stream = req.stream().map_err(|_| JsonBodyError::InvalidJson)?;
    let mut body = BoundedBody::new(MAX_JSON_BODY_BYTES);

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|_| JsonBodyError::InvalidJson)?;
        body.push(&chunk)?;
    }

    parse_json_body(body.as_bytes())
}

fn parse_json_body<T: DeserializeOwned>(bytes: &[u8]) -> std::result::Result<T, JsonBodyError> {
    serde_json::from_slice(bytes).map_err(|_| JsonBodyError::InvalidJson)
}

/// Worker entry: route HTTP requests.
#[event(fetch)]
pub async fn main(req: Request, env: Env, _ctx: Context) -> Result<Response> {
    // CORS preflight (no rate limiting needed)
    if req.method() == Method::Options {
        return cors_response();
    }

    match check_rate_limit(&req, &env).await {
        Ok(Some(count)) => {
            let mut response = api_error(
                "RATE_LIMITED",
                format!("Rate limit exceeded: {count} requests in {RATE_LIMIT_WINDOW}s"),
                429,
            )?;
            add_response_headers(&mut response)?;
            return Ok(response);
        }
        Ok(None) => {}
        Err(error) => {
            console_error!("Rate limit check failed: {error}");
            let mut response = api_error(
                "RATE_LIMIT_UNAVAILABLE",
                "Rate limit service unavailable",
                503,
            )?;
            add_response_headers(&mut response)?;
            return Ok(response);
        }
    }

    let router = Router::new();

    let mut resp = router
        .post_async("/sessions", |mut req, ctx| async move {
            let worker_base_url = req.url()?.origin().ascii_serialization();
            let body: session::CreateRequest = match read_json_body(&mut req).await {
                Ok(body) => body,
                Err(error) => return json_body_error_response(error),
            };
            if let Err(message) = body.validate() {
                return api_error("INVALID_REQUEST", message, 400);
            }
            let store = match SessionStore::new(&ctx.env) {
                Ok(store) => store,
                Err(error) => return storage_error_response(error),
            };
            match store.create(body, &worker_base_url).await {
                Ok(session) => Ok(Response::from_json(&session)?.with_status(201)),
                Err(error) => storage_error_response(error),
            }
        })
        .get_async("/sessions/:id", |_req, ctx| async move {
            let id = ctx.param("id").map_or("", String::as_str);
            if !is_valid_session_id(id) {
                return api_error("INVALID_SESSION_ID", "Invalid session ID", 400);
            }
            let store = match SessionStore::new(&ctx.env) {
                Ok(store) => store,
                Err(error) => return storage_error_response(error),
            };
            match store.get(id).await {
                Ok(Some(session)) => Response::from_json(&session),
                Ok(None) => api_error("SESSION_NOT_FOUND", "Session not found", 404),
                Err(error) => storage_error_response(error),
            }
        })
        .patch_async("/sessions/:id", |mut req, ctx| async move {
            let id = ctx.param("id").map_or("", String::as_str).to_owned();
            if !is_valid_session_id(&id) {
                return api_error("INVALID_SESSION_ID", "Invalid session ID", 400);
            }
            let body: session::UpdateRequest = match read_json_body(&mut req).await {
                Ok(body) => body,
                Err(error) => return json_body_error_response(error),
            };
            if !is_valid_session_secret(&body.secret) {
                return session_mutation_response(Err(SessionMutationError::InvalidSecret));
            }
            let store = match SessionStore::new(&ctx.env) {
                Ok(store) => store,
                Err(error) => return storage_error_response(error),
            };
            session_mutation_response(store.update_state(&id, body).await)
        })
        .delete_async("/sessions/:id", |mut req, ctx| async move {
            let id = ctx.param("id").map_or("", String::as_str).to_owned();
            if !is_valid_session_id(&id) {
                return api_error("INVALID_SESSION_ID", "Invalid session ID", 400);
            }
            let body: session::DeleteRequest = match read_json_body(&mut req).await {
                Ok(body) => body,
                Err(error) => return json_body_error_response(error),
            };
            if !is_valid_session_secret(&body.secret) {
                return session_mutation_response(Err(SessionMutationError::InvalidSecret));
            }
            let store = match SessionStore::new(&ctx.env) {
                Ok(store) => store,
                Err(error) => return storage_error_response(error),
            };
            session_mutation_response(store.cancel(&id, &body.secret).await)
        })
        .run(req, env)
        .await?;

    // Attach security response headers
    add_response_headers(&mut resp)?;
    Ok(resp)
}

fn api_error(code: &str, message: impl Into<String>, status: u16) -> Result<Response> {
    ApiErrorResponse::new(code, message, status).into_worker_response()
}

fn json_body_error_response(error: JsonBodyError) -> Result<Response> {
    match error {
        JsonBodyError::TooLarge => api_error(
            "REQUEST_TOO_LARGE",
            format!("Request body exceeds {MAX_JSON_BODY_BYTES} bytes"),
            413,
        ),
        JsonBodyError::InvalidJson => api_error("INVALID_JSON", "Invalid JSON body", 400),
    }
}

fn session_mutation_response(
    result: std::result::Result<session::SessionView, SessionMutationError>,
) -> Result<Response> {
    match result {
        Ok(view) => Response::from_json(&view),
        Err(error) => session_http_error(&error).into_worker_response(),
    }
}

fn storage_error_response(error: Error) -> Result<Response> {
    session_mutation_response(Err(SessionMutationError::Storage(error)))
}

#[derive(Debug, PartialEq, Eq, serde::Serialize)]
struct ApiErrorBody {
    error: ApiErrorDetails,
}

impl ApiErrorBody {
    fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            error: ApiErrorDetails {
                code: code.into(),
                message: message.into(),
            },
        }
    }
}

#[derive(Debug, PartialEq, Eq, serde::Serialize)]
struct ApiErrorDetails {
    code: String,
    message: String,
}

struct ApiErrorResponse {
    body: ApiErrorBody,
    status: u16,
}

impl ApiErrorResponse {
    fn new(code: impl Into<String>, message: impl Into<String>, status: u16) -> Self {
        Self {
            body: ApiErrorBody::new(code, message),
            status,
        }
    }

    fn into_worker_response(self) -> Result<Response> {
        Ok(Response::from_json(&self.body)?.with_status(self.status))
    }
}

fn session_http_error(error: &SessionMutationError) -> ApiErrorResponse {
    let (code, message, status) = match error {
        SessionMutationError::InvalidUpdate => ("INVALID_UPDATE", "Invalid session update", 400),
        SessionMutationError::InvalidSecret => ("INVALID_SECRET", "Invalid session secret", 403),
        SessionMutationError::NotFound => ("SESSION_NOT_FOUND", "Session not found", 404),
        SessionMutationError::InvalidTransition => (
            "INVALID_TRANSITION",
            "Invalid session state transition",
            409,
        ),
        SessionMutationError::Conflict => (
            "SESSION_CONFLICT",
            "Session state changed; retry the request",
            409,
        ),
        SessionMutationError::Expired => ("SESSION_EXPIRED", "Session expired", 410),
        SessionMutationError::Storage(internal) => {
            log_storage_error(internal);
            ("STORAGE_UNAVAILABLE", "Session storage unavailable", 503)
        }
    };

    ApiErrorResponse::new(code, message, status)
}

#[cfg(target_arch = "wasm32")]
fn log_storage_error(internal: &Error) {
    console_error!("Session mutation storage error: {internal}");
}

#[cfg(not(target_arch = "wasm32"))]
fn log_storage_error(_internal: &Error) {}

// ── Scheduled task: expired session cleanup ──────────────────────────

/// Cron Trigger: clean up expired sessions every 5 minutes.
#[event(scheduled)]
pub async fn scheduled(_event: ScheduledEvent, env: Env, _ctx: ScheduleContext) {
    console_log!("Cron: starting expired session cleanup");

    let store = match SessionStore::new(&env) {
        Ok(s) => s,
        Err(e) => {
            console_log!("Cron: failed to create SessionStore: {e}");
            return;
        }
    };

    match store.cleanup_expired().await {
        Ok(count) => console_log!("Cron: cleaned up {count} expired sessions"),
        Err(e) => console_log!("Cron: cleanup error: {e}"),
    }

    let now_seconds = (js_sys::Date::now() / 1_000.0) as u64;
    let cutoff = rate_limit_window_start(now_seconds);
    match store.cleanup_rate_limits(cutoff).await {
        Ok(count) => console_log!("Cron: cleaned up {count} rate-limit windows"),
        Err(e) => console_log!("Cron: rate-limit cleanup error: {e}"),
    }
}

// ── Rate limiting ─────────────────────────────────────────────

/// Simple IP-based rate limiting (D1-persisted counter).
async fn check_rate_limit(req: &Request, env: &Env) -> Result<Option<u64>> {
    let ip = req
        .headers()
        .get("CF-Connecting-IP")?
        .unwrap_or_else(|| "unknown".to_string());

    let d1 = env.d1("ICNS_DB")?;
    let now_seconds = (js_sys::Date::now() / 1_000.0) as u64;
    let window_start = rate_limit_window_start(now_seconds);

    let increment = d1
        .prepare(
            "INSERT INTO rate_limits (ip, window_start, count) VALUES (?1, ?2, 1) \
             ON CONFLICT(ip, window_start) DO UPDATE SET count = count + 1",
        )
        .bind(&[
            ip.as_str().into(),
            wasm_bindgen::JsValue::from_f64(window_start as f64),
        ])?;
    increment.run().await?;

    let stmt = d1.prepare("SELECT count FROM rate_limits WHERE ip = ?1 AND window_start = ?2");
    let query = stmt.bind(&[
        ip.as_str().into(),
        wasm_bindgen::JsValue::from_f64(window_start as f64),
    ])?;
    let result = query.first::<serde_json::Value>(None).await?;

    if let Some(count) = result
        .as_ref()
        .and_then(|row| row.get("count"))
        .and_then(serde_json::Value::as_u64)
        .filter(|count| *count > u64::from(RATE_LIMIT_MAX))
    {
        return Ok(Some(count));
    }

    Ok(None)
}

fn rate_limit_window_start(now_seconds: u64) -> u64 {
    let window = u64::from(RATE_LIMIT_WINDOW);
    now_seconds - (now_seconds % window)
}

// ── Security response headers ───────────────────────────────────────────

/// Attach security headers to all non-preflight responses.
fn add_response_headers(resp: &mut Response) -> Result<()> {
    let headers = resp.headers_mut();

    // CSP: restrict script/style sources
    headers.set(
        "Content-Security-Policy",
        "default-src 'self'; \
         script-src 'self' 'wasm-unsafe-eval'; \
         style-src 'self' 'unsafe-inline'; \
         img-src 'self' data: blob:; \
         connect-src 'self'; \
         frame-ancestors 'none'; \
         base-uri 'self'; \
         form-action 'self'",
    )?;

    // Other security headers
    headers.set("X-Content-Type-Options", "nosniff")?;
    headers.set("X-Frame-Options", "DENY")?;
    headers.set("Referrer-Policy", "strict-origin-when-cross-origin")?;
    headers.set(
        "Permissions-Policy",
        "camera=(), microphone=(), geolocation=()",
    )?;
    headers.set("Access-Control-Allow-Origin", "*")?;

    Ok(())
}

// ── CORS ─────────────────────────────────────────────────

fn cors_response() -> Result<Response> {
    let mut headers = Headers::new();
    headers.set("Access-Control-Allow-Origin", "*")?;
    headers.set(
        "Access-Control-Allow-Methods",
        "GET, POST, PATCH, DELETE, OPTIONS",
    )?;
    headers.set("Access-Control-Allow-Headers", "Content-Type")?;
    headers.set("Access-Control-Max-Age", "86400")?;
    Ok(Response::empty()
        .unwrap()
        .with_headers(headers)
        .with_status(204))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stable_error_cases() -> [(SessionMutationError, u16, &'static str, &'static str); 6] {
        [
            (
                SessionMutationError::InvalidUpdate,
                400,
                "INVALID_UPDATE",
                "Invalid session update",
            ),
            (
                SessionMutationError::InvalidSecret,
                403,
                "INVALID_SECRET",
                "Invalid session secret",
            ),
            (
                SessionMutationError::NotFound,
                404,
                "SESSION_NOT_FOUND",
                "Session not found",
            ),
            (
                SessionMutationError::InvalidTransition,
                409,
                "INVALID_TRANSITION",
                "Invalid session state transition",
            ),
            (
                SessionMutationError::Conflict,
                409,
                "SESSION_CONFLICT",
                "Session state changed; retry the request",
            ),
            (
                SessionMutationError::Expired,
                410,
                "SESSION_EXPIRED",
                "Session expired",
            ),
        ]
    }

    fn assert_http_error_response(
        response: ApiErrorResponse,
        expected_status: u16,
        expected_code: &str,
        expected_message: &str,
    ) {
        assert_eq!(response.status, expected_status);
        assert_eq!(
            serde_json::to_value(&response.body).unwrap(),
            serde_json::json!({
                "error": {
                    "code": expected_code,
                    "message": expected_message
                }
            })
        );
    }

    #[cfg(target_arch = "wasm32")]
    fn assert_handler_error_response(
        response: Response,
        expected_status: u16,
        expected_code: &str,
        expected_message: &str,
        forbidden_text: Option<&str>,
    ) {
        assert_eq!(response.status_code(), expected_status);

        let ResponseBody::Body(body) = response.body() else {
            panic!("expected a fixed JSON response body");
        };
        let value: serde_json::Value = serde_json::from_slice(body.as_slice()).unwrap();
        assert_eq!(
            value,
            serde_json::json!({
                "error": {
                    "code": expected_code,
                    "message": expected_message
                }
            })
        );
        if let Some(forbidden_text) = forbidden_text {
            assert!(!String::from_utf8_lossy(body.as_slice()).contains(forbidden_text));
        }
    }

    #[cfg(target_arch = "wasm32")]
    fn request_with_body(body: &str, content_length: Option<&str>) -> Request {
        let mut headers = Headers::new();
        headers.set("Content-Type", "application/json").unwrap();
        if let Some(content_length) = content_length {
            headers.set("Content-Length", content_length).unwrap();
        }
        let mut init = RequestInit::new();
        init.with_method(Method::Post)
            .with_headers(headers)
            .with_body(Some(worker::wasm_bindgen::JsValue::from_str(body)));
        Request::new_with_init("https://worker.invalid/sessions", &init).unwrap()
    }

    #[test]
    fn handler_error_mappings_are_stable_on_host() {
        for (error, status, code, message) in stable_error_cases() {
            assert_http_error_response(session_http_error(&error), status, code, message);
        }
    }

    #[test]
    fn storage_errors_are_not_exposed_in_http_mappings() {
        let sentinel = "D1 bind failed: password=do-not-expose";
        let error = SessionMutationError::Storage(Error::RustError(sentinel.to_owned()));
        let response = session_http_error(&error);
        let serialized = serde_json::to_string(&response.body).unwrap();
        assert_http_error_response(
            response,
            503,
            "STORAGE_UNAVAILABLE",
            "Session storage unavailable",
        );
        assert!(!serialized.contains(sentinel));
    }

    #[cfg(target_arch = "wasm32")]
    #[wasm_bindgen_test::wasm_bindgen_test]
    fn handler_boundary_returns_stable_error_responses() {
        for (error, status, code, message) in stable_error_cases() {
            let response = session_mutation_response(Err(error)).unwrap();
            assert_handler_error_response(response, status, code, message, None);
        }

        let sentinel = "D1 bind failed: password=do-not-expose";
        let response = storage_error_response(Error::RustError(sentinel.to_owned())).unwrap();
        assert_handler_error_response(
            response,
            503,
            "STORAGE_UNAVAILABLE",
            "Session storage unavailable",
            Some(sentinel),
        );
    }

    #[test]
    fn rate_limit_windows_align_to_the_configured_interval() {
        assert_eq!(rate_limit_window_start(0), 0);
        assert_eq!(rate_limit_window_start(59), 0);
        assert_eq!(rate_limit_window_start(60), 60);
        assert_eq!(rate_limit_window_start(119), 60);
    }

    #[test]
    fn bounded_body_accepts_exactly_the_limit_across_chunks() {
        let mut body = BoundedBody::new(MAX_JSON_BODY_BYTES);
        body.push(&vec![b'a'; MAX_JSON_BODY_BYTES - 1]).unwrap();
        body.push(b"b").unwrap();

        assert_eq!(body.as_bytes().len(), MAX_JSON_BODY_BYTES);
    }

    #[test]
    fn bounded_body_rejects_one_byte_over_the_limit() {
        let mut body = BoundedBody::new(MAX_JSON_BODY_BYTES);
        body.push(&vec![b'a'; MAX_JSON_BODY_BYTES]).unwrap();

        assert_eq!(body.push(b"b"), Err(JsonBodyError::TooLarge));
    }

    #[test]
    fn bounded_json_rejects_invalid_json() {
        assert!(matches!(
            parse_json_body::<session::CreateRequest>(b"{not-json}"),
            Err(JsonBodyError::InvalidJson)
        ));
    }

    #[cfg(target_arch = "wasm32")]
    #[wasm_bindgen_test::wasm_bindgen_test]
    fn body_boundary_returns_stable_size_and_json_errors() {
        let too_large = json_body_error_response(JsonBodyError::TooLarge).unwrap();
        assert_handler_error_response(
            too_large,
            413,
            "REQUEST_TOO_LARGE",
            "Request body exceeds 8192 bytes",
            None,
        );

        let invalid = json_body_error_response(JsonBodyError::InvalidJson).unwrap();
        assert_handler_error_response(invalid, 400, "INVALID_JSON", "Invalid JSON body", None);
    }

    #[cfg(target_arch = "wasm32")]
    #[wasm_bindgen_test::wasm_bindgen_test]
    async fn streamed_json_enforces_actual_size_independent_of_content_length() {
        let exactly_at_limit = format!("{{}}{}", " ".repeat(MAX_JSON_BODY_BYTES - 2));
        let mut missing_length = request_with_body(&exactly_at_limit, None);
        let parsed: serde_json::Value = read_json_body(&mut missing_length).await.unwrap();
        assert_eq!(parsed, serde_json::json!({}));

        let over_limit = format!("{{}}{}", " ".repeat(MAX_JSON_BODY_BYTES - 1));
        let mut forged_low = request_with_body(&over_limit, Some("2"));
        assert_eq!(
            read_json_body::<serde_json::Value>(&mut forged_low).await,
            Err(JsonBodyError::TooLarge)
        );

        let mut forged_high = request_with_body("{}", Some("999999"));
        let parsed: serde_json::Value = read_json_body(&mut forged_high).await.unwrap();
        assert_eq!(parsed, serde_json::json!({}));

        let mut invalid = request_with_body("{not-json}", None);
        assert_eq!(
            read_json_body::<serde_json::Value>(&mut invalid).await,
            Err(JsonBodyError::InvalidJson)
        );
    }
}
