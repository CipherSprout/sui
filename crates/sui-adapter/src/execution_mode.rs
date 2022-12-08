// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_binary_format::file_format::LocalIndex;
use move_core_types::{
    identifier::Identifier,
    language_storage::{ModuleId, TypeTag},
    resolver::MoveResolver,
};
use move_vm_runtime::session::{SerializedReturnValues, Session};
use move_vm_types::loaded_data::runtime_types::Type;
use sui_types::error::ExecutionError;

pub type TransactionIndex = usize;

pub trait ExecutionMode {
    /// the type of a single Move call execution
    type ExecutionResult;

    /// the gathered results from batched executions
    type ExecutionResults;

    /// Controls two things:
    /// - the calling of arbitrary Move functions
    /// - the ability to instantiate any Move function parameter with a Pure call arg.
    ///   In other words, you can instantiate any struct or object or other value with its BCS bytes.
    fn allow_arbitrary_function_calls() -> bool;

    fn make_result<S: MoveResolver>(
        session: &Session<S>,
        module_id: &ModuleId,
        function: &Identifier,
        type_arguments: &[TypeTag],
        return_values: &SerializedReturnValues,
    ) -> Result<Self::ExecutionResult, ExecutionError>;

    fn empty_results() -> Self::ExecutionResults;

    fn add_result(
        results: &mut Self::ExecutionResults,
        idx: TransactionIndex,
        result: Self::ExecutionResult,
    );
}

pub struct Normal;

impl ExecutionMode for Normal {
    type ExecutionResult = ();
    type ExecutionResults = ();

    fn allow_arbitrary_function_calls() -> bool {
        false
    }

    fn make_result<S: MoveResolver>(
        _session: &Session<S>,
        _module_id: &ModuleId,
        _function: &Identifier,
        _type_arguments: &[TypeTag],
        srv: &SerializedReturnValues,
    ) -> Result<Self::ExecutionResult, ExecutionError> {
        assert_invariant!(srv.return_values.is_empty(), "Return values must be empty");
        Ok(())
    }

    fn empty_results() -> Self::ExecutionResults {}

    fn add_result(_: &mut Self::ExecutionResults, _: TransactionIndex, _: Self::ExecutionResult) {}
}

/// WARNING! Using this mode will bypass all normal checks around Move entry functions! This
/// includes the various rules for function arguments, meaning any object can be created just from
/// BCS bytes!
pub struct DevInspect;

pub type ExecutionResult = (
    /*  mutable_reference_outputs */ Vec<(LocalIndex, Vec<u8>, TypeTag)>,
    /*  return_values */ Vec<(Vec<u8>, TypeTag)>,
);

impl ExecutionMode for DevInspect {
    type ExecutionResult = ExecutionResult;
    type ExecutionResults = Vec<(TransactionIndex, ExecutionResult)>;

    fn allow_arbitrary_function_calls() -> bool {
        true
    }

    fn make_result<S: MoveResolver>(
        session: &Session<S>,
        module_id: &ModuleId,
        function: &Identifier,
        type_arguments: &[TypeTag],
        srv: &SerializedReturnValues,
    ) -> Result<Self::ExecutionResult, ExecutionError> {
        let SerializedReturnValues {
            mutable_reference_outputs,
            return_values,
        } = srv;
        let loaded_function = match session.load_function(module_id, function, type_arguments) {
            Ok(loaded) => loaded,
            Err(_) => {
                return Err(ExecutionError::new_with_source(
                    sui_types::error::ExecutionErrorKind::InvariantViolation,
                    "The function should have been able to load, as it was already executed",
                ));
            }
        };
        let ty_args = &loaded_function.type_arguments;
        let mutable_reference_outputs = mutable_reference_outputs
            .iter()
            .map(|(i, bytes, _)| {
                let ty = subst_ty(&loaded_function.parameters[(*i as usize)], ty_args)?;
                let tag = type_to_type_tag(&ty);
                Ok((*i, bytes.clone(), tag))
            })
            .collect::<Result<Vec<_>, ExecutionError>>()?;
        let return_values = return_values
            .iter()
            .enumerate()
            .map(|(i, (bytes, _))| {
                let ty = subst_ty(&loaded_function.parameters[i], ty_args)?;
                Ok((bytes.clone(), type_to_type_tag(&ty)))
            })
            .collect::<Result<Vec<_>, ExecutionError>>()?;
        Ok((mutable_reference_outputs, return_values))
    }

    fn empty_results() -> Self::ExecutionResults {
        todo!()
    }

    fn add_result(
        results: &mut Self::ExecutionResults,
        idx: TransactionIndex,
        result: Self::ExecutionResult,
    ) {
        results.push((idx, result))
    }
}

fn subst_ty(ty: &Type, ty_args: &[Type]) -> Result<Type, ExecutionError> {
    ty.subst(ty_args).map_err(|_| {
        ExecutionError::new_with_source(
            sui_types::error::ExecutionErrorKind::InvariantViolation,
            "The type should subst, as the function was already executed",
        )
    })
}

fn type_to_type_tag(_ty: &Type) -> TypeTag {
    todo!()
}
