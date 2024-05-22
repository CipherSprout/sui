// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module sui::vec_map_tests {
    use sui::vec_map::{Self, VecMap};

    #[test]
    #[expected_failure(abort_code = vec_map::EKeyAlreadyExists)]
    fun duplicate_key_abort() {
        let mut m = vec_map::empty();
        m.insert(1, true);
        m.insert(1, false);
    }

    #[test]
    #[expected_failure(abort_code = vec_map::EKeyDoesNotExist)]
    fun nonexistent_key_get() {
        let mut m = vec_map::empty();
        m.insert(1, true);
        let k = 2;
        let _v = &m[&k];
    }

    #[test]
    #[expected_failure(abort_code = vec_map::EKeyDoesNotExist)]
    fun nonexistent_key_get_idx_or_abort() {
        let mut m = vec_map::empty();
        m.insert(1, true);
        let k = 2;
        let _idx = m.get_idx(&k);
    }

    #[test]
    #[expected_failure(abort_code = vec_map::EIndexOutOfBounds)]
    fun out_of_bounds_get_entry_by_idx() {
        let mut m = vec_map::empty();
        m.insert(1, true);
        let idx = 1;
        let (_key, _val) = m.get_entry_by_idx(idx);
    }

    #[test]
    #[expected_failure(abort_code = vec_map::EIndexOutOfBounds)]
    fun out_of_bounds_remove_entry_by_idx() {
        let mut m = vec_map::empty();
        m.insert(10, true);
        let idx = 1;
        let (_key, _val) = m.remove_entry_by_idx(idx);
    }

    #[test]
    fun remove_entry_by_idx() {
        let mut m = vec_map::empty();
        m.insert(5, 50);
        m.insert(6, 60);
        m.insert(7, 70);

        let (key, val) = m.remove_entry_by_idx(0);
        assert!(key == 5 && val == 50);
        assert!(m.size() == 2);

        let (key, val) = m.remove_entry_by_idx(1);
        assert!(key == 7 && val == 70);
        assert!(m.size() == 1);
    }

    #[test]
    #[expected_failure(abort_code = vec_map::EMapNotEmpty)]
    fun destroy_non_empty() {
        let mut m = vec_map::empty();
        m.insert(1, true);
        m.destroy_empty()
    }

    #[test]
    fun destroy_empty() {
        let m: VecMap<u64, u64> = vec_map::empty();
        assert!(m.is_empty());
        m.destroy_empty()
    }

    #[test]
    fun smoke() {
        let mut m = vec_map::empty();
        let mut i = 0;
        while (i < 10) {
            let k = i + 2;
            let v = i + 5;
            m.insert(k, v);
            i = i + 1;
        };
        assert!(!m.is_empty());
        assert!(vec_map::size(&m) == 10);
        let mut i = 0;
        // make sure the elements are as expected in all of the getter APIs we expose
        while (i < 10) {
            let k = i + 2;
            assert!(m.contains(&k));
            let v = m[&k];
            assert!(v == i + 5);
            assert!(m.get_idx(&k) == i);
            let (other_k, other_v) = m.get_entry_by_idx(i);
            assert!(*other_k == k);
            assert!(*other_v == v);
            i = i + 1;
        };
        // remove all the elements
        let (keys, values) = (copy m).into_keys_values();
        let mut i = 0;
        while (i < 10) {
            let k = i + 2;
            let (other_k, v) = m.remove(&k);
            assert!(k == other_k);
            assert!(v == i + 5);
            assert!(keys[i] == k);
            assert!(values[i] == v);

            i = i + 1;
        }
    }

    #[test]
    fun return_list_of_keys() {
        let mut m = vec_map::empty();

        assert!(m.keys() == vector[]);

        m.insert(1, true);
        m.insert(5, false);

        assert!(m.keys() == vector[1, 5]);
    }
}
