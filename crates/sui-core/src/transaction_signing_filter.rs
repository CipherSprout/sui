// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[cfg(test)]
#[path = "unit_tests/transaction_deny_tests.rs"]
mod transaction_deny_tests;

pub use sui_transaction_checks::deny::check_transaction_for_signing;
