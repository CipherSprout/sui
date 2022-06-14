// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Example of objects that can be combined to create
/// new objects
module Basics::Sandwich {
    use sui::balance::{Self, Balance};
    use sui::Coin::{Self, Coin};
    use sui::id::{Self, VersionedID};
    use sui::SUI::SUI;
    use sui::Transfer;
    use sui::tx_context::{Self, TxContext};

    struct Ham has key {
        id: VersionedID
    }

    struct Bread has key {
        id: VersionedID
    }

    struct Sandwich has key {
        id: VersionedID,
    }

    // This Capability allows the owner to withdraw profits
    struct GroceryOwnerCapability has key {
        id: VersionedID
    }

    // Grocery is created on module init
    struct Grocery has key {
        id: VersionedID,
        profits: Balance<SUI>
    }

    /// Price for ham
    const HAM_PRICE: u64 = 10;
    /// Price for bread
    const BREAD_PRICE: u64 = 2;

    /// Not enough funds to pay for the good in question
    const EInsufficientFunds: u64 = 0;
    /// Nothing to withdraw
    const ENoProfits: u64 = 1;

    /// On module init, create a grocery
    fun init(ctx: &mut TxContext) {
        Transfer::share_object(Grocery {
            id: tx_context::new_id(ctx),
            profits: balance::zero<SUI>()
        });

        Transfer::transfer(GroceryOwnerCapability {
            id: tx_context::new_id(ctx)
        }, tx_context::sender(ctx));
    }

    /// Exchange `c` for some ham
    public entry fun buy_ham(
        grocery: &mut Grocery,
        c: Coin<SUI>,
        ctx: &mut TxContext
    ) {
        let b = Coin::into_balance(c);
        assert!(balance::value(&b) == HAM_PRICE, EInsufficientFunds);
        balance::join(&mut grocery.profits, b);
        Transfer::transfer(Ham { id: tx_context::new_id(ctx) }, tx_context::sender(ctx))
    }

    /// Exchange `c` for some bread
    public entry fun buy_bread(
        grocery: &mut Grocery,
        c: Coin<SUI>,
        ctx: &mut TxContext
    ) {
        let b = Coin::into_balance(c);
        assert!(balance::value(&b) == BREAD_PRICE, EInsufficientFunds);
        balance::join(&mut grocery.profits, b);
        Transfer::transfer(Bread { id: tx_context::new_id(ctx) }, tx_context::sender(ctx))
    }

    /// Combine the `ham` and `bread` into a delicious sandwich
    public entry fun make_sandwich(
        ham: Ham, bread: Bread, ctx: &mut TxContext
    ) {
        let Ham { id: ham_id } = ham;
        let Bread { id: bread_id } = bread;
        id::delete(ham_id);
        id::delete(bread_id);
        Transfer::transfer(Sandwich { id: tx_context::new_id(ctx) }, tx_context::sender(ctx))
    }

    /// See the profits of a grocery
    public fun profits(grocery: &Grocery): u64 {
        balance::value(&grocery.profits)
    }

    /// Owner of the grocery can collect profits by passing his capability
    public entry fun collect_profits(_cap: &GroceryOwnerCapability, grocery: &mut Grocery, ctx: &mut TxContext) {
        let amount = balance::value(&grocery.profits);

        assert!(amount > 0, ENoProfits);

        // Take a transferable `Coin` from a `Balance`
        let coin = Coin::withdraw(&mut grocery.profits, amount, ctx);

        Transfer::transfer(coin, tx_context::sender(ctx));
    }

    #[test_only]
    public fun init_for_testing(ctx: &mut TxContext) {
        init(ctx);
    }
}

#[test_only]
module Basics::TestSandwich {
    use Basics::Sandwich::{Self, Grocery, GroceryOwnerCapability, Bread, Ham};
    use sui::TestScenario;
    use sui::Coin::{Self};
    use sui::SUI::SUI;

    #[test]
    fun test_make_sandwich() {
        let owner = @0x1;
        let the_guy = @0x2;

        let scenario = &mut TestScenario::begin(&owner);
        TestScenario::next_tx(scenario, &owner);
        {
            Sandwich::init_for_testing(TestScenario::ctx(scenario));
        };

        TestScenario::next_tx(scenario, &the_guy);
        {
            let grocery_wrapper = TestScenario::take_shared<Grocery>(scenario);
            let grocery = TestScenario::borrow_mut(&mut grocery_wrapper);
            let ctx = TestScenario::ctx(scenario);

            Sandwich::buy_ham(
                grocery,
                Coin::mint_for_testing<SUI>(10, ctx),
                ctx
            );

            Sandwich::buy_bread(
                grocery,
                Coin::mint_for_testing<SUI>(2, ctx),
                ctx
            );

            TestScenario::return_shared(scenario, grocery_wrapper);
        };

        TestScenario::next_tx(scenario, &the_guy);
        {
            let ham = TestScenario::take_owned<Ham>(scenario);
            let bread = TestScenario::take_owned<Bread>(scenario);

            Sandwich::make_sandwich(ham, bread, TestScenario::ctx(scenario));
        };

        TestScenario::next_tx(scenario, &owner);
        {
            let grocery_wrapper = TestScenario::take_shared<Grocery>(scenario);
            let grocery = TestScenario::borrow_mut(&mut grocery_wrapper);
            let capability = TestScenario::take_owned<GroceryOwnerCapability>(scenario);

            assert!(Sandwich::profits(grocery) == 12, 0);
            Sandwich::collect_profits(&capability, grocery, TestScenario::ctx(scenario));
            assert!(Sandwich::profits(grocery) == 0, 0);

            TestScenario::return_owned(scenario, capability);
            TestScenario::return_shared(scenario, grocery_wrapper);
        };
    }
}
