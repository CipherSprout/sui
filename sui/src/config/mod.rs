// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::keystore::KeystoreType;
use anyhow::bail;
use std::time::Duration;
use narwhal_config::{
    Authority, Committee as ConsensusCommittee, PrimaryAddresses, Stake, WorkerAddresses,
};
use narwhal_crypto::ed25519::Ed25519PublicKey;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::Value;
use serde_with::{hex::Hex, serde_as};
use std::{
    fmt::{Display, Formatter, Write},
    fs::{self, create_dir_all, File},
    io::{self, BufReader},
    net::{SocketAddr, ToSocketAddrs},
    ops::{Deref, DerefMut},
    path::{Path, PathBuf},
};
use sui_framework::DEFAULT_FRAMEWORK_PATH;
use sui_types::{
    base_types::*,
    committee::{Committee, EpochId},
    crypto::{get_key_pair, KeyPair, PublicKeyBytes},
};
use tracing::{info, log::trace};
use backoff::{retry, ExponentialBackoffBuilder, Error};

pub mod gateway;
pub mod utils;

pub use gateway::{GatewayConfig, GatewayType};

const SUI_DIR: &str = ".sui";
const SUI_CONFIG_DIR: &str = "sui_config";
pub const SUI_NETWORK_CONFIG: &str = "network.conf";
pub const SUI_WALLET_CONFIG: &str = "wallet.conf";
pub const SUI_GATEWAY_CONFIG: &str = "gateway.conf";
pub const SUI_DEV_NET_URL: &str = "https://gateway.devnet.sui.io:9000";

pub fn sui_config_dir() -> Result<PathBuf, anyhow::Error> {
    match std::env::var_os("SUI_CONFIG_DIR") {
        Some(config_env) => Ok(config_env.into()),
        None => match dirs::home_dir() {
            Some(v) => Ok(v.join(SUI_DIR).join(SUI_CONFIG_DIR)),
            None => bail!("Cannot obtain home directory path"),
        },
    }
    .and_then(|dir| {
        if !dir.exists() {
            create_dir_all(dir.clone())?;
        }
        Ok(dir)
    })
}

const DEFAULT_WEIGHT: usize = 1;
const DEFAULT_GAS_AMOUNT: u64 = 100000;
pub const AUTHORITIES_DB_NAME: &str = "authorities_db";
pub const DEFAULT_STARTING_PORT: u16 = 10000;
pub const CONSENSUS_DB_NAME: &str = "consensus_db";

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AuthorityInfo {
    #[serde(serialize_with = "bytes_as_hex", deserialize_with = "bytes_from_hex")]
    pub name: AuthorityName,
    pub host: String,
    pub base_port: u16,
}

#[derive(Serialize, Debug, Clone)]
pub struct AuthorityPrivateInfo {
    pub address: SuiAddress,
    pub public_key: PublicKeyBytes,
    pub host: String,
    pub port: u16,
    pub db_path: PathBuf,
    pub stake: usize,
    pub consensus_address: SocketAddr,
}

impl AuthorityPrivateInfo {
    pub fn copy(&self) -> Self {
        Self {
            address: self.address,
            host: self.host.clone(),
            port: self.port,
            db_path: self.db_path.clone(),
            stake: self.stake,
            consensus_address: self.consensus_address,
            public_key: self.public_key,
        }
    }
}
type AuthorityKeys = (Vec<PublicKeyBytes>, KeyPair);

// Warning: to_socket_addrs() is blocking and can fail.  Be careful where you use it.
fn socket_addr_from_hostport(host: &str, port: u16) -> SocketAddr {
    let mut addresses = format!("{host}:{port}")
        .to_socket_addrs()
        .unwrap_or_else(|e| panic!("Cannot parse or resolve hostnames for {host}:{port}: {e}"));
    addresses
        .next()
        .unwrap_or_else(|| panic!("Hostname/IP resolution failed for {host}"))
}

fn socket_addr_from_hostport_retry(host: &str, port: u16) -> SocketAddr {
    let back_off = ExponentialBackoffBuilder::new()
        .with_initial_interval(Duration::from_millis(500)) 
        .with_multiplier(2.0)
        .with_max_elapsed_time(Some(Duration::from_secs(30)))
        .build();
    let addr = retry(
        back_off,
        || {
        info!("Trying to resolve {host}:{port}");
        let mut addresses = format!("{host}:{port}")
        .to_socket_addrs()
        .map_err(Error::transient)?;
    addresses.next().ok_or(Error::transient(io::Error::new(io::ErrorKind::Other, "Can't find addr".to_string())))
    });
    addr.unwrap()
}

// Custom deserializer with optional default fields
impl<'de> Deserialize<'de> for AuthorityPrivateInfo {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::de::Deserializer<'de>,
    {
        let (_, new_key_pair) = get_key_pair();

        let json = Value::deserialize(deserializer)?;
        let public_key_bytes = if let Some(val) = json.get("public_key") {
            PublicKeyBytes::deserialize(val).map_err(serde::de::Error::custom)?
        } else {
            *new_key_pair.public_key_bytes()
        };
        let host = if let Some(val) = json.get("host") {
            String::deserialize(val).map_err(serde::de::Error::custom)?
        } else {
            "127.0.0.1".to_string()
        };
        let port = if let Some(val) = json.get("port") {
            u16::deserialize(val).map_err(serde::de::Error::custom)?
        } else {
            utils::get_available_port()
        };
        let db_path = if let Some(val) = json.get("db_path") {
            PathBuf::deserialize(val).map_err(serde::de::Error::custom)?
        } else {
            PathBuf::from(".")
                .join(AUTHORITIES_DB_NAME)
                .join(encode_bytes_hex(&public_key_bytes))
        };
        let stake = if let Some(val) = json.get("stake") {
            usize::deserialize(val).map_err(serde::de::Error::custom)?
        } else {
            DEFAULT_WEIGHT
        };
        let consensus_address = if let Some(val) = json.get("consensus_address") {
            SocketAddr::deserialize(val).map_err(serde::de::Error::custom)?
        } else {
            let port = utils::get_available_port();
            socket_addr_from_hostport("127.0.0.1", port)
        };

        Ok(AuthorityPrivateInfo {
            address: SuiAddress::from(&public_key_bytes),
            public_key: public_key_bytes,
            host,
            port,
            db_path,
            stake,
            consensus_address,
        })
    }
}

#[serde_as]
#[derive(Serialize, Deserialize)]
pub struct WalletConfig {
    #[serde_as(as = "Vec<Hex>")]
    pub accounts: Vec<SuiAddress>,
    pub keystore: KeystoreType,
    pub gateway: GatewayType,
    pub active_address: Option<SuiAddress>,
}

impl Config for WalletConfig {}

impl Display for WalletConfig {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let mut writer = String::new();

        writeln!(writer, "Managed addresses : {}", self.accounts.len())?;
        write!(writer, "Active address: ")?;
        match self.active_address {
            Some(r) => writeln!(writer, "{}", r)?,
            None => writeln!(writer, "None")?,
        };
        writeln!(writer, "{}", self.keystore)?;
        write!(writer, "{}", self.gateway)?;

        write!(f, "{}", writer)
    }
}

#[derive(Serialize, Deserialize)]
pub struct NetworkConfig {
    pub epoch: EpochId,
    pub authorities: Vec<AuthorityPrivateInfo>,
    pub buffer_size: usize,
    pub loaded_move_packages: Vec<(PathBuf, ObjectID)>,
    pub key_pair: KeyPair,
}

impl Config for NetworkConfig {}

impl NetworkConfig {
    pub fn get_authority_infos(&self) -> Vec<AuthorityInfo> {
        self.authorities
            .iter()
            .map(|info| AuthorityInfo {
                name: info.public_key,
                host: info.host.clone(),
                base_port: info.port,
            })
            .collect()
    }

    pub fn make_narwhal_committee(&self) -> ConsensusCommittee<Ed25519PublicKey> {
        ConsensusCommittee {
            authorities: self
                .authorities
                .iter()
                .map(|x| {
                    let name = x
                        .public_key
                        .make_narwhal_public_key()
                        .expect("Can't get narwhal public key");
                    info!("Resolving {}: {}:{}", name, &x.host, &x.port);
                    let primary = PrimaryAddresses {
                        primary_to_primary: socket_addr_from_hostport_retry(&x.host, x.port + 100),
                        worker_to_primary: socket_addr_from_hostport_retry(&x.host, x.port + 200),
                    };
                    info!("p2p: {}", primary.primary_to_primary);
                    info!("w2p: {}", primary.worker_to_primary);
                    
                    let p2w = socket_addr_from_hostport_retry(&x.host, x.port + 300);
                    let w2w = socket_addr_from_hostport_retry(&x.host, x.port + 400);

                    info!("p2w: {}", &p2w);
                    info!("txn: {}", &x.consensus_address);
                    info!("w2w: {}", &w2w);
                    let workers = [(
                        /* worker_id */ 0,
                        WorkerAddresses {
                            // primary_to_worker: socket_addr_from_hostport(&x.host, x.port + 300),
                            primary_to_worker: p2w,
                            transactions: x.consensus_address,
                            // worker_to_worker: socket_addr_from_hostport(&x.host, x.port + 400),
                            worker_to_worker: w2w,
                        },
                    )]
                    .iter()
                    .cloned()
                    .collect();
                    let authority = Authority {
                        stake: x.stake as Stake,
                        primary,
                        workers,
                    };
                    (name, authority)
                })
                .collect(),
        }
    }
}

impl From<&NetworkConfig> for Committee {
    fn from(network_config: &NetworkConfig) -> Committee {
        let voting_rights = network_config
            .authorities
            .iter()
            .map(|authority| (authority.public_key, authority.stake))
            .collect();
        Committee::new(network_config.epoch, voting_rights)
    }
}

#[derive(Serialize, Deserialize)]
#[serde(default)]
pub struct GenesisConfig {
    pub authorities: Vec<AuthorityPrivateInfo>,
    pub accounts: Vec<AccountConfig>,
    pub move_packages: Vec<PathBuf>,
    pub sui_framework_lib_path: PathBuf,
    pub move_framework_lib_path: PathBuf,
    pub key_pair: KeyPair,
}

impl Config for GenesisConfig {}

#[derive(Serialize, Deserialize, Default, Debug, Clone)]
#[serde(default)]
pub struct AccountConfig {
    #[serde(
        skip_serializing_if = "Option::is_none",
        serialize_with = "SuiAddress::optional_address_as_hex",
        deserialize_with = "SuiAddress::optional_address_from_hex"
    )]
    pub address: Option<SuiAddress>,
    pub gas_objects: Vec<ObjectConfig>,
    pub gas_object_ranges: Option<Vec<ObjectConfigRange>>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ObjectConfigRange {
    /// Starting object id
    pub offset: ObjectID,
    /// Number of object ids
    pub count: u64,
    /// Gas value per object id
    pub gas_value: u64,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ObjectConfig {
    #[serde(default = "ObjectID::random")]
    pub object_id: ObjectID,
    #[serde(default = "default_gas_value")]
    pub gas_value: u64,
}

fn default_gas_value() -> u64 {
    DEFAULT_GAS_AMOUNT
}

const DEFAULT_NUMBER_OF_AUTHORITIES: usize = 4;
const DEFAULT_NUMBER_OF_ACCOUNT: usize = 5;
const DEFAULT_NUMBER_OF_OBJECT_PER_ACCOUNT: usize = 5;

impl GenesisConfig {
    pub fn default_genesis(
        working_dir: &Path,
        authority_keys: Option<AuthorityKeys>,
    ) -> Result<Self, anyhow::Error> {
        let num_authorities = match &authority_keys {
            Some((public_keys, _)) => public_keys.len(),
            None => DEFAULT_NUMBER_OF_AUTHORITIES,
        };

        GenesisConfig::custom_genesis(
            working_dir,
            num_authorities,
            DEFAULT_NUMBER_OF_ACCOUNT,
            DEFAULT_NUMBER_OF_OBJECT_PER_ACCOUNT,
            authority_keys,
        )
    }

    pub fn custom_genesis(
        working_dir: &Path,
        num_authorities: usize,
        num_accounts: usize,
        num_objects_per_account: usize,
        authority_keys: Option<AuthorityKeys>,
    ) -> Result<Self, anyhow::Error> {
        assert!(
            num_authorities > 0,
            "num_authorities should be larger than 0"
        );
        let mut authorities = Vec::with_capacity(num_authorities);
        for _ in 0..num_authorities {
            // Get default authority config from deserialization logic.
            let mut authority = AuthorityPrivateInfo::deserialize(Value::String(String::new()))?;
            authority.db_path = working_dir
                .join(AUTHORITIES_DB_NAME)
                .join(encode_bytes_hex(&authority.public_key));
            authorities.push(authority)
        }
        let authority_key_pair;
        if let Some((public_keys, keypair)) = authority_keys {
            // Use key pairs if given
            assert_eq!(
                public_keys.len(),
                num_authorities,
                "Number of key pairs does not maych num_authorities"
            );
            public_keys
                .iter()
                .find(|pk| pk == &keypair.public_key_bytes())
                .expect("Keypair should be part of thte committee");
            authority_key_pair = keypair;
            for i in 0..num_authorities {
                authorities[i].public_key = public_keys[i];
                authorities[i].address = SuiAddress::from(&public_keys[i]);
            }
        } else {
            let (address, key_pair) = get_key_pair();
            // If authorities is not empty, we override the first one
            if !authorities.is_empty() {
                authorities[0].address = address;
                authorities[0].public_key = *key_pair.public_key_bytes();
            }
            authority_key_pair = key_pair;
        }
        let mut accounts = Vec::new();
        for _ in 0..num_accounts {
            let mut objects = Vec::new();
            for _ in 0..num_objects_per_account {
                objects.push(ObjectConfig {
                    object_id: ObjectID::random(),
                    gas_value: DEFAULT_GAS_AMOUNT,
                })
            }
            accounts.push(AccountConfig {
                address: None,
                gas_objects: objects,
                gas_object_ranges: Some(Vec::new()),
            })
        }

        Ok(Self {
            authorities,
            accounts,
            key_pair: authority_key_pair,
            ..Default::default()
        })
    }
}

impl Default for GenesisConfig {
    fn default() -> Self {
        Self {
            authorities: vec![],
            accounts: vec![],
            move_packages: vec![],
            sui_framework_lib_path: PathBuf::from(DEFAULT_FRAMEWORK_PATH),
            move_framework_lib_path: PathBuf::from(DEFAULT_FRAMEWORK_PATH)
                .join("deps")
                .join("move-stdlib"),
            key_pair: get_key_pair().1,
        }
    }
}

pub trait Config
where
    Self: DeserializeOwned + Serialize,
{
    fn persisted(self, path: &Path) -> PersistedConfig<Self> {
        PersistedConfig {
            inner: self,
            path: path.to_path_buf(),
        }
    }
}

pub struct PersistedConfig<C> {
    inner: C,
    path: PathBuf,
}

impl<C> PersistedConfig<C>
where
    C: Config,
{
    pub fn read(path: &Path) -> Result<C, anyhow::Error> {
        trace!("Reading config from '{:?}'", path);
        let reader = BufReader::new(File::open(path)?);
        Ok(serde_json::from_reader(reader)?)
    }

    pub fn save(&self) -> Result<(), anyhow::Error> {
        trace!("Writing config to '{:?}'", &self.path);
        let config = serde_json::to_string_pretty(&self.inner)?;
        fs::write(&self.path, config)?;
        Ok(())
    }
}

impl<C> Deref for PersistedConfig<C> {
    type Target = C;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<C> DerefMut for PersistedConfig<C> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

/// Make a default Narwhal-compatible committee.
pub fn make_default_narwhal_committee(
    authorities: &[AuthorityPrivateInfo],
) -> Result<ConsensusCommittee<Ed25519PublicKey>, anyhow::Error> {
    let mut ports = Vec::new();
    for _ in authorities {
        let authority_ports = [
            utils::get_available_port(),
            utils::get_available_port(),
            utils::get_available_port(),
            utils::get_available_port(),
        ];
        ports.push(authority_ports);
    }

    Ok(ConsensusCommittee {
        authorities: authorities
            .iter()
            .enumerate()
            .map(|(i, x)| {
                let name = x
                    .public_key
                    .make_narwhal_public_key()
                    .expect("Can't get narwhal public key");

                let primary = PrimaryAddresses {
                    primary_to_primary: socket_addr_from_hostport("127.0.0.1", ports[i][0]),
                    worker_to_primary: socket_addr_from_hostport("127.0.0.1", ports[i][1]),
                };
                let workers = [(
                    /* worker_id */ 0,
                    WorkerAddresses {
                        primary_to_worker: socket_addr_from_hostport("127.0.0.1", ports[i][2]),
                        transactions: x.consensus_address,
                        worker_to_worker: socket_addr_from_hostport("127.0.0.1", ports[i][3]),
                    },
                )]
                .iter()
                .cloned()
                .collect();

                let authority = Authority {
                    stake: x.stake as Stake,
                    primary,
                    workers,
                };
                (name, authority)
            })
            .collect(),
    })
}
