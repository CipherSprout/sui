// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module sui::tx_contextTests {
    use sui::ID;
    use sui::tx_context;

    #[test]
    fun test_id_generation() {
        let ctx = tx_context::dummy();
        assert!(tx_context::get_ids_created(&ctx) == 0, 0);

        let id1 = tx_context::new_id(&mut ctx);
        let id2 = tx_context::new_id(&mut ctx);

        // new_id should always produce fresh ID's
        assert!(&id1 != &id2, 1);
        assert!(tx_context::get_ids_created(&ctx) == 2, 2);
        ID::delete(id1);
        ID::delete(id2);
    }

}
