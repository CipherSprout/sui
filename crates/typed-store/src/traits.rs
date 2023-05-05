// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::TypedStoreError;
use async_trait::async_trait;
use rocksdb::ReadOptions;
use serde::{de::DeserializeOwned, Serialize};
use std::{borrow::Borrow, collections::BTreeMap, error::Error};

pub trait Map<'a, K, V>
where
    K: Serialize + DeserializeOwned + ?Sized,
    V: Serialize + DeserializeOwned,
{
    type Error: Error;
    type Iterator: Iterator<Item = (K, V)>;
    type SafeIterator: Iterator<Item = Result<(K, V), TypedStoreError>>;
    type Keys: Iterator<Item = Result<K, TypedStoreError>>;
    type Values: Iterator<Item = Result<V, TypedStoreError>>;

    /// Returns true if the map contains a value for the specified key.
    fn contains_key(&self, key: &K) -> Result<bool, Self::Error>;

    /// Returns the value for the given key from the map, if it exists.
    fn get(&self, key: &K) -> Result<Option<V>, Self::Error>;

    fn get_with_opts(&self, key: &K, readopts: ReadOptions) -> Result<Option<V>, Self::Error>;

    /// Returns the raw value (serialized bytes) for the given key from the map, if it exists.
    fn get_raw_bytes(&self, key: &K) -> Result<Option<Vec<u8>>, Self::Error>;

    /// Returns the value for the given key from the map, if it exists
    /// or the given default value if it does not.
    /// This method is not thread safe
    fn get_or_insert_unsafe<F: FnOnce() -> V>(
        &self,
        key: &K,
        default: F,
    ) -> Result<V, Self::Error> {
        self.get(key).and_then(|optv| match optv {
            Some(v) => Ok(v),
            None => {
                self.insert(key, &default())?;
                self.get(key).transpose().expect("default just inserted")
            }
        })
    }

    /// Inserts the given key-value pair into the map.
    fn insert(&self, key: &K, value: &V) -> Result<(), Self::Error>;

    /// Removes the entry for the given key from the map.
    fn remove(&self, key: &K) -> Result<(), Self::Error>;

    /// Removes every key-value pair from the map.
    fn clear(&self) -> Result<(), Self::Error>;

    /// Returns true if the map is empty, otherwise false.
    fn is_empty(&self) -> bool;

    /// Returns an iterator visiting each key-value pair in the map.
    fn iter(&'a self) -> Self::Iterator;

    /// Returns an iterator visiting each key-value pair in the map.
    fn iter_with_opts(&'a self, readopts: ReadOptions) -> Self::Iterator;

    /// Returns an iterator visiting each key-value pair in the map.
    fn iter_with_bounds(&'a self, lower_bound: Option<K>, upper_bound: Option<K>)
        -> Self::Iterator;

    /// Same as `iter` but performs status check
    fn safe_iter(&'a self) -> Self::SafeIterator;

    /// Returns an iterator over each key in the map.
    fn keys(&'a self) -> Self::Keys;

    /// Returns an iterator over each value in the map.
    fn values(&'a self) -> Self::Values;

    /// Returns a vector of values corresponding to the keys provided, non-atomically.
    fn multi_get<J>(&self, keys: impl IntoIterator<Item = J>) -> Result<Vec<Option<V>>, Self::Error>
    where
        J: Borrow<K>,
    {
        keys.into_iter().map(|key| self.get(key.borrow())).collect()
    }

    /// Returns a vector of values corresponding to the keys provided, non-atomically.
    fn multi_get_with_opts<J>(
        &self,
        keys: impl IntoIterator<Item = J>,
        _readopts: ReadOptions,
    ) -> Result<Vec<Option<V>>, Self::Error>
    where
        J: Borrow<K>,
    {
        self.multi_get(keys)
    }

    /// Returns a vector of raw values corresponding to the keys provided, non-atomically.
    fn multi_get_raw_bytes_with_opts<J>(
        &self,
        keys: impl IntoIterator<Item = J>,
        _readopts: ReadOptions,
    ) -> Result<Vec<Option<Vec<u8>>>, Self::Error>
    where
        J: Borrow<K>,
    {
        keys.into_iter()
            .map(|key| self.get_raw_bytes(key.borrow()))
            .collect()
    }

    /// Returns a vector of raw values corresponding to the keys provided, non-atomically.
    fn multi_get_raw_bytes<J>(
        &self,
        keys: impl IntoIterator<Item = J>,
    ) -> Result<Vec<Option<Vec<u8>>>, Self::Error>
    where
        J: Borrow<K>,
    {
        self.multi_get_raw_bytes_with_opts(keys, ReadOptions::default())
    }

    /// Returns a vector of values corresponding to the keys provided, non-atomically.
    fn chunked_multi_get<J>(
        &self,
        keys: impl IntoIterator<Item = J>,
        _chunk_size: usize,
    ) -> Result<Vec<Option<V>>, Self::Error>
    where
        J: Borrow<K>,
    {
        keys.into_iter().map(|key| self.get(key.borrow())).collect()
    }

    /// Inserts key-value pairs, non-atomically.
    fn multi_insert<J, U>(
        &self,
        key_val_pairs: impl IntoIterator<Item = (J, U)>,
    ) -> Result<(), Self::Error>
    where
        J: Borrow<K>,
        U: Borrow<V>,
    {
        key_val_pairs
            .into_iter()
            .try_for_each(|(key, value)| self.insert(key.borrow(), value.borrow()))
    }

    /// Removes keys, non-atomically.
    fn multi_remove<J>(&self, keys: impl IntoIterator<Item = J>) -> Result<(), Self::Error>
    where
        J: Borrow<K>,
    {
        keys.into_iter()
            .try_for_each(|key| self.remove(key.borrow()))
    }

    /// Try to catch up with primary when running as secondary
    fn try_catch_up_with_primary(&self) -> Result<(), Self::Error>;
}

#[async_trait]
pub trait AsyncMap<'a, K, V>
where
    K: Serialize + DeserializeOwned + ?Sized + std::marker::Sync,
    V: Serialize + DeserializeOwned + std::marker::Sync + std::marker::Send,
{
    type Error: Error;
    type Iterator: Iterator<Item = Result<(K, V), TypedStoreError>>;
    type Keys: Iterator<Item = Result<K, TypedStoreError>>;
    type Values: Iterator<Item = Result<V, TypedStoreError>>;

    /// Returns true if the map contains a value for the specified key.
    async fn contains_key(&self, key: &K) -> Result<bool, Self::Error>;

    /// Returns the value for the given key from the map, if it exists.
    async fn get(&self, key: &K) -> Result<Option<V>, Self::Error>;

    /// Returns the raw value (serialized bytes) for the given key from the map, if it exists.
    async fn get_raw_bytes(&self, key: &K) -> Result<Option<Vec<u8>>, Self::Error>;

    /// Returns true if the map is empty, otherwise false.
    async fn is_empty(&self) -> bool;

    /// Returns an iterator visiting each key-value pair in the map.
    async fn iter(&'a self) -> Self::Iterator;

    /// Returns an iterator over each key in the map.
    async fn keys(&'a self) -> Self::Keys;

    /// Returns an iterator over each value in the map.
    async fn values(&'a self) -> Self::Values;

    /// Returns a vector of values corresponding to the keys provided, non-atomically.
    async fn multi_get<J>(
        &self,
        keys: impl IntoIterator<Item = J> + std::marker::Send,
    ) -> Result<Vec<Option<V>>, Self::Error>
    where
        J: Borrow<K>;

    /// Try to catch up with primary when running as secondary
    async fn try_catch_up_with_primary(&self) -> Result<(), Self::Error>;
}

pub struct TableSummary {
    pub num_keys: u64,
    pub key_bytes_total: usize,
    pub value_bytes_total: usize,
    pub key_hist: hdrhistogram::Histogram<u64>,
    pub value_hist: hdrhistogram::Histogram<u64>,
}

pub trait TypedStoreDebug {
    /// Dump a DB table with pagination
    fn dump_table(
        &self,
        table_name: String,
        page_size: u16,
        page_number: usize,
    ) -> eyre::Result<BTreeMap<String, String>>;

    /// Get the name of the DB. This is simply the name of the struct
    fn primary_db_name(&self) -> String;

    /// Get a map of table names to key-value types
    fn describe_all_tables(&self) -> BTreeMap<String, (String, String)>;

    /// Count the entries in the table
    fn count_table_keys(&self, table_name: String) -> eyre::Result<usize>;

    /// Return table summary of the input table
    fn table_summary(&self, table_name: String) -> eyre::Result<TableSummary>;
}
