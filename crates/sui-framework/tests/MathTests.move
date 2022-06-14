// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module sui::MathTests {
    use sui::Math;

    #[test]
    fun test_max() {
        assert!(Math::max(10, 100) == 100, 1);
        assert!(Math::max(100, 10) == 100, 2);
        assert!(Math::max(0, 0) == 0, 3);
    }

    #[test]
    fun test_min() {
        assert!(Math::min(10, 100) == 10, 1);
        assert!(Math::min(100, 10) == 10, 2);
        assert!(Math::min(0, 0) == 0, 3);
    }

    #[test]
    fun test_perfect_sqrt() {
        let i = 0;
        while (i < 1000) {
            assert!(Math::sqrt(i * i) == i, 1);
            i = i + 1;
        }
    }

    #[test]
    // This function tests whether the (square root)^2 equals the
    // initial value OR whether it is equal to the nearest lower
    // number that does.
    fun test_imperfect_sqrt() {
        let i = 1;
        let prev = 1;
        while (i <= 1000) {
            let root = Math::sqrt(i);

            assert!(i == root * root || root == prev, 0);

            prev = root;
            i = i + 1;
        }
    }

    #[test]
    fun test_sqrt_big_numbers() {
        let u64_max = 18446744073709551615;
        assert!(4294967295 == Math::sqrt(u64_max), 0)
    }
}
