// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::postgres_manager::{write, PgPool};
use crate::{BridgeDataSource, TokenTransfer, TokenTransferData, TokenTransferStatus};
use ethers::providers::Provider;
use ethers::providers::{Http, Middleware};
use ethers::types::Address as EthAddress;
use std::sync::Arc;
use sui_bridge::abi::{EthBridgeEvent, EthSuiBridgeEvents};
use sui_bridge::types::EthLog;

pub async fn process_finalized_eth_events(
    mut eth_events_rx: mysten_metrics::metered_channel::Receiver<(EthAddress, u64, Vec<EthLog>)>,
    provider: Arc<Provider<Http>>,
    pool: &PgPool,
) {
    while let Some(event) = eth_events_rx.recv().await {
        for log in event.2.iter() {
            let eth_bridge_event = EthBridgeEvent::try_from_eth_log(log);
            if eth_bridge_event.is_none() {
                continue;
            }
            let bridge_event = eth_bridge_event.unwrap();
            let block_number = log.block_number;
            let block = provider.get_block(log.block_number).await.unwrap().unwrap();
            let timestamp = block.timestamp.as_u64() * 1000;
            let transaction = provider
                .get_transaction(log.tx_hash)
                .await
                .unwrap()
                .unwrap();
            let gas = transaction.gas;
            let tx_hash = log.tx_hash;

            println!("Observed Finalized Eth bridge event: {:#?}", bridge_event);

            match bridge_event {
                EthBridgeEvent::EthSuiBridgeEvents(bridge_event) => match bridge_event {
                    EthSuiBridgeEvents::TokensDepositedFilter(bridge_event) => {
                        println!("Observed Finalized Eth Deposit");
                        let transfer = TokenTransfer {
                            chain_id: bridge_event.source_chain_id,
                            nonce: bridge_event.nonce,
                            block_height: block_number,
                            timestamp_ms: timestamp,
                            txn_hash: tx_hash.as_bytes().to_vec(),
                            txn_sender: bridge_event.sender_address.as_bytes().to_vec(),
                            status: TokenTransferStatus::Deposited,
                            gas_usage: gas.as_u64() as i64,
                            data_source: BridgeDataSource::Eth,
                            data: Some(TokenTransferData {
                                sender_address: bridge_event.sender_address.as_bytes().to_vec(),
                                destination_chain: bridge_event.destination_chain_id,
                                recipient_address: bridge_event.recipient_address.to_vec(),
                                token_id: bridge_event.token_id,
                                amount: bridge_event.sui_adjusted_amount,
                            }),
                        };

                        let _ = write(pool, vec![transfer]);
                    }
                    EthSuiBridgeEvents::TokensClaimedFilter(_)
                    | EthSuiBridgeEvents::PausedFilter(_)
                    | EthSuiBridgeEvents::UnpausedFilter(_)
                    | EthSuiBridgeEvents::UpgradedFilter(_)
                    | EthSuiBridgeEvents::InitializedFilter(_) => (),
                },
                EthBridgeEvent::EthBridgeCommitteeEvents(_)
                | EthBridgeEvent::EthBridgeLimiterEvents(_)
                | EthBridgeEvent::EthBridgeConfigEvents(_)
                | EthBridgeEvent::EthCommitteeUpgradeableContractEvents(_) => (),
            }
        }
    }
}

pub async fn process_unfinalized_eth_events(
    mut eth_events_rx: mysten_metrics::metered_channel::Receiver<(EthAddress, u64, Vec<EthLog>)>,
    provider: Arc<Provider<Http>>,
    pool: &PgPool,
) {
    while let Some(event) = eth_events_rx.recv().await {
        for log in event.2.iter() {
            let eth_bridge_event = EthBridgeEvent::try_from_eth_log(log);
            if eth_bridge_event.is_none() {
                continue;
            }
            let bridge_event = eth_bridge_event.unwrap();
            let block_number = log.block_number;
            let block = provider.get_block(log.block_number).await.unwrap().unwrap();
            let timestamp = block.timestamp.as_u64() * 1000;
            let transaction = provider
                .get_transaction(log.tx_hash)
                .await
                .unwrap()
                .unwrap();
            let gas = transaction.gas;
            let tx_hash = log.tx_hash;

            println!("Observed Unfinalized Eth bridge event: {:#?}", bridge_event);

            match bridge_event {
                EthBridgeEvent::EthSuiBridgeEvents(bridge_event) => match bridge_event {
                    EthSuiBridgeEvents::TokensDepositedFilter(bridge_event) => {
                        println!("Observed Unfinalized Eth Deposit");
                        let transfer = TokenTransfer {
                            chain_id: bridge_event.source_chain_id,
                            nonce: bridge_event.nonce,
                            block_height: block_number,
                            timestamp_ms: timestamp,
                            txn_hash: tx_hash.as_bytes().to_vec(),
                            txn_sender: bridge_event.sender_address.as_bytes().to_vec(),
                            status: TokenTransferStatus::DepositedUnfinalized,
                            gas_usage: gas.as_u64() as i64,
                            data_source: BridgeDataSource::Eth,
                            data: Some(TokenTransferData {
                                sender_address: bridge_event.sender_address.as_bytes().to_vec(),
                                destination_chain: bridge_event.destination_chain_id,
                                recipient_address: bridge_event.recipient_address.to_vec(),
                                token_id: bridge_event.token_id,
                                amount: bridge_event.sui_adjusted_amount,
                            }),
                        };

                        let _ = write(pool, vec![transfer]);
                    }
                    EthSuiBridgeEvents::TokensClaimedFilter(bridge_event) => {
                        println!("Observed Unfinalized Eth Claim");

                        let transfer = TokenTransfer {
                            chain_id: bridge_event.source_chain_id,
                            nonce: bridge_event.nonce,
                            block_height: block_number,
                            timestamp_ms: timestamp,
                            txn_hash: tx_hash.as_bytes().to_vec(),
                            txn_sender: bridge_event.sender_address.to_vec(),
                            status: TokenTransferStatus::Claimed,
                            gas_usage: gas.as_u64() as i64,
                            data_source: BridgeDataSource::Eth,
                            data: None,
                        };

                        let _ = write(pool, vec![transfer]);
                    }
                    EthSuiBridgeEvents::PausedFilter(_)
                    | EthSuiBridgeEvents::UnpausedFilter(_)
                    | EthSuiBridgeEvents::UpgradedFilter(_)
                    | EthSuiBridgeEvents::InitializedFilter(_) => (),
                },
                EthBridgeEvent::EthBridgeCommitteeEvents(_)
                | EthBridgeEvent::EthBridgeLimiterEvents(_)
                | EthBridgeEvent::EthBridgeConfigEvents(_)
                | EthBridgeEvent::EthCommitteeUpgradeableContractEvents(_) => (),
            }
        }
    }
}
