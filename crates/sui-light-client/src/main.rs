// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::anyhow;
use async_trait::async_trait;
use move_core_types::account_address::AccountAddress;
use sui_json_rpc_types::SuiTransactionBlockResponseOptions;

use sui_rest_api::{CheckpointData, Client};
use sui_types::{
    base_types::{ObjectID, SequenceNumber},
    committee::Committee,
    crypto::AuthorityQuorumSignInfo,
    digests::TransactionDigest,
    effects::{TransactionEffects, TransactionEffectsAPI, TransactionEvents},
    message_envelope::Envelope,
    messages_checkpoint::{CertifiedCheckpointSummary, CheckpointSummary, EndOfEpochData},
    object::Data,
};

use sui_config::genesis::Genesis;

use sui_json::SuiJsonValue;
use sui_package_resolver::Result as ResolverResult;
use sui_package_resolver::{Package, PackageStore, Resolver};
use sui_sdk::SuiClientBuilder;

use clap::{Parser, Subcommand};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    str::FromStr,
};
use std::{io::Read, sync::Arc};

/// A light client for the Sui blockchain
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Sets a custom config file
    #[arg(short, long, value_name = "FILE")]
    config: Option<PathBuf>,

    #[command(subcommand)]
    command: Option<SCommands>,
}

struct RemotePackageStore {
    client: Client,
    config: Config,
}

impl RemotePackageStore {
    pub fn new(client: Client, config: Config) -> Self {
        Self { client, config }
    }
}

#[async_trait]
impl PackageStore for RemotePackageStore {
    /// Latest version of the object at `id`.
    async fn version(&self, id: AccountAddress) -> ResolverResult<SequenceNumber> {
        Ok(self.client.get_object(id.into()).await.unwrap().version())
    }
    /// Read package contents. Fails if `id` is not an object, not a package, or is malformed in
    /// some way.
    async fn fetch(&self, id: AccountAddress) -> ResolverResult<Arc<Package>> {
        let object = self.client.get_object(id.into()).await.unwrap();

        // Need to authenticate this object
        let (effects, _) = check_transaction_tid(&self.config, object.previous_transaction)
            .await
            .unwrap();
        // check that this object ID, version and hash is in the effects
        effects
            .all_changed_objects()
            .iter()
            .find(|oref| oref.0 == object.compute_object_reference())
            .unwrap();

        let package = Package::read(&object).unwrap();
        Ok(Arc::new(package))
    }
}

#[derive(Subcommand, Debug)]
enum SCommands {
    /// Sync all end-of-epoch checkpoints
    Sync {},

    /// Checks a specific transaction using the light client
    Transaction {
        /// Transaction hash
        #[arg(short, long, value_name = "TID")]
        tid: String,
    },

    /// Checks a specific object using the light client
    Object {
        /// Transaction hash
        #[arg(short, long, value_name = "OID")]
        oid: String,
    },
}

// The config file for the light client inclding the root of trust genesis digest
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
struct Config {
    /// Full node url
    full_node_url: String,

    /// Checkpoint summary directory
    checkpoint_summary_dir: PathBuf,

    //  Genesis file name
    genesis_filename: PathBuf,
}

// The list of checkpoints at the end of each epoch
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
struct CheckpointsList {
    // List of end of epoch checkpoints
    checkpoints: Vec<u64>,
}

fn read_checkpoint_list(config: &Config) -> anyhow::Result<CheckpointsList> {
    let mut checkpoints_path = config.checkpoint_summary_dir.clone();
    checkpoints_path.push("checkpoints.yaml");
    // Read the resulting file and parse the yaml checkpoint list
    let reader = fs::File::open(checkpoints_path.clone())?;
    Ok(serde_yaml::from_reader(reader)?)
}

fn read_checkpoint(
    config: &Config,
    seq: u64,
) -> anyhow::Result<Envelope<CheckpointSummary, AuthorityQuorumSignInfo<true>>> {
    read_checkpoint_general(config, seq, None)
}

fn read_checkpoint_general(
    config: &Config,
    seq: u64,
    path: Option<&str>,
) -> anyhow::Result<Envelope<CheckpointSummary, AuthorityQuorumSignInfo<true>>> {
    // Read the resulting file and parse the yaml checkpoint list
    let mut checkpoint_path = config.checkpoint_summary_dir.clone();
    if let Some(path) = path {
        checkpoint_path.push(path);
    }
    checkpoint_path.push(format!("{}.yaml", seq));
    let mut reader = fs::File::open(checkpoint_path.clone())?;
    let metadata = fs::metadata(&checkpoint_path)?;
    let mut buffer = vec![0; metadata.len() as usize];
    reader.read_exact(&mut buffer)?;
    bcs::from_bytes(&buffer).map_err(|_| anyhow!("Unable to parse checkpoint file"))
}

fn write_checkpoint(
    config: &Config,
    summary: &Envelope<CheckpointSummary, AuthorityQuorumSignInfo<true>>,
) -> anyhow::Result<()> {
    write_checkpoint_general(config, summary, None)
}

fn write_checkpoint_general(
    config: &Config,
    summary: &Envelope<CheckpointSummary, AuthorityQuorumSignInfo<true>>,
    path: Option<&str>,
) -> anyhow::Result<()> {
    // Write the checkpoint summary to a file
    let mut checkpoint_path = config.checkpoint_summary_dir.clone();
    if let Some(path) = path {
        checkpoint_path.push(path);
    }
    checkpoint_path.push(format!("{}.yaml", summary.sequence_number));
    let mut writer = fs::File::create(checkpoint_path.clone())?;
    let bytes =
        bcs::to_bytes(&summary).map_err(|_| anyhow!("Unable to serialize checkpoint summary"))?;
    writer.write_all(&bytes)?;
    Ok(())
}

fn write_checkpoint_list(
    config: &Config,
    checkpoints_list: &CheckpointsList,
) -> anyhow::Result<()> {
    // Write the checkpoint list to a file
    let mut checkpoints_path = config.checkpoint_summary_dir.clone();
    checkpoints_path.push("checkpoints.yaml");
    let mut writer = fs::File::create(checkpoints_path.clone())?;
    let bytes = serde_yaml::to_vec(&checkpoints_list)?;
    writer
        .write_all(&bytes)
        .map_err(|_| anyhow!("Unable to serialize checkpoint list"))
}

async fn download_checkpoint_summary(
    config: &Config,
    seq: u64,
) -> anyhow::Result<CertifiedCheckpointSummary> {
    // Download the checkpoint from the server
    let client = Client::new(config.full_node_url.as_str());
    client.get_checkpoint_summary(seq).await
}

/// Run binary search to for each end of epoch checkpoint that is missing
/// between the latest on the list and the latest checkpoint.
async fn pre_sync_checkpoints_to_latest(config: &Config) -> anyhow::Result<()> {
    // Get the local checlpoint list
    let mut checkpoints_list: CheckpointsList = read_checkpoint_list(config)?;
    let latest_in_list = checkpoints_list
        .checkpoints
        .last()
        .ok_or(anyhow!("Empty checkpoint list"))?;

    // Download the latest in list checkpoint
    let summary = download_checkpoint_summary(config, *latest_in_list).await?;
    let mut last_epoch = summary.epoch();
    let mut last_checkpoint_seq = summary.sequence_number;

    // Download the very latest checkpoint
    let client = Client::new(config.full_node_url.as_str());
    let latest = client.get_latest_checkpoint().await?;

    // Binary search to find missing checkpoints
    while last_epoch + 1 < latest.epoch() {
        let mut start = last_checkpoint_seq;
        let mut end = latest.sequence_number;

        let taget_epoch = last_epoch + 1;
        // Print target
        println!("Target Epoch: {}", taget_epoch);
        let mut found_summary = None;

        while start < end {
            let mid = (start + end) / 2;
            let summary = download_checkpoint_summary(config, mid).await?;

            // print summary epoch and seq
            println!(
                "Epoch: {} Seq: {}: {}",
                summary.epoch(),
                summary.sequence_number,
                summary.end_of_epoch_data.is_some()
            );

            if summary.epoch() == taget_epoch && summary.end_of_epoch_data.is_some() {
                found_summary = Some(summary);
                break;
            }

            if summary.epoch() <= taget_epoch {
                start = mid + 1;
            } else {
                end = mid;
            }
        }

        if let Some(summary) = found_summary {
            // Note: Do not write summary to file, since we must only persist
            //       checkpoints that have been verified by the previous committee

            // Add to the list
            checkpoints_list.checkpoints.push(summary.sequence_number);
            write_checkpoint_list(config, &checkpoints_list)?;

            // Update
            last_epoch = summary.epoch();
            last_checkpoint_seq = summary.sequence_number;
        }
    }

    Ok(())
}

async fn check_and_sync_checkpoints(config: &Config) -> anyhow::Result<()> {
    pre_sync_checkpoints_to_latest(config).await?;

    // Get the local checlpoint list
    let checkpoints_list: CheckpointsList = read_checkpoint_list(config)?;

    // Load the genesis committee
    let mut genesis_path = config.checkpoint_summary_dir.clone();
    genesis_path.push(&config.genesis_filename);
    let genesis_committee = Genesis::load(&genesis_path)?.committee()?;

    // Check the signatures of all checkpoints
    // And download any missing ones

    let mut prev_committee = genesis_committee;
    for ckp_id in &checkpoints_list.checkpoints {
        // check if there is a file with this name ckp_id.yaml in the checkpoint_summary_dir
        let mut checkpoint_path = config.checkpoint_summary_dir.clone();
        checkpoint_path.push(format!("{}.yaml", ckp_id));

        // If file exists read the file otherwise download it from the server
        let summary = if checkpoint_path.exists() {
            read_checkpoint(config, *ckp_id)?
        } else {
            // Download the checkpoint from the server
            download_checkpoint_summary(config, *ckp_id).await?
        };

        summary.clone().verify(&prev_committee)?;

        // Pirnt the id of the checkpoint and the epoch number
        println!(
            "Epoch: {} Checkpoint ID: {}",
            summary.epoch(),
            summary.digest()
        );

        // Exract the new committee information
        if let Some(EndOfEpochData {
            next_epoch_committee,
            ..
        }) = &summary.end_of_epoch_data
        {
            let next_committee = next_epoch_committee.iter().cloned().collect();
            let committee = Committee::new(summary.epoch().saturating_add(1), next_committee);
            bcs::to_bytes(&committee)?;
            prev_committee = committee;
        } else {
            return Err(anyhow!(
                "Expected all checkpoints to be end-of-epoch checkpoints"
            ));
        }

        // Write the checkpoint summary to a file
        write_checkpoint(config, &summary)?;
    }

    Ok(())
}

async fn read_full_checkpoint(checkpoint_path: &PathBuf) -> anyhow::Result<CheckpointData> {
    let mut reader = fs::File::open(checkpoint_path.clone())?;
    let metadata = fs::metadata(checkpoint_path)?;
    let mut buffer = vec![0; metadata.len() as usize];
    reader.read_exact(&mut buffer)?;
    bcs::from_bytes(&buffer).map_err(|_| anyhow!("Unable to parse checkpoint file"))
}

async fn write_full_checkpoint(
    checkpoint_path: &Path,
    ckpt: &CheckpointData,
) -> anyhow::Result<()> {
    let mut writer = fs::File::create(checkpoint_path)?;
    let bytes =
        bcs::to_bytes(&ckpt).map_err(|_| anyhow!("Unable to serialize checkpoint summary"))?;
    writer.write_all(&bytes)?;
    Ok(())
}

async fn get_full_checkpoint(config: &Config, seq: u64) -> anyhow::Result<CheckpointData> {
    let mut checkpoint_path = config.checkpoint_summary_dir.clone();
    checkpoint_path.push("untrusted_cache");
    checkpoint_path.push(format!("{}.bcs", seq));

    // Try reading the cache
    if let Ok(ckpt) = read_full_checkpoint(&checkpoint_path).await {
        return Ok(ckpt);
    }

    // Downlioading the checkpoint from the server
    let client: Client = Client::new(config.full_node_url.as_str());
    let full_check_point = client.get_full_checkpoint(seq).await?;

    // Add to cache
    write_full_checkpoint(&checkpoint_path, &full_check_point).await?;

    Ok(full_check_point)
}

fn assert_contains_transaction_effects(
    checkpoint: &CheckpointData,
    committee: &Committee,
    tid: TransactionDigest,
) -> anyhow::Result<(TransactionEffects, Option<TransactionEvents>)> {
    let summary = &checkpoint.checkpoint_summary;

    // Verify the checkpoint summary using the committee
    summary.clone().verify(committee)?;

    // Check the validty of the checkpoint contents
    let contents = &checkpoint.checkpoint_contents;
    anyhow::ensure!(
        contents.digest() == &summary.content_digest,
        "The content digest in the checkpoint summary does not match the digest of the checkpoint contents"
    );

    // Check the validity of the transaction
    let matching_tx = checkpoint
        .transactions
        .iter()
        // Note that we get the digest of the effects to ensure this is
        // indeed the correct effects that are authenticated in the contents.
        .find(|tx| tx.effects.execution_digests().transaction == tid)
        .ok_or(anyhow!("Transaction not found in checkpoint contents"))?;

    // The effects are correct by the check above that creates matching_tx

    // Check the events are all correct.
    let events_digest = matching_tx.events.as_ref().map(|events| events.digest());
    anyhow::ensure!(
        events_digest.as_ref() == matching_tx.effects.events_digest(),
        "Events digest does not match"
    );

    // Since we do not check objects we do not return them
    Ok((matching_tx.effects.clone(), matching_tx.events.clone()))
}

async fn check_transaction_tid(
    config: &Config,
    tid: TransactionDigest,
) -> anyhow::Result<(TransactionEffects, Option<TransactionEvents>)> {
    let sui_mainnet: Arc<sui_sdk::SuiClient> = Arc::new(
        SuiClientBuilder::default()
            .build("http://ord-mnt-rpcbig-06.mainnet.sui.io:9000")
            .await
            .unwrap(),
    );
    let read_api = sui_mainnet.read_api();

    // Lookup the transaction id and get the checkpoint sequence number
    let options = SuiTransactionBlockResponseOptions::new();
    let seq = read_api
        .get_transaction_with_options(tid, options)
        .await?
        .checkpoint
        .ok_or(anyhow!("Transaction not found"))?;

    // Download the full checkpoint for this sequence number
    let full_check_point = get_full_checkpoint(config, seq).await?;

    // Load the list of stored checkpoints
    let checkpoints_list: CheckpointsList = read_checkpoint_list(config)?;
    // find the stored checkpoint before the seq checkpoint
    let prev_ckp_id = checkpoints_list
        .checkpoints
        .iter()
        .filter(|ckp_id| **ckp_id < seq)
        .last()
        .ok_or(anyhow!("No checkpoint found before the transaction"))?;

    // Read it from the store
    let prev_ckp = read_checkpoint(config, *prev_ckp_id)?;

    // Get the committee from the previous checkpoint
    let prev_committee = prev_ckp
        .end_of_epoch_data
        .as_ref()
        .ok_or(anyhow!(
            "Expected all checkpoints to be end-of-epoch checkpoints"
        ))?
        .next_epoch_committee
        .iter()
        .cloned()
        .collect();

    // Make a commitee object using this
    let committee = Committee::new(prev_ckp.epoch().saturating_add(1), prev_committee);
    println!("Committee: {:?}", prev_ckp.sequence_number());

    assert_contains_transaction_effects(&full_check_point, &committee, tid)
}

#[tokio::main]
pub async fn main() {
    // Command line arguments and config loading
    let args = Args::parse();

    let path = args
        .config
        .unwrap_or_else(|| panic!("Need a config file path"));
    let reader = fs::File::open(path.clone())
        .unwrap_or_else(|_| panic!("Unable to load config from {}", path.display()));
    let config: Config = serde_yaml::from_reader(reader).unwrap();

    // Print config parameters
    println!(
        "Checkpoint Dir: {}",
        config.checkpoint_summary_dir.display()
    );

    let client: Client = Client::new(config.full_node_url.as_str());
    let remote_package_store = RemotePackageStore::new(client, config.clone());
    let resolver = Resolver::new(remote_package_store);

    match args.command {
        Some(SCommands::Transaction { tid }) => {
            let (effects, events) =
                check_transaction_tid(&config, TransactionDigest::from_str(&tid).unwrap())
                    .await
                    .unwrap();

            let exec_digests = effects.execution_digests();
            println!(
                "Executed TID: {} Effects: {}",
                exec_digests.transaction, exec_digests.effects
            );

            for event in events.as_ref().unwrap().data.iter() {
                let type_layout = resolver
                    .type_layout(event.type_.clone().into())
                    .await
                    .unwrap();

                let json_val =
                    SuiJsonValue::from_bcs_bytes(Some(&type_layout), &event.contents).unwrap();

                println!(
                    "Event:\n - Package: {}\n - Module: {}\n - Sender: {}\n - Type: {}\n{}",
                    event.package_id,
                    event.transaction_module,
                    event.sender,
                    event.type_,
                    serde_json::to_string_pretty(&json_val.to_json_value()).unwrap()
                );
            }
        }
        Some(SCommands::Object { oid }) => {
            let client = Client::new(config.full_node_url.as_str());
            let object = client
                .get_object(ObjectID::from_str(&oid).unwrap())
                .await
                .unwrap();

            // Authenticate the object
            // Need to authenticate this object
            let (effects, _) = check_transaction_tid(&config, object.previous_transaction)
                .await
                .unwrap();

            // check that this object ID, version and hash is in the effects
            effects
                .all_changed_objects()
                .iter()
                .find(|oref| oref.0 == object.compute_object_reference())
                .unwrap();

            if let Data::Move(move_object) = &object.data {
                let object_type = move_object.type_().clone();

                let type_layout = resolver
                    .type_layout(object_type.clone().into())
                    .await
                    .unwrap();

                let json_val =
                    SuiJsonValue::from_bcs_bytes(Some(&type_layout), move_object.contents())
                        .unwrap();

                let (oid, version, hash) = object.compute_object_reference();
                println!(
                    "OID: {}\n - Version: {}\n - Hash: {}\n - Owner: {}\n - Type: {}\n{}",
                    oid,
                    version,
                    hash,
                    object.owner,
                    object_type,
                    serde_json::to_string_pretty(&json_val.to_json_value()).unwrap()
                );
            }
        }

        Some(SCommands::Sync {}) => {
            check_and_sync_checkpoints(&config)
                .await
                .expect("Failed to sync checkpoints");
        }
        _ => {}
    }
}

// Make a test namespace
#[cfg(test)]
mod tests {
    use sui_types::messages_checkpoint::FullCheckpointContents;

    use super::*;
    use std::path::PathBuf;

    async fn read_data() -> (Committee, CheckpointData) {
        let mut d = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        d.push("example_config/20873329.yaml");

        let mut reader = fs::File::open(d.clone()).unwrap();
        let metadata = fs::metadata(&d).unwrap();
        let mut buffer = vec![0; metadata.len() as usize];
        reader.read_exact(&mut buffer).unwrap();
        let checkpoint: Envelope<CheckpointSummary, AuthorityQuorumSignInfo<true>> =
            bcs::from_bytes(&buffer)
                .map_err(|_| anyhow!("Unable to parse checkpoint file"))
                .unwrap();

        let prev_committee = checkpoint
            .end_of_epoch_data
            .as_ref()
            .ok_or(anyhow!(
                "Expected all checkpoints to be end-of-epoch checkpoints"
            ))
            .unwrap()
            .next_epoch_committee
            .iter()
            .cloned()
            .collect();

        // Make a commitee object using this
        let committee = Committee::new(checkpoint.epoch().saturating_add(1), prev_committee);

        let mut d = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        d.push("example_config/20958462.bcs");

        let full_checkpoint = read_full_checkpoint(&d).await.unwrap();

        (committee, full_checkpoint)
    }

    #[tokio::test]
    async fn test_checkpoint_all_good() {
        let (committee, full_checkpoint) = read_data().await;

        assert_contains_transaction_effects(
            &full_checkpoint,
            &committee,
            TransactionDigest::from_str("8RiKBwuAbtu8zNCtz8SrcfHyEUzto6zi6cMVA9t4WhWk").unwrap(),
        )
        .unwrap();
    }

    #[tokio::test]
    async fn test_checkpoint_bad_committee() {
        let (mut committee, full_checkpoint) = read_data().await;

        // Change committee
        committee.epoch += 10;

        assert!(assert_contains_transaction_effects(
            &full_checkpoint,
            &committee,
            TransactionDigest::from_str("8RiKBwuAbtu8zNCtz8SrcfHyEUzto6zi6cMVA9t4WhWk").unwrap(),
        )
        .is_err());
    }

    #[tokio::test]
    async fn test_checkpoint_no_transaction() {
        let (committee, full_checkpoint) = read_data().await;

        assert!(assert_contains_transaction_effects(
            &full_checkpoint,
            &committee,
            TransactionDigest::from_str("8RiKBwuAbtu8zNCtz8SrcfHyEUzto6zj6cMVA9t4WhWk").unwrap(),
        )
        .is_err());
    }

    #[tokio::test]
    async fn test_checkpoint_bad_contents() {
        let (committee, mut full_checkpoint) = read_data().await;

        // Change contents
        let random_contents = FullCheckpointContents::random_for_testing();
        full_checkpoint.checkpoint_contents = random_contents.checkpoint_contents();

        assert!(assert_contains_transaction_effects(
            &full_checkpoint,
            &committee,
            TransactionDigest::from_str("8RiKBwuAbtu8zNCtz8SrcfHyEUzto6zj6cMVA9t4WhWk").unwrap(),
        )
        .is_err());
    }

    #[tokio::test]
    async fn test_checkpoint_bad_events() {
        let (committee, mut full_checkpoint) = read_data().await;

        let event = full_checkpoint.transactions[4]
            .events
            .as_ref()
            .unwrap()
            .data[0]
            .clone();

        for t in &mut full_checkpoint.transactions {
            if let Some(events) = &mut t.events {
                events.data.push(event.clone());
            }
        }

        assert!(assert_contains_transaction_effects(
            &full_checkpoint,
            &committee,
            TransactionDigest::from_str("8RiKBwuAbtu8zNCtz8SrcfHyEUzto6zj6cMVA9t4WhWk").unwrap(),
        )
        .is_err());
    }
}
