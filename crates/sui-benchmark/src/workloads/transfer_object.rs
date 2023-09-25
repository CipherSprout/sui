// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_trait::async_trait;
use rand::seq::IteratorRandom;
use tracing::error;

use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Arc;
use fastcrypto_zkp::bn254::zk_login::ZkLoginInputs;
use itertools::Itertools;

use crate::system_state_observer::SystemStateObserver;
use crate::workloads::payload::Payload;
use crate::workloads::workload::WorkloadBuilder;
use crate::workloads::workload::{
    Workload, ESTIMATED_COMPUTATION_COST, MAX_GAS_FOR_TESTING, STORAGE_COST_PER_COIN,
};
use crate::workloads::{Gas, GasCoinConfig, WorkloadBuilderInfo, WorkloadParams};
use crate::{ExecutionEffects, ValidatorProxy};
use sui_core::test_utils::make_transfer_object_transaction;
use sui_types::{
    base_types::{ObjectRef, SuiAddress},
    crypto::AccountKeyPair,
    transaction::Transaction,
};

/// TODO: This should be the amount that is being transferred instead of MAX_GAS.
/// Number of mist sent to each address on each batch transfer
const _TRANSFER_AMOUNT: u64 = 1;

const JWT: &str = "eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCIsImtpZCI6IjEifQ.eyJhdWQiOiJyczFiaDA2NWk5eWE0eWR2aWZpeGw0a3NzMHVocHQiLCJleHAiOjE2OTIyODQzMzQsImlhdCI6MTY5MjI4MzQzNCwiaXNzIjoiaHR0cHM6Ly9pZC50d2l0Y2gudHYvb2F1dGgyIiwic3ViIjoiOTA0NDQ4NjkyIiwiYXpwIjoicnMxYmgwNjVpOXlhNHlkdmlmaXhsNGtzczB1aHB0Iiwibm9uY2UiOiJoVFBwZ0Y3WEFLYlczN3JFVVM2cEVWWnFtb0kiLCJwcmVmZXJyZWRfdXNlcm5hbWUiOiJqb3lxdnEifQ.M54Sgs6aDu5Mprs_CgXeRbgiErC7oehj-h9oEcBqZFDADwd09zs9hbfDPqUjaNBB-_I6G7kn9e-zwPov8PUecI68kr3oyiCMWhKD-3h1FEu13MZv71B6jhIDMu1_UgI-RSrOQMRvdI8eL3qqD-KsvJuJH1Sz0w56PnB0xupUg-eSvgnMBAo6iTa0t1grX9qGy7U00i_oqn9J4jVGVVEbMhUWROJMjowWdOogJ4_VNqm67JHd_rMZ3xtjLabP6Nk1Gx-VjUbYceNADWUr5xpJveRtvb1FJvd0HSN4mab51zuSUnavCQw2OXbyoH8j6uuQAAKVhG-_Ht1hCvReycGXKw";
const EPHEMERAL_KEY_PAIR: &str = "m/SaagdV+VOBH84SXyaD1QQpw7tJ4HQUfgCJpS6uFV8=";
const EPH_BIG_INT: &str = "84029355920633174015103288781128426107680789454168570548782290541079926444544";
const SALTS: [[u8;1]; 2] = [/* [0], [1], [2], [3], */[4], [44]];

const ADDRESSES: [&str; 2] = [
    /*"362e6ab4bb80f930574c19056ac6c36fd5b780e5ccde36f42bdda0024ae70cf2",
    "5e1edaa0b39e7684fc55a7d00f8e8f17d1fb5d1d86aefcb937cf9038144e3aee",
    "a5ffdc1319abb846fe2bcec2e5a033875d05797b80f3900419136a64ff4356b2",
    "c3c0feedcbf11e29e3f485f5a4e96795b6e93b00a969d615e718bb00b5e1fe10",*/
    "0x94012d22c3f9fd5aa3a220a57a08c7c6f484d78a2f31c8b9b526c8398ec39c66",
    "0x5a3361ffdd15dacceb2d08b58edce5f22075a50c8677f2d75d737f9b40c3e0ba",
];

const ZK_LOGIN_INPUTS: [&str; 2] = [
    "{\"proofPoints\":{\"a\":[\"11418490324423348641149899753361872770644426614579825081181377545725765948794\",\"11849587932245656654270514780776469962785664998349661100053549164804505824480\",\"1\"],\"b\":[[\"10962499593238360374660420449653995278970326541050832756256240243201895468565\",\"3470487068324384680282107948335309316270787325701534789923112939254292418808\"],[\"17553214169721900737533837524815408632579180782789693128668168196139537931637\",\"4541354878110224198456611734682976208297506252071391051832700145741580828518\"],[\"1\",\"0\"]],\"c\":[\"5630405102392562350111726345326851304222454499831178317134245608479546095253\",\"6192060705523712675332277171787104858017048286621815560299058150711559225322\",\"1\"]},\"issBase64Details\":{\"value\":\"wiaXNzIjoiaHR0cHM6Ly9pZC50d2l0Y2gudHYvb2F1dGgyIiw\",\"indexMod4\":2},\"headerBase64\":\"eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCIsImtpZCI6IjEifQ\",\"addressSeed\":\"1477596370901702403243281691219517146662939593514548794817886158963850794449\"}",
    "{\"proofPoints\":{\"a\":[\"19702493827661644079303135700624674427905614754036486340545977517297661879787\",\"14508803204128510147977949782319212533509564505510059189223449120391785694682\",\"1\"],\"b\":[[\"16214818095782563389285660362713221246766167166788464683418294743733002142127\",\"20343421279282360259192398798171704514803496066120845785109301116124961753772\"],[\"2601722422444287625181497645075440345282069897471632932524051312454612188355\",\"3759434735134664342762534858239331430030270827129029168941159483757613889264\"],[\"1\",\"0\"]],\"c\":[\"4072287884398223361037704177136872705749612508445654141786193254806895954632\",\"7349683724394328521489294900790107401493584437887590357436493904990875404446\",\"1\"]},\"issBase64Details\":{\"value\":\"wiaXNzIjoiaHR0cHM6Ly9pZC50d2l0Y2gudHYvb2F1dGgyIiw\",\"indexMod4\":2},\"headerBase64\":\"eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCIsImtpZCI6IjEifQ\",\"addressSeed\":\"2108287817553348682314813966822786468731547396143496504013594730951070391587\"}",
];

fn get_zk_login_inputs(address: &SuiAddress) -> ZkLoginInputs {
    let index = ADDRESSES.iter().find_position(|x| x == &&address.to_string()).unwrap().0;
    serde_json::from_str(ZK_LOGIN_INPUTS[index]).unwrap()
}

#[derive(Debug)]
pub struct TransferObjectTestPayload {
    transfer_object: ObjectRef,
    transfer_from: SuiAddress,
    transfer_to: SuiAddress,
    gas: Vec<Gas>,
    system_state_observer: Arc<SystemStateObserver>,
}

impl Payload for TransferObjectTestPayload {
    fn make_new_payload(&mut self, effects: &ExecutionEffects) {
        if !effects.is_ok() {
            effects.print_gas_summary();
            error!("Transfer tx failed...");
        }

        let recipient = self.gas.iter().find(|x| x.1 != self.transfer_to).unwrap().1;
        let updated_gas: Vec<Gas> = self
            .gas
            .iter()
            .map(|x| {
                if x.1 == self.transfer_from {
                    (effects.gas_object().0, self.transfer_from, x.2.clone())
                } else {
                    x.clone()
                }
            })
            .collect();
        self.transfer_object = effects
            .mutated()
            .iter()
            .find(|(object_ref, _)| object_ref.0 == self.transfer_object.0)
            .map(|x| x.0)
            .unwrap();
        self.transfer_from = self.transfer_to;
        self.transfer_to = recipient;
        self.gas = updated_gas;
    }
    fn make_transaction(&mut self) -> Transaction {
        let (gas_obj, _, keypair) = self.gas.iter().find(|x| x.1 == self.transfer_from).unwrap();

        let zk_login_inputs = get_zk_login_inputs(&self.transfer_from);

        make_transfer_object_transaction(
            self.transfer_object,
            *gas_obj,
            self.transfer_from,
            keypair,
            self.transfer_to,
            self.system_state_observer
                .state
                .borrow()
                .reference_gas_price,
            &zk_login_inputs,
        )
    }
}

impl std::fmt::Display for TransferObjectTestPayload {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "transfer_object")
    }
}

#[derive(Debug)]
pub struct TransferObjectWorkloadBuilder {
    num_transfer_accounts: u64,
    num_payloads: u64,
}

impl TransferObjectWorkloadBuilder {
    pub fn from(
        workload_weight: f32,
        target_qps: u64,
        num_workers: u64,
        in_flight_ratio: u64,
        num_transfer_accounts: u64,
    ) -> Option<WorkloadBuilderInfo> {
        let target_qps = (workload_weight * target_qps as f32) as u64;
        let num_workers = (workload_weight * num_workers as f32).ceil() as u64;
        let max_ops = target_qps * in_flight_ratio;
        if max_ops == 0 || num_workers == 0 {
            None
        } else {
            let workload_params = WorkloadParams {
                target_qps,
                num_workers,
                max_ops,
            };
            let workload_builder = Box::<dyn WorkloadBuilder<dyn Payload>>::from(Box::new(
                TransferObjectWorkloadBuilder {
                    num_transfer_accounts,
                    num_payloads: max_ops,
                },
            ));
            let builder_info = WorkloadBuilderInfo {
                workload_params,
                workload_builder,
            };
            Some(builder_info)
        }
    }
}

#[async_trait]
impl WorkloadBuilder<dyn Payload> for TransferObjectWorkloadBuilder {
    async fn generate_coin_config_for_init(&self) -> Vec<GasCoinConfig> {
        vec![]
    }
    async fn generate_coin_config_for_payloads(&self) -> Vec<GasCoinConfig> {
        let mut address_map = HashMap::new();
        // Have to include not just the coins that are going to be created and sent
        // but the coin being used as gas as well.
        let amount = MAX_GAS_FOR_TESTING
            + ESTIMATED_COMPUTATION_COST
            + STORAGE_COST_PER_COIN * (self.num_transfer_accounts + 1);
        // gas for payloads
        let mut payload_configs = vec![];

        for i in 0..self.num_transfer_accounts {

            let (address, keypair) = (SuiAddress::from_str(ADDRESSES[i as usize]).unwrap(),
            AccountKeyPair::from_str(EPHEMERAL_KEY_PAIR).unwrap());

            let cloned_keypair: Arc<AccountKeyPair> = Arc::new(keypair);
            address_map.insert(address, cloned_keypair.clone());
            for _j in 0..self.num_payloads {
                payload_configs.push(GasCoinConfig {
                    amount,
                    address,
                    keypair: cloned_keypair.clone(),
                });
            }
        }

        let owner = *address_map.keys().choose(&mut rand::thread_rng()).unwrap();

        // transfer tokens
        let mut gas_configs = vec![];
        for _i in 0..self.num_payloads {
            let (address, keypair) = (owner, address_map.get(&owner).unwrap().clone());
            gas_configs.push(GasCoinConfig {
                amount,
                address,
                keypair: keypair.clone(),
            });
        }

        gas_configs.extend(payload_configs);
        gas_configs
    }
    async fn build(
        &self,
        _init_gas: Vec<Gas>,
        payload_gas: Vec<Gas>,
    ) -> Box<dyn Workload<dyn Payload>> {
        Box::<dyn Workload<dyn Payload>>::from(Box::new(TransferObjectWorkload {
            num_tokens: self.num_payloads,
            payload_gas,
        }))
    }
}

#[derive(Debug)]
pub struct TransferObjectWorkload {
    num_tokens: u64,
    payload_gas: Vec<Gas>,
}

#[async_trait]
impl Workload<dyn Payload> for TransferObjectWorkload {
    async fn init(
        &mut self,
        _proxy: Arc<dyn ValidatorProxy + Sync + Send>,
        _system_state_observer: Arc<SystemStateObserver>,
    ) {
        return;
    }
    async fn make_test_payloads(
        &self,
        _proxy: Arc<dyn ValidatorProxy + Sync + Send>,
        system_state_observer: Arc<SystemStateObserver>,
    ) -> Vec<Box<dyn Payload>> {
        let (transfer_tokens, payload_gas) = self.payload_gas.split_at(self.num_tokens as usize);
        let mut gas_by_address: HashMap<SuiAddress, Vec<Gas>> = HashMap::new();
        for gas in payload_gas.iter() {
            gas_by_address
                .entry(gas.1)
                .or_insert_with(|| Vec::with_capacity(1))
                .push(gas.clone());
        }

        let addresses: Vec<SuiAddress> = gas_by_address.keys().cloned().collect();
        let mut transfer_gas: Vec<Vec<Gas>> = vec![];
        for i in 0..self.num_tokens {
            let mut account_transfer_gas = vec![];
            for address in addresses.iter() {
                account_transfer_gas.push(gas_by_address[address][i as usize].clone());
            }
            transfer_gas.push(account_transfer_gas);
        }
        let refs: Vec<(Vec<Gas>, Gas)> = transfer_gas
            .into_iter()
            .zip(transfer_tokens.iter())
            .map(|(g, t)| (g, t.clone()))
            .collect();
        refs.iter()
            .map(|(g, t)| {
                let from = t.1;
                let to = g.iter().find(|x| x.1 != from).unwrap().1;

                Box::new(TransferObjectTestPayload {
                    transfer_object: t.0,
                    transfer_from: from,
                    transfer_to: to,
                    gas: g.to_vec(),
                    system_state_observer: system_state_observer.clone(),
                })
            })
            .map(|b| Box::<dyn Payload>::from(b))
            .collect()
    }
}
