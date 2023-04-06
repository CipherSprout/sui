module deepbook::custodian {
    use sui::balance::{Self, Balance, split};
    use sui::coin::{Self, Coin};
    use sui::object::{Self, UID, ID};
    use sui::table::{Self, Table};
    use sui::tx_context::TxContext;

    friend deepbook::clob;

    // Custodian for limit orders.

    // <<<<<<<<<<<<<<<<<<<<<<<< Error codes <<<<<<<<<<<<<<<<<<<<<<<<
    const EUserBalanceDoesNotExist: u64 = 1;
    // <<<<<<<<<<<<<<<<<<<<<<<< Error codes <<<<<<<<<<<<<<<<<<<<<<<<

    struct Account<phantom T> has store {
        available_balance: Balance<T>,
        locked_balance: Balance<T>,
    }

    struct AccountCap has key, store{ id: UID }

    struct Custodian<phantom T> has key, store {
        id: UID,
        // Holds custody of protocol fees.
        // The logic of withdrawing protocol fees is left out intentionally for further confirmation from Sui foundation.
        custodian_balances: Balance<T>,
        account_balances: Table<ID, Account<T>>,
    }

    public(friend) fun custodian_create_account<BaseAsset, QuoteAsset>(
        base_custodian: &mut Custodian<BaseAsset>,
        quote_custodian: &mut Custodian<QuoteAsset>,
        ctx: &mut TxContext
    ): AccountCap {
        let accountCap = AccountCap { id: object::new(ctx) };
        let user = get_account_cap_id(&accountCap);
        table::add(
                &mut base_custodian.account_balances,
                user,
                Account { available_balance: balance::zero(), locked_balance: balance::zero() }
        );
        table::add(
                &mut quote_custodian.account_balances,
                user,
                Account { available_balance: balance::zero(), locked_balance: balance::zero() }
            );
        accountCap
    }

    public(friend) fun get_account_cap_id(account_cap: &AccountCap): ID {
        object::uid_to_inner(&account_cap.id)
    }

    public(friend) fun new<T>(ctx: &mut TxContext): Custodian<T> {
        Custodian<T> {
            id: object::new(ctx),
            custodian_balances: balance::zero(),
            account_balances: table::new(ctx),
        }
    }

    public(friend) fun deposit<T>(
        custodian: &mut Custodian<T>,
        coin: Coin<T>,
        user: ID
    ) {
        increase_user_available_balance<T>(custodian, user, coin::into_balance(coin));
    }

    public(friend) fun withdraw<T>(
        custodian: &mut Custodian<T>,
        quantity: u64,
        account_cap: &AccountCap,
        ctx: &mut TxContext
    ): Coin<T> {
        let user = get_account_cap_id(account_cap);
        coin::from_balance(decrease_user_available_balance<T>(custodian, user, quantity), ctx)
    }

    public(friend) fun increase_custodian_balance<T>(
        custodian: &mut Custodian<T>,
        quantity: Balance<T>
    ) {
        balance::join(&mut custodian.custodian_balances, quantity);
    }

    public(friend) fun decrease_custodian_balance<T>(
        custodian: &mut Custodian<T>,
        quantity: u64,
    ): Balance<T> {
        split(&mut custodian.custodian_balances, quantity)
    }

    public(friend) fun increase_user_available_balance<T>(
        custodian: &mut Custodian<T>,
        user: ID,
        quantity: Balance<T>,
    ) {
        let account = borrow_mut_account_balance<T>(custodian, user);
        balance::join(&mut account.available_balance, quantity);
    }

    public(friend) fun decrease_user_available_balance<T>(
        custodian: &mut Custodian<T>,
        user: ID,
        quantity: u64,
    ): Balance<T> {
        let account = borrow_mut_account_balance<T>(custodian, user);
        balance::split(&mut account.available_balance, quantity)
    }

    public(friend) fun increase_user_locked_balance<T>(
        custodian: &mut Custodian<T>,
        user: ID,
        quantity: Balance<T>,
    ) {
        let account = borrow_mut_account_balance<T>(custodian, user);
        balance::join(&mut account.locked_balance, quantity);
    }

    public(friend) fun decrease_user_locked_balance<T>(
        custodian: &mut Custodian<T>,
        user: ID,
        quantity: u64,
    ): Balance<T> {
        let account = borrow_mut_account_balance<T>(custodian, user);
        split(&mut account.locked_balance, quantity)
    }


    public(friend) fun account_available_balance<T>(
        custodian: &Custodian<T>,
        user: ID,
    ): u64 {
        balance::value(&table::borrow(&custodian.account_balances, user).available_balance)
    }

    public(friend) fun account_locked_balance<T>(
        custodian: &Custodian<T>,
        user: ID,
    ): u64 {
        balance::value(&table::borrow(&custodian.account_balances, user).locked_balance)
    }

    fun borrow_mut_account_balance<T>(
        custodian: &mut Custodian<T>,
        user: ID,
    ): &mut Account<T> {
        table::borrow_mut(&mut custodian.account_balances, user)
    }

    fun borrow_account_balance<T>(
        custodian: &Custodian<T>,
        user: ID,
    ): &Account<T> {
        assert!(
            table::contains(&custodian.account_balances, user),
            EUserBalanceDoesNotExist
        );
        table::borrow(&custodian.account_balances, user)
    }

    #[test_only]
    friend deepbook::clob_test;
    #[test_only]
    use deepbook::usd::{Self, USD};
    #[test_only]
    use sui::test_scenario::{Self, Scenario};
    #[test_only]
    use sui::transfer;
    #[test_only]
    const ENull: u64 = 0;
    #[test_only]
    public fun assert_user_balance<T>(
        custodian: &Custodian<T>,
        user: ID,
        available_balance: u64,
        locked_balance: u64,
    ) {
        let user_balance = borrow_account_balance<T>(custodian, user);
        assert!(balance::value(&user_balance.available_balance) == available_balance, ENull);
        assert!(balance::value(&user_balance.locked_balance) == locked_balance, ENull)
    }

    #[test_only]
    fun setup_test(
        scenario: &mut Scenario,
    ) {
        usd::init_test(test_scenario::ctx(scenario));
        transfer::share_object<Custodian<USD>>(new<USD>(test_scenario::ctx(scenario)));
    }

    #[test_only]
    public fun mint_account_cap_transfer<BaseAsset, QuoteAsset>(
        base_custodian: &mut Custodian<BaseAsset>,
        quote_custodian: &mut Custodian<QuoteAsset>,
        user: address,
        ctx: &mut TxContext
    ) {
        transfer::transfer(custodian_create_account(base_custodian,  quote_custodian, ctx), user);
    }

    #[test_only]
    public fun test_increase_user_available_balance<T>(
        custodian: &mut Custodian<T>,
        user: ID,
        quantity: u64,
    ) {
        increase_user_available_balance<T>(custodian, user, balance::create_for_testing(quantity));
    }

}
