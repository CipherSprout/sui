/// An entity uses their `&UID` as a token.
/// Based on this token the safe owner grants redeem rights for specific NFT.
/// An entity that has been granted redeem rights can call `get_nft`.
module sui::nft_safe {
    use std::option::{Self, Option};

    use sui::balance::{Self, Balance};
    use sui::coin::{Self, Coin};
    use sui::dynamic_object_field::{Self as dof};
    use sui::object::{Self, ID, UID};
    use sui::package::{Self, Publisher};
    use sui::sui::SUI;
    use sui::tx_context::TxContext;
    use sui::vec_map::{Self, VecMap};

    // === Errors ===

    /// Incorrect owner for the given Safe
    const ESafeOwnerMismatch: u64 = 0;
    /// Safe does not contain the NFT
    const ESafeDoesNotContainNft: u64 = 1;
    /// NFT is already exclusively listed
    const ENftAlreadyExclusivelyListed: u64 = 2;
    /// NFT is already listed
    const ENftAlreadyListed: u64 = 3;
    /// The logic requires that no NFTs are stored in the safe.
    const EMustBeEmpty: u64 = 4;
    /// Publisher does not match the expected type.
    const EPublisherMismatch: u64 = 5;
    /// The amount provided is not enough.
    const ENotEnough: u64 = 6;

    // === Structs ===

    /// Whoever owns this object can perform some admin actions against the
    /// `NftSafe` shared object with the corresponding id.
    struct OwnerCap has key, store {
        id: UID,
        safe: ID,
    }

    struct NftSafe has key, store {
        id: UID,
        /// Accounting for deposited NFTs.
        /// Each dynamic object field NFT is represented in this map.
        refs: VecMap<ID, NftRef>,
        /// TBD: This could be a dynamic field and hence allow for generic
        /// tokens to be stored.
        profits: Balance<SUI>,
    }

    /// Holds info about NFT listing which is used to determine if an entity
    /// is allowed to redeem the NFT.
    struct NftRef has store, drop {
        /// Entities which can use their `&UID` to redeem the NFT.
        /// 
        /// We also configure min listing price.
        /// The item must be bought by the entity by _at least_ this many SUI.
        listed_with: VecMap<ID, u64>,
        /// If set to true, then `listed_with` must have length of 1
        is_exclusively_listed: bool,
        /// Cannot be `Some` if exclusively listed.
        listed_for: Option<u64>,
    }

    /// A "Hot Potato" forcing the buyer to get a transfer permission
    /// from the item type (`NFT`) owner on purchase attempt.
    struct TransferRequest<phantom NFT: key + store> {
        /// Amount of SUI paid for the item. Can be used to
        /// calculate the fee / transfer policy enforcement.
        paid: u64,
        /// The ID of the NftSafe the object is being sold from.
        /// Can be used by the TransferPolicy implementors to
        /// create an allowlist of NftSafes which can trade the type.
        from: ID,
        /// Is some if the item was bought through redeem right specific to a 
        /// trading contract (entity.)
        /// Is none if the item was bought directly from the safe.
        entity: Option<ID>,
    }

    /// A unique capability that allows owner of the `NFT` to authorize
    /// transfers.
    /// Can only be created with the `Publisher` object.
    struct TransferPolicyCap<phantom T: key + store> has key, store {
        id: UID
    }

    // === Events ===

    // === Royalty interface ===

    /// Register a type in the `NftSafe` system and receive an`TransferPolicyCap`
    /// which is required to confirm `NftSafe` deals for the `NFT`.
    /// If there's no `TransferPolicyCap` available for use, the type can not be 
    /// traded in `NftSafe`s.
    public fun new_transfer_policy_cap<T: key + store>(
        pub: &Publisher, ctx: &mut TxContext,
    ): TransferPolicyCap<T> {
        assert!(package::from_package<T>(pub), EPublisherMismatch);
        let id = object::new(ctx);
        TransferPolicyCap { id }
    }

    /// Destroy a `TransferPolicyCap`.
    public fun destroy_transfer_policy_cap<T: key + store>(
        cap: TransferPolicyCap<T>
    ) {
        let TransferPolicyCap { id } = cap;
        object::delete(id);
    }

    /// Allow a `TransferRequest` for the type `NFT`.
    /// The call is protected by the type constraint, as only the publisher of
    /// the `NFT` can get `TransferPolicyCap<NFT>`.
    ///
    /// Note: unless there's a policy for `NFT` to allow transfers, trades will
    /// not be possible.
    public fun allow_transfer<NFT: key + store>(
        _cap: &TransferPolicyCap<NFT>, req: TransferRequest<NFT>,
    ) {
        let TransferRequest { paid: _, from: _, entity: _ } = req;
    }

    // === Safe interface ===

    public fun new(ctx: &mut TxContext): (NftSafe, OwnerCap) {
        let safe = NftSafe {
            id: object::new(ctx),
            refs: vec_map::empty(),
            profits: balance::zero(),
        };
        let cap = OwnerCap {
            id: object::new(ctx),
            safe: object::id(&safe),
        };
        (safe, cap)
    }

    /// Given object is added to the safe and can be listed from now on.
    public fun deposit_nft<NFT: key + store>(
        self: &mut NftSafe,
        owner_cap: &OwnerCap,
        nft: NFT,
    ) {
        let nft_id = object::id(&nft);

        vec_map::insert(&mut self.refs, nft_id, NftRef {
            listed_with: vec_map::empty(),
            is_exclusively_listed: false,
            listed_for: option::none(),
        });

        dof::add(&mut self.id, nft_id, nft);
    }

    /// After this, anyone can buy the NFT from the safe for the given price.
    /// 
    /// # Aborts
    /// * If the NFT has already given exclusive redeem rights.
    public fun list_nft(
        self: &mut NftSafe,
        owner_cap: &OwnerCap,
        nft_id: ID,
        price: u64,
    ) {
        assert_owner_cap(self, owner_cap);
        assert_has_nft(self, &nft_id);

        let ref = vec_map::get_mut(&mut self.refs, &nft_id);
        assert_ref_not_exclusively_listed(ref);

        option::fill(&mut ref.listed_for, price);
    }

    /// Buy a publicly listed NFT.
    /// 
    /// This function returns a hot potato which must be passed around and
    /// finally destroyed in `allow_transfer`.
    /// 
    /// # Aborts
    /// * If the NFT is not publicly listed
    /// * If the wallet doesn't have enough tokens
    public fun purchase<NFT: key + store>(
        self: &mut NftSafe,
        wallet: &mut Coin<SUI>,
        nft_id: ID,
    ): (NFT, TransferRequest<NFT>) {
        assert_has_nft(self, &nft_id);

        // NFT is being transferred - destroy the ref
        let (_, ref) = vec_map::remove(&mut self.refs, &nft_id);
        let listed_for = *option::borrow(&ref.listed_for);

        let payment = balance::split(coin::balance_mut(wallet), listed_for);
        balance::join(&mut self.profits, payment);

        let nft = dof::remove<ID, NFT>(&mut self.id, nft_id);
        (nft, TransferRequest<NFT> {
            paid: listed_for,
            from: object::id(self),
            entity: option::none(),
        })
    }

    /// Multiples entities can have redeem rights for the same NFT.
    /// Additionally, the owner can remove redeem rights for a specific entity
    /// at any time.
    /// 
    /// # Aborts
    /// * If the NFT has already given exclusive redeem rights.
    public fun auth_entity_for_nft_transfer(
        self: &mut NftSafe,
        owner_cap: &OwnerCap,
        entity_id: ID,
        nft_id: ID,
        min_price: u64,
    ) {
        assert_owner_cap(self, owner_cap);
        assert_has_nft(self, &nft_id);

        let ref = vec_map::get_mut(&mut self.refs, &nft_id);
        assert_ref_not_exclusively_listed(ref);

        vec_map::insert(&mut ref.listed_with, entity_id, min_price);
    }

    /// One only entity can have exclusive redeem rights for the same NFT.
    /// Only the same entity can then give up their rights.
    /// Use carefully, if the entity is malicious, they can lock the NFT. 
    /// 
    /// # Note
    /// Unlike with `auth_entity_for_nft_transfer`, we require that the entity 
    /// approves this action `&UID`.
    /// This gives the owner some sort of warranty that the implementation of 
    /// the entity took into account the exclusive listing.
    /// 
    /// # Aborts
    /// * If the NFT already has given up redeem rights (not necessarily exclusive)
    public fun auth_entity_for_exclusive_nft_transfer(
        self: &mut NftSafe,
        owner_cap: &OwnerCap,
        entity_id: &UID,
        nft_id: ID,
        min_price: u64,
    ) {
        assert_owner_cap(self, owner_cap);
        assert_has_nft(self, &nft_id);

        let ref = vec_map::get_mut(&mut self.refs, &nft_id);
        assert_not_listed(ref);

        vec_map::insert(
            &mut ref.listed_with, object::uid_to_inner(entity_id), min_price
        );
        ref.is_exclusively_listed = true;
    }

    /// An entity uses the `&UID` as a token which has been granted a permission 
    /// for transfer of the specific NFT.
    /// With this token, a transfer can be performed.
    ///
    /// This function returns a hot potato which must be passed around and
    /// finally destroyed in `allow_transfer`.
    public fun purchase_as_entity<NFT: key + store>(
        self: &mut NftSafe,
        entity_id: &UID,
        nft_id: ID,
        payment: Coin<SUI>,
    ): (NFT, TransferRequest<NFT>) {
        assert_has_nft(self, &nft_id);

        // NFT is being transferred - destroy the ref
        let (_, ref) = vec_map::remove(&mut self.refs, &nft_id);
        let listed_for = *option::borrow(&ref.listed_for);
        let paid = coin::value(&payment);
        assert!(paid >= listed_for, ENotEnough);
        balance::join(&mut self.profits, coin::into_balance(payment));

        // aborts if entity is not included in the map
        let entity_auth = object::uid_to_inner(entity_id);
        vec_map::remove(&mut ref.listed_with, &entity_auth);

        let nft = dof::remove<ID, NFT>(&mut self.id, nft_id);
        (nft, TransferRequest<NFT> {
            paid,
            from: object::id(self),
            entity: option::none(),
        })
    }

    /// Get an NFT out of the safe as the owner.
    public fun get_nft_as_owner<NFT: key + store>(
        self: &mut NftSafe,
        owner_cap: &OwnerCap,
        nft_id: ID,
    ): NFT {
        assert_owner_cap(self, owner_cap);
        assert_has_nft(self, &nft_id);

        // NFT is being transferred - destroy the ref
        let (_, ref) = vec_map::remove(&mut self.refs, &nft_id);
        assert_ref_not_exclusively_listed(&ref);

        dof::remove<ID, NFT>(&mut self.id, nft_id)
    }

    /// An entity can remove itself from accessing (ie. delist) an NFT.
    /// 
    /// This method is the only way an exclusive listing can be delisted.
    /// 
    /// # Aborts
    /// * If the entity is not listed as an auth for this NFT.
    public fun remove_entity_from_nft_listing(
        self: &mut NftSafe,
        entity_id: &UID,
        nft_id: &ID,
    ) {
        assert_has_nft(self, nft_id);
        
        let ref = vec_map::get_mut(&mut self.refs, nft_id);
        // aborts if the entity is not in the map
        let entity_auth = object::uid_to_inner(entity_id);
        vec_map::remove(&mut ref.listed_with, &entity_auth);
        ref.is_exclusively_listed = false; // no-op unless it was exclusive
    }

    /// The safe owner can remove an entity from accessing an NFT unless
    /// it's listed exclusively.
    /// An exclusive listing can be canceled only via
    /// `remove_auth_from_nft_listing`.
    /// 
    /// # Aborts
    /// * If the NFT is exclusively listed.
    /// * If the entity is not listed as an auth for this NFT.
    public fun remove_entity_from_nft_listing_as_owner(
        self: &mut NftSafe,
        owner_cap: &OwnerCap,
        entity_id: &ID,
        nft_id: &ID,
    ) {
        assert_owner_cap(self, owner_cap);
        assert_has_nft(self, nft_id);
        
        let ref = vec_map::get_mut(&mut self.refs, nft_id);
        assert_ref_not_exclusively_listed(ref);
        // aborts if the entity is not in the map
        vec_map::remove(&mut ref.listed_with, entity_id);
    }

    /// Removes all access to an NFT.
    /// An exclusive listing can be canceled only via
    /// `remove_auth_from_nft_listing`.
    /// 
    /// # Aborts
    /// * If the NFT is exclusively listed.
   public fun delist_nft(
        self: &mut NftSafe,
        owner_cap: &OwnerCap,
        nft_id: &ID,
    ) {
        assert_owner_cap(self, owner_cap);
        assert_has_nft(self, nft_id);
        
        let ref = vec_map::get_mut(&mut self.refs, nft_id);
        assert_ref_not_exclusively_listed(ref);
        ref.listed_with = vec_map::empty();
    }

    /// If there are no deposited NFTs in the safe, the safe is destroyed.
    /// Only works for non-shared safes.
    public fun destroy_empty(
        self: NftSafe, owner_cap: OwnerCap, ctx: &mut TxContext,
    ): Coin<SUI> {
        assert_owner_cap(&self, &owner_cap);
        assert!(vec_map::is_empty(&self.refs), EMustBeEmpty);

        let NftSafe { id, refs, profits } = self;
        let OwnerCap { id: cap_id, safe: _ } = owner_cap;
        vec_map::destroy_empty(refs);
        object::delete(id);
        object::delete(cap_id);

        coin::from_balance(profits, ctx)
    }

    /// Withdraws profits from the safe.
    /// If `amount` is `none`, withdraws all profits.
    /// Otherwise attempts to withdraw the specified amount.
    /// Fails if there are not enough token.
    public fun withdraw(
        self: &mut NftSafe,
        owner_cap: &OwnerCap, 
        amount: Option<u64>,
        ctx: &mut TxContext,
    ): Coin<SUI> {
        assert_owner_cap(self, owner_cap);

        let amount = if (option::is_some(&amount)) {
            let amt = option::destroy_some(amount);
            assert!(amt <= balance::value(&self.profits), ENotEnough);
            amt
        } else {
            balance::value(&self.profits)
        };

        coin::take(&mut self.profits, amount, ctx)
    }

    // === Getters ===

    public fun nfts_count(self: &NftSafe): u64 {
        vec_map::size(&self.refs)
    }

    public fun borrow_nft<NFT: key + store>(
        self: &NftSafe, nft_id: ID,
    ): &NFT {
        assert_has_nft(self, &nft_id);
        dof::borrow<ID, NFT>(&self.id, nft_id)
    }

    public fun has_nft<NFT: key + store>(
        self: &NftSafe, nft_id: ID,
    ): bool {
        dof::exists_with_type<ID, NFT>(&self.id, nft_id)
    }

    public fun owner_cap_safe(cap: &OwnerCap): ID { cap.safe }

    // === Assertions ===

    public fun assert_owner_cap(self: &NftSafe, cap: &OwnerCap) {
        assert!(cap.safe == object::id(self), ESafeOwnerMismatch);
    }

    public fun assert_has_nft(self: &NftSafe, nft: &ID) {
        assert!(vec_map::contains(&self.refs, nft), ESafeDoesNotContainNft);
    }

    public fun assert_not_exclusively_listed(
        self: &NftSafe, nft: &ID
    ) {
        let ref = vec_map::get(&self.refs, nft);
        assert_ref_not_exclusively_listed(ref);
    }

    fun assert_ref_not_exclusively_listed(ref: &NftRef) {
        assert!(!ref.is_exclusively_listed, ENftAlreadyExclusivelyListed);
    }

    fun assert_not_listed(ref: &NftRef) {
        assert!(vec_map::size(&ref.listed_with) == 0, ENftAlreadyListed);
        assert!(option::is_none(&ref.listed_for), ENftAlreadyListed);
    }
}
