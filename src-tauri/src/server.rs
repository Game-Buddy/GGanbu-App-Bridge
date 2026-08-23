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
#[path = "tests/server.rs"]
mod tests;
