// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module sui::Genesis {
    use std::vector;

    use sui::Coin;
    use sui::SUI;
    use sui::SuiSystem;
    use sui::TxContext::TxContext;
    use sui::Validator;

    /// The initial amount of SUI locked in the storage fund.
    /// 10^14, an arbitrary number.
    const INIT_STORAGE_FUND: u64 = 100000000000000;

    /// Initial value of the lower-bound on the amount of stake required to become a validator.
    const INIT_MIN_VALIDATOR_STAKE: u64 = 100000000000000;

    /// Initial value of the upper-bound on the number of validators.
    const INIT_MAX_VALIDATOR_COUNT: u64 = 100;

    /// This function will be explicitly called once at genesis.
    /// It will create a singleton SuiSystemState object, which contains
    /// all the information we need in the system.
    fun create(
        validator_pubkeys: vector<vector<u8>>,
        validator_sui_addresses: vector<address>,
        validator_names: vector<vector<u8>>,
        validator_net_addresses: vector<vector<u8>>,
        validator_stakes: vector<u64>,
        ctx: &mut TxContext,
    ) {
        let treasury_cap = SUI::new(ctx);
        let storage_fund = Coin::mint_balance(INIT_STORAGE_FUND, &mut treasury_cap);
        let validators = vector::empty();
        let count = vector::length(&validator_pubkeys);
        assert!(
            vector::length(&validator_sui_addresses) == count
                && vector::length(&validator_stakes) == count
                && vector::length(&validator_names) == count
                && vector::length(&validator_net_addresses) == count,
            1
        );
        let i = 0;
        while (i < count) {
            let sui_address = *vector::borrow(&validator_sui_addresses, i);
            let pubkey = *vector::borrow(&validator_pubkeys, i);
            let name = *vector::borrow(&validator_names, i);
            let net_address = *vector::borrow(&validator_net_addresses, i);
            let stake = *vector::borrow(&validator_stakes, i);
            vector::push_back(&mut validators, Validator::new(
                sui_address,
                pubkey,
                name,
                net_address,
                Coin::mint_balance(stake, &mut treasury_cap),
            ));
            i = i + 1;
        };
        SuiSystem::create(
            validators,
            treasury_cap,
            storage_fund,
            INIT_MAX_VALIDATOR_COUNT,
            INIT_MIN_VALIDATOR_STAKE,
        );
    }
}
