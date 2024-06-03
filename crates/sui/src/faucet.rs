// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use axum::{
    extract::Path,
    response::IntoResponse,
    routing::{get, post},
    Extension, Json, Router,
};
use fastcrypto::traits::KeyPair;
use http::{Method, StatusCode};
use std::{net::SocketAddr, path::PathBuf, sync::Arc};

use sui_cluster_test::{
    cluster::LocalNewCluster,
    faucet::{FaucetClient, FaucetClientFactory},
};
use sui_config::Config;
use sui_config::SUI_KEYSTORE_FILENAME;
use sui_faucet::{
    BatchFaucetResponse, BatchStatusFaucetResponse, FaucetError, FaucetRequest, FaucetResponse,
    FixedAmountRequest,
};
use sui_keys::keystore::{AccountKeystore, FileBasedKeystore, Keystore};
use sui_sdk::{
    sui_client_config::{SuiClientConfig, SuiEnv},
    wallet_context::WalletContext,
};
use sui_types::{
    base_types::SuiAddress,
    crypto::{AccountKeyPair, SuiKeyPair},
};

use tower::ServiceBuilder;
use tower_http::cors::{Any, CorsLayer};
use tracing::info;
use uuid::Uuid;

pub(crate) struct AppState {
    faucet: Arc<dyn FaucetClient + Sync + Send>,
}

impl AppState {
    pub fn new(faucet: Arc<dyn FaucetClient + Sync + Send>) -> Self {
        Self { faucet }
    }

    #[allow(unused)]
    pub async fn new_from_cluster(cluster: &LocalNewCluster) -> Self {
        let faucet = FaucetClientFactory::new_from_cluster(cluster).await;
        Self::new(faucet)
    }
}

/// Start the faucet from an `AppState` on the given port.
pub(crate) async fn start_faucet(app_state: Arc<AppState>, port: u16) -> Result<()> {
    let cors = CorsLayer::new()
        .allow_methods(vec![Method::GET, Method::POST])
        .allow_headers(Any)
        .allow_origin(Any);

    let app = Router::new()
        .route("/", get(health))
        .route("/gas", post(faucet_request))
        .route("/v1/gas", post(faucet_batch_request))
        .route("/v1/status/:task_id", get(request_status))
        .layer(
            ServiceBuilder::new()
                .layer(cors)
                .layer(Extension(app_state))
                .into_inner(),
        );

    let addr = SocketAddr::from(([127, 0, 0, 1], port));

    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await?;

    Ok(())
}

/// basic handler that responds with a static string
async fn health() -> &'static str {
    "OK"
}

/// Requests a SUI token from the faucet.
async fn faucet_request(
    Extension(state): Extension<Arc<AppState>>,
    Json(payload): Json<FaucetRequest>,
) -> impl IntoResponse {
    let result = match payload {
        FaucetRequest::FixedAmountRequest(FixedAmountRequest { recipient }) => {
            state.faucet.request_sui_coins(recipient).await
        }
        _ => {
            return (
                StatusCode::BAD_REQUEST,
                Json(FaucetResponse::from(FaucetError::Internal(
                    "Input Error.".to_string(),
                ))),
            )
        }
    };

    if !result.transferred_gas_objects.is_empty() {
        (StatusCode::CREATED, Json(result))
    } else {
        (StatusCode::INTERNAL_SERVER_ERROR, Json(result))
    }
}

/// Make a request to the faucet that will be batched with other rqeuests.
async fn faucet_batch_request(
    Extension(state): Extension<Arc<AppState>>,
    Json(payload): Json<FaucetRequest>,
) -> impl IntoResponse {
    let result = match payload {
        FaucetRequest::FixedAmountRequest(FixedAmountRequest { recipient }) => {
            state.faucet.batch_request_sui_coins(recipient).await
        }
        _ => {
            return (
                StatusCode::BAD_REQUEST,
                Json(BatchFaucetResponse::from(FaucetError::Internal(
                    "Input Error.".to_string(),
                ))),
            )
        }
    };
    if result.task.is_some() {
        (StatusCode::CREATED, Json(result))
    } else {
        (StatusCode::INTERNAL_SERVER_ERROR, Json(result))
    }
}

async fn request_status(
    Extension(state): Extension<Arc<AppState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match Uuid::parse_str(&id) {
        Ok(task_id) => {
            let status = state.faucet.get_batch_send_status(task_id).await;
            (StatusCode::CREATED, Json(status))
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(BatchStatusFaucetResponse::from(FaucetError::Internal(
                e.to_string(),
            ))),
        ),
    }
}

/// Faucet requires its own wallet context in order to send SUI to addresses.
/// This functions initializes a wallet context in a temporary directory, and returns the object.
pub fn new_wallet_context_for_faucet(
    kp: AccountKeyPair,
    config_dir: PathBuf,
    fullnode_url: String,
) -> Result<WalletContext, anyhow::Error> {
    let wallet_config_path = config_dir.join("client.yaml");
    info!("Use RPC: {}", &fullnode_url);
    let keystore_path = config_dir.join(SUI_KEYSTORE_FILENAME);
    let mut keystore = Keystore::from(FileBasedKeystore::new(&keystore_path).unwrap());
    let address: SuiAddress = kp.public().into();
    keystore.add_key(None, SuiKeyPair::Ed25519(kp)).unwrap();
    SuiClientConfig {
        keystore,
        envs: vec![SuiEnv {
            alias: "localnet".to_string(),
            rpc: fullnode_url.into(),
            ws: None,
            basic_auth: None,
        }],
        active_address: Some(address),
        active_env: Some("localnet".to_string()),
    }
    .persisted(&wallet_config_path)
    .save()
    .unwrap();

    info!(
        "Initialize wallet from config path: {:?}",
        wallet_config_path
    );

    Ok(
        WalletContext::new(&wallet_config_path, None, None).unwrap_or_else(|e| {
            panic!(
                "Failed to init wallet context from path {:?}, error: {e}",
                wallet_config_path
            )
        }),
    )
}
