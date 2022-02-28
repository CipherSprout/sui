// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::config::{AccountInfo, Config, WalletConfig};
use crate::sui_json::{resolve_move_function_args, SuiJsonValue};
use core::fmt;
use std::fmt::{Debug, Display, Formatter};
use sui_core::authority_client::AuthorityClient;
use sui_core::client::{Client, ClientAddressManager, ClientState};
use sui_types::base_types::{decode_bytes_hex, ObjectID, ObjectRef, SuiAddress};
use sui_types::crypto::get_key_pair;
use sui_types::gas_coin::GasCoin;
use sui_types::messages::{CertifiedTransaction, ExecutionStatus, TransactionEffects};
use sui_types::move_package::resolve_and_type_check;
use sui_types::object::ObjectRead::Exists;

use anyhow::anyhow;
use colored::Colorize;
use move_core_types::identifier::Identifier;
use move_core_types::language_storage::TypeTag;
use move_core_types::parser::parse_type_tag;
use serde::ser::Error;
use serde::Serialize;
use std::fmt::Write;
use std::time::Instant;
use structopt::clap::AppSettings;
use structopt::StructOpt;
use sui_types::error::SuiError;
use sui_types::object::ObjectRead;
use tracing::info;

#[derive(StructOpt)]
#[structopt(name = "", rename_all = "kebab-case")]
#[structopt(setting(AppSettings::NoBinaryName))]
pub struct WalletOpts {
    #[structopt(subcommand)]
    pub command: WalletCommands,
    /// Return command outputs in json format.
    #[structopt(long, global = true)]
    pub json: bool,
}

#[derive(StructOpt)]
#[structopt(rename_all = "kebab-case")]
#[structopt(setting(AppSettings::NoBinaryName))]
pub enum WalletCommands {
    /// Get obj info
    #[structopt(name = "object")]
    Object {
        /// Object ID of the object to fetch
        #[structopt(long)]
        id: ObjectID,
    },

    /// Publish Move modules
    #[structopt(name = "publish")]
    Publish {
        /// Path to directory containing a Move package
        #[structopt(long)]
        path: String,

        /// ID of the gas object for gas payment, in 20 bytes Hex string
        #[structopt(long)]
        gas: ObjectID,

        /// gas budget for running module initializers
        #[structopt(default_value = "0")]
        gas_budget: u64,
    },

    /// Call Move function
    #[structopt(name = "call")]
    Call {
        /// Object ID of the package, which contains the module
        #[structopt(long)]
        package: ObjectID,
        /// The name of the module in the package
        #[structopt(long)]
        module: Identifier,
        /// Function name in module
        #[structopt(long)]
        function: Identifier,
        /// Function name in module
        #[structopt(long, parse(try_from_str = parse_type_tag))]
        type_args: Vec<TypeTag>,
        /// Simplified ordered args like in the function syntax
        /// ObjectIDs, Addresses must be hex strings
        #[structopt(long)]
        args: Vec<SuiJsonValue>,
        /// ID of the gas object for gas payment, in 20 bytes Hex string
        #[structopt(long)]
        gas: ObjectID,
        /// Gas budget for this call
        #[structopt(long)]
        gas_budget: u64,
    },

    /// Transfer an object
    #[structopt(name = "transfer")]
    Transfer {
        /// Recipient address
        #[structopt(long, parse(try_from_str = decode_bytes_hex))]
        to: SuiAddress,

        /// Object to transfer, in 20 bytes Hex string
        #[structopt(long)]
        object_id: ObjectID,

        /// ID of the gas object for gas payment, in 20 bytes Hex string
        #[structopt(long)]
        gas: ObjectID,
    },
    /// Synchronize client state with authorities.
    #[structopt(name = "sync")]
    SyncClientState {
        #[structopt(long, parse(try_from_str = decode_bytes_hex))]
        address: SuiAddress,
    },

    /// Obtain the Addresses managed by the wallet.
    #[structopt(name = "addresses")]
    Addresses,

    /// Generate new address and keypair.
    #[structopt(name = "new-address")]
    NewAddress,

    /// Obtain all objects owned by the address.
    #[structopt(name = "objects")]
    Objects {
        /// Address owning the objects
        #[structopt(long, parse(try_from_str = decode_bytes_hex))]
        address: SuiAddress,
    },

    /// Obtain all gas objects owned by the address.
    #[structopt(name = "gas")]
    Gas {
        /// Address owning the objects
        #[structopt(long, parse(try_from_str = decode_bytes_hex))]
        address: SuiAddress,
    },
}

impl WalletCommands {
    pub async fn execute(
        &mut self,
        context: &mut WalletContext,
    ) -> Result<WalletCommandResult, anyhow::Error> {
        Ok(match self {
            WalletCommands::Publish {
                path,
                gas,
                gas_budget,
            } => {
                // Find owner of gas object
                let sender = &context
                    .address_manager
                    .get_object_owner(*gas)
                    .await?
                    .get_single_owner_address()?;
                let client_state = context.get_or_create_client_state(sender)?;
                let gas_obj_ref = client_state.latest_object_ref(gas)?;

                let (cert, effects) = client_state
                    .publish(path.clone(), gas_obj_ref, *gas_budget)
                    .await?;

                if matches!(effects.status, ExecutionStatus::Failure { .. }) {
                    return Err(anyhow!("Error publishing module: {:#?}", effects.status));
                };
                WalletCommandResult::Publish(cert, effects)
            }

            WalletCommands::Object { id } => {
                // Fetch the object ref
                let object_read = context.address_manager.get_object_info(*id).await?;
                WalletCommandResult::Object(object_read)
            }
            WalletCommands::Call {
                package,
                module,
                function,
                type_args,
                gas,
                gas_budget,
                args,
            } => {
                let sender = &context
                    .address_manager
                    .get_object_owner(*gas)
                    .await?
                    .get_single_owner_address()?;
                let client_state = context.get_or_create_client_state(sender)?;

                let package_obj_info = client_state.get_object_info(*package).await?;
                let package_obj = package_obj_info.object().clone()?;
                let package_obj_ref = package_obj_info.reference().unwrap();

                // These steps can potentially be condensed and moved into the client/manager level
                // Extract the input args
                let (object_ids, pure_args) = resolve_move_function_args(
                    package_obj,
                    module.clone(),
                    function.clone(),
                    args.clone(),
                )?;

                // Fetch all the objects needed for this call
                let mut input_objs = vec![];
                for obj_id in object_ids.clone() {
                    input_objs.push(client_state.get_object_info(obj_id).await?.into_object()?);
                }

                // Pass in the objects for a deeper check
                // We can technically move this to impl MovePackage
                resolve_and_type_check(
                    package_obj.clone(),
                    module,
                    function,
                    type_args,
                    input_objs,
                    pure_args.clone(),
                )?;

                // Fetch the object info for the gas obj
                let gas_obj_ref = client_state.latest_object_ref(gas)?;

                // Fetch the objects for the object args
                let mut object_args_refs = Vec::new();
                for obj_id in object_ids {
                    let obj_info = client_state.get_object_info(obj_id).await?;
                    object_args_refs.push(obj_info.object()?.to_object_reference());
                }

                let (cert, effects) = client_state
                    .move_call(
                        package_obj_ref,
                        module.to_owned(),
                        function.to_owned(),
                        type_args.clone(),
                        gas_obj_ref,
                        object_args_refs,
                        vec![],
                        pure_args,
                        *gas_budget,
                    )
                    .await?;
                if matches!(effects.status, ExecutionStatus::Failure { .. }) {
                    return Err(anyhow!("Error calling module: {:#?}", effects.status));
                }
                WalletCommandResult::Call(cert, effects)
            }

            WalletCommands::Transfer { to, object_id, gas } => {
                let from = &context
                    .address_manager
                    .get_object_owner(*gas)
                    .await?
                    .get_single_owner_address()?;
                let client_state = context.get_or_create_client_state(from)?;
                let time_start = Instant::now();
                let (cert, effects) = client_state.transfer_object(*object_id, *gas, *to).await?;
                let time_total = time_start.elapsed().as_micros();

                if matches!(effects.status, ExecutionStatus::Failure { .. }) {
                    return Err(anyhow!("Error transferring object: {:#?}", effects.status));
                }
                WalletCommandResult::Transfer(time_total, cert, effects)
            }

            WalletCommands::Addresses => WalletCommandResult::Addresses(
                context
                    .address_manager
                    .get_managed_address_states()
                    .keys()
                    .copied()
                    .collect(),
            ),

            WalletCommands::Objects { address } => {
                let client_state = context.get_or_create_client_state(address)?;
                WalletCommandResult::Objects(
                    client_state
                        .object_refs()
                        .map(|(_, object_ref)| object_ref)
                        .collect(),
                )
            }

            WalletCommands::SyncClientState { address } => {
                let client_state = context.get_or_create_client_state(address)?;
                client_state.sync_client_state().await?;
                WalletCommandResult::SyncClientState
            }
            WalletCommands::NewAddress => {
                let (address, key) = get_key_pair();
                context.config.accounts.push(AccountInfo {
                    address,
                    key_pair: key,
                });
                context.config.save()?;
                // Create an address to be managed
                context.get_or_create_client_state(&address)?;
                WalletCommandResult::NewAddress(address)
            }
            WalletCommands::Gas { address } => {
                let client_state = context.get_or_create_client_state(address)?;
                client_state.sync_client_state().await?;
                let object_ids = client_state.get_owned_objects();

                // TODO: We should ideally fetch the objects from local cache
                let mut coins = Vec::new();
                for obj in object_ids {
                    match context.address_manager.get_object_info(obj).await? {
                        Exists(_, o, _) => {
                            if matches!( o.type_(), Some(v)  if *v == GasCoin::type_()) {
                                // Okay to unwrap() since we already checked type
                                let gas_coin = GasCoin::try_from(o.data.try_as_move().unwrap())?;
                                coins.push(gas_coin);
                            }
                        }
                        _ => continue,
                    }
                }
                WalletCommandResult::Gas(coins)
            }
        })
    }
}

pub struct WalletContext {
    pub config: WalletConfig,
    pub address_manager: ClientAddressManager<AuthorityClient>,
}

impl WalletContext {
    pub fn new(config: WalletConfig) -> Result<Self, anyhow::Error> {
        let path = config.db_folder_path.clone();
        let addresses = config
            .accounts
            .iter()
            .map(|info| info.address)
            .collect::<Vec<_>>();

        let committee = config.make_committee();
        let authority_clients = config.make_authority_clients();
        let mut context = Self {
            config,
            address_manager: ClientAddressManager::new(path, committee, authority_clients),
        };
        // Pre-populate client state for each address in the config.
        for address in addresses {
            context.get_or_create_client_state(&address)?;
        }
        Ok(context)
    }

    pub fn get_or_create_client_state(
        &mut self,
        owner: &SuiAddress,
    ) -> Result<&mut ClientState<AuthorityClient>, SuiError> {
        let kp = Box::pin(self.config.get_account_cfg_info(owner)?.key_pair.copy());
        self.address_manager.get_or_create_state_mut(*owner, kp)
    }
}

impl Display for WalletCommandResult {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let mut writer = String::new();
        match self {
            WalletCommandResult::Publish(cert, effects) => {
                writeln!(writer, "{}", write_cert_and_effects(cert, effects)?)?;
            }
            WalletCommandResult::Object(object_read) => {
                let object = object_read.object().map_err(fmt::Error::custom)?;
                writeln!(writer, "{}", object)?;
            }
            WalletCommandResult::Call(cert, effects) => {
                writeln!(writer, "{}", write_cert_and_effects(cert, effects)?)?;
            }
            WalletCommandResult::Transfer(time_elapsed, cert, effects) => {
                writeln!(writer, "Transfer confirmed after {} us", time_elapsed)?;
                writeln!(writer, "{}", write_cert_and_effects(cert, effects)?)?;
            }
            WalletCommandResult::Addresses(addresses) => {
                writeln!(writer, "Showing {} results.", addresses.len())?;
                for address in addresses {
                    writeln!(writer, "{}", address)?;
                }
            }
            WalletCommandResult::Objects(object_refs) => {
                writeln!(writer, "Showing {} results.", object_refs.len())?;
                for object_ref in object_refs {
                    writeln!(writer, "{:?}", object_ref)?;
                }
            }
            WalletCommandResult::SyncClientState => {
                writeln!(writer, "Client state sync complete.")?;
            }
            WalletCommandResult::NewAddress(address) => {
                writeln!(writer, "Created new keypair for address : {}", &address)?;
            }
            WalletCommandResult::Gas(gases) => {
                // TODO: generalize formatting of CLI
                writeln!(
                    writer,
                    " {0: ^40} | {1: ^10} | {2: ^11}",
                    "Object ID", "Version", "Gas Value"
                )?;
                writeln!(
                    writer,
                    "----------------------------------------------------------------------"
                )?;
                for gas in gases {
                    writeln!(
                        writer,
                        " {0: ^40} | {1: ^10} | {2: ^11}",
                        gas.id(),
                        u64::from(gas.version()),
                        gas.value()
                    )?;
                }
            }
        }
        write!(f, "{}", writer)
    }
}

fn write_cert_and_effects(
    cert: &CertifiedTransaction,
    effects: &TransactionEffects,
) -> Result<String, fmt::Error> {
    let mut writer = String::new();
    writeln!(writer, "{}", "----- Certificate ----".bold())?;
    writeln!(writer, "{}", cert)?;
    writeln!(writer, "{}", "----- Transaction Effects ----".bold())?;
    writeln!(writer, "{}", effects)?;
    Ok(writer)
}

impl Debug for WalletCommandResult {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            WalletCommandResult::Object(object_read) => {
                let object = object_read.object().map_err(fmt::Error::custom)?;
                let layout = object_read.layout().map_err(fmt::Error::custom)?;
                object
                    .to_json(layout)
                    .map_err(fmt::Error::custom)?
                    .to_string()
            }
            _ => serde_json::to_string(self).map_err(fmt::Error::custom)?,
        };
        write!(f, "{}", s)
    }
}

impl WalletCommandResult {
    pub fn print(&self, pretty: bool) {
        let line = if pretty {
            format!("{}", self)
        } else {
            format!("{:?}", self)
        };
        // Log line by line
        for line in line.lines() {
            info!("{}", line)
        }
    }
}

#[derive(Serialize)]
#[serde(untagged)]
pub enum WalletCommandResult {
    Publish(CertifiedTransaction, TransactionEffects),
    Object(ObjectRead),
    Call(CertifiedTransaction, TransactionEffects),
    Transfer(
        // Skipping serialisation for elapsed time.
        #[serde(skip)] u128,
        CertifiedTransaction,
        TransactionEffects,
    ),
    Addresses(Vec<SuiAddress>),
    Objects(Vec<ObjectRef>),
    SyncClientState,
    NewAddress(SuiAddress),
    Gas(Vec<GasCoin>),
}
