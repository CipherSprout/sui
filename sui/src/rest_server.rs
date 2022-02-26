// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use dropshot::{endpoint, Query};
use dropshot::{
    ApiDescription, ConfigDropshot, ConfigLogging, ConfigLoggingLevel, HttpError, HttpResponseOk,
    HttpResponseUpdatedNoContent, HttpServerStarter, RequestContext,
};
use hyper::StatusCode;
use serde_json::json;
use sui::config::{Config, GenesisConfig, NetworkConfig, WalletConfig};
use sui::sui_commands;
use sui::wallet_commands::WalletContext;
use sui_core::client::Client;
use sui_types::base_types::*;
use sui_types::committee::Committee;

use futures::stream::{futures_unordered::FuturesUnordered, StreamExt as _};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::fs;
use std::net::{Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use sui_types::object::ObjectRead;
use tokio::task::{self, JoinHandle};
use tracing::{error, info};

use std::sync::{Arc, Mutex};

#[tokio::main]
async fn main() -> Result<(), String> {
    let config_dropshot: ConfigDropshot = ConfigDropshot {
        bind_address: SocketAddr::from((Ipv4Addr::new(127, 0, 0, 1), 5000)),
        ..Default::default()
    };

    let config_logging = ConfigLogging::StderrTerminal {
        level: ConfigLoggingLevel::Info,
    };
    let log = config_logging
        .to_logger("rest_server")
        .map_err(|error| format!("failed to create logger: {}", error))?;

    tracing_subscriber::fmt().init();

    let mut api = ApiDescription::new();
    api.register(start).unwrap();
    api.register(genesis).unwrap();
    api.register(stop).unwrap();
    api.register(get_addresses).unwrap();
    api.register(get_objects).unwrap();
    api.register(object_info).unwrap();

    let api_context = ServerContext::new();

    let server = HttpServerStarter::new(&config_dropshot, api, api_context, &log)
        .map_err(|error| format!("failed to create server: {}", error))?
        .start();

    server.await
}

/**
 * Server context (state shared by handler functions)
 */
struct ServerContext {
    genesis_config_path: String,
    wallet_config_path: String,
    network_config_path: String,
    authority_db_path: String,
    client_db_path: Arc<Mutex<String>>,
    authority_handles: Arc<Mutex<Vec<JoinHandle<()>>>>,
    wallet_context: Arc<Mutex<Option<WalletContext>>>,
}

impl ServerContext {
    pub fn new() -> ServerContext {
        ServerContext {
            genesis_config_path: String::from("genesis.conf"),
            wallet_config_path: String::from("wallet.conf"),
            network_config_path: String::from("./network.conf"),
            authority_db_path: String::from("./authorities_db"),
            client_db_path: Arc::new(Mutex::new(String::new())),
            authority_handles: Arc::new(Mutex::new(Vec::new())),
            wallet_context: Arc::new(Mutex::new(None)),
        }
    }
}

/**
 * 'GenesisResponse' returns the genesis of wallet & network config.
 */
#[derive(Deserialize, Serialize, JsonSchema)]
struct GenesisResponse {
    wallet_config: serde_json::Value,
    network_config: serde_json::Value,
}

/**
 * [SUI] Use to provide server configurations for genesis.
 */
#[endpoint {
    method = POST,
    path = "/debug/sui/genesis",
}]
async fn genesis(
    rqctx: Arc<RequestContext<ServerContext>>,
) -> Result<HttpResponseOk<GenesisResponse>, HttpError> {
    let server_context = rqctx.context();
    let genesis_config_path = &server_context.genesis_config_path;
    let network_config_path = &server_context.network_config_path;
    let wallet_config_path = &server_context.wallet_config_path;

    let mut network_config = NetworkConfig::read_or_create(&PathBuf::from(network_config_path))
        .map_err(|error| {
            custom_http_error(
                StatusCode::CONFLICT,
                format!("Unable to read network config: {error}"),
            )
        })?;

    if !network_config.authorities.is_empty() {
        return Err(custom_http_error(
            StatusCode::CONFLICT,
            String::from("Cannot run genesis on a existing network, stop network to try again."),
        ));
    }

    let working_dir = network_config.config_path().parent().unwrap().to_owned();
    let genesis_conf = GenesisConfig::default_genesis(&working_dir.join(genesis_config_path))
        .map_err(|error| {
            custom_http_error(
                StatusCode::CONFLICT,
                format!("Unable to create default genesis configuration: {error}"),
            )
        })?;

    let wallet_path = working_dir.join(wallet_config_path);
    let mut wallet_config =
        WalletConfig::create(&working_dir.join(wallet_path)).map_err(|error| {
            custom_http_error(
                StatusCode::CONFLICT,
                format!("Wallet config was unable to be created: {error}"),
            )
        })?;
    // Need to use a random id because rocksdb locks on current process which
    // means even if the directory is deleted the lock will remain causing an
    // IO Error when a restart is attempted.
    let client_db_path = format!("client_db_{:?}", ObjectID::random());
    wallet_config.db_folder_path = working_dir.join(&client_db_path);
    *server_context.client_db_path.lock().unwrap() = client_db_path;

    sui_commands::genesis(&mut network_config, genesis_conf, &mut wallet_config)
        .await
        .map_err(|err| {
            custom_http_error(
                StatusCode::FAILED_DEPENDENCY,
                format!("Genesis error: {:?}", err),
            )
        })?;

    Ok(HttpResponseOk(GenesisResponse {
        wallet_config: json!(wallet_config),
        network_config: json!(network_config),
    }))
}

/**
 * [SUI] Start servers with specified configurations.
 */
#[endpoint {
    method = POST,
    path = "/debug/sui/start",
}]
async fn start(
    rqctx: Arc<RequestContext<ServerContext>>,
) -> Result<HttpResponseOk<String>, HttpError> {
    let server_context = rqctx.context();
    let network_config_path = &server_context.network_config_path;

    let network_config = NetworkConfig::read_or_create(&PathBuf::from(network_config_path))
        .map_err(|error| {
            custom_http_error(
                StatusCode::CONFLICT,
                format!("Unable to read network config: {error}"),
            )
        })?;

    if network_config.authorities.is_empty() {
        return Err(custom_http_error(
            StatusCode::CONFLICT,
            String::from("No authority configured for the network, please run genesis."),
        ));
    }

    {
        if !(*server_context.authority_handles.lock().unwrap()).is_empty() {
            return Err(custom_http_error(
                StatusCode::FORBIDDEN,
                String::from("Sui network is already running."),
            ));
        }
    }

    let committee = Committee::new(
        network_config
            .authorities
            .iter()
            .map(|info| (*info.key_pair.public_key_bytes(), info.stake))
            .collect(),
    );
    let mut handles = FuturesUnordered::new();

    for authority in &network_config.authorities {
        let server = sui_commands::make_server(
            authority,
            &committee,
            vec![],
            &[],
            network_config.buffer_size,
        )
        .await
        .map_err(|error| {
            custom_http_error(
                StatusCode::CONFLICT,
                format!("Unable to make server: {error}"),
            )
        })?;
        handles.push(async move {
            match server.spawn().await {
                Ok(server) => Ok(server),
                Err(err) => {
                    return Err(custom_http_error(
                        StatusCode::FAILED_DEPENDENCY,
                        format!("Failed to start server: {}", err),
                    ));
                }
            }
        })
    }

    let num_authorities = handles.len();
    info!("Started {} authorities", num_authorities);

    while let Some(spawned_server) = handles.next().await {
        server_context
            .authority_handles
            .lock()
            .unwrap()
            .push(task::spawn(async {
                if let Err(err) = spawned_server.unwrap().join().await {
                    error!("Server ended with an error: {}", err);
                }
            }));
    }

    let wallet_config_path = &server_context.wallet_config_path;

    let wallet_config =
        WalletConfig::read_or_create(&PathBuf::from(wallet_config_path)).map_err(|error| {
            custom_http_error(
                StatusCode::CONFLICT,
                format!("Unable to read wallet config: {error}"),
            )
        })?;

    let addresses = wallet_config
        .accounts
        .iter()
        .map(|info| info.address)
        .collect::<Vec<_>>();
    let mut wallet_context = WalletContext::new(wallet_config).map_err(|error| {
        custom_http_error(
            StatusCode::CONFLICT,
            format!("Can't create new wallet context: {error}"),
        )
    })?;

    // Sync all accounts.
    for address in addresses.iter() {
        let client_state = wallet_context
            .get_or_create_client_state(address)
            .map_err(|error| {
                custom_http_error(
                    StatusCode::CONFLICT,
                    format!("Can't create client state: {error}"),
                )
            })?;

        client_state.sync_client_state().await.map_err(|err| {
            custom_http_error(
                StatusCode::FAILED_DEPENDENCY,
                format!("Sync error: {:?}", err),
            )
        })?;
    }

    *server_context.wallet_context.lock().unwrap() = Some(wallet_context);

    Ok(HttpResponseOk(format!(
        "Started {} authorities",
        num_authorities
    )))
}

/**
 * [SUI] Stop servers and delete storage.
 */
#[endpoint {
    method = POST,
    path = "/debug/sui/stop",
}]
async fn stop(
    rqctx: Arc<RequestContext<ServerContext>>,
) -> Result<HttpResponseUpdatedNoContent, HttpError> {
    let server_context = rqctx.context();

    for authority_handle in &*server_context.authority_handles.lock().unwrap() {
        authority_handle.abort();
    }
    (*server_context.authority_handles.lock().unwrap()).clear();

    fs::remove_dir_all(server_context.client_db_path.lock().unwrap().clone()).ok();
    fs::remove_dir_all(&server_context.authority_db_path).ok();
    fs::remove_file(&server_context.network_config_path).ok();
    fs::remove_file(&server_context.wallet_config_path).ok();

    Ok(HttpResponseUpdatedNoContent())
}

/**
 * `GetAddressResponse` represents the list of managed accounts for this client.
 */
#[derive(Deserialize, Serialize, JsonSchema)]
struct GetAddressResponse {
    addresses: Vec<String>,
}

/**
 * [WALLET] Retrieve all managed accounts.
 */
#[endpoint {
    method = GET,
    path = "/wallet/addresses",
}]
async fn get_addresses(
    rqctx: Arc<RequestContext<ServerContext>>,
) -> Result<HttpResponseOk<GetAddressResponse>, HttpError> {
    let server_context = rqctx.context();
    // TODO: Find a better way to utilize wallet context here that does not require 'take()'
    let wallet_context = server_context.wallet_context.lock().unwrap().take();
    let mut wallet_context = wallet_context.ok_or_else(|| {
        custom_http_error(
            StatusCode::FAILED_DEPENDENCY,
            "Wallet Context does not exist.".to_string(),
        )
    })?;

    let addresses: Vec<SuiAddress> = wallet_context
        .address_manager
        .get_managed_address_states()
        .keys()
        .copied()
        .collect();

    // TODO: Speed up sync operations by kicking them off concurrently.
    // Also need to investigate if this should be an automatic sync or manually triggered.
    for address in addresses.iter() {
        let client_state = match wallet_context.get_or_create_client_state(address) {
            Ok(client_state) => client_state,
            Err(err) => {
                *server_context.wallet_context.lock().unwrap() = Some(wallet_context);
                return Err(custom_http_error(
                    StatusCode::FAILED_DEPENDENCY,
                    format!("Can't create client state: {err}"),
                ));
            }
        };

        if let Err(err) = client_state.sync_client_state().await {
            *server_context.wallet_context.lock().unwrap() = Some(wallet_context);
            return Err(custom_http_error(
                StatusCode::FAILED_DEPENDENCY,
                format!("Can't create client state: {err}"),
            ));
        }
    }

    *server_context.wallet_context.lock().unwrap() = Some(wallet_context);

    Ok(HttpResponseOk(GetAddressResponse {
        addresses: addresses
            .into_iter()
            .map(|address| format!("{}", address))
            .collect(),
    }))
}

/**
* 'GetObjectsRequest' represents the request to get objects for an address.
*/
#[derive(Deserialize, Serialize, JsonSchema)]
struct GetObjectsRequest {
    address: String,
}

#[derive(Deserialize, Serialize, JsonSchema)]
struct Object {
    object_id: String,
    object_ref: serde_json::Value,
}

/**
 * 'GetObjectsResponse' is a collection of objects owned by an address.
 */
#[derive(Deserialize, Serialize, JsonSchema)]
struct GetObjectsResponse {
    objects: Vec<Object>,
}

/**
 * [WALLET] Return all objects owned by the account address.
 */
// TODO: Add pagination support
#[endpoint {
    method = GET,
    path = "/wallet/objects",
}]
async fn get_objects(
    rqctx: Arc<RequestContext<ServerContext>>,
    query: Query<GetObjectsRequest>,
) -> Result<HttpResponseOk<GetObjectsResponse>, HttpError> {
    let server_context = rqctx.context();

    let get_objects_params = query.into_inner();
    let address = get_objects_params.address;

    let wallet_context = &mut *server_context.wallet_context.lock().unwrap();
    let wallet_context = wallet_context.as_mut().ok_or_else(|| {
        custom_http_error(
            StatusCode::FAILED_DEPENDENCY,
            "Wallet Context does not exist.".to_string(),
        )
    })?;

    let address = &decode_bytes_hex(address.as_str()).map_err(|error| {
        custom_http_error(
            StatusCode::FAILED_DEPENDENCY,
            format!("Could not decode address from hex {error}"),
        )
    })?;

    let client_state = match wallet_context.get_or_create_client_state(address) {
        Ok(client_state) => client_state,
        Err(error) => {
            return Err(custom_http_error(
                StatusCode::FAILED_DEPENDENCY,
                format!("Could not get or create client state: {error}"),
            ));
        }
    };
    let object_refs = client_state.object_refs();

    Ok(HttpResponseOk(GetObjectsResponse {
        objects: object_refs
            .map(|(obj_id, object_ref)| Object {
                object_id: obj_id.to_string(),
                object_ref: json!(object_ref),
            })
            .collect::<Vec<Object>>(),
    }))
}

/**
* `GetObjectInfoRequest` represents the owner & object for which info is to be
* retrieved.
*/
#[derive(Deserialize, Serialize, JsonSchema)]
struct GetObjectInfoRequest {
    owner: String,
    object_id: String,
}

/**
* 'ObjectInfoResponse' represents the object info on the network.
*/
#[derive(Deserialize, Serialize, JsonSchema)]
struct ObjectInfoResponse {
    owner: String,
    version: String,
    id: String,
    readonly: String,
    obj_type: String,
    data: serde_json::Value,
}

/**
 * [WALLET] Get object info.
 */
#[endpoint {
    method = GET,
    path = "/wallet/object_info",
}]
async fn object_info(
    rqctx: Arc<RequestContext<ServerContext>>,
    query: Query<GetObjectInfoRequest>,
) -> Result<HttpResponseOk<ObjectInfoResponse>, HttpError> {
    let server_context = rqctx.context();
    let object_info_params = query.into_inner();

    // TODO: Find a better way to utilize wallet context here that does not require 'take()'
    let wallet_context = server_context.wallet_context.lock().unwrap().take();
    let mut wallet_context = wallet_context.ok_or_else(|| {
        custom_http_error(
            StatusCode::FAILED_DEPENDENCY,
            "Wallet Context does not exist.".to_string(),
        )
    })?;

    let object_id = match ObjectID::try_from(object_info_params.object_id) {
        Ok(object_id) => object_id,
        Err(error) => {
            *server_context.wallet_context.lock().unwrap() = Some(wallet_context);
            return Err(custom_http_error(
                StatusCode::FAILED_DEPENDENCY,
                format!("{error}"),
            ));
        }
    };

    let owner = match decode_bytes_hex(object_info_params.owner.as_str()) {
        Ok(owner) => owner,
        Err(error) => {
            *server_context.wallet_context.lock().unwrap() = Some(wallet_context);
            return Err(custom_http_error(
                StatusCode::FAILED_DEPENDENCY,
                format!("Could not decode address from hex {error}"),
            ));
        }
    };

    // Fetch the object ref
    let client_state = match wallet_context.get_or_create_client_state(&owner) {
        Ok(client_state) => client_state,
        Err(error) => {
            *server_context.wallet_context.lock().unwrap() = Some(wallet_context);
            return Err(custom_http_error(
                StatusCode::FAILED_DEPENDENCY,
                format!(
                    "Could not get client state for account {:?}: {error}",
                    owner
                ),
            ));
        }
    };

    let (object, layout) = match client_state.get_object_info(object_id).await {
        Ok(ObjectRead::Exists(_, object, layout)) => (object, layout),
        Ok(ObjectRead::Deleted(_)) => {
            *server_context.wallet_context.lock().unwrap() = Some(wallet_context);
            return Err(custom_http_error(
                StatusCode::FAILED_DEPENDENCY,
                format!("Object ({object_id}) was deleted."),
            ));
        }
        Ok(ObjectRead::NotExists(_)) => {
            *server_context.wallet_context.lock().unwrap() = Some(wallet_context);
            return Err(custom_http_error(
                StatusCode::FAILED_DEPENDENCY,
                format!("Object ({object_id}) does not exist."),
            ));
        }
        Err(error) => {
            *server_context.wallet_context.lock().unwrap() = Some(wallet_context);
            return Err(custom_http_error(
                StatusCode::FAILED_DEPENDENCY,
                format!("Error while getting object info: {:?}", error),
            ));
        }
    };

    let object_data = object.to_json(&layout).unwrap_or_else(|_| json!(""));

    *server_context.wallet_context.lock().unwrap() = Some(wallet_context);

    Ok(HttpResponseOk(ObjectInfoResponse {
        owner: format!("{:?}", object.owner),
        version: format!("{:?}", object.version().value()),
        id: format!("{:?}", object.id()),
        readonly: format!("{:?}", object.is_read_only()),
        obj_type: format!(
            "{:?}",
            object
                .data
                .type_()
                .map_or("Type Unwrap Failed".to_owned(), |type_| type_
                    .module
                    .as_ident_str()
                    .to_string())
        ),
        data: object_data,
    }))
}

fn custom_http_error(status_code: http::StatusCode, message: String) -> HttpError {
    HttpError::for_client_error(None, status_code, message)
}
