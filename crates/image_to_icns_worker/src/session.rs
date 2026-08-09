//! Session data model and state machine.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use worker::*;

const MAX_OUTPUT_BYTE_LEN: u64 = 64 * 1024 * 1024;

/// Session creation request (initiated by Agent or user).
#[derive(Debug, Deserialize)]
pub struct CreateRequest {
    /// Optional source format hint (png / jpeg / svg).
    pub source_format: Option<String>,
}

impl CreateRequest {
    pub fn validate(&self) -> std::result::Result<(), &'static str> {
        match self.source_format.as_deref() {
            None | Some("png" | "jpeg" | "svg") => Ok(()),
            Some(_) => Err("source_format must be png, jpeg, or svg"),
        }
    }
}

/// Session view returned to caller (internal secret excluded).
#[derive(Debug, Serialize)]
pub struct SessionView {
    pub session_id: String,
    pub editor_url: String,
    pub state: String,
    pub expires_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_byte_len: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub representation_count: Option<u32>,
}

/// Internal session state (stored in D1).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionRecord {
    pub session_id: String,
    pub secret_hash: String,
    pub state: SessionState,
    pub source_format: Option<String>,
    pub created_at: String,
    pub expires_at: String,
    pub output_byte_len: Option<u64>,
    pub representation_count: Option<u32>,
    pub error_code: Option<String>,
}

/// Session state machine.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionState {
    Created,
    Editing,
    Completed,
    Cancelled,
    Expired,
    Failed,
}

impl SessionState {
    /// Allows active Sessions to advance while preventing terminal-state re-entry.
    pub fn can_transition_to(&self, target: &SessionState) -> bool {
        matches!(
            (self, target),
            (SessionState::Created, SessionState::Editing)
                | (SessionState::Created, SessionState::Completed)
                | (SessionState::Created, SessionState::Cancelled)
                | (SessionState::Created, SessionState::Expired)
                | (SessionState::Created, SessionState::Failed)
                | (SessionState::Editing, SessionState::Completed)
                | (SessionState::Editing, SessionState::Cancelled)
                | (SessionState::Editing, SessionState::Expired)
                | (SessionState::Editing, SessionState::Failed)
        )
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            SessionState::Created => "created",
            SessionState::Editing => "editing",
            SessionState::Completed => "completed",
            SessionState::Cancelled => "cancelled",
            SessionState::Expired => "expired",
            SessionState::Failed => "failed",
        }
    }
}

impl SessionRecord {
    fn effective_state(&self, now: &str) -> SessionState {
        if matches!(self.state, SessionState::Created | SessionState::Editing)
            && self.expires_at.as_str() <= now
        {
            SessionState::Expired
        } else {
            self.state.clone()
        }
    }

    fn authorize_mutation(
        &self,
        secret: &str,
        now: &str,
    ) -> std::result::Result<(), SessionMutationError> {
        let hash = hex::encode(Sha256::digest(secret.as_bytes()));
        if hash != self.secret_hash {
            return Err(SessionMutationError::InvalidSecret);
        }
        if self.effective_state(now) == SessionState::Expired {
            return Err(SessionMutationError::Expired);
        }
        Ok(())
    }

    fn into_view(self, editor_base_url: &Url, now: &str) -> Result<SessionView> {
        let state = self.effective_state(now);
        let editor_url = build_editor_url(editor_base_url, &self.session_id, None, None)?;
        Ok(SessionView {
            session_id: self.session_id,
            editor_url,
            state: state.as_str().to_owned(),
            expires_at: self.expires_at,
            output_byte_len: self.output_byte_len,
            representation_count: self.representation_count,
        })
    }
}

/// Stable domain errors returned by authenticated Session mutations.
#[derive(Debug)]
pub enum SessionMutationError {
    InvalidUpdate,
    InvalidSecret,
    NotFound,
    InvalidTransition,
    Conflict,
    Expired,
    Storage(Error),
}

impl From<Error> for SessionMutationError {
    fn from(error: Error) -> Self {
        Self::Storage(error)
    }
}

/// Session update request (initiated by Web editor).
#[derive(Debug, Deserialize)]
pub struct UpdateRequest {
    /// Target state.
    pub state: String,
    /// One-time secret for authentication.
    pub secret: String,
    /// Output metadata for completed updates.
    #[serde(default)]
    pub output_byte_len: Option<u64>,
    #[serde(default)]
    pub representation_count: Option<u32>,
    /// Machine-readable failure code for failed updates.
    #[serde(default)]
    pub error_code: Option<String>,
}

impl UpdateRequest {
    fn validate(&self) -> std::result::Result<SessionState, SessionMutationError> {
        let target = match self.state.as_str() {
            "editing" => SessionState::Editing,
            "completed" => SessionState::Completed,
            "failed" => SessionState::Failed,
            _ => return Err(SessionMutationError::InvalidUpdate),
        };
        let has_output_metadata =
            self.output_byte_len.is_some() || self.representation_count.is_some();

        match target {
            SessionState::Editing => {
                if has_output_metadata || self.error_code.is_some() {
                    return Err(SessionMutationError::InvalidUpdate);
                }
            }
            SessionState::Completed => {
                if self
                    .output_byte_len
                    .is_none_or(|size| size == 0 || size > MAX_OUTPUT_BYTE_LEN)
                    || self.representation_count != Some(10)
                    || self.error_code.is_some()
                {
                    return Err(SessionMutationError::InvalidUpdate);
                }
            }
            SessionState::Failed => {
                if has_output_metadata
                    || !self
                        .error_code
                        .as_deref()
                        .is_some_and(is_valid_machine_error_code)
                {
                    return Err(SessionMutationError::InvalidUpdate);
                }
            }
            _ => unreachable!("client-owned target states are exhaustive"),
        }

        Ok(target)
    }
}

fn is_valid_machine_error_code(value: &str) -> bool {
    (1..=64).contains(&value.len())
        && value
            .bytes()
            .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'_')
}

/// Session cancellation request (requires secret verification).
#[derive(Debug, Deserialize)]
pub struct DeleteRequest {
    pub secret: String,
}

/// Session storage layer (D1 database operations).
pub struct SessionStore {
    d1: D1Database,
    editor_base_url: Url,
}

impl SessionStore {
    pub fn new(env: &Env) -> Result<Self> {
        let d1 = env.d1("ICNS_DB")?;
        let editor_base_url = env.var("EDITOR_BASE_URL")?.to_string();
        let editor_base_url = parse_editor_base_url(&editor_base_url)?;
        Ok(Self {
            d1,
            editor_base_url,
        })
    }

    /// Create a new Session, return editor URL with one-time secret.
    pub async fn create(&self, req: CreateRequest, worker_base_url: &str) -> Result<SessionView> {
        req.validate()
            .map_err(|message| Error::RustError(message.to_owned()))?;
        let session_id = generate_id(32);
        let secret = generate_id(64);
        let secret_hash = hex::encode(Sha256::digest(secret.as_bytes()));
        let now = js_sys::Date::new_0().to_iso_string().as_string().unwrap();
        let expires = js_sys::Date::new_0();
        expires.set_minutes(expires.get_minutes() + 30);
        let expires_at = expires.to_iso_string().as_string().unwrap();

        let record = SessionRecord {
            session_id: session_id.clone(),
            secret_hash,
            state: SessionState::Created,
            source_format: req.source_format,
            created_at: now,
            expires_at: expires_at.clone(),
            output_byte_len: None,
            representation_count: None,
            error_code: None,
        };

        self.insert_record(&record).await?;

        let editor_url = build_editor_url(
            &self.editor_base_url,
            &session_id,
            Some(&secret),
            Some(worker_base_url),
        )?;

        Ok(SessionView {
            session_id,
            editor_url,
            state: SessionState::Created.as_str().to_owned(),
            expires_at,
            output_byte_len: None,
            representation_count: None,
        })
    }

    /// Query Session view by ID.
    pub async fn get(&self, id: &str) -> Result<Option<SessionView>> {
        let stmt = self
            .d1
            .prepare("SELECT * FROM sessions WHERE session_id = ?1");
        let query = stmt.bind(&[id.into()])?;
        let result = query.first::<SessionRecord>(None).await?;

        match result {
            Some(record) => Ok(Some(
                record.into_view(&self.editor_base_url, &current_timestamp())?,
            )),
            None => Ok(None),
        }
    }

    async fn insert_record(&self, record: &SessionRecord) -> Result<()> {
        let stmt = self.d1.prepare(
            "INSERT INTO sessions (session_id, secret_hash, state, source_format, \
             created_at, expires_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        );
        let bindings = [
            D1Type::Text(&record.session_id),
            D1Type::Text(&record.secret_hash),
            D1Type::Text(record.state.as_str()),
            d1_optional_text(record.source_format.as_deref()),
            D1Type::Text(&record.created_at),
            D1Type::Text(&record.expires_at),
        ];
        let query = stmt.bind_refs(&bindings)?;
        query.run().await?;
        Ok(())
    }

    /// Update Session state (requires secret verification, validates state machine).
    pub async fn update_state(
        &self,
        session_id: &str,
        req: UpdateRequest,
    ) -> std::result::Result<SessionView, SessionMutationError> {
        let target = req.validate()?;

        let record = self
            .get_record(session_id)
            .await?
            .ok_or(SessionMutationError::NotFound)?;

        record.authorize_mutation(&req.secret, &current_timestamp())?;

        if !record.state.can_transition_to(&target) {
            return Err(SessionMutationError::InvalidTransition);
        }

        let stmt = self.d1.prepare(
            "UPDATE sessions SET state = ?1, output_byte_len = ?2, representation_count = ?3, \
             error_code = ?4 WHERE session_id = ?5 AND state = ?6 \
             AND expires_at > strftime('%Y-%m-%dT%H:%M:%fZ', 'now')",
        );
        let output_byte_len = d1_optional_integer(req.output_byte_len)?;
        let representation_count = d1_optional_integer(req.representation_count.map(u64::from))?;
        let bindings = [
            D1Type::Text(target.as_str()),
            output_byte_len,
            representation_count,
            d1_optional_text(req.error_code.as_deref()),
            D1Type::Text(session_id),
            D1Type::Text(record.state.as_str()),
        ];
        let query = stmt.bind_refs(&bindings)?;
        let result = query.run().await?;
        if result
            .meta()?
            .and_then(|metadata| metadata.changes)
            .unwrap_or(0)
            != 1
        {
            return Err(self.mutation_conflict(session_id).await);
        }

        let editor_url = build_editor_url(&self.editor_base_url, session_id, None, None)?;
        Ok(SessionView {
            session_id: session_id.to_owned(),
            editor_url,
            state: target.as_str().to_owned(),
            expires_at: record.expires_at,
            output_byte_len: req.output_byte_len,
            representation_count: req.representation_count,
        })
    }

    /// Cancel Session (requires secret verification).
    pub async fn cancel(
        &self,
        session_id: &str,
        secret: &str,
    ) -> std::result::Result<SessionView, SessionMutationError> {
        let record = self
            .get_record(session_id)
            .await?
            .ok_or(SessionMutationError::NotFound)?;

        record.authorize_mutation(secret, &current_timestamp())?;
        if !record.state.can_transition_to(&SessionState::Cancelled) {
            return Err(SessionMutationError::InvalidTransition);
        }

        let stmt = self.d1.prepare(
            "UPDATE sessions SET state = 'cancelled' WHERE session_id = ?1 AND state = ?2 \
             AND expires_at > strftime('%Y-%m-%dT%H:%M:%fZ', 'now')",
        );
        let query = stmt.bind(&[session_id.into(), record.state.as_str().into()])?;
        let result = query.run().await?;
        if result
            .meta()?
            .and_then(|metadata| metadata.changes)
            .unwrap_or(0)
            != 1
        {
            return Err(self.mutation_conflict(session_id).await);
        }
        Ok(SessionView {
            session_id: session_id.to_owned(),
            editor_url: build_editor_url(&self.editor_base_url, session_id, None, None)?,
            state: SessionState::Cancelled.as_str().to_owned(),
            expires_at: record.expires_at,
            output_byte_len: record.output_byte_len,
            representation_count: record.representation_count,
        })
    }

    async fn get_record(&self, session_id: &str) -> Result<Option<SessionRecord>> {
        let stmt = self
            .d1
            .prepare("SELECT * FROM sessions WHERE session_id = ?1");
        let query = stmt.bind(&[session_id.into()])?;
        query.first::<SessionRecord>(None).await
    }

    async fn mutation_conflict(&self, session_id: &str) -> SessionMutationError {
        match self.get_record(session_id).await {
            Ok(Some(record))
                if record.effective_state(&current_timestamp()) == SessionState::Expired =>
            {
                SessionMutationError::Expired
            }
            Ok(_) => SessionMutationError::Conflict,
            Err(error) => SessionMutationError::Storage(error),
        }
    }

    /// Reclaim terminal and long-expired Sessions (Cron invocation).
    pub async fn cleanup_expired(&self) -> Result<usize> {
        let cutoff = js_sys::Date::new_0();
        cutoff.set_hours(cutoff.get_hours() - 24);
        let cutoff_str = cutoff.to_iso_string().as_string().unwrap();

        let stmt = self.d1.prepare(
            "DELETE FROM sessions WHERE \
             (state IN ('completed', 'cancelled', 'expired', 'failed') AND created_at < ?1) \
             OR (state IN ('created', 'editing') AND expires_at < ?1)",
        );
        let query = stmt.bind(&[cutoff_str.as_str().into()])?;
        let result = query.run().await?;

        let deleted = result
            .meta()?
            .and_then(|metadata| metadata.changes)
            .unwrap_or(0);
        console_log!("Expired cleanup: cut-off at {cutoff_str}");
        Ok(deleted)
    }

    /// Delete fixed-window rate-limit counters older than the active window.
    pub async fn cleanup_rate_limits(&self, cutoff: u64) -> Result<usize> {
        let stmt = self
            .d1
            .prepare("DELETE FROM rate_limits WHERE window_start < ?1");
        let query = stmt.bind(&[wasm_bindgen::JsValue::from_f64(cutoff as f64)])?;
        let result = query.run().await?;
        Ok(result
            .meta()?
            .and_then(|metadata| metadata.changes)
            .unwrap_or(0))
    }
}

fn current_timestamp() -> String {
    js_sys::Date::new_0().to_iso_string().as_string().unwrap()
}

fn generate_id(byte_len: usize) -> String {
    let mut bytes = vec![0u8; byte_len];
    getrandom::getrandom(&mut bytes).expect("Crypto API available");
    hex::encode(bytes)
}

fn build_editor_url(
    editor_base_url: &Url,
    session_id: &str,
    secret: Option<&str>,
    worker_base_url: Option<&str>,
) -> Result<String> {
    let mut url = editor_base_url.clone();
    {
        let mut query = url.query_pairs_mut();
        query.clear().append_pair("session", session_id);
        if let Some(secret) = secret {
            query.append_pair("secret", secret);
        }
        if let Some(worker_base_url) = worker_base_url {
            query.append_pair("worker", worker_base_url);
        }
    }
    let fragment = url.query().map(str::to_owned);
    url.set_query(None);
    url.set_fragment(fragment.as_deref());
    Ok(url.into())
}

fn parse_editor_base_url(value: &str) -> Result<Url> {
    let url = Url::parse(value)
        .map_err(|error| Error::RustError(format!("Invalid EDITOR_BASE_URL: {error}")))?;
    let allowed_scheme = match url.scheme() {
        "https" => url.host_str().is_some(),
        "http" => matches!(
            url.host_str(),
            Some("localhost" | "127.0.0.1" | "[::1]" | "::1")
        ),
        _ => false,
    };
    let has_userinfo = !url.username().is_empty() || url.password().is_some();

    if !allowed_scheme || has_userinfo || url.query().is_some() || url.fragment().is_some() {
        return Err(Error::RustError(
            "Invalid EDITOR_BASE_URL: expected HTTPS or loopback HTTP without userinfo, query, or fragment"
                .to_owned(),
        ));
    }

    Ok(url)
}

fn d1_optional_text(value: Option<&str>) -> D1Type<'_> {
    value.map_or(D1Type::Null, D1Type::Text)
}

fn d1_optional_integer(
    value: Option<u64>,
) -> std::result::Result<D1Type<'static>, SessionMutationError> {
    value.map_or(Ok(D1Type::Null), |value| {
        i32::try_from(value)
            .map(D1Type::Integer)
            .map_err(|_| SessionMutationError::InvalidUpdate)
    })
}

fn is_fixed_hex(value: &str, expected_len: usize) -> bool {
    value.len() == expected_len && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

pub(crate) fn is_valid_session_id(value: &str) -> bool {
    is_fixed_hex(value, 64)
}

pub(crate) fn is_valid_session_secret(value: &str) -> bool {
    is_fixed_hex(value, 128)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session_record(state: SessionState, expires_at: &str) -> SessionRecord {
        let secret = "b".repeat(128);
        SessionRecord {
            session_id: "a".repeat(64),
            secret_hash: hex::encode(Sha256::digest(secret.as_bytes())),
            state,
            source_format: Some("png".to_owned()),
            created_at: "2026-08-09T00:00:00.000Z".to_owned(),
            expires_at: expires_at.to_owned(),
            output_byte_len: None,
            representation_count: None,
            error_code: None,
        }
    }

    fn create_request(source_format: Option<&str>) -> CreateRequest {
        CreateRequest {
            source_format: source_format.map(str::to_owned),
        }
    }

    #[test]
    fn creation_requests_accept_only_supported_source_formats() {
        assert!(create_request(None).validate().is_ok());
        for source_format in ["png", "jpeg", "svg"] {
            assert!(create_request(Some(source_format)).validate().is_ok());
        }
        assert!(create_request(Some("gif")).validate().is_err());
        assert!(create_request(Some("")).validate().is_err());
    }

    #[test]
    fn completed_session_views_include_output_metadata() {
        let view = SessionView {
            session_id: "session".to_owned(),
            editor_url: "https://example.com/?session=session".to_owned(),
            state: "completed".to_owned(),
            expires_at: "2026-08-09T00:00:00.000Z".to_owned(),
            output_byte_len: Some(42),
            representation_count: Some(10),
        };
        let value = serde_json::to_value(view).unwrap();

        assert_eq!(value["output_byte_len"], 42);
        assert_eq!(value["representation_count"], 10);
    }

    #[test]
    fn session_credentials_have_fixed_hex_encodings() {
        assert!(is_valid_session_id(&"a".repeat(64)));
        assert!(!is_valid_session_id(&"a".repeat(63)));
        assert!(!is_valid_session_id(&format!("{}g", "a".repeat(63))));

        assert!(is_valid_session_secret(&"b".repeat(128)));
        assert!(!is_valid_session_secret(&"b".repeat(127)));
        assert!(!is_valid_session_secret("../secret"));
    }

    #[test]
    fn editor_urls_include_encoded_session_credentials_and_worker_origin() {
        let editor_base_url =
            parse_editor_base_url("https://tinkora.github.io/image_to_icns/").unwrap();
        let editor_url = build_editor_url(
            &editor_base_url,
            &"a".repeat(64),
            Some(&"b".repeat(128)),
            Some("https://image-to-icns.example.workers.dev"),
        )
        .unwrap();
        let editor_url = Url::parse(&editor_url).unwrap();
        assert!(editor_url.query().is_none());
        let fragment_url = Url::parse(&format!(
            "https://local.invalid/?{}",
            editor_url.fragment().unwrap()
        ))
        .unwrap();
        let params = fragment_url
            .query_pairs()
            .collect::<std::collections::HashMap<_, _>>();

        assert_eq!(params.get("session").unwrap(), &"a".repeat(64).as_str());
        assert_eq!(params.get("secret").unwrap(), &"b".repeat(128).as_str());
        assert_eq!(
            params.get("worker").unwrap(),
            "https://image-to-icns.example.workers.dev"
        );
    }

    #[test]
    fn completed_updates_require_verified_output_metadata() {
        let valid = UpdateRequest {
            state: "completed".to_owned(),
            secret: "b".repeat(128),
            output_byte_len: Some(42),
            representation_count: Some(10),
            error_code: None,
        };
        assert_eq!(valid.validate().unwrap(), SessionState::Completed);

        let missing_size = UpdateRequest {
            output_byte_len: None,
            ..valid
        };
        assert!(missing_size.validate().is_err());

        for invalid_size in [0, MAX_OUTPUT_BYTE_LEN + 1] {
            let invalid = UpdateRequest {
                state: "completed".to_owned(),
                secret: "b".repeat(128),
                output_byte_len: Some(invalid_size),
                representation_count: Some(10),
                error_code: None,
            };
            assert!(matches!(
                invalid.validate(),
                Err(SessionMutationError::InvalidUpdate)
            ));
        }

        let invalid_count = UpdateRequest {
            state: "completed".to_owned(),
            secret: "b".repeat(128),
            output_byte_len: Some(42),
            representation_count: Some(9),
            error_code: None,
        };
        assert!(invalid_count.validate().is_err());

        let unexpected_error_code = UpdateRequest {
            state: "completed".to_owned(),
            secret: "b".repeat(128),
            output_byte_len: Some(42),
            representation_count: Some(10),
            error_code: Some("ICNS_ENCODE_FAILED".to_owned()),
        };
        assert!(unexpected_error_code.validate().is_err());
    }

    #[test]
    fn non_completed_updates_reject_output_metadata() {
        let update = UpdateRequest {
            state: "editing".to_owned(),
            secret: "b".repeat(128),
            output_byte_len: Some(42),
            representation_count: Some(10),
            error_code: None,
        };

        assert!(update.validate().is_err());

        let update = UpdateRequest {
            state: "editing".to_owned(),
            secret: "b".repeat(128),
            output_byte_len: None,
            representation_count: None,
            error_code: Some("ICNS_ENCODE_FAILED".to_owned()),
        };

        assert!(update.validate().is_err());
    }

    #[test]
    fn failed_updates_require_a_bounded_machine_error_code() {
        let missing_code = UpdateRequest {
            state: "failed".to_owned(),
            secret: "b".repeat(128),
            output_byte_len: None,
            representation_count: None,
            error_code: None,
        };
        assert!(missing_code.validate().is_err());

        let valid = UpdateRequest {
            error_code: Some("ICNS_ENCODE_FAILED".to_owned()),
            ..missing_code
        };
        assert_eq!(valid.validate().unwrap(), SessionState::Failed);

        for error_code in [
            "",
            "lowercase",
            "HAS-HYPHEN",
            "HAS SPACE",
            "NON_ASCII_\u{9519}\u{8bef}",
        ] {
            let invalid = UpdateRequest {
                state: "failed".to_owned(),
                secret: "b".repeat(128),
                output_byte_len: None,
                representation_count: None,
                error_code: Some(error_code.to_owned()),
            };
            assert!(invalid.validate().is_err(), "accepted {error_code:?}");
        }

        let boundary = UpdateRequest {
            state: "failed".to_owned(),
            secret: "b".repeat(128),
            output_byte_len: None,
            representation_count: None,
            error_code: Some("A".repeat(64)),
        };
        assert_eq!(boundary.validate().unwrap(), SessionState::Failed);

        let too_long = UpdateRequest {
            error_code: Some("A".repeat(65)),
            ..boundary
        };
        assert!(too_long.validate().is_err());
    }

    #[test]
    fn clients_cannot_set_server_owned_states() {
        for state in ["created", "cancelled", "expired"] {
            let update = UpdateRequest {
                state: state.to_owned(),
                secret: "b".repeat(128),
                output_byte_len: None,
                representation_count: None,
                error_code: None,
            };
            assert!(update.validate().is_err());
        }
    }

    #[test]
    fn active_sessions_follow_the_supported_happy_path() {
        assert!(SessionState::Created.can_transition_to(&SessionState::Editing));
        assert!(SessionState::Editing.can_transition_to(&SessionState::Completed));
    }

    #[test]
    fn overdue_active_sessions_are_read_as_expired_without_cron() {
        let expires_at = "2026-08-09T00:30:00.000Z";
        let now = "2026-08-09T00:30:00.000Z";

        for state in [SessionState::Created, SessionState::Editing] {
            let record = session_record(state, expires_at);
            assert_eq!(record.effective_state(now), SessionState::Expired);
        }

        let completed = session_record(SessionState::Completed, expires_at);
        assert_eq!(
            completed.effective_state(now),
            SessionState::Completed,
            "terminal outcomes must not be overwritten by expiry projection"
        );
    }

    #[cfg(target_arch = "wasm32")]
    #[wasm_bindgen_test::wasm_bindgen_test]
    fn overdue_session_http_views_serialize_as_expired() {
        let record = session_record(SessionState::Editing, "2026-08-09T00:29:59.999Z");
        let editor_base_url = Url::parse("https://example.com/editor/").unwrap();
        let view = record
            .into_view(&editor_base_url, "2026-08-09T00:30:00.000Z")
            .unwrap();
        let response = Response::from_json(&view).unwrap();
        let ResponseBody::Body(body) = response.body() else {
            panic!("expected a fixed JSON response body");
        };
        let value: serde_json::Value = serde_json::from_slice(body.as_slice()).unwrap();

        assert_eq!(value["state"], "expired");
    }

    #[test]
    fn overdue_sessions_reject_mutations_even_with_the_correct_secret() {
        let record = session_record(SessionState::Editing, "2026-08-09T00:29:59.999Z");

        assert!(matches!(
            record.authorize_mutation(&"b".repeat(128), "2026-08-09T00:30:00.000Z"),
            Err(SessionMutationError::Expired)
        ));
    }

    #[test]
    fn overdue_sessions_do_not_reveal_expiry_to_unauthorized_callers() {
        let record = session_record(SessionState::Editing, "2026-08-09T00:29:59.999Z");

        assert!(matches!(
            record.authorize_mutation(&"c".repeat(128), "2026-08-09T00:30:00.000Z"),
            Err(SessionMutationError::InvalidSecret)
        ));
    }

    #[test]
    fn created_sessions_can_complete_when_the_editing_event_is_missed() {
        assert!(SessionState::Created.can_transition_to(&SessionState::Completed));
    }

    #[test]
    fn terminal_sessions_cannot_transition_again() {
        for state in [
            SessionState::Completed,
            SessionState::Cancelled,
            SessionState::Expired,
            SessionState::Failed,
        ] {
            assert!(!state.can_transition_to(&SessionState::Expired));
            assert!(!state.can_transition_to(&SessionState::Failed));
        }
    }

    #[test]
    fn optional_d1_bindings_use_supported_null_text_and_integer_types() {
        assert!(matches!(d1_optional_text(None), D1Type::Null));
        assert!(matches!(d1_optional_integer(None).unwrap(), D1Type::Null));

        match d1_optional_text(Some("png")) {
            D1Type::Text(value) => assert_eq!(value, "png"),
            _ => panic!("source format must bind as D1 text"),
        }
        match d1_optional_text(Some("ICNS_ENCODE_FAILED")) {
            D1Type::Text(value) => assert_eq!(value, "ICNS_ENCODE_FAILED"),
            _ => panic!("error code must bind as D1 text"),
        }
        match d1_optional_integer(Some(MAX_OUTPUT_BYTE_LEN)).unwrap() {
            D1Type::Integer(value) => assert_eq!(value, MAX_OUTPUT_BYTE_LEN as i32),
            _ => panic!("output byte length must bind as a D1 integer"),
        }
        match d1_optional_integer(Some(10)).unwrap() {
            D1Type::Integer(value) => assert_eq!(value, 10),
            _ => panic!("representation count must bind as a D1 integer"),
        }
    }

    #[test]
    fn editor_base_urls_allow_https_and_loopback_http() {
        for value in [
            "https://example.com/editor/",
            "http://localhost:4173/editor/",
            "http://127.0.0.1/editor/",
            "http://[::1]/editor/",
        ] {
            assert!(parse_editor_base_url(value).is_ok(), "rejected {value:?}");
        }
    }

    #[test]
    fn editor_base_urls_reject_unsafe_origins_and_embedded_state() {
        for value in [
            "http://example.com/editor/",
            "https://user@example.com/editor/",
            "https://example.com/editor/?mode=remote",
            "https://example.com/editor/#session=existing",
            "ftp://example.com/editor/",
        ] {
            assert!(parse_editor_base_url(value).is_err(), "accepted {value:?}");
        }
    }
}
