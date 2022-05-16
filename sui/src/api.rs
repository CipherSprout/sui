// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use jsonrpsee::core::RpcResult;
use jsonrpsee_proc_macros::rpc;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_with::serde_as;

use sui_core::gateway_state::GatewayTxSeqNumber;
use sui_core::gateway_types::{GetObjectInfoResponse, SuiInputObjectKind, SuiObjectRef};
use sui_core::gateway_types::{TransactionEffectsResponse, TransactionResponse};
use sui_core::sui_json::SuiJsonValue;
use sui_open_rpc_macros::open_rpc;
use sui_types::sui_serde::Base64;
use sui_types::{
    base_types::{ObjectID, SuiAddress, TransactionDigest},
    crypto::SignableBytes,
    messages::TransactionData,
};

use crate::rpc_gateway::responses::ObjectResponse;
use crate::rpc_gateway::responses::SuiTypeTag;

#[open_rpc(
    name = "Sui JSON-RPC",
    namespace = "sui",
    contact_name = "Mysten Labs",
    contact_url = "https://mystenlabs.com",
    contact_email = "build@mystenlabs.com",
    license = "Apache-2.0",
    license_url = "https://raw.githubusercontent.com/MystenLabs/sui/main/LICENSE",
    description = "Sui JSON-RPC API for interaction with the Sui network gateway."
)]
#[rpc(server, client, namespace = "sui")]
pub trait RpcGateway {
    /// Create a transaction to transfer a Sui coin from one address to another.
    #[method(name = "transferCoin")]
    async fn transfer_coin(
        &self,
        signer: SuiAddress,
        object_id: ObjectID,
        gas: Option<ObjectID>,
        gas_budget: u64,
        recipient: SuiAddress,
    ) -> RpcResult<TransactionBytes>;

    /// Execute a Move call transaction by calling the specified function in the module of a given package.
    #[method(name = "moveCall")]
    async fn move_call(
        &self,
        signer: SuiAddress,
        package_object_id: ObjectID,
        module: String,
        function: String,
        type_arguments: Vec<SuiTypeTag>,
        arguments: Vec<SuiJsonValue>,
        gas: Option<ObjectID>,
        gas_budget: u64,
    ) -> RpcResult<TransactionBytes>;

    /// Publish Move module.
    #[method(name = "publish")]
    async fn publish(
        &self,
        sender: SuiAddress,
        compiled_modules: Vec<Base64>,
        gas: Option<ObjectID>,
        gas_budget: u64,
    ) -> RpcResult<TransactionBytes>;

    #[method(name = "splitCoin")]
    async fn split_coin(
        &self,
        signer: SuiAddress,
        coin_object_id: ObjectID,
        split_amounts: Vec<u64>,
        gas: Option<ObjectID>,
        gas_budget: u64,
    ) -> RpcResult<TransactionBytes>;

    #[method(name = "mergeCoins")]
    async fn merge_coin(
        &self,
        signer: SuiAddress,
        primary_coin: ObjectID,
        coin_to_merge: ObjectID,
        gas: Option<ObjectID>,
        gas_budget: u64,
    ) -> RpcResult<TransactionBytes>;

    /// Execute the transaction using the transaction data, signature and public key.
    #[method(name = "executeTransaction")]
    async fn execute_transaction(
        &self,
        tx_bytes: Base64,
        signature: Base64,
        pub_key: Base64,
    ) -> RpcResult<TransactionResponse>;

    /// Synchronize client state with validators.
    #[method(name = "syncAccountState")]
    async fn sync_account_state(&self, address: SuiAddress) -> RpcResult<()>;

    /// Return the list of objects owned by an address.
    #[method(name = "getOwnedObjects")]
    async fn get_owned_objects(&self, owner: SuiAddress) -> RpcResult<ObjectResponse>;

    #[method(name = "getTotalTransactionNumber")]
    async fn get_total_transaction_number(&self) -> RpcResult<u64>;

    #[method(name = "getTransactionsInRange")]
    async fn get_transactions_in_range(
        &self,
        start: GatewayTxSeqNumber,
        end: GatewayTxSeqNumber,
    ) -> RpcResult<Vec<(GatewayTxSeqNumber, TransactionDigest)>>;

    #[method(name = "getRecentTransactions")]
    async fn get_recent_transactions(
        &self,
        count: u64,
    ) -> RpcResult<Vec<(GatewayTxSeqNumber, TransactionDigest)>>;

    #[method(name = "getTransaction")]
    async fn get_transaction(
        &self,
        digest: TransactionDigest,
    ) -> RpcResult<TransactionEffectsResponse>;

    /// Return the object information for a specified object
    #[method(name = "getObjectInfo")]
    async fn get_object_info(&self, object_id: ObjectID) -> RpcResult<GetObjectInfoResponse>;
}

#[serde_as]
#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TransactionBytes {
    pub tx_bytes: Base64,
    pub gas: SuiObjectRef,
    pub input_objects: Vec<SuiInputObjectKind>,
}

impl TransactionBytes {
    pub fn from_data(data: TransactionData) -> Result<Self, anyhow::Error> {
        Ok(Self {
            tx_bytes: Base64::from_bytes(&data.to_bytes()),
            gas: data.gas().into(),
            input_objects: data
                .input_objects()?
                .into_iter()
                .map(SuiInputObjectKind::from)
                .collect(),
        })
    }

    pub fn to_data(self) -> Result<TransactionData, anyhow::Error> {
        TransactionData::from_signable_bytes(&self.tx_bytes.to_vec()?)
    }
}
