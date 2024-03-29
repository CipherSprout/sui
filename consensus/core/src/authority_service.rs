// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{sync::Arc, time::Duration};

use async_trait::async_trait;
use bytes::Bytes;
use consensus_config::AuthorityIndex;
use parking_lot::RwLock;
use tokio::time::sleep;
use tracing::{info, warn};

use crate::{
    block::{timestamp_utc_ms, BlockAPI as _, BlockRef, SignedBlock, VerifiedBlock},
    block_verifier::BlockVerifier,
    context::Context,
    core_thread::CoreThreadDispatcher,
    dag_state::DagState,
    error::{ConsensusError, ConsensusResult},
    network::NetworkService,
    synchronizer::SynchronizerHandle,
};

/// Authority's network interface.
pub(crate) struct AuthorityService<C: CoreThreadDispatcher> {
    context: Arc<Context>,
    block_verifier: Arc<dyn BlockVerifier>,
    core_dispatcher: Arc<C>,
    synchronizer: Arc<SynchronizerHandle>,
    dag_state: Arc<RwLock<DagState>>,
}

impl<C: CoreThreadDispatcher> AuthorityService<C> {
    pub(crate) fn new(
        context: Arc<Context>,
        block_verifier: Arc<dyn BlockVerifier>,
        core_dispatcher: Arc<C>,
        synchronizer: Arc<SynchronizerHandle>,
        dag_state: Arc<RwLock<DagState>>,
    ) -> Self {
        Self {
            context,
            block_verifier,
            core_dispatcher,
            synchronizer,
            dag_state,
        }
    }

    pub(crate) async fn handle_received_block(
        &self,
        peer: AuthorityIndex,
        serialized_block: Bytes,
    ) -> ConsensusResult<()> {
        // TODO: dedup block verifications, here and with fetched blocks.
        let signed_block: SignedBlock =
            bcs::from_bytes(&serialized_block).map_err(ConsensusError::MalformedBlock)?;

        // Reject blocks not produced by the peer.
        if peer != signed_block.author() {
            self.context
                .metrics
                .node_metrics
                .invalid_blocks
                .with_label_values(&[&peer.to_string(), "send_block"])
                .inc();
            let e = ConsensusError::UnexpectedAuthority(signed_block.author(), peer);
            info!("Block with wrong authority from {}: {}", peer, e);
            return Err(e);
        }

        // Reject blocks failing validations.
        if let Err(e) = self.block_verifier.verify(&signed_block) {
            self.context
                .metrics
                .node_metrics
                .invalid_blocks
                .with_label_values(&[&peer.to_string(), "send_block"])
                .inc();
            info!("Invalid block from {}: {}", peer, e);
            return Err(e);
        }
        let verified_block = VerifiedBlock::new_verified(signed_block, serialized_block);

        // Reject block with timestamp too far in the future.
        let forward_time_drift = Duration::from_millis(
            verified_block
                .timestamp_ms()
                .saturating_sub(timestamp_utc_ms()),
        );
        if forward_time_drift > self.context.parameters.max_forward_time_drift {
            return Err(ConsensusError::BlockTooFarInFuture {
                block_timestamp: verified_block.timestamp_ms(),
                forward_time_drift,
            });
        }

        // Wait until the block's timestamp is current.
        if forward_time_drift > Duration::ZERO {
            self.context
                .metrics
                .node_metrics
                .block_timestamp_drift_wait_ms
                .with_label_values(&[&peer.to_string()])
                .inc_by(forward_time_drift.as_millis() as u64);
            sleep(forward_time_drift).await;
        }

        let missing_ancestors = self
            .core_dispatcher
            .add_blocks(vec![verified_block])
            .await
            .map_err(|_| ConsensusError::Shutdown)?;

        if !missing_ancestors.is_empty() {
            // schedule the fetching of them from this peer
            if let Err(err) = self
                .synchronizer
                .fetch_blocks(missing_ancestors, peer)
                .await
            {
                warn!("Errored while trying to fetch missing ancestors via synchronizer: {err}");
            }
        }

        Ok(())
    }
}

#[async_trait]
impl<C: CoreThreadDispatcher> NetworkService for AuthorityService<C> {
    async fn handle_send_block(
        &self,
        peer: AuthorityIndex,
        serialized_block: Bytes,
    ) -> ConsensusResult<()> {
        self.handle_received_block(peer, serialized_block).await
    }

    async fn handle_fetch_blocks(
        &self,
        peer: AuthorityIndex,
        block_refs: Vec<BlockRef>,
    ) -> ConsensusResult<Vec<Bytes>> {
        const MAX_ALLOWED_FETCH_BLOCKS: usize = 200;

        if block_refs.len() > MAX_ALLOWED_FETCH_BLOCKS {
            return Err(ConsensusError::TooManyFetchBlocksRequested(peer));
        }

        // Some quick validation of the requested block refs
        for block in &block_refs {
            if !self.context.committee.is_valid_index(block.author) {
                return Err(ConsensusError::InvalidAuthorityIndex {
                    index: block.author,
                    max: self.context.committee.size(),
                });
            }
            if block.round == 0 {
                return Err(ConsensusError::UnexpectedGenesisBlockRequested);
            }
        }

        // For now ask dag state directly
        let blocks = self.dag_state.read().get_blocks(&block_refs);

        // Return the serialised blocks
        let result = blocks
            .into_iter()
            .flatten()
            .map(|block| block.serialized().clone())
            .collect::<Vec<_>>();

        Ok(result)
    }
}
