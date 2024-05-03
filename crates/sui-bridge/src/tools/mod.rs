// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::config::read_bridge_client_key;
use crate::crypto::BridgeAuthorityPublicKeyBytes;
use crate::types::{
    AssetPriceUpdateAction, BlocklistCommitteeAction, BlocklistType, EmergencyAction,
    EmergencyActionType, EvmContractUpgradeAction, LimitUpdateAction,
};
use crate::utils::{get_eth_signer_client, EthSigner};
use anyhow::anyhow;
use clap::*;
use ethers::abi::param_type::Reader;
use ethers::abi::token::{LenientTokenizer, Tokenizer};
use ethers::abi::Abi;
use ethers::contract::BaseContract;
use ethers::types::Address as EthAddress;
use fastcrypto::encoding::Encoding;
use fastcrypto::encoding::Hex;
use serde::{Deserialize, Serialize};
use serde_with::serde_as;
use std::fs;
use std::path::PathBuf;
use std::str::FromStr;
use sui_config::Config;
use sui_sdk::SuiClientBuilder;
use sui_types::base_types::ObjectRef;
use sui_types::base_types::SuiAddress;
use sui_types::bridge::BridgeChainId;
use sui_types::crypto::SuiKeyPair;

use crate::types::BridgeAction;

#[derive(Parser)]
#[clap(rename_all = "kebab-case")]
pub struct Args {
    #[clap(subcommand)]
    pub command: BridgeValidatorCommand,
}

#[derive(Parser)]
#[clap(rename_all = "kebab-case")]
pub enum BridgeValidatorCommand {
    #[clap(name = "create-bridge-validator-key")]
    CreateBridgeValidatorKey { path: PathBuf },
    #[clap(name = "create-bridge-client-key")]
    CreateBridgeClientKey {
        path: PathBuf,
        #[clap(name = "use-ecdsa", long)]
        use_ecdsa: bool,
    },
    #[clap(name = "create-bridge-node-config-template")]
    CreateBridgeNodeConfigTemplate {
        path: PathBuf,
        #[clap(name = "run-client", long)]
        run_client: bool,
    },
    /// Client to facilitate and execute Bridge governance actions
    #[clap(name = "client")]
    GovernanceClient {
        /// Path of BridgeCliConfig
        #[clap(long = "config-path")]
        config_path: PathBuf,
        #[clap(long = "chain-id")]
        chain_id: u8,
        #[clap(subcommand)]
        cmd: GovernanceClientCommands,
    },
}

#[derive(Parser)]
#[clap(rename_all = "kebab-case")]
pub enum GovernanceClientCommands {
    #[clap(name = "emergency-button")]
    EmergencyButton {
        #[clap(name = "nonce", long)]
        nonce: u64,
        #[clap(name = "action-type", long)]
        action_type: EmergencyActionType,
    },
    #[clap(name = "update-committee-blocklist")]
    UpdateCommitteeBlocklist {
        #[clap(name = "nonce", long)]
        nonce: u64,
        #[clap(name = "blocklist-type", long)]
        blocklist_type: BlocklistType,
        #[clap(name = "pubkey-hex", long)]
        pubkeys_hex: Vec<BridgeAuthorityPublicKeyBytes>,
    },
    #[clap(name = "update-limit")]
    UpdateLimit {
        #[clap(name = "nonce", long)]
        nonce: u64,
        #[clap(name = "sending-chain", long)]
        sending_chain: u8,
        #[clap(name = "new-usd-limit", long)]
        new_usd_limit: u64,
    },
    #[clap(name = "update-asset-price")]
    UpdateAssetPrice {
        #[clap(name = "nonce", long)]
        nonce: u64,
        #[clap(name = "token-id", long)]
        token_id: u8,
        #[clap(name = "new-usd-price", long)]
        new_usd_price: u64,
    },
    #[clap(name = "upgrade-evm-contract")]
    UpgradeEVMContract {
        #[clap(name = "nonce", long)]
        nonce: u64,
        #[clap(name = "proxy-address", long)]
        proxy_address: EthAddress,
        #[clap(name = "implementation-address", long)]
        implementation_address: EthAddress,
        #[clap(name = "contract-abi-name", long)]
        contract_abi_name: String,
        #[clap(name = "function-selector", long)]
        function_selector: String,
        #[clap(name = "params", long)]
        params: Vec<String>,
    },
}

pub fn make_action(chain_id: BridgeChainId, cmd: &GovernanceClientCommands) -> BridgeAction {
    match cmd {
        GovernanceClientCommands::EmergencyButton { nonce, action_type } => {
            BridgeAction::EmergencyAction(EmergencyAction {
                nonce: *nonce,
                chain_id,
                action_type: *action_type,
            })
        }
        GovernanceClientCommands::UpdateCommitteeBlocklist {
            nonce,
            blocklist_type,
            pubkeys_hex,
        } => BridgeAction::BlocklistCommitteeAction(BlocklistCommitteeAction {
            nonce: *nonce,
            chain_id,
            blocklist_type: *blocklist_type,
            blocklisted_members: pubkeys_hex.clone(),
        }),
        GovernanceClientCommands::UpdateLimit {
            nonce,
            sending_chain,
            new_usd_limit,
        } => {
            let sending_chain_id =
                BridgeChainId::try_from(*sending_chain).expect("Invalid sending chain id");
            BridgeAction::LimitUpdateAction(LimitUpdateAction {
                nonce: *nonce,
                chain_id,
                sending_chain_id,
                new_usd_limit: *new_usd_limit,
            })
        }
        GovernanceClientCommands::UpdateAssetPrice {
            nonce,
            token_id,
            new_usd_price,
        } => BridgeAction::AssetPriceUpdateAction(AssetPriceUpdateAction {
            nonce: *nonce,
            chain_id,
            token_id: *token_id,
            new_usd_price: *new_usd_price,
        }),
        GovernanceClientCommands::UpgradeEVMContract {
            nonce,
            proxy_address,
            implementation_address,
            contract_abi_name,
            function_selector,
            params,
        } => {
            let abi_path = format!("./abi/{}.json", contract_abi_name);
            let abi = fs::read_to_string(abi_path).unwrap();
            let contract = BaseContract::from(serde_json::from_str::<Abi>(&abi.clone()).unwrap());

            if contract_abi_name == "mock_sui_bridge_v2" {
                let param1 =
                    LenientTokenizer::tokenize(&Reader::read("uint256").unwrap(), &params[0])
                        .unwrap()
                        .into_uint()
                        .unwrap();
                let param2 = LenientTokenizer::tokenize(&Reader::read("bool").unwrap(), &params[1])
                    .unwrap()
                    .into_bool()
                    .unwrap();
                let param3 =
                    LenientTokenizer::tokenize(&Reader::read("string").unwrap(), &params[2])
                        .unwrap()
                        .into_string()
                        .unwrap();

                // given function_selector like "initializeV2Params(uint256,bool,string)"
                let call_data = contract
                    .encode(&function_selector, (param1, param2, param3))
                    .unwrap();

                BridgeAction::EvmContractUpgradeAction(EvmContractUpgradeAction {
                    nonce: *nonce,
                    chain_id,
                    proxy_address: *proxy_address,
                    new_impl_address: *implementation_address,
                    call_data: call_data.to_vec(),
                })
            } else {
                panic!("Invalid contract name");
            }
        }
    }
}

pub fn select_contract_address(
    config: &BridgeCliConfig,
    cmd: &GovernanceClientCommands,
) -> EthAddress {
    match cmd {
        GovernanceClientCommands::EmergencyButton { .. } => config.eth_sui_bridge_proxy_address,
        GovernanceClientCommands::UpdateCommitteeBlocklist { .. } => {
            config.eth_bridge_committee_proxy_address
        }
        GovernanceClientCommands::UpdateLimit { .. } => config.eth_bridge_limiter_proxy_address,
        GovernanceClientCommands::UpdateAssetPrice { .. } => {
            config.eth_bridge_limiter_proxy_address
        }
        GovernanceClientCommands::UpgradeEVMContract { proxy_address, .. } => *proxy_address,
    }
}

// TODO: eth_bridge_committee and eth_bridge_limiter can be referenced from the sui_bridge contract
#[serde_as]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct BridgeCliConfig {
    /// Rpc url for Sui fullnode, used for query stuff and submit transactions.
    pub sui_rpc_url: String,
    /// Rpc url for Eth fullnode, used for query stuff.
    pub eth_rpc_url: String,
    /// Proxy addresss for SuiBridge deployed on Eth
    pub eth_sui_bridge_proxy_address: EthAddress,
    /// Proxy addresss for BridgeCommittee deployed on Eth
    pub eth_bridge_committee_proxy_address: EthAddress,
    /// Proxy addresss for BridgeLimiter deployed on Eth
    pub eth_bridge_limiter_proxy_address: EthAddress,
    /// Path of the file where bridge client key (any SuiKeyPair) is stored as Base64 encoded `flag || privkey`.
    /// The derived accounts
    pub bridge_client_key_path_base64_sui_key: PathBuf,
}

impl Config for BridgeCliConfig {}

impl BridgeCliConfig {
    pub async fn get_eth_signer_client(self: &BridgeCliConfig) -> anyhow::Result<EthSigner> {
        let client_key = read_bridge_client_key(&self.bridge_client_key_path_base64_sui_key)?;
        let private_key = Hex::encode(client_key.to_bytes_no_flag());
        let url = self.eth_rpc_url.clone();
        get_eth_signer_client(&url, &private_key).await
    }

    pub async fn get_sui_account_info(
        self: &BridgeCliConfig,
    ) -> anyhow::Result<(SuiKeyPair, SuiAddress, ObjectRef)> {
        let client_key = read_bridge_client_key(&self.bridge_client_key_path_base64_sui_key)?;
        let pubkey = client_key.public();
        let sui_client_address = SuiAddress::from(&pubkey);
        println!("Using Sui address: {:?}", sui_client_address);
        let sui_sdk_client = SuiClientBuilder::default()
            .build(self.sui_rpc_url.clone())
            .await?;
        let gases = sui_sdk_client
            .coin_read_api()
            .get_coins(sui_client_address, None, None, None)
            .await?
            .data;
        // TODO: is 5 Sui a good number?
        let gas = gases
            .into_iter()
            .find(|coin| coin.balance >= 5_000_000_000)
            .ok_or(anyhow!("Did not find gas object with enough balance"))?;
        println!("Using Gas object: {}", gas.coin_object_id);
        Ok((client_key, sui_client_address, gas.object_ref()))
    }
}
