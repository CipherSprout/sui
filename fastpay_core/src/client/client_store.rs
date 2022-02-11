use super::*;
use fastx_types::object::Object;
use rocksdb::{DBWithThreadMode, MultiThreaded};
use std::path::PathBuf;
use std::sync::Arc;
use typed_store::rocks::DBMap;

const CERT_CF_NAME: &str = "certificates";
const SEQ_NUMBER_CF_NAME: &str = "object_sequence_numbers";
const OBJ_REF_CF_NAME: &str = "object_refs";
const TX_DIGEST_TO_CERT_CF_NAME: &str = "object_certs";
const PENDING_ORDERS_CF_NAME: &str = "pending_orders";
const OBJECT_CF_NAME: &str = "objects";

pub fn init_store(path: PathBuf, names: Vec<&str>) -> Arc<DBWithThreadMode<MultiThreaded>> {
    open_cf(&path, None, &names).expect("Cannot open DB.")
}

fn reopen_db<K, V>(db: &Arc<DBWithThreadMode<MultiThreaded>>, name: &str) -> DBMap<K, V> {
    DBMap::reopen(db, Some(name)).expect(&format!("Cannot open {} CF.", name)[..])
}

const MANAGED_ADDRESS_PATHS_CF_NAME: &str = "managed_address_paths";

// Structure
// AddressManagerStore1
//     |
//     |
//     ------ SingleAddressStore1
//     |
//     ------ SingleAddressStore1
//     |
//     ------ SingleAddressStore1
//     |
//     ------ SingleAddressStore1
pub struct ClientAddressManagerStore {
    // Address manager path
    pub path: PathBuf,
    // Table of managed addresses to their paths
    pub managed_address_paths: DBMap<FastPayAddress, PathBuf>,
}
impl ClientAddressManagerStore {
    /// Open a store for the manager
    pub fn open(path: PathBuf) -> Self {
        // Open column family
        let db = client_store::init_store(path.clone(), vec![MANAGED_ADDRESS_PATHS_CF_NAME]);
        ClientAddressManagerStore {
            path,
            managed_address_paths: client_store::reopen_db(&db, MANAGED_ADDRESS_PATHS_CF_NAME),
        }
    }

    /// Make a DB path for a given address
    fn make_db_path_for_address(&self, address: FastPayAddress) -> PathBuf {
        let mut hasher = sha3::Sha3_256::default();
        sha3::Digest::update(&mut hasher, address);
        let hash = sha3::Digest::finalize(hasher);
        let mut path = self.path.clone();
        path.push(PathBuf::from(format!("/addresses/{:02x}", hash)));
        path
    }

    /// Add an address to be managed
    /// Overwites existing if present
    pub fn manage_new_address(
        &self,
        address: FastPayAddress,
    ) -> Result<client_store::ClientSingleAddressStore, typed_store::rocks::TypedStoreError>
    {
        // Create an a path for this address
        let path = self.make_db_path_for_address(address);
        self.managed_address_paths.insert(&address, &path)?;
        Ok(ClientSingleAddressStore::new(path))
    }

    /// Gets managed address if any
    pub fn get_managed_address(
        &self,
        address: FastPayAddress,
    ) -> Result<client_store::ClientSingleAddressStore, typed_store::rocks::TypedStoreError>
    {
        // Create an a path for this address
        let path = self.make_db_path_for_address(address);
        self.managed_address_paths.get(&address)?;
        Ok(ClientSingleAddressStore::new(path))
    }

    /// Check if an address is managed
    pub fn is_managed_address(
        &self,
        address: FastPayAddress,
    ) -> Result<bool, typed_store::rocks::TypedStoreError> {
        self.managed_address_paths.contains_key(&address)
    }
}

pub struct ClientSingleAddressStore {
    // Table of objects to orders pending on the objects
    pub pending_orders: DBMap<ObjectID, Order>,
    // The remaining fields are used to minimize networking, and may not always be persisted locally.
    /// Known certificates, indexed by TX digest.
    pub certificates: DBMap<TransactionDigest, CertifiedOrder>,
    /// The known objects with it's sequence number owned by the client.
    pub object_sequence_numbers: DBMap<ObjectID, SequenceNumber>,
    /// Confirmed objects with it's ref owned by the client.
    pub object_refs: DBMap<ObjectID, ObjectRef>,
    /// Certificate <-> object id linking map.
    pub object_certs: DBMap<ObjectID, Vec<TransactionDigest>>,
    /// Map from object ref to actual object to track object history
    /// There can be duplicates and we never delete objects
    pub objects: DBMap<ObjectRef, Object>,
}

impl ClientSingleAddressStore {
    pub fn new(path: PathBuf) -> Self {
        // Open column families
        let db = client_store::init_store(
            path,
            vec![
                PENDING_ORDERS_CF_NAME,
                CERT_CF_NAME,
                SEQ_NUMBER_CF_NAME,
                OBJ_REF_CF_NAME,
                TX_DIGEST_TO_CERT_CF_NAME,
                OBJECT_CF_NAME,
            ],
        );

        ClientSingleAddressStore {
            pending_orders: client_store::reopen_db(&db, PENDING_ORDERS_CF_NAME),
            certificates: client_store::reopen_db(&db, CERT_CF_NAME),
            object_sequence_numbers: client_store::reopen_db(&db, SEQ_NUMBER_CF_NAME),
            object_refs: client_store::reopen_db(&db, OBJ_REF_CF_NAME),
            object_certs: client_store::reopen_db(&db, TX_DIGEST_TO_CERT_CF_NAME),
            objects: client_store::reopen_db(&db, OBJECT_CF_NAME),
        }
    }
}
