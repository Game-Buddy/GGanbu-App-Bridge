// SPDX-FileCopyrightText: 2026 Game Buddy
// SPDX-License-Identifier: AGPL-3.0-only

use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode, header::CONTENT_TYPE},
    response::{IntoResponse, Response},
};
use base64::Engine as _;
use chacha20poly1305::aead::{AeadInPlace, KeyInit};
use chrono::{Duration, SecondsFormat, Utc};
use hkdf::Hkdf;
use rand::RngCore;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use sha2::Sha256;

use crate::{
    actions::{
        ActionResolver, ResolveError, ResolvedShortcut, TargetPlatform, is_valid_action_code,
    },
    keyboard::KeyboardExecution,
    state::{ActionAttempt, ActionValidation, SharedBridgeState, StatePublisher},
};

pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");
pub const PROTOCOL_VERSION: u8 = 1;

#[derive(Clone)]
pub struct ProtocolModule {
    bridge: SharedBridgeState,
    publisher: StatePublisher,
    action_resolver: ActionResolver,
    keyboard_execution: KeyboardExecution,
    security: crate::security::SecurityState,
}

impl ProtocolModule {
    pub(crate) fn new(
        bridge: SharedBridgeState,
        publisher: StatePublisher,
        action_resolver: ActionResolver,
        keyboard_execution: KeyboardExecution,
        security: crate::security::SecurityState,
    ) -> Self {
        Self {
            bridge,
            publisher,
            action_resolver,
            keyboard_execution,
            security,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HealthResponse {
    service: &'static str,
    app_version: &'static str,
    protocol_version: u8,
    status: &'static str,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SecureMessageCommand {
    message: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MessageAcceptedResponse {
    accepted: bool,
    sequence: u64,
    received_at: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActionResolvedResponse {
    bindings: Vec<ActionBindingResponse>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ActionBindingResponse {
    action: String,
    label: String,
    platform: TargetPlatform,
    binding: ResolvedShortcut,
}

#[derive(Debug, Serialize)]
struct ErrorEnvelope {
    error: ErrorBody,
}

#[derive(Debug, Serialize)]
struct ErrorBody {
    code: &'static str,
    message: &'static str,
}

#[derive(Clone, Copy, Debug)]
pub enum ApiError {
    InvalidJson,
    OriginNotAllowed,
    NotFound,
    PayloadTooLarge,
    UnsupportedMediaType,
    InvalidMessage,
    InvalidActionRequest,
    UnknownAction,
    UnsupportedPlatform,
    ConnectionRequired,
    PairingUnavailable,
    PairingAuthenticationFailed,
    PairingProtocolFailed,
    DuplicateRequest,
    ExecutionFailed,
    StorageUnavailable,
}

impl ApiError {
    fn details(self) -> (StatusCode, &'static str, &'static str) {
        match self {
            Self::InvalidJson => (
                StatusCode::BAD_REQUEST,
                "invalid_json",
                "request body must contain valid JSON",
            ),
            Self::OriginNotAllowed => (
                StatusCode::FORBIDDEN,
                "origin_not_allowed",
                "request origin is not approved",
            ),
            Self::NotFound => (
                StatusCode::NOT_FOUND,
                "not_found",
                "the requested route does not exist",
            ),
            Self::PayloadTooLarge => (
                StatusCode::PAYLOAD_TOO_LARGE,
                "payload_too_large",
                "request body must not exceed 32768 bytes",
            ),
            Self::UnsupportedMediaType => (
                StatusCode::UNSUPPORTED_MEDIA_TYPE,
                "unsupported_media_type",
                "Content-Type must be application/json",
            ),
            Self::InvalidMessage => (
                StatusCode::UNPROCESSABLE_ENTITY,
                "invalid_message",
                "message must contain between 1 and 256 characters",
            ),
            Self::InvalidActionRequest => (
                StatusCode::UNPROCESSABLE_ENTITY,
                "invalid_action_request",
                "actions must be a non-empty list of valid action IDs",
            ),
            Self::UnknownAction => (
                StatusCode::UNPROCESSABLE_ENTITY,
                "unknown_action",
                "action is not in the companion allowlist",
            ),
            Self::UnsupportedPlatform => (
                StatusCode::NOT_IMPLEMENTED,
                "unsupported_platform",
                "action mappings are unavailable for this operating system",
            ),
            Self::ConnectionRequired => (
                StatusCode::CONFLICT,
                "connection_required",
                "an active paired connection is required for keyboard execution",
            ),
            Self::PairingUnavailable => (
                StatusCode::CONFLICT,
                "pairing_unavailable",
                "pairing is unavailable",
            ),
            Self::PairingAuthenticationFailed => (
                StatusCode::UNAUTHORIZED,
                "pairing_authentication_failed",
                "pairing authentication failed",
            ),
            Self::PairingProtocolFailed => (
                StatusCode::BAD_REQUEST,
                "pairing_protocol_failed",
                "pairing message is invalid",
            ),
            Self::DuplicateRequest => (
                StatusCode::CONFLICT,
                "duplicate_request",
                "the request was already processed or the session request limit was reached",
            ),
            Self::ExecutionFailed => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "execution_failed",
                "the operating system did not accept the keyboard shortcut",
            ),
            Self::StorageUnavailable => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "storage_unavailable",
                "security data could not be saved",
            ),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, code, message) = self.details();
        (
            status,
            Json(ErrorEnvelope {
                error: ErrorBody { code, message },
            }),
        )
            .into_response()
    }
}

pub async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        service: "gganbu-bridge",
        app_version: APP_VERSION,
        protocol_version: PROTOCOL_VERSION,
        status: "ok",
    })
}

pub async fn accept_message(
    State(state): State<ProtocolModule>,
    headers: HeaderMap,
    Json(envelope): Json<SecureEnvelope>,
) -> Result<Json<SecureEnvelope>, ApiError> {
    if !has_json_content_type(&headers) {
        return Err(ApiError::UnsupportedMediaType);
    }

    let session_id = envelope.session_id.clone();
    let request: SecureMessageCommand = decrypt_secure_payload(&state, &envelope)?;

    let message = request.message.trim();
    if !(1..=256).contains(&message.chars().count()) {
        return Err(ApiError::InvalidMessage);
    }

    let received_at = Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true);
    let accepted =
        state
            .bridge
            .accept_message(message.to_owned(), received_at.clone(), &state.publisher);

    let response = encrypt_secure_payload(
        &state,
        &session_id,
        &MessageAcceptedResponse {
            accepted: true,
            sequence: accepted.sequence,
            received_at,
        },
    )?;
    Ok(Json(response))
}

pub async fn resolve_action(
    State(state): State<ProtocolModule>,
    headers: HeaderMap,
    Json(envelope): Json<SecureEnvelope>,
) -> Result<Json<SecureEnvelope>, ApiError> {
    if !has_json_content_type(&headers) {
        return Err(ApiError::UnsupportedMediaType);
    }

    let session_id = envelope.session_id.clone();
    let request: ResolveCommand = decrypt_secure_payload(&state, &envelope)?;

    if request.actions.is_empty()
        || request.actions.len() > 128
        || request
            .actions
            .iter()
            .any(|action| !is_valid_action_code(action))
    {
        return Err(ApiError::InvalidActionRequest);
    }

    let received_at = Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true);
    let mut bindings = Vec::with_capacity(request.actions.len());
    for (index, action) in request.actions.iter().enumerate() {
        let resolution = match state.action_resolver.resolve(action) {
            Ok(resolution) => resolution,
            Err(ResolveError::UnknownAction) => continue,
            Err(ResolveError::CatalogUnavailable) => return Err(ApiError::ExecutionFailed),
            Err(ResolveError::UnsupportedPlatform) => {
                state.bridge.record_action_attempt(
                    ActionAttempt {
                        action: action.clone(),
                        request_id: format!("resolve-{index}"),
                        label: None,
                        platform: state.action_resolver.platform(),
                        shortcut: None,
                        validation: ActionValidation::Rejected,
                        validation_message:
                            "Rejected: this operating system has no action mapping.".to_owned(),
                        executed: false,
                        received_at: received_at.clone(),
                        sequence: 0,
                    },
                    &state.publisher,
                );
                return Err(ApiError::UnsupportedPlatform);
            }
        };

        state.bridge.record_action_attempt(
            ActionAttempt {
                action: action.clone(),
                request_id: format!("resolve-{index}"),
                label: Some(resolution.label.to_owned()),
                platform: resolution.platform,
                shortcut: Some(resolution.shortcut.clone()),
                validation: ActionValidation::Accepted,
                validation_message: "Preview resolved; keyboard execution was not requested."
                    .to_owned(),
                executed: false,
                received_at: received_at.clone(),
                sequence: 0,
            },
            &state.publisher,
        );
        bindings.push(ActionBindingResponse {
            action: resolution.action,
            label: resolution.label,
            platform: resolution.platform,
            binding: resolution.shortcut,
        });
    }

    let response =
        encrypt_secure_payload(&state, &session_id, &ActionResolvedResponse { bindings })?;
    Ok(Json(response))
}

pub async fn not_found() -> ApiError {
    ApiError::NotFound
}

fn has_json_content_type(headers: &HeaderMap) -> bool {
    headers
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<mime::Mime>().ok())
        .is_some_and(|value| value.type_() == mime::APPLICATION && value.subtype() == mime::JSON)
}

fn is_valid_request_id(request_id: &str) -> bool {
    (1..=128).contains(&request_id.len())
        && request_id.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.' | ':')
        })
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PairingMessageRequest {
    version: u8,
    message: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PairingRegistrationStartRequest {
    device_id: String,
    display_name: String,
    message: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PairingRegistrationFinishRequest {
    message: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PairingMessageResponse {
    version: u8,
    message: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PairingFinishedResponse {
    version: u8,
    session_id: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PairingRegistrationFinishedResponse {
    version: u8,
    device_id: String,
}

fn decode_pairing_message(message: &str) -> Result<Vec<u8>, ApiError> {
    decode_base64_message(message, 4096)
}

fn decode_secure_message(message: &str) -> Result<Vec<u8>, ApiError> {
    decode_base64_message(message, crate::server::BODY_LIMIT_BYTES)
}

fn decode_base64_message(message: &str, max_encoded_len: usize) -> Result<Vec<u8>, ApiError> {
    if message.is_empty() || message.len() > max_encoded_len {
        return Err(ApiError::PairingProtocolFailed);
    }
    base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(message)
        .or_else(|_| base64::engine::general_purpose::STANDARD.decode(message))
        .map_err(|_| ApiError::PairingProtocolFailed)
}
fn encode_pairing_message(message: &[u8]) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(message)
}

fn publish_security_snapshot(state: &ProtocolModule) {
    state
        .bridge
        .set_security_snapshot(&state.security, &state.publisher);
}

fn pairing_authentication_failed(state: &ProtocolModule) -> ApiError {
    state.security.record_pairing_failure();
    publish_security_snapshot(state);
    ApiError::PairingAuthenticationFailed
}

fn device_authentication_failed(state: &ProtocolModule, device_id: &str) -> ApiError {
    state.security.record_device_login_failure(device_id);
    ApiError::PairingAuthenticationFailed
}

fn active_pairing(state: &ProtocolModule) -> Result<crate::security::PairingSession, ApiError> {
    let pairing = state
        .security
        .pairing()
        .ok_or(ApiError::PairingUnavailable)?;
    if pairing.is_expired(Utc::now()) {
        state.security.clear_pairing_and_code();
        publish_security_snapshot(state);
        return Err(ApiError::PairingUnavailable);
    }
    Ok(pairing)
}

pub async fn pairing_auth_start(
    State(state): State<ProtocolModule>,
    headers: HeaderMap,
    Json(request): Json<PairingMessageRequest>,
) -> Result<Json<PairingMessageResponse>, ApiError> {
    if !has_json_content_type(&headers) {
        return Err(ApiError::UnsupportedMediaType);
    }
    if request.version != PROTOCOL_VERSION {
        return Err(ApiError::PairingProtocolFailed);
    }
    let pairing = active_pairing(&state)?;
    let request = decode_pairing_message(&request.message)
        .map_err(|_| pairing_authentication_failed(&state))?;
    let (response, login_state) = state
        .security
        .opaque()
        .login_start(
            &pairing.opaque_password_file,
            &request,
            pairing.pairing_id.as_bytes(),
        )
        .map_err(|_| pairing_authentication_failed(&state))?;
    if !state.security.set_login_state(login_state) {
        return Err(ApiError::PairingUnavailable);
    }
    Ok(Json(PairingMessageResponse {
        version: PROTOCOL_VERSION,
        message: encode_pairing_message(&response),
    }))
}

pub async fn pairing_auth_finish(
    State(state): State<ProtocolModule>,
    headers: HeaderMap,
    Json(request): Json<PairingMessageRequest>,
) -> Result<Json<PairingFinishedResponse>, ApiError> {
    if !has_json_content_type(&headers) {
        return Err(ApiError::UnsupportedMediaType);
    }
    if request.version != PROTOCOL_VERSION {
        return Err(ApiError::PairingProtocolFailed);
    }
    active_pairing(&state)?;
    let message = decode_pairing_message(&request.message)
        .map_err(|_| pairing_authentication_failed(&state))?;
    let login_state = state
        .security
        .take_login_state()
        .ok_or_else(|| pairing_authentication_failed(&state))?;
    let key = state
        .security
        .opaque()
        .login_finish(&login_state, &message)
        .map_err(|_| pairing_authentication_failed(&state))?;
    let mut rng = rand::rngs::OsRng;
    let session_id = format!("{:016x}{:016x}", rng.next_u64(), rng.next_u64());
    if !state
        .security
        .set_temporary_session(session_id.clone(), key)
    {
        return Err(ApiError::PairingUnavailable);
    }
    Ok(Json(PairingFinishedResponse {
        version: PROTOCOL_VERSION,
        session_id,
    }))
}

pub async fn pairing_register_start(
    State(state): State<ProtocolModule>,
    headers: HeaderMap,
    Json(envelope): Json<SecureEnvelope>,
) -> Result<Json<SecureEnvelope>, ApiError> {
    if !has_json_content_type(&headers) {
        return Err(ApiError::UnsupportedMediaType);
    }
    active_pairing(&state)?;
    let session_id = envelope.session_id.clone();
    let request: PairingRegistrationStartRequest = decrypt_temporary_payload(&state, &envelope)
        .map_err(|_| pairing_authentication_failed(&state))?;
    if request.device_id.is_empty()
        || request.device_id.len() > 128
        || request.display_name.trim().is_empty()
        || request.display_name.chars().count() > 80
    {
        return Err(pairing_authentication_failed(&state));
    }
    let message = decode_pairing_message(&request.message)
        .map_err(|_| pairing_authentication_failed(&state))?;
    let response = state
        .security
        .opaque()
        .registration_start(&message, request.device_id.as_bytes())
        .map_err(|_| pairing_authentication_failed(&state))?;
    if !state.security.set_registration_state(
        vec![1],
        request.device_id,
        request.display_name.trim().to_owned(),
    ) {
        return Err(ApiError::PairingUnavailable);
    }
    let response = PairingMessageResponse {
        version: PROTOCOL_VERSION,
        message: encode_pairing_message(&response),
    };
    Ok(Json(encrypt_temporary_payload(
        &state,
        &session_id,
        &response,
    )?))
}

pub async fn pairing_register_finish(
    State(state): State<ProtocolModule>,
    headers: HeaderMap,
    Json(envelope): Json<SecureEnvelope>,
) -> Result<Json<SecureEnvelope>, ApiError> {
    if !has_json_content_type(&headers) {
        return Err(ApiError::UnsupportedMediaType);
    }
    active_pairing(&state)?;
    let session_id = envelope.session_id.clone();
    let request: PairingRegistrationFinishRequest = decrypt_temporary_payload(&state, &envelope)
        .map_err(|_| pairing_authentication_failed(&state))?;
    let message = decode_pairing_message(&request.message)
        .map_err(|_| pairing_authentication_failed(&state))?;
    let (_state, device_id, display_name) = state
        .security
        .take_registration_state()
        .ok_or_else(|| pairing_authentication_failed(&state))?;
    let password_file = state
        .security
        .opaque()
        .registration_finish(&[], &message)
        .map_err(|_| pairing_authentication_failed(&state))?;
    let now = Utc::now();
    state
        .security
        .add_device(crate::security::DeviceRecord::new(
            device_id.clone(),
            display_name,
            password_file,
            now,
        ))
        .map_err(|error| {
            tracing::error!(%error, "could not save paired device");
            ApiError::StorageUnavailable
        })?;
    let response = encrypt_temporary_payload(
        &state,
        &session_id,
        &PairingRegistrationFinishedResponse {
            version: PROTOCOL_VERSION,
            device_id,
        },
    )?;
    state.security.clear_pairing_and_code();
    publish_security_snapshot(&state);
    Ok(Json(response))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SessionStartRequest {
    version: u8,
    device_id: String,
    message: String,
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SessionFinishRequest {
    version: u8,
    device_id: String,
    handshake_id: String,
    message: String,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionStartResponse {
    version: u8,
    handshake_id: String,
    message: String,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionFinishResponse {
    version: u8,
    session_id: String,
    expires_at: String,
}
fn session_keys(opaque_key: &[u8]) -> Result<([u8; 32], [u8; 32]), ApiError> {
    let mut receive = [0_u8; 32];
    let mut send = [0_u8; 32];
    let hkdf = Hkdf::<Sha256>::new(None, opaque_key);
    hkdf.expand(b"gganbu/client-to-bridge", &mut receive)
        .map_err(|_| ApiError::PairingProtocolFailed)?;
    hkdf.expand(b"gganbu/bridge-to-client", &mut send)
        .map_err(|_| ApiError::PairingProtocolFailed)?;
    Ok((receive, send))
}

pub async fn session_start(
    State(state): State<ProtocolModule>,
    headers: HeaderMap,
    Json(request): Json<SessionStartRequest>,
) -> Result<Json<SessionStartResponse>, ApiError> {
    if !has_json_content_type(&headers) {
        return Err(ApiError::UnsupportedMediaType);
    }
    if request.version != PROTOCOL_VERSION {
        return Err(ApiError::PairingProtocolFailed);
    }
    if !state.security.allow_device_login(&request.device_id) {
        return Err(ApiError::PairingAuthenticationFailed);
    }
    let device = state
        .security
        .find_device(&request.device_id)
        .ok_or_else(|| device_authentication_failed(&state, &request.device_id))?;
    let message = decode_pairing_message(&request.message)
        .map_err(|_| device_authentication_failed(&state, &request.device_id))?;
    let (response, server_state) = state
        .security
        .opaque()
        .login_start(
            &device.opaque_password_file,
            &message,
            request.device_id.as_bytes(),
        )
        .map_err(|_| device_authentication_failed(&state, &request.device_id))?;
    let mut rng = rand::rngs::OsRng;
    let handshake_id = format!("{:016x}{:016x}", rng.next_u64(), rng.next_u64());
    if !state.security.begin_device_login(
        handshake_id.clone(),
        request.device_id.clone(),
        server_state,
    ) {
        return Err(device_authentication_failed(&state, &request.device_id));
    }
    Ok(Json(SessionStartResponse {
        version: PROTOCOL_VERSION,
        handshake_id,
        message: encode_pairing_message(&response),
    }))
}

pub async fn session_finish(
    State(state): State<ProtocolModule>,
    headers: HeaderMap,
    Json(request): Json<SessionFinishRequest>,
) -> Result<Json<SessionFinishResponse>, ApiError> {
    if !has_json_content_type(&headers) {
        return Err(ApiError::UnsupportedMediaType);
    }
    if request.version != PROTOCOL_VERSION {
        return Err(ApiError::PairingProtocolFailed);
    }
    if !state.security.allow_device_login(&request.device_id) {
        return Err(ApiError::PairingAuthenticationFailed);
    }
    let message = decode_pairing_message(&request.message)
        .map_err(|_| device_authentication_failed(&state, &request.device_id))?;
    let server_state = state
        .security
        .take_device_login(&request.handshake_id, &request.device_id)
        .ok_or_else(|| device_authentication_failed(&state, &request.device_id))?;
    let opaque_key = state
        .security
        .opaque()
        .login_finish(&server_state, &message)
        .map_err(|_| device_authentication_failed(&state, &request.device_id))?;
    state
        .security
        .clear_device_login_failures(&request.device_id);
    let (receive_key, send_key) = session_keys(&opaque_key)?;
    let mut rng = rand::rngs::OsRng;
    let session_id = format!("{:016x}{:016x}", rng.next_u64(), rng.next_u64());
    let expires_at = Utc::now() + Duration::minutes(30);
    let session = crate::security::ConnectionSession::new_with_keys(
        session_id.clone(),
        request.device_id.clone(),
        receive_key,
        send_key,
        expires_at,
    );
    state
        .security
        .add_connection(session)
        .map_err(|_| ApiError::PairingAuthenticationFailed)?;
    state
        .security
        .mark_device_seen(&request.device_id, Utc::now())
        .map_err(|error| {
            state.security.remove_connection(&session_id);
            tracing::error!(%error, "could not save device activity");
            ApiError::StorageUnavailable
        })?;
    publish_security_snapshot(&state);
    Ok(Json(SessionFinishResponse {
        version: PROTOCOL_VERSION,
        session_id,
        expires_at: expires_at.to_rfc3339(),
    }))
}

pub async fn session_close(
    State(state): State<ProtocolModule>,
    headers: HeaderMap,
    Json(envelope): Json<SecureEnvelope>,
) -> Result<Json<SecureEnvelope>, ApiError> {
    if !has_json_content_type(&headers) {
        return Err(ApiError::UnsupportedMediaType);
    }
    let session_id = envelope.session_id.clone();
    let _: SecureCloseCommand = decrypt_secure_payload(&state, &envelope)?;
    let response =
        encrypt_secure_payload(&state, &session_id, &SecureCloseResult { closed: true })?;
    if !state.security.remove_connection(&session_id) {
        return Err(ApiError::PairingAuthenticationFailed);
    }
    state
        .bridge
        .set_security_snapshot(&state.security, &state.publisher);
    Ok(Json(response))
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SecureEnvelope {
    version: u8,
    session_id: String,
    counter: u64,
    ciphertext: String,
    authentication_tag: String,
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ResolveCommand {
    actions: Vec<String>,
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SecureCommand {
    action: String,
    request_id: String,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SecureCloseCommand {}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SecureCommandResult {
    accepted: bool,
    request_id: String,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SecureCloseResult {
    closed: bool,
}

fn secure_nonce(session_id: &str, counter: u64) -> [u8; 12] {
    use sha2::Digest;
    let mut hash = Sha256::new();
    hash.update(session_id.as_bytes());
    hash.update(counter.to_be_bytes());
    let digest = hash.finalize();
    digest[..12].try_into().expect("nonce length")
}
fn secure_aad(version: u8, session_id: &str, counter: u64) -> Vec<u8> {
    let mut aad = Vec::with_capacity(10 + session_id.len());
    aad.push(version);
    aad.extend_from_slice(session_id.as_bytes());
    aad.extend_from_slice(&counter.to_be_bytes());
    aad
}

fn temporary_keys(opaque_key: &[u8]) -> Result<([u8; 32], [u8; 32]), ApiError> {
    let mut receive = [0_u8; 32];
    let mut send = [0_u8; 32];
    let hkdf = Hkdf::<Sha256>::new(None, opaque_key);
    hkdf.expand(b"gganbu/pairing-client-to-bridge", &mut receive)
        .map_err(|_| ApiError::PairingProtocolFailed)?;
    hkdf.expand(b"gganbu/pairing-bridge-to-client", &mut send)
        .map_err(|_| ApiError::PairingProtocolFailed)?;
    Ok((receive, send))
}

fn decrypt_temporary_payload<T: DeserializeOwned>(
    state: &ProtocolModule,
    envelope: &SecureEnvelope,
) -> Result<T, ApiError> {
    if envelope.version != PROTOCOL_VERSION
        || envelope.session_id.is_empty()
        || envelope.session_id.len() > 128
    {
        return Err(ApiError::PairingAuthenticationFailed);
    }
    let opaque_key = state
        .security
        .temporary_receive_key(&envelope.session_id, envelope.counter)
        .ok_or(ApiError::PairingAuthenticationFailed)?;
    let (key, _) = temporary_keys(&opaque_key)?;
    let mut plaintext = decode_secure_message(&envelope.ciphertext)
        .map_err(|_| ApiError::PairingAuthenticationFailed)?;
    let tag = decode_pairing_message(&envelope.authentication_tag)
        .map_err(|_| ApiError::PairingAuthenticationFailed)?;
    if tag.len() != 16 {
        return Err(ApiError::PairingAuthenticationFailed);
    }
    chacha20poly1305::ChaCha20Poly1305::new((&key).into())
        .decrypt_in_place_detached(
            chacha20poly1305::Nonce::from_slice(&secure_nonce(
                &envelope.session_id,
                envelope.counter,
            )),
            &secure_aad(envelope.version, &envelope.session_id, envelope.counter),
            &mut plaintext,
            chacha20poly1305::Tag::from_slice(&tag),
        )
        .map_err(|_| ApiError::PairingAuthenticationFailed)?;
    if !state
        .security
        .commit_temporary_receive_counter(&envelope.session_id, envelope.counter)
    {
        return Err(ApiError::PairingAuthenticationFailed);
    }
    serde_json::from_slice(&plaintext).map_err(|_| ApiError::PairingProtocolFailed)
}

fn encrypt_temporary_payload<T: Serialize>(
    state: &ProtocolModule,
    session_id: &str,
    payload: &T,
) -> Result<SecureEnvelope, ApiError> {
    let (opaque_key, counter) = state
        .security
        .next_temporary_send_key(session_id)
        .ok_or(ApiError::PairingAuthenticationFailed)?;
    let (_, key) = temporary_keys(&opaque_key)?;
    let mut ciphertext =
        serde_json::to_vec(payload).map_err(|_| ApiError::PairingProtocolFailed)?;
    let tag = chacha20poly1305::ChaCha20Poly1305::new((&key).into())
        .encrypt_in_place_detached(
            chacha20poly1305::Nonce::from_slice(&secure_nonce(session_id, counter)),
            &secure_aad(PROTOCOL_VERSION, session_id, counter),
            &mut ciphertext,
        )
        .map_err(|_| ApiError::PairingProtocolFailed)?;
    Ok(SecureEnvelope {
        version: PROTOCOL_VERSION,
        session_id: session_id.to_owned(),
        counter,
        ciphertext: encode_pairing_message(&ciphertext),
        authentication_tag: encode_pairing_message(&tag),
    })
}

fn decrypt_secure_payload<T: DeserializeOwned>(
    state: &ProtocolModule,
    envelope: &SecureEnvelope,
) -> Result<T, ApiError> {
    if envelope.version != PROTOCOL_VERSION
        || envelope.session_id.is_empty()
        || envelope.session_id.len() > 128
    {
        return Err(ApiError::PairingAuthenticationFailed);
    }
    let key = state
        .security
        .receive_key(&envelope.session_id, envelope.counter)
        .ok_or(ApiError::PairingAuthenticationFailed)?;
    let mut plaintext = decode_secure_message(&envelope.ciphertext)
        .map_err(|_| ApiError::PairingAuthenticationFailed)?;
    let tag_bytes = decode_pairing_message(&envelope.authentication_tag)
        .map_err(|_| ApiError::PairingAuthenticationFailed)?;
    if tag_bytes.len() != 16 {
        return Err(ApiError::PairingAuthenticationFailed);
    }
    let cipher = chacha20poly1305::ChaCha20Poly1305::new((&key).into());
    let nonce = secure_nonce(&envelope.session_id, envelope.counter);
    cipher
        .decrypt_in_place_detached(
            chacha20poly1305::Nonce::from_slice(&nonce),
            &secure_aad(envelope.version, &envelope.session_id, envelope.counter),
            &mut plaintext,
            chacha20poly1305::Tag::from_slice(&tag_bytes),
        )
        .map_err(|_| ApiError::PairingAuthenticationFailed)?;
    if !state
        .security
        .commit_receive_counter(&envelope.session_id, envelope.counter)
    {
        return Err(ApiError::PairingAuthenticationFailed);
    }
    serde_json::from_slice(&plaintext).map_err(|_| ApiError::PairingProtocolFailed)
}

fn encrypt_secure_payload<T: Serialize>(
    state: &ProtocolModule,
    session_id: &str,
    payload: &T,
) -> Result<SecureEnvelope, ApiError> {
    let (send_key, counter) = state
        .security
        .next_send_key(session_id)
        .ok_or(ApiError::PairingAuthenticationFailed)?;
    let mut ciphertext = serde_json::to_vec(payload).map_err(|_| ApiError::ExecutionFailed)?;
    let nonce = secure_nonce(session_id, counter);
    let cipher = chacha20poly1305::ChaCha20Poly1305::new((&send_key).into());
    let tag = cipher
        .encrypt_in_place_detached(
            chacha20poly1305::Nonce::from_slice(&nonce),
            &secure_aad(PROTOCOL_VERSION, session_id, counter),
            &mut ciphertext,
        )
        .map_err(|_| ApiError::ExecutionFailed)?;
    Ok(SecureEnvelope {
        version: PROTOCOL_VERSION,
        session_id: session_id.to_owned(),
        counter,
        ciphertext: encode_pairing_message(&ciphertext),
        authentication_tag: encode_pairing_message(&tag),
    })
}

pub async fn secure_command(
    State(state): State<ProtocolModule>,
    headers: HeaderMap,
    Json(envelope): Json<SecureEnvelope>,
) -> Result<Json<SecureEnvelope>, ApiError> {
    if !has_json_content_type(&headers) {
        return Err(ApiError::UnsupportedMediaType);
    }
    let session_id = envelope.session_id.clone();
    let command: SecureCommand = decrypt_secure_payload(&state, &envelope)?;
    if !is_valid_action_code(&command.action) || !is_valid_request_id(&command.request_id) {
        return Err(ApiError::InvalidActionRequest);
    }
    let received_at = Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true);
    let resolution = match state.action_resolver.resolve(&command.action) {
        Ok(resolution) => resolution,
        Err(error) => {
            let (api_error, message) = match error {
                ResolveError::UnknownAction => (
                    ApiError::UnknownAction,
                    "Rejected: action is not allowlisted.",
                ),
                ResolveError::CatalogUnavailable => (
                    ApiError::ExecutionFailed,
                    "Rejected: the action catalog is temporarily unavailable.",
                ),
                ResolveError::UnsupportedPlatform => (
                    ApiError::UnsupportedPlatform,
                    "Rejected: this operating system has no action mapping.",
                ),
            };
            state.bridge.record_execute_attempt(
                ActionAttempt {
                    action: command.action,
                    request_id: command.request_id,
                    label: None,
                    platform: state.action_resolver.platform(),
                    shortcut: None,
                    validation: ActionValidation::Rejected,
                    validation_message: message.to_owned(),
                    executed: false,
                    received_at,
                    sequence: 0,
                },
                &state.publisher,
            );
            return Err(api_error);
        }
    };
    if !state
        .security
        .reserve_request_id(&session_id, &command.request_id)
    {
        return Err(ApiError::DuplicateRequest);
    }
    let execution = state.keyboard_execution.clone();
    let key_codes = resolution.shortcut.key_codes.clone();
    let execution_result = tokio::task::spawn_blocking(move || execution.execute(&key_codes))
        .await
        .map_err(|error| format!("keyboard execution task failed: {error}"))
        .and_then(|result| result.map_err(|error| error.to_string()));
    if let Err(error) = execution_result {
        state.bridge.record_execute_attempt(
            ActionAttempt {
                action: command.action,
                request_id: command.request_id,
                label: Some(resolution.label),
                platform: resolution.platform,
                shortcut: Some(resolution.shortcut),
                validation: ActionValidation::Rejected,
                validation_message: format!("Execution failed: {error}"),
                executed: false,
                received_at,
                sequence: 0,
            },
            &state.publisher,
        );
        return Err(ApiError::ExecutionFailed);
    }
    state.bridge.record_execute_attempt(
        ActionAttempt {
            action: command.action,
            request_id: command.request_id.clone(),
            label: Some(resolution.label),
            platform: resolution.platform,
            shortcut: Some(resolution.shortcut),
            validation: ActionValidation::Accepted,
            validation_message: "Keyboard shortcut executed successfully.".to_owned(),
            executed: true,
            received_at,
            sequence: 0,
        },
        &state.publisher,
    );
    let response = encrypt_secure_payload(
        &state,
        &session_id,
        &SecureCommandResult {
            accepted: true,
            request_id: command.request_id,
        },
    )?;
    Ok(Json(response))
}
