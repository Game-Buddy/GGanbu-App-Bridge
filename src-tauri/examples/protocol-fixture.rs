// SPDX-FileCopyrightText: 2026 Game Buddy
// SPDX-License-Identifier: AGPL-3.0-only

use gganbu_app_bridge_lib::{
    config::AllowedOrigins,
    keyboard::{ExecutionContext, ExecutionError, KeyboardExecution, KeyboardExecutor},
    security::{PairingSession, SecurityState},
    server::build_router_with_execution_and_security,
    state::{SharedBridgeState, noop_publisher},
};
use std::sync::Arc;

struct NoInput;
impl KeyboardExecutor for NoInput {
    fn execute(&self, _: &[u16], context: &ExecutionContext) -> Result<(), ExecutionError> {
        context.check()
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let password = std::env::var("GGANBU_BRIDGE_TEST_PAIRING_CODE")?;
    let security = SecurityState::default();
    let record = security
        .opaque()
        .create_password_file(password.as_bytes(), b"integration-pairing")?;
    security
        .start_pairing_with_code(
            PairingSession::new(
                "integration-pairing".into(),
                record,
                chrono::Utc::now() + chrono::Duration::minutes(1),
            ),
            password,
        )
        .map_err(|_| "pairing failed")?;
    let origins = AllowedOrigins::parse_override(None)?;
    let router = build_router_with_execution_and_security(
        origins,
        SharedBridgeState::new(Vec::new()),
        noop_publisher(),
        KeyboardExecution::with_executor(Arc::new(NoInput)),
        security,
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    println!("http://{}", listener.local_addr()?);
    axum::serve(
        listener,
        router.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .await?;
    Ok(())
}
