// SPDX-FileCopyrightText: 2026 Game Buddy
// SPDX-License-Identifier: AGPL-3.0-only

use std::{
    collections::HashMap,
    io,
    net::{IpAddr, SocketAddr},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use axum::{
    Router,
    body::Body,
    extract::{ConnectInfo, State},
    http::{
        Method, Request, StatusCode,
        header::{CACHE_CONTROL, CONTENT_TYPE},
    },
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;
use tower_http::{
    cors::{AllowOrigin, CorsLayer},
    limit::RequestBodyLimitLayer,
    trace::TraceLayer,
};

use crate::{
    actions::ActionResolver,
    config::AllowedOrigins,
    keyboard::KeyboardExecution,
    protocol::{self, ApiError},
    state::{ServerStatus, SharedBridgeState, StatePublisher},
};

pub const BODY_LIMIT_BYTES: usize = 32 * 1024;

#[derive(Clone, Default)]
struct PairingRateLimiter {
    attempts: Arc<Mutex<HashMap<IpAddr, Vec<Instant>>>>,
}

impl PairingRateLimiter {
    fn allow(&self, ip: IpAddr) -> bool {
        const WINDOW: Duration = Duration::from_secs(60);
        const MAX_ATTEMPTS: usize = 30;
        let now = Instant::now();
        let mut attempts = self.attempts.lock().expect("rate limiter lock poisoned");
        attempts.retain(|_, entries| {
            entries.retain(|started| now.duration_since(*started) < WINDOW);
            !entries.is_empty()
        });
        let entries = attempts.entry(ip).or_default();
        entries.retain(|started| now.duration_since(*started) < WINDOW);
        if entries.len() >= MAX_ATTEMPTS {
            return false;
        }
        entries.push(now);
        true
    }
}

pub fn build_router(
    origins: AllowedOrigins,
    bridge: SharedBridgeState,
    publisher: StatePublisher,
) -> Router {
    build_router_with_execution(origins, bridge, publisher, KeyboardExecution::disabled())
}

pub fn build_router_with_execution(
    origins: AllowedOrigins,
    bridge: SharedBridgeState,
    publisher: StatePublisher,
    keyboard_execution: KeyboardExecution,
) -> Router {
    build_router_with_execution_and_security(
        origins,
        bridge,
        publisher,
        keyboard_execution,
        crate::security::SecurityState::default(),
    )
}

pub fn build_router_with_execution_and_security(
    origins: AllowedOrigins,
    bridge: SharedBridgeState,
    publisher: StatePublisher,
    keyboard_execution: KeyboardExecution,
    security: crate::security::SecurityState,
) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(AllowOrigin::list(origins.header_values()))
        .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
        .allow_headers([CONTENT_TYPE])
        .allow_private_network(true);
    let origin_policy = Arc::new(origins);
    let pairing_rate_limiter = PairingRateLimiter::default();
    let action_resolver = ActionResolver::for_current_platform();

    Router::new()
        .route("/v1/health", get(protocol::health))
        .route("/v1/message", post(protocol::accept_message))
        .route("/v1/messages", post(protocol::accept_message))
        .route("/v1/actions/resolve", post(protocol::resolve_action))
        .route("/v1/actions/execute", post(protocol::secure_command))
        .route("/v1/pairing/auth/start", post(protocol::pairing_auth_start))
        .route(
            "/v1/pairing/auth/finish",
            post(protocol::pairing_auth_finish),
        )
        .route(
            "/v1/pairing/register/start",
            post(protocol::pairing_register_start),
        )
        .route(
            "/v1/pairing/register/finish",
            post(protocol::pairing_register_finish),
        )
        .route("/v1/session/start", post(protocol::session_start))
        .route("/v1/session/finish", post(protocol::session_finish))
        .route("/v1/session/close", post(protocol::session_close))
        .fallback(protocol::not_found)
        .with_state(protocol::ProtocolModule::new(
            bridge,
            publisher,
            action_resolver,
            keyboard_execution,
            security,
        ))
        .layer(TraceLayer::new_for_http())
        .layer(RequestBodyLimitLayer::new(BODY_LIMIT_BYTES))
        .layer(middleware::from_fn(normalize_limit_response))
        .layer(middleware::from_fn(enforce_json_content_type))
        .layer(middleware::from_fn(add_no_store_header))
        .layer(cors)
        .layer(middleware::from_fn_with_state(
            pairing_rate_limiter,
            enforce_pairing_rate_limit,
        ))
        .layer(middleware::from_fn_with_state(
            origin_policy,
            enforce_origin,
        ))
}

async fn enforce_pairing_rate_limit(
    State(limiter): State<PairingRateLimiter>,
    request: Request<Body>,
    next: Next,
) -> Response {
    if request.method() == Method::POST
        && (request.uri().path().starts_with("/v1/pairing/")
            || request.uri().path().starts_with("/v1/session/"))
    {
        let ip = request
            .extensions()
            .get::<ConnectInfo<SocketAddr>>()
            .map_or(IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED), |info| {
                info.0.ip()
            });
        if !limiter.allow(ip) {
            tracing::warn!(%ip, "pairing rate limit exceeded");
            return ApiError::PairingAuthenticationFailed.into_response();
        }
    }
    next.run(request).await
}

async fn enforce_origin(
    axum::extract::State(origins): axum::extract::State<Arc<AllowedOrigins>>,
    request: Request<Body>,
    next: Next,
) -> Response {
    if request
        .headers()
        .get(http::header::ORIGIN)
        .is_some_and(|origin| !origins.contains_header(origin))
    {
        return ApiError::OriginNotAllowed.into_response();
    }

    next.run(request).await
}

async fn normalize_limit_response(request: Request<Body>, next: Next) -> Response {
    let response = next.run(request).await;
    if response.status() == StatusCode::PAYLOAD_TOO_LARGE {
        ApiError::PayloadTooLarge.into_response()
    } else {
        response
    }
}

async fn enforce_json_content_type(request: Request<Body>, next: Next) -> Response {
    if request.method() == Method::POST {
        let is_json = request
            .headers()
            .get(CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| {
                value.split(';').next().is_some_and(|media_type| {
                    media_type.trim().eq_ignore_ascii_case("application/json")
                })
            });
        if !is_json {
            return ApiError::UnsupportedMediaType.into_response();
        }
    }
    next.run(request).await
}

async fn add_no_store_header(request: Request<Body>, next: Next) -> Response {
    let mut response = next.run(request).await;
    response
        .headers_mut()
        .insert(CACHE_CONTROL, http::HeaderValue::from_static("no-store"));
    response
}

pub async fn bind_all_interfaces(port: u16) -> io::Result<TcpListener> {
    TcpListener::bind(SocketAddr::from((std::net::Ipv4Addr::UNSPECIFIED, port))).await
}

pub async fn serve_on_address(
    address: SocketAddr,
    router: Router,
    bridge: SharedBridgeState,
    publisher: StatePublisher,
    cancellation: CancellationToken,
) -> io::Result<()> {
    let listener = match TcpListener::bind(address).await {
        Ok(listener) => listener,
        Err(error) => {
            bridge.set_server_status(
                ServerStatus::Error,
                Some(format!("Unable to listen on {address}: {error}")),
                &publisher,
            );
            return Err(error);
        }
    };

    serve_listener(listener, router, bridge, publisher, cancellation).await
}

pub async fn serve_listener(
    listener: TcpListener,
    router: Router,
    bridge: SharedBridgeState,
    publisher: StatePublisher,
    cancellation: CancellationToken,
) -> io::Result<()> {
    let address = listener.local_addr()?;
    bridge.set_server_status(ServerStatus::Running, None, &publisher);
    tracing::info!(%address, "bridge server listening");

    let result = axum::serve(
        listener,
        router.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(cancellation.cancelled_owned())
    .await;

    match &result {
        Ok(()) => bridge.set_server_status(ServerStatus::Stopped, None, &publisher),
        Err(error) => bridge.set_server_status(
            ServerStatus::Error,
            Some(format!("Local server stopped unexpectedly: {error}")),
            &publisher,
        ),
    }

    result
}

#[cfg(test)]
mod tests {
    use std::{net::IpAddr, sync::Mutex, time::Duration};

    use axum::{
        body::Body,
        http::{Method, Request, StatusCode, header},
    };
    use base64::Engine as _;
    use chacha20poly1305::aead::{AeadInPlace, KeyInit};
    use http_body_util::BodyExt;
    use serde_json::{Value, json};
    use sha2::Digest;
    use tokio_util::sync::CancellationToken;
    use tower::ServiceExt;

    use super::*;
    use crate::{
        config::AllowedOrigins,
        keyboard::{ExecutionError, KeyboardExecutor},
        security::{ConnectionSession, DeviceRecord, PairingSession, SecurityState},
        state::{ServerStatus, noop_publisher},
    };

    const TEST_SESSION_ID: &str = "test-session";
    const TEST_SESSION_KEY: [u8; 32] = [1; 32];

    #[derive(Debug, Default)]
    struct RecordingExecutor {
        calls: Mutex<Vec<Vec<u16>>>,
        failure: Option<&'static str>,
    }

    impl KeyboardExecutor for RecordingExecutor {
        fn execute(&self, key_codes: &[u16]) -> Result<(), ExecutionError> {
            self.calls.lock().unwrap().push(key_codes.to_vec());
            match self.failure {
                Some(message) => Err(ExecutionError::new(message)),
                None => Ok(()),
            }
        }
    }

    fn active_security() -> SecurityState {
        let security = SecurityState::default();
        let now = chrono::Utc::now();
        security
            .add_device(DeviceRecord::new(
                "test-device".to_owned(),
                "Test Device".to_owned(),
                vec![1],
                now,
            ))
            .unwrap();
        assert!(
            security
                .add_connection(ConnectionSession::new(
                    TEST_SESSION_ID.to_owned(),
                    "test-device".to_owned(),
                    TEST_SESSION_KEY,
                    now + chrono::Duration::minutes(30),
                ))
                .is_ok()
        );
        security
    }

    fn test_app() -> (Router, SharedBridgeState) {
        let origins = AllowedOrigins::parse_override(None).unwrap();
        let bridge = SharedBridgeState::new(origins.values().to_vec());
        let router = build_router_with_execution_and_security(
            origins,
            bridge.clone(),
            noop_publisher(),
            KeyboardExecution::disabled(),
            active_security(),
        );
        (router, bridge)
    }

    fn test_execution_app(executor: Arc<RecordingExecutor>) -> (Router, SharedBridgeState) {
        let origins = AllowedOrigins::parse_override(None).unwrap();
        let bridge = SharedBridgeState::new(origins.values().to_vec());
        let execution = KeyboardExecution::with_executor(executor);
        let router = build_router_with_execution_and_security(
            origins,
            bridge.clone(),
            noop_publisher(),
            execution,
            active_security(),
        );
        (router, bridge)
    }

    fn request(method: Method, uri: &str, body: impl Into<Body>) -> Request<Body> {
        Request::builder()
            .method(method)
            .uri(uri)
            .body(body.into())
            .unwrap()
    }

    fn json_request(body: impl Into<Body>) -> Request<Body> {
        Request::builder()
            .method(Method::POST)
            .uri("/v1/message")
            .header(header::CONTENT_TYPE, "application/json")
            .body(body.into())
            .unwrap()
    }

    fn secure_nonce(session_id: &str, counter: u64) -> [u8; 12] {
        let mut hash = sha2::Sha256::new();
        hash.update(session_id.as_bytes());
        hash.update(counter.to_be_bytes());
        hash.finalize()[..12].try_into().unwrap()
    }

    fn secure_aad(session_id: &str, counter: u64) -> Vec<u8> {
        let mut aad = Vec::with_capacity(10 + session_id.len());
        aad.push(crate::protocol::PROTOCOL_VERSION);
        aad.extend_from_slice(session_id.as_bytes());
        aad.extend_from_slice(&counter.to_be_bytes());
        aad
    }

    fn secure_request(uri: &str, plaintext: &str, counter: u64, key: [u8; 32]) -> Request<Body> {
        let mut ciphertext = plaintext.as_bytes().to_vec();
        let cipher = chacha20poly1305::ChaCha20Poly1305::new((&key).into());
        let nonce = secure_nonce(TEST_SESSION_ID, counter);
        let tag = cipher
            .encrypt_in_place_detached(
                chacha20poly1305::Nonce::from_slice(&nonce),
                &secure_aad(TEST_SESSION_ID, counter),
                &mut ciphertext,
            )
            .unwrap();
        let body = json!({
            "version": crate::protocol::PROTOCOL_VERSION,
            "sessionId": TEST_SESSION_ID,
            "counter": counter,
            "ciphertext": base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(ciphertext),
            "authenticationTag": base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(tag),
        })
        .to_string();
        Request::builder()
            .method(Method::POST)
            .uri(uri)
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(body))
            .unwrap()
    }

    async fn response_json(response: Response) -> Value {
        let body = response.into_body().collect().await.unwrap().to_bytes();
        serde_json::from_slice(&body).unwrap()
    }

    async fn send_secure(
        router: &Router,
        uri: &str,
        body: &str,
        counter: u64,
        key: [u8; 32],
    ) -> Response {
        router
            .clone()
            .oneshot(secure_request(uri, body, counter, key))
            .await
            .unwrap()
    }

    async fn decrypt_secure_response(response: Response) -> Response {
        if response.status() != StatusCode::OK {
            return response;
        }
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let envelope: Value = serde_json::from_slice(&body).unwrap();
        let counter = envelope["counter"].as_u64().unwrap();
        let mut ciphertext = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(envelope["ciphertext"].as_str().unwrap())
            .unwrap();
        let tag = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(envelope["authenticationTag"].as_str().unwrap())
            .unwrap();
        let cipher = chacha20poly1305::ChaCha20Poly1305::new((&TEST_SESSION_KEY).into());
        cipher
            .decrypt_in_place_detached(
                chacha20poly1305::Nonce::from_slice(&secure_nonce(TEST_SESSION_ID, counter)),
                &secure_aad(TEST_SESSION_ID, counter),
                &mut ciphertext,
                chacha20poly1305::Tag::from_slice(&tag),
            )
            .unwrap();
        Response::builder()
            .status(StatusCode::OK)
            .body(Body::from(ciphertext))
            .unwrap()
    }

    async fn send_json(router: &Router, body: &str, counter: u64) -> Response {
        decrypt_secure_response(
            send_secure(router, "/v1/message", body, counter, TEST_SESSION_KEY).await,
        )
        .await
    }

    async fn send_action(router: &Router, body: &str, counter: u64) -> Response {
        decrypt_secure_response(
            send_secure(
                router,
                "/v1/actions/resolve",
                body,
                counter,
                TEST_SESSION_KEY,
            )
            .await,
        )
        .await
    }

    async fn send_execution(router: &Router, body: &str, counter: u64, key: [u8; 32]) -> Response {
        send_secure(router, "/v1/actions/execute", body, counter, key).await
    }

    #[test]
    fn pairing_rate_limiter_rejects_bursts_per_ip() {
        let limiter = PairingRateLimiter::default();
        let ip = "192.0.2.1".parse().unwrap();
        for _ in 0..30 {
            assert!(limiter.allow(ip));
        }
        assert!(!limiter.allow(ip));
        assert!(limiter.allow("192.0.2.2".parse().unwrap()));
    }

    #[tokio::test]
    async fn health_returns_versioned_schema() {
        let (router, _) = test_app();
        let response = router
            .oneshot(request(Method::GET, "/v1/health", Body::empty()))
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get(header::CACHE_CONTROL).unwrap(),
            "no-store"
        );
        assert_eq!(
            response_json(response).await,
            json!({
                "service": "gganbu-bridge",
                "appVersion": crate::protocol::APP_VERSION,
                "protocolVersion": 1,
                "status": "ok"
            })
        );
    }

    #[tokio::test]
    async fn stolen_temporary_session_id_cannot_register_without_key_proof() {
        let origins = AllowedOrigins::parse_override(None).unwrap();
        let bridge = SharedBridgeState::new(origins.values().to_vec());
        let security = SecurityState::default();
        let pairing = PairingSession::new(
            "pairing".into(),
            vec![1],
            chrono::Utc::now() + chrono::Duration::minutes(1),
        );
        assert!(
            security
                .start_pairing_with_code(pairing, "12345678".into())
                .is_ok()
        );
        assert!(security.set_temporary_session(TEST_SESSION_ID.into(), vec![9; 64]));
        bridge.set_security_snapshot(&security, &noop_publisher());
        let router = build_router_with_execution_and_security(
            origins,
            bridge.clone(),
            noop_publisher(),
            KeyboardExecution::disabled(),
            security,
        );

        let response = send_secure(
            &router,
            "/v1/pairing/register/start",
            r#"{"deviceId":"attacker","displayName":"Attacker","message":"AA"}"#,
            1,
            [2; 32],
        )
        .await;
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(
            response_json(response).await["error"]["code"],
            "pairing_authentication_failed"
        );
        assert_eq!(bridge.snapshot().active_pairing.unwrap().failed_attempts, 1);
    }

    #[tokio::test]
    async fn expired_pairing_clears_the_published_snapshot() {
        let origins = AllowedOrigins::parse_override(None).unwrap();
        let bridge = SharedBridgeState::new(origins.values().to_vec());
        let security = SecurityState::default();
        let pairing = PairingSession::new(
            "pairing".into(),
            vec![1],
            chrono::Utc::now() - chrono::Duration::seconds(1),
        );
        assert!(
            security
                .start_pairing_with_code(pairing, "12345678".into())
                .is_ok()
        );
        assert!(security.set_temporary_session(TEST_SESSION_ID.into(), vec![9; 64]));
        let router = build_router_with_execution_and_security(
            origins,
            bridge.clone(),
            noop_publisher(),
            KeyboardExecution::disabled(),
            security.clone(),
        );

        let response = send_secure(
            &router,
            "/v1/pairing/register/start",
            r#"{"deviceId":"device","displayName":"Browser","message":"AA"}"#,
            1,
            [2; 32],
        )
        .await;
        assert_eq!(response.status(), StatusCode::CONFLICT);
        assert!(security.pairing().is_none());
        assert!(bridge.snapshot().active_pairing.is_none());
    }

    #[tokio::test]
    async fn valid_messages_are_trimmed_sequenced_and_replace_only_the_latest() {
        let (router, bridge) = test_app();
        bridge.set_server_status(ServerStatus::Running, None, &noop_publisher());

        let first = send_json(&router, r#"{"message":"  hello  "}"#, 1).await;
        assert_eq!(first.status(), StatusCode::OK);
        let first_body = response_json(first).await;
        assert_eq!(first_body["accepted"], true);
        assert_eq!(first_body["sequence"], 1);
        assert!(
            first_body["receivedAt"]
                .as_str()
                .is_some_and(|value| value.ends_with('Z'))
        );

        let second = send_json(&router, r#"{"message":"second"}"#, 2).await;
        assert_eq!(second.status(), StatusCode::OK);
        assert_eq!(response_json(second).await["sequence"], 2);

        let snapshot = bridge.snapshot();
        let latest = snapshot.last_message.unwrap();
        assert_eq!(latest.text, "second");
        assert_eq!(latest.sequence, 2);
        assert!(latest.received_at.ends_with('Z'));
        assert_eq!(snapshot.server_status, ServerStatus::Running);
        assert_eq!(snapshot.revision, 3);
    }

    #[tokio::test]
    async fn known_actions_resolve_without_execution_and_update_state() {
        let (router, bridge) = test_app();
        let response = send_action(&router, r#"{"actions":["ID_SCOUT_UAV"]}"#, 1).await;

        assert_eq!(response.status(), StatusCode::OK);
        let body = response_json(response).await;
        assert_eq!(body["bindings"].as_array().unwrap().len(), 1);
        assert_eq!(body["bindings"][0]["action"], "ID_SCOUT_UAV");
        assert_eq!(body["bindings"][0]["label"], "Scout UAV");
        assert_eq!(body["bindings"][0]["platform"], "linux");
        assert_eq!(body["bindings"][0]["binding"]["keyCodes"], json!([46, 47]));
        assert_eq!(body["bindings"][0]["binding"]["modifiers"], json!([]));
        assert_eq!(body["bindings"][0]["binding"]["keys"], json!(["C", "V"]));
        assert_eq!(body["bindings"][0]["binding"]["display"], "C + V");

        let snapshot = bridge.snapshot();
        let attempt = snapshot.last_action.unwrap();
        assert_eq!(attempt.action, "ID_SCOUT_UAV");
        assert_eq!(attempt.request_id, "resolve-0");
        assert_eq!(attempt.shortcut.unwrap().display, "C + V");
        assert_eq!(attempt.validation, crate::state::ActionValidation::Accepted);
        assert!(!attempt.executed);
        assert!(snapshot.last_message.is_none());
    }

    #[tokio::test]
    async fn resolve_accepts_the_maximum_batch_and_rejects_an_oversized_outer_body() {
        let (router, _) = test_app();
        let actions = (0..128)
            .map(|index| format!("ID_{index:03}_{}", "X".repeat(121)))
            .collect::<Vec<_>>();
        assert!(actions.iter().all(|action| action.len() == 128));
        let plaintext = json!({ "actions": actions }).to_string();
        let accepted = send_action(&router, &plaintext, 1).await;
        assert_eq!(accepted.status(), StatusCode::OK);
        assert!(
            response_json(accepted).await["bindings"]
                .as_array()
                .unwrap()
                .is_empty()
        );

        let oversized = "x".repeat(BODY_LIMIT_BYTES + 1);
        let rejected = router
            .clone()
            .oneshot(json_request(oversized))
            .await
            .unwrap();
        assert_eq!(rejected.status(), StatusCode::PAYLOAD_TOO_LARGE);
        assert_eq!(
            response_json(rejected).await["error"]["code"],
            "payload_too_large"
        );
    }

    #[tokio::test]
    async fn message_and_resolve_require_an_active_paired_connection() {
        let origins = AllowedOrigins::parse_override(None).unwrap();
        let bridge = SharedBridgeState::new(origins.values().to_vec());
        let router = build_router(origins, bridge.clone(), noop_publisher());

        let message_response = send_secure(
            &router,
            "/v1/message",
            r#"{"message":"must not be accepted"}"#,
            1,
            TEST_SESSION_KEY,
        )
        .await;
        assert_eq!(message_response.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(
            response_json(message_response).await["error"]["code"],
            "pairing_authentication_failed"
        );

        let resolve_response = send_action(
            &router,
            r#"{"action":"ID_SCOUT_UAV","requestId":"resolve-without-connection"}"#,
            2,
        )
        .await;
        assert_eq!(resolve_response.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(
            response_json(resolve_response).await["error"]["code"],
            "pairing_authentication_failed"
        );

        let snapshot = bridge.snapshot();
        assert!(snapshot.last_message.is_none());
        assert!(snapshot.last_action.is_none());
    }

    #[tokio::test]
    async fn authenticated_actions_execute_only_resolved_scan_codes() {
        let executor = Arc::new(RecordingExecutor::default());
        let (router, bridge) = test_execution_app(executor.clone());
        let response = send_execution(
            &router,
            r#"{"action":"ID_SCOUT_UAV","requestId":"execute-001"}"#,
            1,
            TEST_SESSION_KEY,
        )
        .await;

        assert_eq!(response.status(), StatusCode::OK);
        let body = response_json(response).await;
        assert_eq!(body["sessionId"], TEST_SESSION_ID);
        assert_eq!(body["counter"], 1);
        assert_eq!(*executor.calls.lock().unwrap(), vec![vec![46, 47]]);
        let attempt = bridge.snapshot().last_action.unwrap();
        assert!(attempt.executed);
        assert_eq!(attempt.validation, crate::state::ActionValidation::Accepted);
    }

    #[tokio::test]
    async fn execution_requires_proof_for_the_specific_session() {
        let executor = Arc::new(RecordingExecutor::default());
        let (router, bridge) = test_execution_app(executor.clone());
        let forged = send_execution(
            &router,
            r#"{"action":"ID_SCOUT_UAV","requestId":"forged-device"}"#,
            1,
            [2; 32],
        )
        .await;
        assert_eq!(forged.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(
            response_json(forged).await["error"]["code"],
            "pairing_authentication_failed"
        );
        assert!(executor.calls.lock().unwrap().is_empty());
        assert!(bridge.snapshot().last_action.is_none());

        let owner = send_execution(
            &router,
            r#"{"action":"ID_SCOUT_UAV","requestId":"session-owner"}"#,
            1,
            TEST_SESSION_KEY,
        )
        .await;
        assert_eq!(owner.status(), StatusCode::OK);
        assert_eq!(*executor.calls.lock().unwrap(), vec![vec![46, 47]]);
    }

    #[tokio::test]
    async fn plaintext_bearer_requests_cannot_execute() {
        let executor = Arc::new(RecordingExecutor::default());
        let (router, bridge) = test_execution_app(executor.clone());
        let request = Request::builder()
            .method(Method::POST)
            .uri("/v1/actions/execute")
            .header(header::CONTENT_TYPE, "application/json")
            .header(
                header::AUTHORIZATION,
                "Bearer 0123456789abcdef0123456789abcdef",
            )
            .body(Body::from(
                r#"{"action":"ID_SCOUT_UAV","requestId":"legacy"}"#,
            ))
            .unwrap();
        let response = router.clone().oneshot(request).await.unwrap();
        assert!(response.status().is_client_error());
        assert!(executor.calls.lock().unwrap().is_empty());
        assert!(bridge.snapshot().last_action.is_none());
    }

    #[tokio::test]
    async fn replayed_counters_and_duplicate_request_ids_execute_only_once() {
        let executor = Arc::new(RecordingExecutor::default());
        let (router, _) = test_execution_app(executor.clone());
        let command = r#"{"action":"ID_SCOUT_UAV","requestId":"execute-once"}"#;

        assert_eq!(
            send_execution(&router, command, 1, TEST_SESSION_KEY)
                .await
                .status(),
            StatusCode::OK
        );
        let replay = send_execution(&router, command, 1, TEST_SESSION_KEY).await;
        assert_eq!(replay.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(
            response_json(replay).await["error"]["code"],
            "pairing_authentication_failed"
        );
        let duplicate = send_execution(&router, command, 2, TEST_SESSION_KEY).await;
        assert_eq!(duplicate.status(), StatusCode::CONFLICT);
        assert_eq!(
            response_json(duplicate).await["error"]["code"],
            "duplicate_request"
        );
        assert_eq!(*executor.calls.lock().unwrap(), vec![vec![46, 47]]);
    }

    #[tokio::test]
    async fn operating_system_execution_failures_are_visible_and_not_marked_executed() {
        let executor = Arc::new(RecordingExecutor {
            calls: Mutex::default(),
            failure: Some("permission denied by test backend"),
        });
        let (router, bridge) = test_execution_app(executor);
        let response = send_execution(
            &router,
            r#"{"action":"ID_SCOUT_UAV","requestId":"execute-failure"}"#,
            1,
            TEST_SESSION_KEY,
        )
        .await;

        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(
            response_json(response).await["error"]["code"],
            "execution_failed"
        );
        let attempt = bridge.snapshot().last_action.unwrap();
        assert!(!attempt.executed);
        assert_eq!(attempt.validation, crate::state::ActionValidation::Rejected);
        assert!(attempt.validation_message.contains("permission denied"));
    }

    #[tokio::test]
    async fn session_close_requires_session_proof() {
        let executor = Arc::new(RecordingExecutor::default());
        let origins = AllowedOrigins::parse_override(None).unwrap();
        let bridge = SharedBridgeState::new(origins.values().to_vec());
        let security = active_security();
        let router = build_router_with_execution_and_security(
            origins,
            bridge,
            noop_publisher(),
            KeyboardExecution::with_executor(executor),
            security.clone(),
        );

        let forged = send_secure(&router, "/v1/session/close", "{}", 1, [2; 32]).await;
        assert_eq!(forged.status(), StatusCode::UNAUTHORIZED);
        assert!(security.has_active_connection());

        let owner = send_secure(&router, "/v1/session/close", "{}", 1, TEST_SESSION_KEY).await;
        assert_eq!(owner.status(), StatusCode::OK);
        assert!(!security.has_active_connection());
    }

    #[tokio::test]
    async fn unknown_actions_are_skipped_without_changing_state() {
        let (router, bridge) = test_app();
        let response = send_action(&router, r#"{"actions":["fire_weapon"]}"#, 1).await;

        assert_eq!(response.status(), StatusCode::OK);
        assert!(
            response_json(response).await["bindings"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        assert!(bridge.snapshot().last_action.is_none());
    }

    #[tokio::test]
    async fn malformed_action_requests_cannot_replace_the_last_action() {
        let (router, bridge) = test_app();
        assert_eq!(
            send_action(&router, r#"{"actions":["ID_TACTICAL_MAP"]}"#, 1,)
                .await
                .status(),
            StatusCode::OK
        );
        let valid_snapshot = bridge.snapshot();

        let cases = [
            (r#"{}"#, StatusCode::BAD_REQUEST),
            (r#"{"actions":[]}"#, StatusCode::UNPROCESSABLE_ENTITY),
            (r#"{"actions":[7]}"#, StatusCode::BAD_REQUEST),
            (
                r#"{"actions":["Toggle Map"]}"#,
                StatusCode::UNPROCESSABLE_ENTITY,
            ),
            (
                r#"{"actions":["ID_TACTICAL_MAP","bad id"]}"#,
                StatusCode::UNPROCESSABLE_ENTITY,
            ),
            (
                r#"{"actions":["ID_TACTICAL_MAP"],"shortcut":"Ctrl+X"}"#,
                StatusCode::BAD_REQUEST,
            ),
            (r#"{"actions":["ID_TACTICAL_MAP]"#, StatusCode::BAD_REQUEST),
        ];

        for (index, (body, expected_status)) in cases.into_iter().enumerate() {
            let response = send_action(&router, body, (index + 2) as u64).await;
            assert_eq!(response.status(), expected_status, "body: {body}");
            let code = response_json(response).await["error"]["code"]
                .as_str()
                .unwrap()
                .to_owned();
            if expected_status == StatusCode::BAD_REQUEST {
                assert_eq!(code, "pairing_protocol_failed");
            } else {
                assert_eq!(code, "invalid_action_request");
            }
            assert_eq!(bridge.snapshot(), valid_snapshot);
        }
    }

    #[tokio::test]
    async fn action_attempt_sequences_ignore_skipped_unknown_codes() {
        let (router, bridge) = test_app();
        assert_eq!(
            send_action(&router, r#"{"actions":["ID_RANGEFINDER"]}"#, 1,)
                .await
                .status(),
            StatusCode::OK
        );
        assert_eq!(
            send_action(&router, r#"{"actions":["unknown_code"]}"#, 2,)
                .await
                .status(),
            StatusCode::OK
        );
        let third = send_action(&router, r#"{"actions":["ID_CAMERA_BINOCULARS"]}"#, 3).await;

        assert_eq!(
            response_json(third).await["bindings"][0]["action"],
            "ID_CAMERA_BINOCULARS"
        );
        assert_eq!(bridge.snapshot().last_action.unwrap().sequence, 2);
    }

    #[tokio::test]
    async fn invalid_message_shapes_use_required_statuses_and_preserve_state() {
        let (router, bridge) = test_app();
        assert_eq!(
            send_json(&router, r#"{"message":"keep me"}"#, 1)
                .await
                .status(),
            StatusCode::OK
        );
        let valid_snapshot = bridge.snapshot();

        let cases = [
            (r#"{"message":""}"#, StatusCode::UNPROCESSABLE_ENTITY),
            (r#"{"message":"   "}"#, StatusCode::UNPROCESSABLE_ENTITY),
            (r#"{}"#, StatusCode::BAD_REQUEST),
            (r#"{"message":42}"#, StatusCode::BAD_REQUEST),
            (
                r#"{"message":"okay","extra":true}"#,
                StatusCode::BAD_REQUEST,
            ),
            (r#"{"message":"unterminated}"#, StatusCode::BAD_REQUEST),
        ];

        for (index, (body, expected_status)) in cases.into_iter().enumerate() {
            let response = send_json(&router, body, (index + 2) as u64).await;
            assert_eq!(response.status(), expected_status, "body: {body}");
            let json = response_json(response).await;
            let expected_code = if expected_status == StatusCode::BAD_REQUEST {
                "pairing_protocol_failed"
            } else {
                "invalid_message"
            };
            assert_eq!(json["error"]["code"], expected_code);
            assert_eq!(bridge.snapshot(), valid_snapshot);
        }

        let too_long = "x".repeat(257);
        let response = send_json(&router, &json!({ "message": too_long }).to_string(), 8).await;
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(
            response_json(response).await["error"]["code"],
            "invalid_message"
        );
        assert_eq!(bridge.snapshot(), valid_snapshot);
    }

    #[tokio::test]
    async fn oversized_and_wrong_content_type_requests_are_rejected_without_state_change() {
        let (router, bridge) = test_app();
        assert_eq!(
            send_json(&router, r#"{"message":"keep this"}"#, 1)
                .await
                .status(),
            StatusCode::OK
        );
        let valid_snapshot = bridge.snapshot();

        let oversized = format!(r#"{{"message":"{}"}}"#, "x".repeat(BODY_LIMIT_BYTES));
        let response = router
            .clone()
            .oneshot(json_request(oversized))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
        assert_eq!(
            response_json(response).await["error"]["code"],
            "payload_too_large"
        );

        for content_type in [None, Some("text/plain"), Some("application/problem+json")] {
            let mut builder = Request::builder().method(Method::POST).uri("/v1/message");
            if let Some(content_type) = content_type {
                builder = builder.header(header::CONTENT_TYPE, content_type);
            }
            let response = router
                .clone()
                .oneshot(builder.body(Body::from(r#"{"message":"hello"}"#)).unwrap())
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::UNSUPPORTED_MEDIA_TYPE);
            assert_eq!(
                response_json(response).await["error"]["code"],
                "unsupported_media_type"
            );
        }

        assert_eq!(bridge.snapshot(), valid_snapshot);
    }

    #[tokio::test]
    async fn unknown_routes_return_json_error() {
        let (router, _) = test_app();
        let response = router
            .oneshot(request(Method::GET, "/v1/unknown", Body::empty()))
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        assert_eq!(response_json(response).await["error"]["code"], "not_found");
    }

    #[tokio::test]
    async fn allowed_origin_preflight_includes_cors_and_private_network_headers() {
        let (router, _) = test_app();
        let response = router
            .oneshot(
                Request::builder()
                    .method(Method::OPTIONS)
                    .uri("/v1/message")
                    .header(header::ORIGIN, "http://localhost:5173")
                    .header(header::ACCESS_CONTROL_REQUEST_METHOD, "POST")
                    .header(header::ACCESS_CONTROL_REQUEST_HEADERS, "content-type")
                    .header("access-control-request-private-network", "true")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers()[header::ACCESS_CONTROL_ALLOW_ORIGIN],
            "http://localhost:5173"
        );
        assert_eq!(
            response.headers()["access-control-allow-private-network"],
            "true"
        );
        assert!(
            response
                .headers()
                .get(header::ACCESS_CONTROL_ALLOW_CREDENTIALS)
                .is_none()
        );
    }

    #[tokio::test]
    async fn disallowed_origin_is_forbidden_and_requests_without_origin_remain_available() {
        let (router, bridge) = test_app();
        let forbidden = router
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/v1/message")
                    .header(header::ORIGIN, "https://evil.example")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(r#"{"message":"blocked"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(forbidden.status(), StatusCode::FORBIDDEN);
        assert_eq!(
            response_json(forbidden).await["error"]["code"],
            "origin_not_allowed"
        );
        assert!(bridge.snapshot().last_message.is_none());

        let native = send_json(&router, r#"{"message":"native diagnostic"}"#, 1).await;
        assert_eq!(native.status(), StatusCode::OK);
        assert_eq!(
            bridge.snapshot().last_message.unwrap().text,
            "native diagnostic"
        );
    }

    #[tokio::test]
    async fn listener_helper_binds_all_ipv4_interfaces() {
        let listener = bind_all_interfaces(0).await.unwrap();
        let address = listener.local_addr().unwrap();
        assert_eq!(address.ip(), IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED));
    }

    #[tokio::test]
    async fn bind_failure_moves_state_to_error() {
        let occupied = bind_all_interfaces(0).await.unwrap();
        let address = occupied.local_addr().unwrap();
        let (router, bridge) = test_app();

        let result = serve_on_address(
            address,
            router,
            bridge.clone(),
            noop_publisher(),
            CancellationToken::new(),
        )
        .await;

        assert!(result.is_err());
        let snapshot = bridge.snapshot();
        assert_eq!(snapshot.server_status, ServerStatus::Error);
        assert!(
            snapshot
                .server_error
                .is_some_and(|error| error.contains(&address.to_string()))
        );
    }

    #[tokio::test]
    async fn cancellation_completes_shutdown_and_releases_listener() {
        let listener = bind_all_interfaces(0).await.unwrap();
        let address = listener.local_addr().unwrap();
        let (router, bridge) = test_app();
        let cancellation = CancellationToken::new();
        let task = tokio::spawn(serve_listener(
            listener,
            router,
            bridge.clone(),
            noop_publisher(),
            cancellation.clone(),
        ));

        tokio::task::yield_now().await;
        assert_eq!(bridge.snapshot().server_status, ServerStatus::Running);
        cancellation.cancel();
        let result = tokio::time::timeout(Duration::from_secs(2), task)
            .await
            .expect("server did not shut down")
            .expect("server task panicked");
        assert!(result.is_ok());
        assert_eq!(bridge.snapshot().server_status, ServerStatus::Stopped);

        let rebound = TcpListener::bind(address).await;
        assert!(rebound.is_ok(), "listener was not released: {rebound:?}");
    }
}
