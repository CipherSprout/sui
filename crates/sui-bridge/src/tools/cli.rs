// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use clap::*;
use fastcrypto::encoding::{Base64, Encoding, Hex};
use fastcrypto::secp256k1::{Secp256k1KeyPair, Secp256k1PrivateKey};
use fastcrypto::traits::{EncodeDecodeBase64, ToFromBytes};
use move_core_types::identifier::Identifier;
use move_core_types::language_storage::{StructTag, TypeTag};
use shared_crypto::intent::Intent;
use shared_crypto::intent::IntentMessage;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;
use sui_bridge::client::bridge_authority_aggregator::BridgeAuthorityAggregator;
use sui_bridge::crypto::{
    BridgeAuthorityPublicKeyBytes, BridgeAuthorityRecoverableSignature, BridgeAuthoritySignInfo,
};
use sui_bridge::eth_transaction_builder::build_eth_transaction;
use sui_bridge::sui_client::SuiClient;
use sui_bridge::sui_transaction_builder::{
    build_add_tokens_on_sui_transaction, build_sui_transaction,
};
use sui_bridge::tools::{
    make_action, select_contract_address, Args, BridgeCliConfig, BridgeValidatorCommand,
};
use sui_bridge::types::{
    AddTokensOnSuiAction, AssetPriceUpdateAction, BridgeAction, BridgeCommitteeValiditySignInfo,
    CertifiedBridgeAction, LimitUpdateAction, VerifiedCertifiedBridgeAction,
};
use sui_bridge::utils::{
    generate_bridge_authority_key_and_write_to_file, generate_bridge_client_key_and_write_to_file,
    generate_bridge_node_config_and_write_to_file,
};
use sui_config::Config;
use sui_json_rpc_types::{ObjectChange, SuiTransactionBlockResponseOptions};
use sui_move_build::BuildConfig;
use sui_sdk::{SuiClient as SuiSdkClient, SuiClientBuilder};
use sui_types::base_types::{ObjectRef, SuiAddress};
use sui_types::bridge::{BridgeChainId, BRIDGE_MODULE_NAME};
use sui_types::crypto::{Signature, SuiKeyPair};
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use sui_types::transaction::{ObjectArg, Transaction, TransactionData};
use sui_types::BRIDGE_PACKAGE_ID;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Init logging
    let (_guard, _filter_handle) = telemetry_subscribers::TelemetryConfig::new()
        .with_env()
        .init();
    let args = Args::parse();

    match args.command {
        BridgeValidatorCommand::CreateBridgeValidatorKey { path } => {
            generate_bridge_authority_key_and_write_to_file(&path)?;
            println!("Bridge validator key generated at {}", path.display());
        }
        BridgeValidatorCommand::CreateBridgeClientKey { path, use_ecdsa } => {
            generate_bridge_client_key_and_write_to_file(&path, use_ecdsa)?;
            println!("Bridge client key generated at {}", path.display());
        }
        BridgeValidatorCommand::CreateBridgeNodeConfigTemplate { path, run_client } => {
            generate_bridge_node_config_and_write_to_file(&path, run_client)?;
            println!(
                "Bridge node config template generated at {}",
                path.display()
            );
        }

        BridgeValidatorCommand::GovernanceClient {
            config_path,
            chain_id,
            cmd,
        } => {
            let chain_id = BridgeChainId::try_from(chain_id).expect("Invalid chain id");
            println!("Chain ID: {:?}", chain_id);
            let config = BridgeCliConfig::load(config_path).expect("Couldn't load BridgeCliConfig");
            let sui_client = SuiClient::<SuiSdkClient>::new(&config.sui_rpc_url).await?;

            let (sui_key, sui_address, gas_object_ref) = config
                .get_sui_account_info()
                .await
                .expect("Failed to get sui account info");
            let bridge_summary = sui_client
                .get_bridge_summary()
                .await
                .expect("Failed to get bridge summary");
            let bridge_committee = Arc::new(
                sui_client
                    .get_bridge_committee()
                    .await
                    .expect("Failed to get bridge committee"),
            );
            let agg = BridgeAuthorityAggregator::new(bridge_committee);

            // Handle Sui Side
            if chain_id.is_sui_chain() {
                let sui_chain_id = BridgeChainId::try_from(bridge_summary.chain_id).unwrap();
                assert_eq!(
                    sui_chain_id, chain_id,
                    "Chain ID mismatch, expected: {:?}, got from url: {:?}",
                    chain_id, sui_chain_id
                );
                // Create BridgeAction
                let sui_action = make_action(sui_chain_id, &cmd);
                println!("Action to execute on Sui: {:?}", sui_action);
                let threshold = sui_action.approval_threshold();
                let certified_action = agg
                    .request_committee_signatures(sui_action, threshold)
                    .await
                    .expect("Failed to request committee signatures");
                let bridge_arg = sui_client
                    .get_mutable_bridge_object_arg_must_succeed()
                    .await;
                let id_token_map = sui_client.get_token_id_map().await.unwrap();
                let tx = build_sui_transaction(
                    sui_address,
                    &gas_object_ref,
                    certified_action,
                    bridge_arg,
                    &id_token_map,
                )
                .expect("Failed to build sui transaction");
                let sui_sig = Signature::new_secure(
                    &IntentMessage::new(Intent::sui_transaction(), tx.clone()),
                    &sui_key,
                );
                let tx = Transaction::from_data(tx, vec![sui_sig]);
                let resp = sui_client
                    .execute_transaction_block_with_effects(tx)
                    .await
                    .expect("Failed to execute transaction block with effects");
                if resp.status_ok().unwrap() {
                    println!("Sui Transaction succeeded: {:?}", resp.digest);
                } else {
                    println!(
                        "Sui Transaction failed: {:?}. Effects: {:?}",
                        resp.digest, resp.effects
                    );
                }
                return Ok(());
            }

            // Handle eth side
            // TODO assert chain id returned from rpc matches chain_id
            let eth_signer_client = config
                .get_eth_signer_client()
                .await
                .expect("Failed to get eth signer client");
            println!("Using Eth address: {:?}", eth_signer_client.address());
            // Create BridgeAction
            let eth_action = make_action(chain_id, &cmd);
            println!("Action to execute on Eth: {:?}", eth_action);
            // Create Eth Signer Client
            let threshold = eth_action.approval_threshold();
            let certified_action = agg
                .request_committee_signatures(eth_action, threshold)
                .await
                .expect("Failed to request committee signatures");
            let contract_address = select_contract_address(&config, &cmd);
            let tx = build_eth_transaction(contract_address, eth_signer_client, certified_action)
                .await
                .expect("Failed to build eth transaction");
            println!("sending Eth tx: {:?}", tx);
            match tx.send().await {
                Ok(tx_hash) => {
                    println!("Transaction sent with hash: {:?}", tx_hash);
                }
                Err(err) => {
                    let revert = err.as_revert();
                    println!("Transaction reverted: {:?}", revert);
                }
            };

            return Ok(());
        }
    }

    Ok(())
}

#[cfg(test)]
// Account key for gas
const ACCOUNT_KEYPAIR: &str = "";
#[cfg(test)]
// Committee key, can be obtained from bridgenet host or sui-operation repo
const COMMITTEE_KEYS: [&str; 4] = ["", "", "", ""];
#[cfg(test)]
// bridgenet fullnode url
const SUI_RPC_URL: &'static str = "";
#[cfg(test)]
fn sign_bridge_message(
    action: &BridgeAction,
) -> BTreeMap<BridgeAuthorityPublicKeyBytes, BridgeAuthorityRecoverableSignature> {
    COMMITTEE_KEYS
        .iter()
        .map(|key| {
            let bytes = Base64::decode(key).unwrap();
            let key = Secp256k1PrivateKey::from_bytes(&bytes).unwrap();
            let key = Secp256k1KeyPair::from(key);
            let sig = BridgeAuthoritySignInfo::new(&action, &key);
            ((&key.public).into(), sig.signature)
        })
        .collect()
}

#[tokio::test]
async fn add_tokens() {
    // Validator keys for paying for publish gas
    let keypair = SuiKeyPair::decode_base64(ACCOUNT_KEYPAIR).unwrap();
    let address = SuiAddress::from(&keypair.public());

    let sui_client = SuiClientBuilder::default()
        .build(SUI_RPC_URL)
        .await
        .unwrap();

    let bridge_client = SuiClient::new(SUI_RPC_URL).await.unwrap();
    let bridge = bridge_client
        .get_mutable_bridge_object_arg_must_succeed()
        .await;

    let coin_dir = PathBuf::from("../../bridge/move/tokens");
    let coins = [
        ("btc", 1u8, 6200000000000u64),
        ("eth", 2, 430000000000),
        ("usdc", 3, 100000000),
        ("usdt", 4, 100000000),
    ];

    let mut registered_coins = vec![];

    for (coin, id, value) in coins {
        // 1. publish coins
        let (metadata, tc, uc, coin_type) =
            publish_token(&sui_client, coin_dir.join(coin), address, &keypair).await;

        // 2. register coins
        let mut ptb = ProgrammableTransactionBuilder::default();
        let tc_arg = ptb.obj(ObjectArg::ImmOrOwnedObject(tc)).unwrap();
        let uc_arg = ptb.obj(ObjectArg::ImmOrOwnedObject(uc)).unwrap();
        let metadata_arg = ptb.obj(ObjectArg::ImmOrOwnedObject(metadata)).unwrap();
        let bridge_arg = ptb.obj(bridge).unwrap();

        let coins = sui_client
            .coin_read_api()
            .get_coins(address, None, None, None)
            .await
            .unwrap();
        let gas = coins.data.first().unwrap().object_ref();

        let ref_gas_price = sui_client
            .read_api()
            .get_reference_gas_price()
            .await
            .unwrap();

        ptb.programmable_move_call(
            BRIDGE_PACKAGE_ID,
            BRIDGE_MODULE_NAME.into(),
            Identifier::new("register_foreign_token").unwrap(),
            vec![coin_type.clone()],
            vec![bridge_arg, tc_arg, uc_arg, metadata_arg],
        );
        let tx_data = TransactionData::new_programmable(
            address,
            vec![gas],
            ptb.finish(),
            100000000,
            ref_gas_price,
        );
        let tx = Transaction::from_data_and_signer(tx_data, vec![&keypair]);
        sui_client
            .quorum_driver_api()
            .execute_transaction_block(
                tx,
                SuiTransactionBlockResponseOptions::new().with_effects(),
                None,
            )
            .await
            .unwrap();
        registered_coins.push((coin_type, id, value))
    }

    // 3. approve new tokens
    let coins = sui_client
        .coin_read_api()
        .get_coins(address, None, None, None)
        .await
        .unwrap();
    let gas = coins.data.first().unwrap().object_ref();

    let token_ids = registered_coins
        .iter()
        .map(|(_, id, _)| *id)
        .collect::<Vec<_>>();
    let token_type_names = registered_coins
        .iter()
        .map(|(coin_type, _, _)| coin_type.clone())
        .collect::<Vec<_>>();
    let token_prices = registered_coins
        .iter()
        .map(|(_, _, token_price)| *token_price)
        .collect::<Vec<_>>();

    let add_token_action = BridgeAction::AddTokensOnSuiAction(AddTokensOnSuiAction {
        nonce: 0,
        chain_id: BridgeChainId::SuiCustom,
        native: false,
        token_ids: token_ids.clone(),
        token_type_names: token_type_names.clone(),
        token_prices: token_prices.clone(),
    });
    let sigs = sign_bridge_message(&add_token_action);
    let certified_action = CertifiedBridgeAction::new_from_data_and_sig(
        add_token_action,
        BridgeCommitteeValiditySignInfo { signatures: sigs },
    );
    let action_certificate = VerifiedCertifiedBridgeAction::new_from_verified(certified_action);
    let tx_data =
        build_add_tokens_on_sui_transaction(address, &gas, action_certificate, bridge).unwrap();
    let tx = Transaction::from_data_and_signer(tx_data, vec![&keypair]);
    let response = sui_client
        .quorum_driver_api()
        .execute_transaction_block(
            tx,
            SuiTransactionBlockResponseOptions::new().with_effects(),
            None,
        )
        .await
        .unwrap();

    println!("{:?}", response.effects.unwrap())
}

#[cfg(test)]
async fn publish_token(
    sui_client: &SuiSdkClient,
    path: PathBuf,
    address: SuiAddress,
    keypair: &SuiKeyPair,
) -> (ObjectRef, ObjectRef, ObjectRef, TypeTag) {
    let coins = sui_client
        .coin_read_api()
        .get_coins(address, None, None, None)
        .await
        .unwrap();
    let gas = coins.data.first().unwrap().object_ref();
    let ref_gas_price = sui_client
        .read_api()
        .get_reference_gas_price()
        .await
        .unwrap();
    let compiled_package = BuildConfig::new_for_testing().build(path).unwrap();
    let all_module_bytes = compiled_package.get_package_bytes(false);
    let dependencies = compiled_package.get_dependency_original_package_ids();

    let mut ptb = ProgrammableTransactionBuilder::default();

    let cap = ptb.publish_upgradeable(all_module_bytes, dependencies);
    ptb.transfer_arg(address, cap);

    let tx_data = TransactionData::new_programmable(
        address,
        vec![gas],
        ptb.finish(),
        100000000,
        ref_gas_price,
    );
    let tx = Transaction::from_data_and_signer(tx_data, vec![keypair]);
    let response = sui_client
        .quorum_driver_api()
        .execute_transaction_block(
            tx,
            SuiTransactionBlockResponseOptions::new()
                .with_effects()
                .with_object_changes(),
            None,
        )
        .await
        .unwrap();

    let (metadata, _) = find_new_object(response.object_changes.as_ref(), "CoinMetadata").unwrap();
    let (tc, coin_type) = find_new_object(response.object_changes.as_ref(), "TreasuryCap").unwrap();
    let (uc, _) = find_new_object(response.object_changes.as_ref(), "UpgradeCap").unwrap();

    (
        metadata,
        tc,
        uc,
        coin_type.type_params.first().unwrap().clone(),
    )
}

#[cfg(test)]
fn find_new_object(oc: Option<&Vec<ObjectChange>>, type_: &str) -> Option<(ObjectRef, StructTag)> {
    oc?.iter().find_map(|o| match o {
        ObjectChange::Created {
            object_type,
            object_id,
            version,
            digest,
            ..
        } => {
            if object_type.name.to_string() == type_ {
                Some(((*object_id, *version, *digest), object_type.clone()))
            } else {
                None
            }
        }
        _ => None,
    })
}

#[tokio::test]
async fn approve_limit_change() {
    let committee_keys = COMMITTEE_KEYS
        .iter()
        .map(|key| {
            let bytes = Base64::decode(key).unwrap();
            let key = Secp256k1PrivateKey::from_bytes(&bytes).unwrap();
            Secp256k1KeyPair::from(key)
        })
        .collect::<Vec<_>>();

    // Validator keys for paying for publish gas
    let keypair = SuiKeyPair::decode_base64(ACCOUNT_KEYPAIR).unwrap();
    let address = SuiAddress::from(&keypair.public());

    let sui_client = SuiClientBuilder::default()
        .build(SUI_RPC_URL)
        .await
        .unwrap();

    let bridge_client = SuiClient::new(SUI_RPC_URL).await.unwrap();
    let bridge = bridge_client
        .get_mutable_bridge_object_arg_must_succeed()
        .await;

    let coins = sui_client
        .coin_read_api()
        .get_coins(address, None, None, None)
        .await
        .unwrap();
    let gas = coins.data.first().unwrap().object_ref();
    let ref_gas_price = sui_client
        .read_api()
        .get_reference_gas_price()
        .await
        .unwrap();

    let action = BridgeAction::AssetPriceUpdateAction(AssetPriceUpdateAction {
        nonce: 3,
        chain_id: BridgeChainId::SuiCustom,
        token_id: 4,
        new_usd_price: 100000000,
    });

    let committee = bridge_client.get_bridge_committee().await.unwrap();
    let sigs = committee_keys
        .iter()
        .map(|key| {
            let sig = BridgeAuthoritySignInfo::new(&action, &key);
            let pubkey = BridgeAuthorityPublicKeyBytes::from(&key.public);
            println!("{:?}", pubkey.to_eth_address());

            sig.verify(&action, &committee).unwrap();
            sig.signature.as_bytes().to_vec()
        })
        .collect::<Vec<_>>();

    let mut ptb = ProgrammableTransactionBuilder::default();

    let bridge_arg = ptb.obj(bridge).unwrap();

    let source_chain = ptb.pure(2u8).unwrap();
    let seq_num = ptb.pure(3u64).unwrap();
    let token_id = ptb.pure(4u8).unwrap();
    let token_price = ptb.pure(100000000u64).unwrap();

    let msg = ptb.programmable_move_call(
        BRIDGE_PACKAGE_ID,
        Identifier::new("message").unwrap(),
        Identifier::new("create_update_asset_price_message").unwrap(),
        vec![],
        vec![token_id, source_chain, seq_num, token_price],
    );

    let sigs_arg = ptb.pure(sigs).unwrap();

    ptb.programmable_move_call(
        BRIDGE_PACKAGE_ID,
        BRIDGE_MODULE_NAME.into(),
        Identifier::new("execute_system_message").unwrap(),
        vec![],
        vec![bridge_arg, msg, sigs_arg],
    );

    let tx_data = TransactionData::new_programmable(
        address,
        vec![gas],
        ptb.finish(),
        100000000,
        ref_gas_price,
    );
    let tx = Transaction::from_data_and_signer(tx_data, vec![&keypair]);
    let response = sui_client
        .quorum_driver_api()
        .execute_transaction_block(
            tx,
            SuiTransactionBlockResponseOptions::new().with_effects(),
            None,
        )
        .await
        .unwrap();
    println!("{:?}", response.effects.unwrap())
}

#[tokio::test]
async fn approve_transfer_limit_change() {
    let committee_keys = COMMITTEE_KEYS
        .iter()
        .map(|key| {
            let bytes = Base64::decode(key).unwrap();
            let key = Secp256k1PrivateKey::from_bytes(&bytes).unwrap();
            Secp256k1KeyPair::from(key)
        })
        .collect::<Vec<_>>();

    // Validator keys for paying for publish gas
    let keypair = SuiKeyPair::decode_base64(ACCOUNT_KEYPAIR).unwrap();
    let address = SuiAddress::from(&keypair.public());

    let sui_client = SuiClientBuilder::default()
        .build(SUI_RPC_URL)
        .await
        .unwrap();

    let bridge_client = SuiClient::new(SUI_RPC_URL).await.unwrap();
    let bridge = bridge_client
        .get_mutable_bridge_object_arg_must_succeed()
        .await;

    let coins = sui_client
        .coin_read_api()
        .get_coins(address, None, None, None)
        .await
        .unwrap();
    let gas = coins.data.first().unwrap().object_ref();
    let ref_gas_price = sui_client
        .read_api()
        .get_reference_gas_price()
        .await
        .unwrap();

    let action = BridgeAction::LimitUpdateAction(LimitUpdateAction {
        nonce: 0,
        chain_id: BridgeChainId::SuiCustom,
        sending_chain_id: BridgeChainId::EthSepolia,
        new_usd_limit: 6200000000000,
    });

    println!("{}", Hex::encode(&action.to_bytes()));

    let committee = bridge_client.get_bridge_committee().await.unwrap();
    let sigs = committee_keys
        .iter()
        .map(|key| {
            let sig = BridgeAuthoritySignInfo::new(&action, &key);
            let pubkey = BridgeAuthorityPublicKeyBytes::from(&key.public);
            println!("{:?}", pubkey.to_eth_address());
            sig.verify(&action, &committee).unwrap();
            println!("signature: {}", Hex::encode(sig.signature.as_bytes()));
            sig.signature.as_bytes().to_vec()
        })
        .collect::<Vec<_>>();

    let mut ptb = ProgrammableTransactionBuilder::default();

    let bridge_arg = ptb.obj(bridge).unwrap();

    let sending_chain = ptb.pure(11u8).unwrap();
    let seq_num = ptb.pure(0u64).unwrap();
    let receiving_chain = ptb.pure(2u8).unwrap();
    let token_price = ptb.pure(6200000000000u64).unwrap();

    let msg = ptb.programmable_move_call(
        BRIDGE_PACKAGE_ID,
        Identifier::new("message").unwrap(),
        Identifier::new("create_update_bridge_limit_message").unwrap(),
        vec![],
        vec![receiving_chain, seq_num, sending_chain, token_price],
    );

    let sigs_arg = ptb.pure(sigs).unwrap();

    ptb.programmable_move_call(
        BRIDGE_PACKAGE_ID,
        BRIDGE_MODULE_NAME.into(),
        Identifier::new("execute_system_message").unwrap(),
        vec![],
        vec![bridge_arg, msg, sigs_arg],
    );

    let tx_data = TransactionData::new_programmable(
        address,
        vec![gas],
        ptb.finish(),
        100000000,
        ref_gas_price,
    );
    let tx = Transaction::from_data_and_signer(tx_data, vec![&keypair]);
    let response = sui_client
        .quorum_driver_api()
        .execute_transaction_block(
            tx,
            SuiTransactionBlockResponseOptions::new().with_effects(),
            None,
        )
        .await
        .unwrap();
    println!("{:?}", response.effects.unwrap())
}

#[tokio::test]
async fn send_sui() {
    // Validator keys for paying for publish gas
    let keypair = SuiKeyPair::decode_base64(ACCOUNT_KEYPAIR).unwrap();
    let address = SuiAddress::from(&keypair.public());

    let sui_client = SuiClientBuilder::default()
        .build(SUI_RPC_URL)
        .await
        .unwrap();

    let coins = sui_client
        .coin_read_api()
        .get_coins(address, None, None, None)
        .await
        .unwrap();
    let gas = coins.data.first().unwrap().object_ref();
    let ref_gas_price = sui_client
        .read_api()
        .get_reference_gas_price()
        .await
        .unwrap();

    let mut ptb = ProgrammableTransactionBuilder::default();

    ptb.pay_sui(
        vec![SuiAddress::from_str(
            "0x2fd42dfdbd2eb7055a7bc7d4ce000ae53cc22f0c2f2006862bebc8df1f676027",
        )
        .unwrap()],
        vec![10_000_000_000],
    )
    .unwrap();

    let tx_data = TransactionData::new_programmable(
        address,
        vec![gas],
        ptb.finish(),
        100000000,
        ref_gas_price,
    );
    let tx = Transaction::from_data_and_signer(tx_data, vec![&keypair]);
    let response = sui_client
        .quorum_driver_api()
        .execute_transaction_block(
            tx,
            SuiTransactionBlockResponseOptions::new()
                .with_effects()
                .with_object_changes(),
            None,
        )
        .await
        .unwrap();

    println!("{:?}", response.effects.unwrap())
}
