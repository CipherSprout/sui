// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anemo::PeerId;
use fastcrypto::encoding::{Encoding, Hex};
use fastcrypto::groups::bls12381;
use fastcrypto::serde_helpers::ToFromByteArray;
use fastcrypto::traits::{KeyPair, ToFromBytes};
use fastcrypto_tbls::nodes::PartyId;
use fastcrypto_tbls::{dkg, nodes};
use rand::rngs::{OsRng, StdRng};
use rand::SeedableRng;
use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Weak};
use sui_types::base_types::AuthorityName;
use sui_types::committee::{Committee, EpochId, StakeUnit};
use sui_types::crypto::{AuthorityKeyPair, RandomnessRound};
use sui_types::error::{SuiError, SuiResult};
use sui_types::messages_consensus::ConsensusTransaction;
use sui_types::sui_system_state::epoch_start_sui_system_state::EpochStartSystemStateTrait;
use tokio::sync::{Mutex, OnceCell};
use tracing::{debug, error, info};
use typed_store::rocks::DBBatch;
use typed_store::Map;

use crate::authority::authority_per_epoch_store::{AuthorityEpochTables, AuthorityPerEpochStore};
use crate::authority::epoch_start_configuration::EpochStartConfigTrait;
use crate::consensus_adapter::ConsensusAdapter;

type PkG = bls12381::G2Element;
type EncG = bls12381::G2Element;

const SINGLETON_KEY: u64 = 0;

// State machine for randomness DKG and generation.
//
// DKG protocol:
// 1. This validator sends out a `Message` to all other validators.
// 2. Once sufficient valid `Message`s are received from other validators via consensus and
//    procesed, this validator sends out a `Confirmation` to all other validators.
// 3. Once sufficient `Confirmation`s are received from other validators via consensus and
//    processed, they are combined to form a public VSS key and local private key shares.
// 4. Randomness generation begins.
//
// Randomness generation:
// 1. For each new round, AuthorityPerEpochStore eventually calls `generate_next_randomness`.
// 2. This kicks off a process in RandomnessEventLoop to send partial signatures for the new
//    round to all other validators.
// 3. Once enough partial signautres for the round are collected, a RandomnessStateUpdate
//    transaction is generated and injected into the TransactionManager.
// 4. Once the RandomnessStateUpdate transaction is seen in a certified checkpoint, the
//    round is complete and we stop sending partial signatures for it.
pub struct RandomnessManager {
    inner: Mutex<Inner>,
}

pub struct Inner {
    epoch_store: Weak<AuthorityPerEpochStore>,
    consensus_adapter: Arc<ConsensusAdapter>,
    network_handle: sui_network::randomness::Handle,

    // State for DKG.
    party: dkg::Party<PkG, EncG>,
    processed_messages: BTreeMap<PartyId, dkg::ProcessedMessage<PkG, EncG>>,
    used_messages: OnceCell<dkg::UsedProcessedMessages<PkG, EncG>>,
    confirmations: BTreeMap<PartyId, dkg::Confirmation<EncG>>,
    dkg_output: OnceCell<dkg::Output<PkG, EncG>>,

    // State for randomness generation.
    authority_info: HashMap<AuthorityName, (PeerId, PartyId)>,
    next_randomness_round: RandomnessRound,
}

impl RandomnessManager {
    // Returns None in case of invalid input or other failure to initialize DKG.
    pub async fn try_new(
        epoch_store_weak: Weak<AuthorityPerEpochStore>,
        consensus_adapter: Arc<ConsensusAdapter>,
        network_handle: sui_network::randomness::Handle,
        authority_key_pair: &AuthorityKeyPair,
    ) -> Option<Self> {
        let epoch_store = match epoch_store_weak.upgrade() {
            Some(epoch_store) => epoch_store,
            None => {
                error!(
                    "could not construct RandomnessManager: AuthorityPerEpochStore already gone"
                );
                return None;
            }
        };
        let tables = match epoch_store.tables() {
            Ok(tables) => tables,
            Err(_) => {
                error!("could not construct RandomnessManager: AuthorityPerEpochStore tables already gone");
                return None;
            }
        };
        let protocol_config = epoch_store.protocol_config();

        let name: AuthorityName = authority_key_pair.public().into();
        let committee = epoch_store.committee();
        let info = RandomnessManager::randomness_dkg_info_from_committee(committee);
        if tracing::enabled!(tracing::Level::DEBUG) {
            // Log first few entries in DKG info for debugging.
            for (id, name, pk, stake) in info.iter().filter(|(id, _, _, _)| *id < 3) {
                let pk_bytes = pk.as_element().to_byte_array();
                debug!("random beacon: DKG info: id={id}, stake={stake}, name={name}, pk={pk_bytes:x?}");
            }
        }
        let authority_ids: HashMap<_, _> =
            info.iter().map(|(id, name, _, _)| (*name, *id)).collect();
        let authority_peer_ids = epoch_store
            .epoch_start_config()
            .epoch_start_state()
            .get_authority_names_to_peer_ids();
        let authority_info = authority_ids
            .into_iter()
            .map(|(name, id)| {
                let peer_id = authority_peer_ids
                    .get(&name)
                    .expect("authority name should be in peer_ids")
                    .clone();
                (name, (peer_id, id))
            })
            .collect();
        let nodes = info
            .iter()
            .map(|(id, _, pk, stake)| nodes::Node::<EncG> {
                id: *id,
                pk: pk.clone(),
                weight: (*stake).try_into().expect("stake should fit in u16"),
            })
            .collect();
        let nodes = match nodes::Nodes::new(nodes) {
            Ok(nodes) => nodes,
            Err(err) => {
                error!("random beacon: error while initializing Nodes: {err:?}");
                return None;
            }
        };
        let (nodes, t) = nodes.reduce(
            committee
                .validity_threshold()
                .try_into()
                .expect("validity threshold should fit in u16"),
            protocol_config.random_beacon_reduction_allowed_delta(),
        );
        let total_weight = nodes.total_weight();
        let num_nodes = nodes.num_nodes();
        let prefix_str = format!(
            "dkg {} {}",
            Hex::encode(epoch_store.get_chain_identifier().as_bytes()),
            committee.epoch()
        );
        let randomness_private_key = bls12381::Scalar::from_byte_array(
            authority_key_pair
                .copy()
                .private()
                .as_bytes()
                .try_into()
                .expect("key length should match"),
        )
        .expect("should work to convert BLS key to Scalar");
        let party = match dkg::Party::<PkG, EncG>::new(
            fastcrypto_tbls::ecies::PrivateKey::<bls12381::G2Element>::from(randomness_private_key),
            nodes,
            t.into(),
            fastcrypto_tbls::random_oracle::RandomOracle::new(prefix_str.as_str()),
            &mut rand::thread_rng(),
        ) {
            Ok(party) => party,
            Err(err) => {
                error!("random beacon: error while initializing Party: {err:?}");
                return None;
            }
        };
        info!(
            "random beacon: state initialized with authority={name}, total_weight={total_weight}, t={t}, num_nodes={num_nodes}, oracle initial_prefix={prefix_str:?}",
        );

        // Load existing data from store.
        let mut inner = Inner {
            epoch_store: epoch_store_weak,
            consensus_adapter,
            network_handle,
            party,
            processed_messages: BTreeMap::new(),
            used_messages: OnceCell::new(),
            confirmations: BTreeMap::new(),
            dkg_output: OnceCell::new(),
            authority_info,
            next_randomness_round: RandomnessRound(0),
        };
        let dkg_output = tables
            .dkg_output
            .get(&SINGLETON_KEY)
            .expect("typed_store should not fail");
        if let Some(dkg_output) = dkg_output {
            info!(
                "random beacon: loaded existing DKG output for epoch {}",
                committee.epoch()
            );
            epoch_store
                .metrics
                .epoch_random_beacon_dkg_num_shares
                .set(dkg_output.shares.as_ref().map_or(0, |shares| shares.len()) as i64);
            inner
                .dkg_output
                .set(dkg_output.clone())
                .expect("setting new OnceCell should succeed");

            // Load randomness generation state.
            inner.next_randomness_round = tables
                .randomness_next_round
                .get(&SINGLETON_KEY)
                .expect("typed_store should not fail")
                .unwrap_or(RandomnessRound(0));

            // Resume randomness generation from where we left off.
            inner
                .network_handle
                .update_epoch(
                    committee.epoch(),
                    inner.authority_info.clone(),
                    dkg_output,
                    inner.party.t(),
                )
                .await;
            for result in tables.randomness_rounds_pending.safe_iter() {
                let (round, _) = result.expect("typed_store should not fail");
                inner
                    .network_handle
                    .send_partial_signatures(committee.epoch(), round)
                    .await;
            }
        } else {
            info!(
                "random beacon: no existing DKG output found for epoch {}",
                committee.epoch()
            );
            // Load intermediate data.
            inner.processed_messages.extend(
                tables
                    .dkg_processed_messages
                    .safe_iter()
                    .map(|result| result.expect("typed_store should not fail")),
            );
            if let Some(used_messages) = tables
                .dkg_used_messages
                .get(&SINGLETON_KEY)
                .expect("typed_store should not fail")
            {
                inner
                    .used_messages
                    .set(used_messages.clone())
                    .expect("setting new OnceCell should succeed");
            }
            inner.confirmations.extend(
                tables
                    .dkg_confirmations
                    .safe_iter()
                    .map(|result| result.expect("typed_store should not fail")),
            );
        }

        Some(RandomnessManager {
            inner: Mutex::new(inner),
        })
    }

    /// Sends the initial dkg::Message to begin the randomness DKG protocol.
    pub async fn start_dkg(&self) -> SuiResult {
        self.inner.lock().await.start_dkg()
    }

    /// Processes all received messages and advances the randomness DKG state machine when possible,
    /// sending out a dkg::Confirmation and generating final output.
    pub async fn advance_dkg(&self, batch: &mut DBBatch) -> SuiResult {
        self.inner.lock().await.advance_dkg(batch).await
    }

    /// Adds a received dkg::Message to the randomness DKG state machine.
    // TODO: change to take raw bytes and skip deserialization if a message
    // was already received from the same validator.
    pub async fn add_message(
        &self,
        batch: &mut DBBatch,
        msg: dkg::Message<PkG, EncG>,
    ) -> SuiResult {
        self.inner.lock().await.add_message(batch, msg)
    }

    /// Adds a received dkg::Confirmation to the randomness DKG state machine.
    // TODO: change to take raw bytes and skip deserialization if a message
    // was already received from the same validator.
    pub async fn add_confirmation(
        &self,
        batch: &mut DBBatch,
        conf: dkg::Confirmation<EncG>,
    ) -> SuiResult {
        self.inner.lock().await.add_confirmation(batch, conf)
    }

    /// Reserves the next available round number for randomness generation. Once the given
    /// batch is written, it's considered safe to begin generation for the returned round.
    pub async fn reserve_next_randomness(&self, batch: &mut DBBatch) -> SuiResult<RandomnessRound> {
        self.inner.lock().await.reserve_next_randomness(batch).await
    }

    /// Starts the process of generating the given RandomnessRound.
    pub async fn generate_randomness(&self, epoch: EpochId, randomness_round: RandomnessRound) {
        self.inner
            .lock()
            .await
            .generate_randomness(epoch, randomness_round)
            .await
    }

    /// Notifies the randomness manager that randomness for the given round has been durably
    /// committed in a checkpoint. This completes the process of generating randomness for the
    /// round.
    pub async fn notify_randomness_in_checkpoint(&self, round: RandomnessRound) -> SuiResult {
        self.inner
            .lock()
            .await
            .notify_randomness_in_checkpoint(round)
            .await
    }

    fn randomness_dkg_info_from_committee(
        committee: &Committee,
    ) -> Vec<(
        u16,
        AuthorityName,
        fastcrypto_tbls::ecies::PublicKey<bls12381::G2Element>,
        StakeUnit,
    )> {
        committee
            .members()
            .map(|(name, stake)| {
                let index: u16 = committee
                    .authority_index(name)
                    .expect("lookup of known committee member should succeed")
                    .try_into()
                    .expect("authority index should fit in u16");
                let pk = bls12381::G2Element::from_byte_array(
                    committee
                        .public_key(name)
                        .expect("lookup of known committee member should succeed")
                        .as_bytes()
                        .try_into()
                        .expect("key length should match"),
                )
                .expect("should work to convert BLS key to G2Element");
                (
                    index,
                    name.clone(),
                    fastcrypto_tbls::ecies::PublicKey::from(pk),
                    *stake,
                )
            })
            .collect()
    }
}

impl Inner {
    pub fn start_dkg(&mut self) -> SuiResult {
        if self.used_messages.initialized() || self.dkg_output.initialized() {
            // DKG already started (or completed).
            return Ok(());
        }

        let msg = self.party.create_message(&mut rand::thread_rng());
        info!(
                "random beacon: created DKG Message with sender={}, vss_pk.degree={}, encrypted_shares.len()={}",
                msg.sender,
                msg.vss_pk.degree(),
                msg.encrypted_shares.len(),
            );

        let epoch_store = self.epoch_store()?;
        let transaction = ConsensusTransaction::new_randomness_dkg_message(epoch_store.name, &msg);
        self.consensus_adapter
            .submit(transaction, None, &epoch_store)?;
        Ok(())
    }

    pub async fn advance_dkg(&mut self, batch: &mut DBBatch) -> SuiResult {
        let epoch_store = self.epoch_store()?;

        // Once we have enough ProcessedMessages, send a Confirmation.
        if !self.dkg_output.initialized() && !self.used_messages.initialized() {
            match self.party.merge(
                &self
                    .processed_messages
                    .values()
                    .cloned()
                    .collect::<Vec<_>>(),
            ) {
                Ok((conf, used_msgs)) => {
                    info!(
                        "random beacon: sending DKG Confirmation with {} complaints",
                        conf.complaints.len()
                    );
                    if self.used_messages.set(used_msgs.clone()).is_err() {
                        error!("BUG: used_messages should only ever be set once");
                    }
                    batch.insert_batch(
                        &self.tables()?.dkg_used_messages,
                        std::iter::once((SINGLETON_KEY, used_msgs)),
                    )?;

                    let transaction = ConsensusTransaction::new_randomness_dkg_confirmation(
                        epoch_store.name,
                        &conf,
                    );
                    self.consensus_adapter
                        .submit(transaction, None, &epoch_store)?;
                }
                Err(fastcrypto::error::FastCryptoError::NotEnoughInputs) => (), // wait for more input
                Err(e) => debug!("random beacon: error while merging DKG Messages: {e:?}"),
            }
        }

        // Once we have enough Confirmations, process them and update shares.
        if !self.dkg_output.initialized() && self.used_messages.initialized() {
            match self.party.complete(
                self.used_messages
                    .get()
                    .expect("checked above that `used_messages` is initialized"),
                &self.confirmations.values().cloned().collect::<Vec<_>>(),
                self.party.t() * 2 - 1, // t==f+1, we want 2f+1
                &mut StdRng::from_rng(OsRng).expect("RNG construction should not fail"),
            ) {
                Ok(output) => {
                    let num_shares = output.shares.as_ref().map_or(0, |shares| shares.len());
                    info!("random beacon: DKG complete with {num_shares} shares for this node");
                    epoch_store
                        .metrics
                        .epoch_random_beacon_dkg_num_shares
                        .set(output.shares.as_ref().map_or(0, |shares| shares.len()) as i64);
                    self.dkg_output
                        .set(output.clone())
                        .expect("checked above that `dkg_output` is uninitialized");
                    self.network_handle
                        .update_epoch(
                            epoch_store.committee().epoch(),
                            self.authority_info.clone(),
                            output.clone(),
                            self.party.t(),
                        )
                        .await;
                    batch.insert_batch(
                        &self.tables()?.dkg_output,
                        std::iter::once((SINGLETON_KEY, output)),
                    )?;
                }
                Err(fastcrypto::error::FastCryptoError::NotEnoughInputs) => (), // wait for more input
                Err(e) => error!("random beacon: error while processing DKG Confirmations: {e:?}"),
            }
        }

        Ok(())
    }

    pub fn add_message(&mut self, batch: &mut DBBatch, msg: dkg::Message<PkG, EncG>) -> SuiResult {
        if self.used_messages.initialized() || self.dkg_output.initialized() {
            // We've already sent a `Confirmation`, so we can't add any more messages.
            return Ok(());
        }
        match self.party.process_message(msg, &mut rand::thread_rng()) {
            Ok(processed) => {
                self.processed_messages
                    .insert(processed.message.sender, processed.clone());
                batch.insert_batch(
                    &self.tables()?.dkg_processed_messages,
                    std::iter::once((processed.message.sender, processed)),
                )?;
            }
            Err(err) => {
                debug!("random beacon: error while processing DKG Message: {err:?}");
            }
        }
        Ok(())
    }

    pub fn add_confirmation(
        &mut self,
        batch: &mut DBBatch,
        conf: dkg::Confirmation<EncG>,
    ) -> SuiResult {
        if self.dkg_output.initialized() {
            // Once we have completed DKG, no more `Confirmation`s are needed.
            return Ok(());
        }
        self.confirmations.insert(conf.sender, conf.clone());
        batch.insert_batch(
            &self.tables()?.dkg_confirmations,
            std::iter::once((conf.sender, conf)),
        )?;
        Ok(())
    }

    pub async fn reserve_next_randomness(
        &mut self,
        batch: &mut DBBatch,
    ) -> SuiResult<RandomnessRound> {
        let randomness_round = self.next_randomness_round;
        self.next_randomness_round = self
            .next_randomness_round
            .checked_add(1)
            .expect("RandomnessRound should not overflow");

        batch.insert_batch(
            &self.tables()?.randomness_rounds_pending,
            std::iter::once((randomness_round, ())),
        )?;
        batch.insert_batch(
            &self.tables()?.randomness_next_round,
            std::iter::once((SINGLETON_KEY, self.next_randomness_round)),
        )?;

        Ok(randomness_round)
    }

    pub async fn generate_randomness(&self, epoch: EpochId, randomness_round: RandomnessRound) {
        self.network_handle
            .send_partial_signatures(epoch, randomness_round)
            .await;
    }

    pub async fn notify_randomness_in_checkpoint(&self, round: RandomnessRound) -> SuiResult {
        self.tables()?.randomness_rounds_pending.remove(&round)?;
        self.network_handle
            .cancel_send_partial_signatures(self.epoch_store()?.committee().epoch(), round)
            .await;
        Ok(())
    }

    fn epoch_store(&self) -> SuiResult<Arc<AuthorityPerEpochStore>> {
        self.epoch_store.upgrade().ok_or(SuiError::EpochEnded)
    }

    fn tables(&self) -> SuiResult<Arc<AuthorityEpochTables>> {
        self.epoch_store()?.tables()
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        authority::test_authority_builder::TestAuthorityBuilder,
        consensus_adapter::{
            ConnectionMonitorStatusForTests, ConsensusAdapter, ConsensusAdapterMetrics,
            MockSubmitToConsensus,
        },
        epoch::randomness::*,
    };
    use std::num::NonZeroUsize;
    use sui_types::messages_consensus::ConsensusTransactionKind;
    use tokio::sync::mpsc;

    #[tokio::test]
    async fn test_dkg() {
        telemetry_subscribers::init_for_testing();

        let network_config =
            sui_swarm_config::network_config_builder::ConfigBuilder::new_with_temp_dir()
                .committee_size(NonZeroUsize::new(4).unwrap())
                .with_reference_gas_price(500)
                .build();

        let mut epoch_stores = Vec::new();
        let mut randomness_managers = Vec::new();
        let (tx_consensus, mut rx_consensus) = mpsc::channel(100);

        for validator in network_config.validator_configs.iter() {
            // Send consensus messages to channel.
            let mut mock_consensus_client = MockSubmitToConsensus::new();
            let tx_consensus = tx_consensus.clone();
            mock_consensus_client
                .expect_submit_to_consensus()
                .withf(move |transaction: &ConsensusTransaction, _epoch_store| {
                    tx_consensus.try_send(transaction.clone()).unwrap();
                    true
                })
                .returning(|_, _| Ok(()));

            let state = TestAuthorityBuilder::new()
                .with_genesis_and_keypair(&network_config.genesis, validator.protocol_key_pair())
                .build()
                .await;
            let consensus_adapter = Arc::new(ConsensusAdapter::new(
                Arc::new(mock_consensus_client),
                state.name,
                Arc::new(ConnectionMonitorStatusForTests {}),
                100_000,
                100_000,
                None,
                None,
                ConsensusAdapterMetrics::new_test(),
                state.epoch_store_for_testing().protocol_config().clone(),
            ));
            let epoch_store = state.epoch_store_for_testing();
            let randomness_manager = RandomnessManager::try_new(
                Arc::downgrade(&epoch_store),
                consensus_adapter.clone(),
                sui_network::randomness::Handle::new_stub(),
                validator.protocol_key_pair(),
            )
            .await
            .unwrap();

            epoch_stores.push(epoch_store);
            randomness_managers.push(randomness_manager);
        }

        // Generate and distribute Messages.
        let mut dkg_messages = Vec::new();
        for randomness_manager in &randomness_managers {
            randomness_manager.start_dkg().await.unwrap();

            let dkg_message = rx_consensus.recv().await.unwrap();
            match dkg_message.kind {
                ConsensusTransactionKind::RandomnessDkgMessage(_, bytes) => {
                    let msg: fastcrypto_tbls::dkg::Message<PkG, EncG> = bcs::from_bytes(&bytes)
                        .expect("DKG message deserialization should not fail");
                    dkg_messages.push(msg);
                }
                _ => panic!("wrong type of message sent"),
            }
        }
        for i in 0..randomness_managers.len() {
            let mut batch = epoch_stores[i]
                .tables()
                .unwrap()
                .dkg_processed_messages
                .batch();
            for dkg_message in dkg_messages.iter().cloned() {
                randomness_managers[i]
                    .add_message(&mut batch, dkg_message)
                    .await
                    .unwrap();
            }
            randomness_managers[i]
                .advance_dkg(&mut batch)
                .await
                .unwrap();
            batch.write().unwrap();
        }

        // Generate and distribute Confirmations.
        let mut dkg_confirmations = Vec::new();
        for _ in 0..randomness_managers.len() {
            let dkg_confirmation = rx_consensus.recv().await.unwrap();
            match dkg_confirmation.kind {
                ConsensusTransactionKind::RandomnessDkgConfirmation(_, bytes) => {
                    let msg: fastcrypto_tbls::dkg::Confirmation<EncG> = bcs::from_bytes(&bytes)
                        .expect("DKG confirmation deserialization should not fail");
                    dkg_confirmations.push(msg);
                }
                _ => panic!("wrong type of message sent"),
            }
        }
        for i in 0..randomness_managers.len() {
            let mut batch = epoch_stores[i].tables().unwrap().dkg_confirmations.batch();
            for dkg_confirmation in dkg_confirmations.iter().cloned() {
                randomness_managers[i]
                    .add_confirmation(&mut batch, dkg_confirmation)
                    .await
                    .unwrap();
            }
            randomness_managers[i]
                .advance_dkg(&mut batch)
                .await
                .unwrap();
            batch.write().unwrap();
        }

        // Verify DKG completed.
        for randomness_manager in &randomness_managers {
            assert!(randomness_manager
                .inner
                .lock()
                .await
                .dkg_output
                .initialized());
        }
    }
}
