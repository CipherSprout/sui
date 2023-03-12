// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::ops::Neg;
use std::sync::Arc;

use async_trait::async_trait;
use move_core_types::language_storage::TypeTag;
use tokio::sync::RwLock;

use sui_core::authority::AuthorityState;
use sui_json_rpc_types::{BalanceChange, BalanceChangeType};
use sui_types::base_types::{MoveObjectType, ObjectID, ObjectRef, SequenceNumber};
use sui_types::coin::Coin;
use sui_types::error::SuiError;
use sui_types::gas::GasCostSummary;
use sui_types::gas_coin::GAS;
use sui_types::messages::TransactionEffectsAPI;
use sui_types::messages::{ExecutionStatus, TransactionEffects};
use sui_types::object::{Object, Owner};
use sui_types::storage::WriteKind;

pub async fn get_balance_change_from_effect<P: ObjectProvider<Error = E>, E>(
    object_provider: &P,
    effects: &TransactionEffects,
) -> Result<Vec<BalanceChange>, E> {
    let (_, gas_owner) = effects.gas_object();

    // Only charge gas when tx fails, skip all object parsing
    if effects.status() != &ExecutionStatus::Success {
        return Ok(vec![BalanceChange {
            owner: *gas_owner,
            change_type: BalanceChangeType::Gas,
            coin_type: GAS::type_tag(),
            amount: effects.gas_cost_summary().net_gas_usage().neg() as i128,
        }]);
    }

    let all_mutated: Vec<(&ObjectRef, &Owner, WriteKind)> = effects.all_mutated();
    let all_mutated = all_mutated
        .iter()
        .map(|((id, version, _), _, _)| (*id, *version))
        .collect::<Vec<_>>();

    get_balance_change(
        object_provider,
        *gas_owner,
        effects.gas_cost_summary(),
        effects.modified_at_versions(),
        &all_mutated,
    )
    .await
}

pub async fn get_balance_change<P: ObjectProvider<Error = E>, E>(
    object_provider: &P,
    gas_owner: Owner,
    gas_cost_summary: &GasCostSummary,
    modified_at_version: &[(ObjectID, SequenceNumber)],
    all_mutated: &[(ObjectID, SequenceNumber)],
) -> Result<Vec<BalanceChange>, E> {
    // 1. subtract all input coins
    let balances = fetch_coins(object_provider, modified_at_version)
        .await?
        .into_iter()
        .fold(
            BTreeMap::<_, i128>::new(),
            |mut acc, (owner, type_, amount)| {
                *acc.entry((owner, type_)).or_default() -= amount as i128;
                acc
            },
        );
    // 2. add all mutated coins
    let mut balances = fetch_coins(object_provider, all_mutated)
        .await?
        .into_iter()
        .fold(balances, |mut acc, (owner, type_, amount)| {
            *acc.entry((owner, type_)).or_default() += amount as i128;
            acc
        });
    // 3. add back gas cost (gas are accounted separately)
    *balances.entry((gas_owner, GAS::type_tag())).or_default() +=
        gas_cost_summary.net_gas_usage() as i128;

    let mut gas = vec![BalanceChange {
        owner: gas_owner,
        change_type: BalanceChangeType::Gas,
        coin_type: GAS::type_tag(),
        amount: gas_cost_summary.net_gas_usage().neg() as i128,
    }];

    let balance_changes = balances
        .into_iter()
        .filter_map(|((owner, coin_type), amount)| {
            if amount == 0 {
                return None;
            }
            let change_type = if amount.is_negative() {
                BalanceChangeType::Pay
            } else {
                BalanceChangeType::Receive
            };
            Some(BalanceChange {
                owner,
                change_type,
                coin_type,
                amount,
            })
        })
        .collect::<Vec<_>>();
    gas.extend(balance_changes);
    Ok(gas)
}

async fn fetch_coins<P: ObjectProvider<Error = E>, E>(
    object_provider: &P,
    objects: &[(ObjectID, SequenceNumber)],
) -> Result<Vec<(Owner, TypeTag, u64)>, E> {
    let mut all_mutated_coins = vec![];
    for (id, version) in objects {
        if let Ok(o) = object_provider.get_object(id, version).await {
            if let Some(type_) = o.type_() {
                match type_ {
                    MoveObjectType::GasCoin => all_mutated_coins.push((
                        o.owner,
                        GAS::type_tag(),
                        // we know this is a coin, safe to unwrap
                        Coin::extract_balance_if_coin(&o).unwrap().unwrap(),
                    )),
                    MoveObjectType::Coin(coin_type) => all_mutated_coins.push((
                        o.owner,
                        coin_type.clone(),
                        // we know this is a coin, safe to unwrap
                        Coin::extract_balance_if_coin(&o).unwrap().unwrap(),
                    )),
                    _ => {}
                }
            }
        }
    }
    Ok(all_mutated_coins)
}

#[async_trait]
pub trait ObjectProvider {
    type Error;
    async fn get_object(
        &self,
        id: &ObjectID,
        version: &SequenceNumber,
    ) -> Result<Object, Self::Error>;
    async fn find_object_less_then_version(
        &self,
        id: &ObjectID,
        version: &SequenceNumber,
    ) -> Result<Option<Object>, Self::Error>;
}

#[async_trait]
impl ObjectProvider for Arc<AuthorityState> {
    type Error = SuiError;
    async fn get_object(
        &self,
        id: &ObjectID,
        version: &SequenceNumber,
    ) -> Result<Object, Self::Error> {
        Ok(self
            .get_past_object_read(id, *version)
            .await?
            .into_object()?)
    }

    async fn find_object_less_then_version(
        &self,
        id: &ObjectID,
        version: &SequenceNumber,
    ) -> Result<Option<Object>, Self::Error> {
        Ok(self.database.find_object_lt_or_eq_version(*id, *version))
    }
}

pub struct ObjectProviderCache<P> {
    object_cache: RwLock<BTreeMap<(ObjectID, SequenceNumber), Object>>,
    last_version_cache: RwLock<BTreeMap<(ObjectID, SequenceNumber), SequenceNumber>>,
    provider: P,
}

impl<P> ObjectProviderCache<P> {
    pub fn new(provider: P) -> Self {
        Self {
            object_cache: Default::default(),
            last_version_cache: Default::default(),
            provider,
        }
    }
}

#[async_trait]
impl<P, E> ObjectProvider for ObjectProviderCache<P>
where
    P: ObjectProvider<Error = E> + Sync + Send,
    E: Sync + Send,
{
    type Error = P::Error;

    async fn get_object(
        &self,
        id: &ObjectID,
        version: &SequenceNumber,
    ) -> Result<Object, Self::Error> {
        if let Some(o) = self.object_cache.read().await.get(&(*id, *version)) {
            return Ok(o.clone());
        }
        let o = self.provider.get_object(id, version).await?;
        self.object_cache
            .write()
            .await
            .insert((*id, *version), o.clone());
        Ok(o)
    }

    async fn find_object_less_then_version(
        &self,
        id: &ObjectID,
        version: &SequenceNumber,
    ) -> Result<Option<Object>, Self::Error> {
        if let Some(version) = self.last_version_cache.read().await.get(&(*id, *version)) {
            return Ok(self.get_object(id, version).await.ok());
        }
        if let Some(o) = self
            .provider
            .find_object_less_then_version(id, version)
            .await?
        {
            self.object_cache
                .write()
                .await
                .insert((*id, o.version()), o.clone());
            self.last_version_cache
                .write()
                .await
                .insert((*id, *version), o.version());
            Ok(Some(o))
        } else {
            Ok(None)
        }
    }
}
