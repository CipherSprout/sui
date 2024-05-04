/* 20240504
* Parse consensus output byte array to ConsensusTransactionWrapper
* Replace origin trait consensus_types/consensus_output_api.rs
*/
use std::fmt::Display;

use crate::{consensus_types::AuthorityIndex, ConsensusTransactionWrapper};
use consensus_core::BlockAPI;
use fastcrypto::hash::Hash;
use narwhal_types::{BatchAPI, CertificateAPI, ConsensusOutputDigest, HeaderAPI};
use sui_types::{digests::ConsensusCommitDigest, messages_consensus::ConsensusTransaction};

/// A list of tuples of:
/// (certificate origin authority index, all transactions corresponding to the certificate).
/// For each transaction, returns the serialized transaction and the deserialized transaction.
type ConsensusOutputTransactions<'a> =
    Vec<(AuthorityIndex, Vec<(&'a [u8], ConsensusTransactionWrapper)>)>;

pub(crate) trait ConsensusOutputAPI: Display {
    fn reputation_score_sorted_desc(&self) -> Option<Vec<(AuthorityIndex, u64)>>;
    fn leader_round(&self) -> u64;
    fn leader_author_index(&self) -> AuthorityIndex;

    /// Returns epoch UNIX timestamp in milliseconds
    fn commit_timestamp_ms(&self) -> u64;

    /// Returns a unique global index for each committed sub-dag.
    fn commit_sub_dag_index(&self) -> u64;

    /// Returns all transactions in the commit.
    fn transactions(&self) -> ConsensusOutputTransactions<'_>;

    /// Returns the digest of consensus output.
    fn consensus_digest(&self) -> ConsensusCommitDigest;
}

impl ConsensusOutputAPI for narwhal_types::ConsensusOutput {
    fn reputation_score_sorted_desc(&self) -> Option<Vec<(AuthorityIndex, u64)>> {
        if !self.sub_dag.reputation_score.final_of_schedule {
            return None;
        }
        Some(
            self.sub_dag
                .reputation_score
                .authorities_by_score_desc()
                .into_iter()
                .map(|(id, score)| (id.0 as AuthorityIndex, score))
                .collect(),
        )
    }

    fn leader_round(&self) -> u64 {
        self.sub_dag.leader_round()
    }

    fn leader_author_index(&self) -> AuthorityIndex {
        self.sub_dag.leader.origin().0 as AuthorityIndex
    }

    fn commit_timestamp_ms(&self) -> u64 {
        self.sub_dag.commit_timestamp()
    }

    fn commit_sub_dag_index(&self) -> u64 {
        self.sub_dag.sub_dag_index
    }

    fn transactions(&self) -> ConsensusOutputTransactions {
        assert!(self.sub_dag.certificates.len() == self.batches.len());
        self.sub_dag
            .certificates
            .iter()
            .zip(&self.batches)
            .map(|(cert, batches)| {
                assert_eq!(cert.header().payload().len(), batches.len());
                
                let transactions: Vec<(&[u8], ConsensusTransactionWrapper)> = batches
                    .iter()
                    .flat_map(|batch| {
                        let digest = batch.digest();
                        assert!(cert.header().payload().contains_key(&digest));
                        /*
                        * 20240504 
                        * Keep original closure for reference,
                        * replace by custom closure for parser serialized transaction
                        */
                        // let _original_closure = move |serialized_transaction: &Vec<u8>| {
                        //     let transaction = match bcs::from_bytes::<ConsensusTransaction>(serialized_transaction)
                        //     {
                        //         Ok(transaction) => transaction,
                        //         Err(err) => {
                        //             // This should have been prevented by Narwhal batch verification.
                        //             panic!(
                        //                                 "Unexpected malformed transaction (failed to deserialize): {}\nCertificate={:?} BatchDigest={:?} Transaction={:?}",
                        //                                 err, cert, digest, serialized_transaction
                        //                             );
                        //         }
                        //     };
                        //     (serialized_transaction.as_ref(), transaction)
                        // };
    
                        batch.transactions().iter().map(move |serialized_transaction: &Vec<u8>| {
                            let transaction = bcs::from_bytes::<ConsensusTransaction>(serialized_transaction)
                            .map_or_else(
                                    |_| match bcs::from_bytes::<ConsensusTransactionWrapper>(
                                        serialized_transaction,
                                    ) {
                                        Ok(transaction) => transaction,
                                        Err(err) => {
                                            // This should have been prevented by Narwhal batch verification.
                                            panic!(
                                                "Unexpected malformed transaction (failed to deserialize): {}\nCertificate={:?} BatchDigest={:?} Transaction={:?}",
                                                err, cert, digest, serialized_transaction
                                            );
                                        }
                                    },
                                    |transaction| {
                                        ConsensusTransactionWrapper::Consensus(transaction)
                                    },
                                );
                            (serialized_transaction.as_ref(), transaction)
                        })
                    })
                    .collect();
                (cert.origin().0 as AuthorityIndex, transactions)
            })
            .collect()
    }

    fn consensus_digest(&self) -> ConsensusCommitDigest {
        // We port ConsensusOutputDigest, a narwhal space object, into ConsensusCommitDigest, a sui-core space object.
        // We assume they always have the same format.
        static_assertions::assert_eq_size!(ConsensusCommitDigest, ConsensusOutputDigest);
        ConsensusCommitDigest::new(self.digest().into_inner())
    }
}

impl ConsensusOutputAPI for consensus_core::CommittedSubDag {
    fn reputation_score_sorted_desc(&self) -> Option<Vec<(AuthorityIndex, u64)>> {
        // TODO: Implement this in Mysticeti.
        None
    }

    fn leader_round(&self) -> u64 {
        self.leader.round as u64
    }

    fn leader_author_index(&self) -> AuthorityIndex {
        self.leader.author.value() as AuthorityIndex
    }

    fn commit_timestamp_ms(&self) -> u64 {
        // TODO: Enforce ordered timestamp in Mysticeti.
        self.timestamp_ms
    }

    fn commit_sub_dag_index(&self) -> u64 {
        self.commit_index.into()
    }

    fn transactions(&self) -> ConsensusOutputTransactions {
        self.blocks
            .iter()
            .map(|block| {
                let round = block.round();
                let author = block.author().value() as AuthorityIndex;
                /*
                * 20240504 
                * Keep original closure for reference,
                * replace by custom closure for parser serialized transaction
                */
                // let original_closure = |tx| {
                //     let transaction = bcs::from_bytes::<ConsensusTransaction>(tx.data());
                //     match transaction {
                //         Ok(transaction) => Some((
                //             tx.data(),
                //             transaction,
                //         )),
                //         Err(err) => {
                //             tracing::error!("Failed to deserialize sequenced consensus transaction(this should not happen) {} from {author} at {round}", err);
                //             None
                //         },
                //     }
                // };
                let transactions: Vec<_> = block
                    .transactions()
                    .iter()
                    .flat_map(|tx| {
                        let data = tx.data();
                        bcs::from_bytes::<ConsensusTransaction>(data)
                            .map_or_else(
                                |_| match bcs::from_bytes::<ConsensusTransactionWrapper>(
                                    data,
                                ) {
                                    Ok(transaction) => Some((data, transaction)),
                                    Err(err) => {
                                        // This should have been prevented by Narwhal batch verification.
                                        tracing::error!("Failed to deserialize sequenced consensus transaction(this should not happen) {} from {author} at {round}", err);
                                        None
                                    }
                                },
                                |transaction| {
                                    Some((data, ConsensusTransactionWrapper::Consensus(transaction)))
                                },
                            )
                    })
                    .collect();
                (author, transactions)
            })
            .collect()
    }

    fn consensus_digest(&self) -> ConsensusCommitDigest {
        // TODO(mysticeti): implement consensus output digest.
        ConsensusCommitDigest::default()
    }
}
