// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::connection::{Connection, CursorType, Edge};
use async_graphql::*;
use move_binary_format::access::ModuleAccess;
use move_binary_format::binary_views::BinaryIndexedView;
use move_disassembler::disassembler::Disassembler;
use move_ir_types::location::Loc;

use crate::context_data::db_data_provider::PgManager;
use crate::error::Error;
use sui_package_resolver::Module as ParsedMoveModule;

use super::cursor::{Cursor, Page};
use super::move_function::MoveFunction;
use super::move_struct::MoveStruct;
use super::{base64::Base64, move_package::MovePackage, sui_address::SuiAddress};

#[derive(Clone)]
pub(crate) struct MoveModule {
    pub storage_id: SuiAddress,
    pub native: Vec<u8>,
    pub parsed: ParsedMoveModule,
}

pub(crate) type CFriend = Cursor<usize>;
pub(crate) type CStruct = Cursor<String>;
pub(crate) type CFunction = Cursor<String>;

/// Represents a module in Move, a library that defines struct types
/// and functions that operate on these types.
#[Object]
impl MoveModule {
    /// The package that this Move module was defined in
    async fn package(&self, ctx: &Context<'_>) -> Result<MovePackage> {
        ctx.data_unchecked::<PgManager>()
            .fetch_move_package(self.storage_id, None)
            .await
            .extend()?
            .ok_or_else(|| {
                Error::Internal(format!(
                    "Cannot load package for module {}::{}",
                    self.storage_id,
                    self.parsed.name(),
                ))
            })
            .extend()
    }

    /// The module's (unqualified) name.
    async fn name(&self) -> &str {
        self.parsed.name()
    }

    /// Format version of this module's bytecode.
    async fn file_format_version(&self) -> u32 {
        self.parsed.bytecode().version
    }

    /// Modules that this module considers friends (these modules can access `public(friend)`
    /// functions from this module).
    async fn friends(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<CFriend>,
        last: Option<u64>,
        before: Option<CFriend>,
    ) -> Result<Connection<String, MoveModule>> {
        let page = Page::from_params(ctx.data_unchecked(), first, after, last, before)?;

        let bytecode = self.parsed.bytecode();
        let total = bytecode.friend_decls.len();

        // Add one to make [lo, hi) a half-open interval ((after, before) is an open interval).
        let mut lo = page.after().map_or(0, |a| *a + 1);
        let mut hi = page.before().map_or(total, |b| *b);

        let mut connection = Connection::new(false, false);
        if hi <= lo {
            return Ok(connection);
        } else if (hi - lo) > page.limit() {
            if page.is_from_front() {
                hi = lo + page.limit();
            } else {
                lo = hi - page.limit();
            }
        }

        connection.has_previous_page = 0 < lo;
        connection.has_next_page = hi < total;

        let runtime_id = *bytecode.self_id().address();
        let Some(package) = ctx
            .data_unchecked::<PgManager>()
            .fetch_move_package(self.storage_id, None)
            .await
            .extend()?
        else {
            return Err(Error::Internal(format!(
                "Failed to load package for module: {}",
                self.storage_id,
            ))
            .extend());
        };

        // Select `friend_decls[lo..hi]` using iterators to enumerate before taking a sub-sequence
        // from it, to get pairs `(i, friend_decls[i])`.
        for idx in lo..hi {
            let decl = &bytecode.friend_decls[idx];
            let friend_pkg = bytecode.address_identifier_at(decl.address);
            let friend_mod = bytecode.identifier_at(decl.name);

            if friend_pkg != &runtime_id {
                return Err(Error::Internal(format!(
                    "Friend module of {} from a different package: {}::{}",
                    runtime_id.to_canonical_display(/* with_prefix */ true),
                    friend_pkg.to_canonical_display(/* with_prefix */ true),
                    friend_mod,
                ))
                .extend());
            }

            let Some(friend) = package.module_impl(friend_mod.as_str()).extend()? else {
                return Err(Error::Internal(format!(
                    "Failed to load friend module of {}::{}: {}",
                    self.storage_id,
                    self.parsed.name(),
                    friend_mod,
                ))
                .extend());
            };

            let cursor = Cursor::new(idx).encode_cursor();
            connection.edges.push(Edge::new(cursor, friend));
        }

        Ok(connection)
    }

    /// Look-up the definition of a struct defined in this module, by its name.
    #[graphql(name = "struct")]
    async fn struct_(&self, name: String) -> Result<Option<MoveStruct>> {
        self.struct_impl(name).extend()
    }

    /// Iterate through the structs defined in this module.
    async fn structs(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<CStruct>,
        last: Option<u64>,
        before: Option<CStruct>,
    ) -> Result<Option<Connection<String, MoveStruct>>> {
        let page = Page::from_params(ctx.data_unchecked(), first, after, last, before)?;
        let after = page.after().map(String::as_str);
        let before = page.before().map(String::as_str);
        let struct_range = self.parsed.structs(after, before);

        let mut connection = Connection::new(false, false);
        let struct_names = if page.is_from_front() {
            struct_range.take(page.limit()).collect()
        } else {
            let mut names: Vec<_> = struct_range.rev().take(page.limit()).collect();
            names.reverse();
            names
        };

        connection.has_previous_page = struct_names
            .first()
            .is_some_and(|fst| self.parsed.structs(None, Some(fst)).next().is_some());

        connection.has_next_page = struct_names
            .last()
            .is_some_and(|lst| self.parsed.structs(Some(lst), None).next().is_some());

        for name in struct_names {
            let Some(struct_) = self.struct_impl(name.to_string()).extend()? else {
                return Err(Error::Internal(format!(
                    "Cannot deserialize struct {name} in module {}::{}",
                    self.storage_id,
                    self.parsed.name(),
                )))
                .extend();
            };

            let cursor = Cursor::new(name.to_string()).encode_cursor();
            connection.edges.push(Edge::new(cursor, struct_));
        }

        if connection.edges.is_empty() {
            Ok(None)
        } else {
            Ok(Some(connection))
        }
    }

    /// Look-up the signature of a function defined in this module, by its name.
    async fn function(&self, name: String) -> Result<Option<MoveFunction>> {
        self.function_impl(name).extend()
    }

    /// Iterate through the signatures of functions defined in this module.
    async fn functions(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<CFunction>,
        last: Option<u64>,
        before: Option<CFunction>,
    ) -> Result<Option<Connection<String, MoveFunction>>> {
        let page = Page::from_params(ctx.data_unchecked(), first, after, last, before)?;
        let after = page.after().map(String::as_str);
        let before = page.before().map(String::as_str);
        let function_range = self.parsed.functions(after, before);

        let mut connection = Connection::new(false, false);
        let function_names = if page.is_from_front() {
            function_range.take(page.limit()).collect()
        } else {
            let mut names: Vec<_> = function_range.rev().take(page.limit()).collect();
            names.reverse();
            names
        };

        connection.has_previous_page = function_names
            .first()
            .is_some_and(|fst| self.parsed.functions(None, Some(fst)).next().is_some());

        connection.has_next_page = function_names
            .last()
            .is_some_and(|lst| self.parsed.functions(Some(lst), None).next().is_some());

        for name in function_names {
            let Some(function) = self.function_impl(name.to_string()).extend()? else {
                return Err(Error::Internal(format!(
                    "Cannot deserialize function {name} in module {}::{}",
                    self.storage_id,
                    self.parsed.name(),
                )))
                .extend();
            };

            let cursor = Cursor::new(name.to_string()).encode_cursor();
            connection.edges.push(Edge::new(cursor, function));
        }

        if connection.edges.is_empty() {
            Ok(None)
        } else {
            Ok(Some(connection))
        }
    }

    /// The Base64 encoded bcs serialization of the module.
    async fn bytes(&self) -> Option<Base64> {
        Some(Base64::from(self.native.clone()))
    }

    /// Textual representation of the module's bytecode.
    async fn disassembly(&self) -> Result<Option<String>> {
        let view = BinaryIndexedView::Module(self.parsed.bytecode());
        Ok(Some(
            Disassembler::from_view(view, Loc::invalid())
                .map_err(|e| Error::Internal(format!("Error creating disassembler: {e}")))
                .extend()?
                .disassemble()
                .map_err(|e| Error::Internal(format!("Error creating disassembly: {e}")))
                .extend()?,
        ))
    }
}

impl MoveModule {
    fn struct_impl(&self, name: String) -> Result<Option<MoveStruct>, Error> {
        let def = match self.parsed.struct_def(&name) {
            Ok(Some(def)) => def,
            Ok(None) => return Ok(None),
            Err(e) => return Err(Error::Internal(e.to_string())),
        };

        Ok(Some(MoveStruct::new(
            self.parsed.name().to_string(),
            name,
            def,
        )))
    }

    pub(crate) fn function_impl(&self, name: String) -> Result<Option<MoveFunction>, Error> {
        let def = match self.parsed.function_def(&name) {
            Ok(Some(def)) => def,
            Ok(None) => return Ok(None),
            Err(e) => return Err(Error::Internal(e.to_string())),
        };

        Ok(Some(MoveFunction::new(
            self.storage_id,
            self.parsed.name().to_string(),
            name,
            def,
        )))
    }
}
