// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

#[macro_use(sp)]
extern crate move_ir_types;

pub mod analyzer;
pub mod compiler_info;
pub mod completion;
pub mod context;
pub mod diagnostics;
pub mod info;
pub mod inlay_hints;
pub mod symbols;
pub mod utils;
pub mod vfs;
