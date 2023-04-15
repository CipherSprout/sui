// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::natives::{
    get_nested_struct_field, get_object_id,
    object_runtime::{object_store::ObjectResult, ObjectRuntime},
    NativesCostTable,
};
use move_binary_format::errors::{PartialVMError, PartialVMResult};
use move_core_types::{
    account_address::AccountAddress,
    gas_algebra::InternalGas,
    language_storage::{StructTag, TypeTag},
    value::MoveTypeLayout,
    vm_status::StatusCode,
};
use move_vm_runtime::native_charge_gas_early_exit;
use move_vm_runtime::native_functions::NativeContext;
use move_vm_types::{
    loaded_data::runtime_types::Type,
    natives::function::NativeResult,
    pop_arg,
    values::{StructRef, Value},
};
use smallvec::smallvec;
use std::collections::VecDeque;
use sui_types::{base_types::MoveObjectType, dynamic_field::derive_dynamic_field_id};

const E_KEY_DOES_NOT_EXIST: u64 = 1;
const E_FIELD_TYPE_MISMATCH: u64 = 2;
const E_BCS_SERIALIZATION_FAILURE: u64 = 3;

macro_rules! get_or_fetch_object {
    ($context:ident, $ty_args:ident, $parent:ident, $child_id:ident, $ty_cost_per_byte:expr) => {{
        let child_ty = $ty_args.pop().unwrap();
        native_charge_gas_early_exit!(
            $context,
            $ty_cost_per_byte * u64::from(child_ty.size()).into()
        );

        assert!($ty_args.is_empty());
        let (layout, tag) = match get_tag_and_layout($context, &child_ty)? {
            Some(res) => res,
            None => {
                return Ok(NativeResult::err(
                    $context.gas_used(),
                    E_BCS_SERIALIZATION_FAILURE,
                ))
            }
        };
        let object_runtime: &mut ObjectRuntime = $context.extensions_mut().get_mut();
        object_runtime.get_or_fetch_child_object(
            $parent,
            $child_id,
            &child_ty,
            layout,
            MoveObjectType::from(tag),
        )?
    }};
}

#[derive(Clone)]
pub struct DynamicFieldHashTypeAndKeyCostParams {
    pub dynamic_field_hash_type_and_key_cost_base: InternalGas,
    pub dynamic_field_hash_type_and_key_type_cost_per_byte: InternalGas,
    pub dynamic_field_hash_type_and_key_value_cost_per_byte: InternalGas,
    pub dynamic_field_hash_type_and_key_type_tag_cost_per_byte: InternalGas,
}

/***************************************************************************************************
 * native fun hash_type_and_key
 * Implementation of the Move native function `hash_type_and_key<K: copy + drop + store>(parent: address, k: K): address`
 *   gas cost: dynamic_field_hash_type_and_key_cost_base                            | covers various fixed costs in the oper
 *              + dynamic_field_hash_type_and_key_type_cost_per_byte * size_of(K)   | covers cost of operating on the type `K`
 *              + dynamic_field_hash_type_and_key_value_cost_per_byte * size_of(k)  | covers cost of operating on the value `k`
 *              + dynamic_field_hash_type_and_key_type_tag_cost_per_byte * size_of(type_tag(k))    | covers cost of operating on the type tag of `K`
 **************************************************************************************************/
pub fn hash_type_and_key(
    context: &mut NativeContext,
    mut ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    assert_eq!(ty_args.len(), 1);
    assert_eq!(args.len(), 2);

    let dynamic_field_hash_type_and_key_cost_params = context
        .extensions_mut()
        .get::<NativesCostTable>()
        .dynamic_field_hash_type_and_key_cost_params
        .clone();

    // Charge base fee
    native_charge_gas_early_exit!(
        context,
        dynamic_field_hash_type_and_key_cost_params.dynamic_field_hash_type_and_key_cost_base
    );

    let k_ty = ty_args.pop().unwrap();
    let k: Value = args.pop_back().unwrap();
    let parent = pop_arg!(args, AccountAddress);

    // Get size info for costing for derivations, serializations, etc
    let k_ty_size = u64::from(k_ty.size());
    let k_value_size = u64::from(k.legacy_size());
    native_charge_gas_early_exit!(
        context,
        dynamic_field_hash_type_and_key_cost_params
            .dynamic_field_hash_type_and_key_type_cost_per_byte
            * k_ty_size.into()
            + dynamic_field_hash_type_and_key_cost_params
                .dynamic_field_hash_type_and_key_value_cost_per_byte
                * k_value_size.into()
    );

    let k_tag = context.type_to_type_tag(&k_ty)?;
    let k_tag_size = u64::from(k_tag.abstract_size_for_gas_metering());

    native_charge_gas_early_exit!(
        context,
        dynamic_field_hash_type_and_key_cost_params
            .dynamic_field_hash_type_and_key_type_tag_cost_per_byte
            * k_tag_size.into()
    );

    let cost = context.gas_used();

    let k_layout = match context.type_to_type_layout(&k_ty) {
        Ok(Some(layout)) => layout,
        _ => return Ok(NativeResult::err(cost, E_BCS_SERIALIZATION_FAILURE)),
    };
    let Some(k_bytes) = k.simple_serialize(&k_layout) else {
        return Ok(NativeResult::err(
            cost,
            E_BCS_SERIALIZATION_FAILURE,
        ))
    };
    let Ok(id) = derive_dynamic_field_id(parent, &k_tag, &k_bytes) else {
        return Ok(NativeResult::err(
            cost,
            E_BCS_SERIALIZATION_FAILURE,
        ));
    };

    Ok(NativeResult::ok(cost, smallvec![Value::address(id.into())]))
}

#[derive(Clone)]
pub struct DynamicFieldAddChildObjectCostParams {
    pub dynamic_field_add_child_object_cost_base: InternalGas,
    pub dynamic_field_add_child_object_type_cost_per_byte: InternalGas,
    pub dynamic_field_add_child_object_value_cost_per_byte: InternalGas,
    pub dynamic_field_add_child_object_struct_tag_cost_per_byte: InternalGas,
}

/***************************************************************************************************
 * native fun add_child_object
 * throws `E_KEY_ALREADY_EXISTS` if a child already exists with that ID
 * Implementation of the Move native function `add_child_object<Child: key>(parent: address, child: Child)`
 *   gas cost: dynamic_field_add_child_object_cost_base                    | covers various fixed costs in the oper
 *              + dynamic_field_add_child_object_type_cost_per_byte * size_of(Child)        | covers cost of operating on the type `Child`
 *              + dynamic_field_add_child_object_value_cost_per_byte * size_of(child)       | covers cost of operating on the value `child`
 *              + dynamic_field_add_child_object_struct_tag_cost_per_byte * size_of(struct)tag(Child))  | covers cost of operating on the struct tag of `Child`
 **************************************************************************************************/
pub fn add_child_object(
    context: &mut NativeContext,
    mut ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    assert!(ty_args.len() == 1);
    assert!(args.len() == 2);

    let dynamic_field_add_child_object_cost_params = context
        .extensions_mut()
        .get::<NativesCostTable>()
        .dynamic_field_add_child_object_cost_params
        .clone();

    // Charge base fee
    native_charge_gas_early_exit!(
        context,
        dynamic_field_add_child_object_cost_params.dynamic_field_add_child_object_cost_base
    );

    let child = args.pop_back().unwrap();
    let parent = pop_arg!(args, AccountAddress).into();
    assert!(args.is_empty());

    let child_value_size = u64::from(child.legacy_size());
    // ID extraction step
    native_charge_gas_early_exit!(
        context,
        dynamic_field_add_child_object_cost_params
            .dynamic_field_add_child_object_value_cost_per_byte
            * child_value_size.into()
    );

    // TODO remove this copy_value, which will require VM changes
    let child_id = get_object_id(child.copy_value().unwrap())
        .unwrap()
        .value_as::<AccountAddress>()
        .unwrap()
        .into();
    let child_ty = ty_args.pop().unwrap();
    let child_type_size = u64::from(child_ty.size());

    native_charge_gas_early_exit!(
        context,
        dynamic_field_add_child_object_cost_params
            .dynamic_field_add_child_object_type_cost_per_byte
            * child_type_size.into()
    );

    assert!(ty_args.is_empty());
    let tag = match context.type_to_type_tag(&child_ty)? {
        TypeTag::Struct(s) => *s,
        _ => {
            return Err(
                PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                    .with_message("Sui verifier guarantees this is a struct".to_string()),
            )
        }
    };

    let struct_tag_size = u64::from(tag.abstract_size_for_gas_metering());
    native_charge_gas_early_exit!(
        context,
        dynamic_field_add_child_object_cost_params
            .dynamic_field_add_child_object_struct_tag_cost_per_byte
            * struct_tag_size.into()
    );

    let object_runtime: &mut ObjectRuntime = context.extensions_mut().get_mut();
    object_runtime.add_child_object(
        parent,
        child_id,
        &child_ty,
        MoveObjectType::from(tag),
        child,
    )?;
    Ok(NativeResult::ok(context.gas_used(), smallvec![]))
}

#[derive(Clone)]
pub struct DynamicFieldBorrowChildObjectCostParams {
    pub dynamic_field_borrow_child_object_cost_base: InternalGas,
    pub dynamic_field_borrow_child_object_child_ref_cost_per_byte: InternalGas,
    pub dynamic_field_borrow_child_object_type_cost_per_byte: InternalGas,
}

/***************************************************************************************************
 * native fun borrow_child_object
 * throws `E_KEY_DOES_NOT_EXIST` if a child does not exist with that ID at that type
 * or throws `E_FIELD_TYPE_MISMATCH` if the type does not match (as the runtime does not distinguish different reference types)
 * Implementation of the Move native function `borrow_child_object_mut<Child: key>(parent: &mut UID, id: address): &mut Child`
 *   gas cost: dynamic_field_borrow_child_object_cost_base                    | covers various fixed costs in the oper
 *              + dynamic_field_borrow_child_object_child_ref_cost_per_byte  * size_of(&Child)  | covers cost of fetching and returning `&Child`
 *              + dynamic_field_borrow_child_object_type_cost_per_byte  * size_of(Child)        | covers cost of operating on type `Child`
 **************************************************************************************************/
pub fn borrow_child_object(
    context: &mut NativeContext,
    mut ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    assert!(ty_args.len() == 1);
    assert!(args.len() == 2);

    let dynamic_field_borrow_child_object_cost_params = context
        .extensions_mut()
        .get::<NativesCostTable>()
        .dynamic_field_borrow_child_object_cost_params
        .clone();
    native_charge_gas_early_exit!(
        context,
        dynamic_field_borrow_child_object_cost_params.dynamic_field_borrow_child_object_cost_base
    );

    let child_id = pop_arg!(args, AccountAddress).into();

    let parent_uid = pop_arg!(args, StructRef).read_ref().unwrap();
    // UID { id: ID { bytes: address } }
    let parent = get_nested_struct_field(parent_uid, &[0, 0])
        .unwrap()
        .value_as::<AccountAddress>()
        .unwrap()
        .into();

    assert!(args.is_empty());
    let global_value_result = get_or_fetch_object!(
        context,
        ty_args,
        parent,
        child_id,
        dynamic_field_borrow_child_object_cost_params
            .dynamic_field_borrow_child_object_type_cost_per_byte
    );
    let global_value = match global_value_result {
        ObjectResult::MismatchedType => {
            return Ok(NativeResult::err(context.gas_used(), E_FIELD_TYPE_MISMATCH))
        }
        ObjectResult::Loaded(gv) => gv,
    };
    if !global_value.exists()? {
        return Ok(NativeResult::err(context.gas_used(), E_KEY_DOES_NOT_EXIST));
    }
    let child_ref = global_value.borrow_global().map_err(|err| {
        assert!(err.major_status() != StatusCode::MISSING_DATA);
        err
    })?;

    native_charge_gas_early_exit!(
        context,
        dynamic_field_borrow_child_object_cost_params
            .dynamic_field_borrow_child_object_child_ref_cost_per_byte
            * u64::from(child_ref.legacy_size()).into()
    );

    Ok(NativeResult::ok(context.gas_used(), smallvec![child_ref]))
}

#[derive(Clone)]
pub struct DynamicFieldRemoveChildObjectCostParams {
    pub dynamic_field_remove_child_object_cost_base: InternalGas,
    pub dynamic_field_remove_child_object_child_cost_per_byte: InternalGas,
    pub dynamic_field_remove_child_object_type_cost_per_byte: InternalGas,
}
/***************************************************************************************************
 * native fun remove_child_object
 * throws `E_KEY_DOES_NOT_EXIST` if a child does not exist with that ID at that type
 * or throws `E_FIELD_TYPE_MISMATCH` if the type does not match
 * Implementation of the Move native function `remove_child_object<Child: key>(parent: address, id: address): Child`
 *   gas cost: dynamic_field_remove_child_object_cost_base                    | covers various fixed costs in the oper
 *              + dynamic_field_remove_child_object_type_cost_per_byte * size_of(Child)      | covers cost of operating on type `Child`
 *              + dynamic_field_remove_child_object_child_cost_per_byte  * size_of(child)     | covers cost of fetching and returning value of type `Child`
 **************************************************************************************************/
pub fn remove_child_object(
    context: &mut NativeContext,
    mut ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    assert!(ty_args.len() == 1);
    assert!(args.len() == 2);

    let dynamic_field_remove_child_object_cost_params = context
        .extensions_mut()
        .get::<NativesCostTable>()
        .dynamic_field_remove_child_object_cost_params
        .clone();
    native_charge_gas_early_exit!(
        context,
        dynamic_field_remove_child_object_cost_params.dynamic_field_remove_child_object_cost_base
    );

    let child_id = pop_arg!(args, AccountAddress).into();
    let parent = pop_arg!(args, AccountAddress).into();
    assert!(args.is_empty());
    let global_value_result = get_or_fetch_object!(
        context,
        ty_args,
        parent,
        child_id,
        dynamic_field_remove_child_object_cost_params
            .dynamic_field_remove_child_object_type_cost_per_byte
    );
    let global_value = match global_value_result {
        ObjectResult::MismatchedType => {
            return Ok(NativeResult::err(context.gas_used(), E_FIELD_TYPE_MISMATCH))
        }
        ObjectResult::Loaded(gv) => gv,
    };
    if !global_value.exists()? {
        return Ok(NativeResult::err(context.gas_used(), E_KEY_DOES_NOT_EXIST));
    }
    let child = global_value.move_from().map_err(|err| {
        assert!(err.major_status() != StatusCode::MISSING_DATA);
        err
    })?;

    native_charge_gas_early_exit!(
        context,
        dynamic_field_remove_child_object_cost_params
            .dynamic_field_remove_child_object_child_cost_per_byte
            * u64::from(child.legacy_size()).into()
    );

    Ok(NativeResult::ok(context.gas_used(), smallvec![child]))
}

#[derive(Clone)]
pub struct DynamicFieldHasChildObjectCostParams {
    // All inputs are constant same size. No need for special costing as this is a lookup
    pub dynamic_field_has_child_object_cost_base: InternalGas,
}
/***************************************************************************************************
 * native fun has_child_object
 * Implementation of the Move native function `has_child_object(parent: address, id: address): bool`
 *   gas cost: dynamic_field_has_child_object_cost_base                    | covers various fixed costs in the oper
 **************************************************************************************************/
pub fn has_child_object(
    context: &mut NativeContext,
    ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    assert!(ty_args.is_empty());
    assert!(args.len() == 2);

    let dynamic_field_has_child_object_cost_params = context
        .extensions_mut()
        .get::<NativesCostTable>()
        .dynamic_field_has_child_object_cost_params
        .clone();
    native_charge_gas_early_exit!(
        context,
        dynamic_field_has_child_object_cost_params.dynamic_field_has_child_object_cost_base
    );

    let child_id = pop_arg!(args, AccountAddress).into();
    let parent = pop_arg!(args, AccountAddress).into();
    let object_runtime: &mut ObjectRuntime = context.extensions_mut().get_mut();
    let has_child = object_runtime.child_object_exists(parent, child_id)?;
    Ok(NativeResult::ok(
        context.gas_used(),
        smallvec![Value::bool(has_child)],
    ))
}

#[derive(Clone)]
pub struct DynamicFieldHasChildObjectWithTyCostParams {
    pub dynamic_field_has_child_object_with_ty_cost_base: InternalGas,
    pub dynamic_field_has_child_object_with_ty_type_cost_per_byte: InternalGas,
    pub dynamic_field_has_child_object_with_ty_type_tag_cost_per_byte: InternalGas,
}
/***************************************************************************************************
 * native fun has_child_object_with_ty
 * Implementation of the Move native function `has_child_object_with_ty<Child: key>(parent: address, id: address): bool`
 *   gas cost: dynamic_field_has_child_object_with_ty_cost_base               | covers various fixed costs in the oper
 *              + dynamic_field_has_child_object_with_ty_type_cost_per_byte * size_of(Child)        | covers cost of operating on type `Child`
 *              + dynamic_field_has_child_object_with_ty_type_tag_cost_per_byte * size_of(Child)    | covers cost of fetching and returning value of type tag for `Child`
 **************************************************************************************************/
pub fn has_child_object_with_ty(
    context: &mut NativeContext,
    mut ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    assert!(ty_args.len() == 1);
    assert!(args.len() == 2);

    let dynamic_field_has_child_object_with_ty_cost_params = context
        .extensions_mut()
        .get::<NativesCostTable>()
        .dynamic_field_has_child_object_with_ty_cost_params
        .clone();
    native_charge_gas_early_exit!(
        context,
        dynamic_field_has_child_object_with_ty_cost_params
            .dynamic_field_has_child_object_with_ty_cost_base
    );

    let child_id = pop_arg!(args, AccountAddress).into();
    let parent = pop_arg!(args, AccountAddress).into();
    assert!(args.is_empty());
    let ty = ty_args.pop().unwrap();

    native_charge_gas_early_exit!(
        context,
        dynamic_field_has_child_object_with_ty_cost_params
            .dynamic_field_has_child_object_with_ty_type_cost_per_byte
            * u64::from(ty.size()).into()
    );

    let tag: StructTag = match context.type_to_type_tag(&ty)? {
        TypeTag::Struct(s) => *s,
        _ => {
            return Err(
                PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                    .with_message("Sui verifier guarantees this is a struct".to_string()),
            )
        }
    };

    native_charge_gas_early_exit!(
        context,
        dynamic_field_has_child_object_with_ty_cost_params
            .dynamic_field_has_child_object_with_ty_type_tag_cost_per_byte
            * u64::from(tag.abstract_size_for_gas_metering()).into()
    );

    let object_runtime: &mut ObjectRuntime = context.extensions_mut().get_mut();
    let has_child = object_runtime.child_object_exists_and_has_type(
        parent,
        child_id,
        &MoveObjectType::from(tag),
    )?;
    Ok(NativeResult::ok(
        context.gas_used(),
        smallvec![Value::bool(has_child)],
    ))
}

fn get_tag_and_layout(
    context: &NativeContext,
    ty: &Type,
) -> PartialVMResult<Option<(MoveTypeLayout, StructTag)>> {
    let layout = match context.type_to_type_layout(ty)? {
        None => return Ok(None),
        Some(layout) => layout,
    };
    let tag = match context.type_to_type_tag(ty)? {
        TypeTag::Struct(s) => s,
        _ => {
            return Err(
                PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                    .with_message("Sui verifier guarantees this is a struct".to_string()),
            )
        }
    };
    Ok(Some((layout, *tag)))
}

#[test]
fn test_u64_dynamic_field_first_key() {
    let u64_tag = TypeTag::U64;
    let u64_tag_bytes = bcs::to_bytes(&u64_tag).unwrap();
    assert_eq!(u64_tag_bytes, [2]);

    let u64_value = Value::u64(0);
    let u64_value_bytes = u64_value.simple_serialize(&MoveTypeLayout::U64).unwrap();
    assert_eq!(u64_value_bytes, [0, 0, 0, 0, 0, 0, 0, 0]);

    let table_id =
        AccountAddress::from_hex_literal("0x03c8e4462dfb7deecabb5af3dc6e95a02619ebae").unwrap();

    let mut hasher = Sha3_256::default();
    hasher.update(table_id);
    hasher.update(u64_value_bytes);
    hasher.update(u64_tag_bytes);
    let hash = hasher.finalize();

    let first_key = ObjectID::try_from(&hash.as_ref()[0..ObjectID::LENGTH]).unwrap();
    assert_eq!(
        "0xb55d2a87319747315615cb05a62d1b59307c832e",
        first_key.to_string()
    );
}

#[test]
fn test_u64_dynamic_object_field_first_key() {
    use move_core_types::{identifier::Identifier, value::MoveStructLayout};
    use move_vm_types::values::Struct;
    use std::str::FromStr;

    let struct_tag = StructTag {
        address: AccountAddress::from_hex_literal("0x0000000000000000000000000000000000000002")
            .unwrap(),
        module: Identifier::from_str("dynamic_object_field").unwrap(),
        name: Identifier::from_str("Wrapper").unwrap(),
        type_params: vec![TypeTag::U64],
    };

    let wrapper_u64_tag = TypeTag::Struct(Box::new(struct_tag));
    let wrapper_u64_tag_bytes = bcs::to_bytes(&wrapper_u64_tag).unwrap();
    let expect = [
        7u8, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 2, 20, 100, 121, 110, 97,
        109, 105, 99, 95, 111, 98, 106, 101, 99, 116, 95, 102, 105, 101, 108, 100, 7, 87, 114, 97,
        112, 112, 101, 114, 1, 2,
    ]
    .to_vec();
    assert_eq!(expect, wrapper_u64_tag_bytes);

    let wrapper_u64_layout =
        MoveTypeLayout::Struct(MoveStructLayout::Runtime(vec![MoveTypeLayout::U64]));

    let wrapper_u64_value = Value::struct_(Struct::pack([Value::u64(0)]));
    let wrapper_u64_value_bytes = wrapper_u64_value
        .simple_serialize(&wrapper_u64_layout)
        .unwrap();
    assert_eq!(wrapper_u64_value_bytes, [0, 0, 0, 0, 0, 0, 0, 0]);

    let table_id =
        AccountAddress::from_hex_literal("0xa236bdcab2880a9c7d5ef9974796bd4126c52eef").unwrap();

    let mut hasher = Sha3_256::default();
    hasher.update(table_id);
    hasher.update(wrapper_u64_value_bytes);
    hasher.update(wrapper_u64_tag_bytes);
    let hash = hasher.finalize();

    let first_key = ObjectID::try_from(&hash.as_ref()[0..ObjectID::LENGTH]).unwrap();
    assert_eq!(
        "0xdf50efa50e58c86d8417095299c0c7cbec92deb7",
        first_key.to_string()
    );
}
