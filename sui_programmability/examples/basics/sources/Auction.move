/// This is an implementation of an English auction
/// (https://en.wikipedia.org/wiki/English_auction). There are 3 types
/// of parties participating in an auction:
/// - auctioneer - this is a trusted party that runs the auction
/// - owner - this is the original owner of an item that is sold at an
/// auction; the owner submits a request to an auctioneer that runs
/// the auction
/// - bidders - these are parties interested in purchasing items sold
/// at an auction; they submit bids to an auctioneer to affect the
/// state of an auction
///
/// A typical lifetime of an auction looks as follows:
/// - auction starts by the owner sending an item to be sold along with
/// its own address to the auctioneer who creates and initializes an
/// auction
/// - bidders send bid to the auctioneer for a given auction
/// consisting of the funds they intend to use for the item's purchase
/// and their addresses
/// - the auctioneer periodically inspects the bids:
///   - if the inspected bid is higher than the current bid (initially
///   0), the auction is updated with the current bid and funds
///   representing previous highest bid are sent to the original owner
///   - otherwise (bid is too low) the bidder's funds are sent back to
///   the bidder and the auction remains unchanged
/// - the auctioneer eventually ends the auction
///   - if no bids were received, the item goes back to the original owner
///   - otherwise the funds accumulated in the auction go to the
///   original owner and the item goes to the bidder that won the
///   auction

module Basics::Auction {
    use Std::Option::{Self, Option};

    use Sui::Coin::{Self, Coin};
    use Sui::GAS::GAS;
    use Sui::ID::{Self, ID, VersionedID};
    use Sui::Transfer;
    use Sui::TxContext::{Self,TxContext};

    // Error codes.

    /// A bid submitted for the wrong (e.g. non-existent) auction.
    const EWRONG_AUCTION: u64 = 1;

    /// Maintains the state of the auction owned by a trusted
    /// auctioneer.
    struct Auction<T:  key + store> has key {
        id: VersionedID,
        /// Item to be sold.
        to_sell: T,
        /// Coin representing the current bid (starts with no bid).
        funds: Option<Coin<GAS>>,
        /// Address of the highest bidder.
        highest_bidder: address,
    }

    /// Represents a bid sent by a bidder to the auctioneer.
    struct Bid has key {
        id: VersionedID,
        /// Address of the bidder
        bidder: address,
        /// ID of the Auction object this bid is intended for
        auction_id: ID,
        /// Coin used for bidding.
        coin: Coin<GAS>
    }

    // Entry functions.

    /// Creates an auction. It would be more natural to generate
    /// auction_id in crate_auction and be able to return it so that
    /// it can be shared with bidders but we cannot do this at the
    /// moment. This is executed by the owner of the asset to be
    /// auctioned.
    public fun create_auction<T: key + store >(to_sell: T, id: VersionedID, auctioneer: address, ctx: &mut TxContext) {
        // A question one might asked is how do we know that to_sell
        // is owned by the caller of this entry function and the
        // answer is that it's checked by the runtime.
        let auction = Auction<T> {
            id: id,
            to_sell: to_sell,
            funds: Option::none(),
            // set highest_bidder to the owner so that we can return
            // the item if no one bids on it
            highest_bidder: TxContext::sender(ctx),
        };
        Transfer::transfer(auction, auctioneer);
    }

    /// Creates a bid a and send it to the auctioneer along with the
    /// ID of the auction. This is executed by a bidder.
    public fun bid(coin: Coin<GAS>, auction_id: ID, auctioneer: address, ctx: &mut TxContext) {
        let bid = Bid {
            id: TxContext::new_id(ctx),
            bidder: TxContext::sender(ctx),
            auction_id: auction_id,
            coin: coin,
        };
        Transfer::transfer(bid, auctioneer);
    }

    /// Updates the auction based on the information in the bid
    /// (update auction if higher bid received and send coin back for
    /// bids that are too low). This is executed by the auctioneer.
    public fun update_auction<T: key + store>(auction: &mut Auction<T>, bid: Bid, _ctx: &mut TxContext) {
        let Bid { id, bidder, auction_id, coin } = bid;
        ID::delete(id);

        assert!(ID::inner(&auction.id) == &auction_id, EWRONG_AUCTION);
        if (Option::is_none(&auction.funds)) {
            // first bid
            Option::fill(&mut auction.funds, coin);
            auction.highest_bidder = bidder;
        } else {
            let prev_funds_value = Coin::value(Option::borrow(&mut auction.funds));
            if (Coin::value(&coin) > prev_funds_value) {
                // a bid higher than currently highest bid received

                // update auction to reflect highest bid
                let prev_funds = Option::swap(&mut auction.funds, coin);
                // transfer previously highest bid to its bidder
                Coin::transfer(prev_funds, auction.highest_bidder);
                // update auction to reflect the new bidder
                auction.highest_bidder = bidder;
            } else {
                // a bid is too low - return funds to the bidder
                Coin::transfer(coin, bidder);
            }
        }
    }

    /// Ends the auction - transfers item to the currently highest
    /// bidder or to the original owner if no bids have been placed.
    public fun end_auction<T: key + store>(auction: Auction<T>, _ctx: &mut TxContext) {
        let Auction { id, to_sell, funds, highest_bidder } = auction;
        ID::delete(id);

        if (Option::is_some<Coin<GAS>>(&funds)) {
            // bids have been placed - send the funds to the item owner
            let prev_funds = Option::extract(&mut funds);
            Transfer::transfer(prev_funds, highest_bidder);
        };
        // there are no funds left regardless of the result, but the
        // funds value still needs to be destroyed
        Option::destroy_none(funds);

        // send item back to the highest bidder or to the original
        // owner (highest_bidder can represent both depending on
        // whether there were any bids or not)
        Transfer::transfer(to_sell, highest_bidder);
    }
}
