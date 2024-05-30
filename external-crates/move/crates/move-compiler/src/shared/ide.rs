// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeSet;

use crate::{
    expansion::ast as E, naming::ast as N, parser::ast as P, shared::Name, typing::ast as T,
};

use move_ir_types::location::Loc;
use move_symbol_pool::Symbol;

//*************************************************************************************************
// Types
//*************************************************************************************************

#[derive(Debug, Clone, Default)]
pub struct IDEInfo {
    annotations: Vec<(Loc, IDEAnnotation)>,
}

#[derive(Debug, Clone)]
/// An individual IDE annotation.
pub enum IDEAnnotation {
    /// A macro call site.
    MacroCallInfo(Box<MacroCallInfo>),
    /// An expanded lambda site.
    ExpandedLambda,
    /// Autocomplete information.
    AutocompleteInfo(Box<AutocompleteInfo>),
    /// Match Missing Arm.
    MissingMatchArms(Box<MissingMatchArmsInfo>),
}

#[derive(Debug, Clone)]
pub struct MacroCallInfo {
    /// Module where the macro is defined
    pub module: E::ModuleIdent,
    /// Name of the macro function
    pub name: P::FunctionName,
    /// Optional method name if macro invoked as dot-call
    pub method_name: Option<Name>,
    /// Type params at macro's call site
    pub type_arguments: Vec<N::Type>,
    /// By-value args (at this point there should only be one, representing receiver arg)
    pub by_value_args: Vec<T::SequenceItem>,
}

#[derive(Debug, Clone)]
pub struct AutocompleteInfo {
    /// Methods that are valid autocompletes
    pub methods: BTreeSet<(E::ModuleIdent, P::FunctionName)>,
    /// Fields that are valid autocompletes (e.g., for a struct)
    /// TODO: possibly extend this with type information?
    pub fields: BTreeSet<Symbol>,
}

#[derive(Debug, Clone)]
pub struct MissingMatchArmsInfo {
    /// A vector of arm patterns that can be inserted to make the match complete.
    /// Note the span information on these is _wrong_ and must be recomputed after insertion.
    pub arms: Vec<PatternSuggestion>,
}

/// Suggested new entries for a pattern. Note that any location information points to the
/// definition site. As this is largely suggested text, it lacks location information.
#[derive(Debug, Clone)]
pub enum PatternSuggestion {
    Wildcard,
    Binder(Symbol),
    Value(E::Value_),
    UnpackPositionalStruct {
        module: E::ModuleIdent,
        name: P::DatatypeName,
        /// The number of wildcards to generate.
        field_count: usize,
    },
    UnpackNamedStruct {
        module: E::ModuleIdent,
        name: P::DatatypeName,
        /// The fields, in order, to generate
        fields: Vec<Symbol>,
    },
    /// A tag-style variant that takes no arguments
    UnpackEmptyVariant {
        module: E::ModuleIdent,
        enum_name: P::DatatypeName,
        variant_name: P::VariantName,
    },
    UnpackPositionalVariant {
        module: E::ModuleIdent,
        enum_name: P::DatatypeName,
        variant_name: P::VariantName,
        /// The number of wildcards to generate.
        field_count: usize,
    },
    UnpackNamedVariant {
        module: E::ModuleIdent,
        enum_name: P::DatatypeName,
        variant_name: P::VariantName,
        /// The fields, in order, to generate
        fields: Vec<Symbol>,
    },
}

//*************************************************************************************************
// Impls
//*************************************************************************************************

impl IDEInfo {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_ide_annotation(&mut self, loc: Loc, info: IDEAnnotation) {
        self.annotations.push((loc, info));
    }

    pub fn extend(&mut self, mut other: Self) {
        self.annotations.append(&mut other.annotations);
    }

    pub fn is_empty(&self) -> bool {
        self.annotations.is_empty()
    }

    pub fn iter(&self) -> std::slice::Iter<'_, (Loc, IDEAnnotation)> {
        self.annotations.iter()
    }

    pub fn iter_mut(&mut self) -> std::slice::IterMut<'_, (Loc, IDEAnnotation)> {
        self.annotations.iter_mut()
    }
}

impl IntoIterator for IDEInfo {
    type Item = (Loc, IDEAnnotation);
    type IntoIter = std::vec::IntoIter<Self::Item>;

    fn into_iter(self) -> Self::IntoIter {
        self.annotations.into_iter()
    }
}
