// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { BCS, TypeName } from '@mysten/bcs';
import { bcs } from '../types/sui-bcs';

export const ARGUMENT_INNER = 'Argument';
export const VECTOR = 'vector';
export const OPTION = 'Option<T>';
export const CALL_ARG = 'CallArg';
export const TYPE_TAG = 'TypeTag';
export const OBJECT_ARG = 'ObjectArg';
export const PROGRAMMABLE_TX = 'ProgrammableTransaction';
export const PROGRAMMABLE_CALL_INNER = 'ProgrammableMoveCall';
export const COMMAND_INNER = 'Command';

export const ENUM_KIND = 'EnumKind';

/** Wrapper around Command Enum to support `kind` matching in TS */
export const COMMAND: TypeName = [ENUM_KIND, COMMAND_INNER];
/** Wrapper around Argument Enum to support `kind` matching in TS */
export const ARGUMENT: TypeName = [ENUM_KIND, ARGUMENT_INNER];

/** Command types */

export type Option<T> = { some: T } | { none: true };

export const builder = new BCS(bcs)
  .registerEnumType(OPTION, {
    None: null,
    Some: 'T',
  })
  .registerStructType(PROGRAMMABLE_TX, {
    inputs: [VECTOR, CALL_ARG],
    commands: [VECTOR, COMMAND],
  })
  .registerEnumType(ARGUMENT_INNER, {
    GasCoin: null,
    Input: { index: BCS.U16 },
    Result: { index: BCS.U16 },
    NestedResult: { index: BCS.U16, resultIndex: BCS.U16 },
  })
  .registerStructType(PROGRAMMABLE_CALL_INNER, {
    package: BCS.ADDRESS,
    module: BCS.STRING,
    function: BCS.STRING,
    type_arguments: [VECTOR, TYPE_TAG],
    arguments: [VECTOR, ARGUMENT],
  })
  .registerEnumType(COMMAND_INNER, {
    /**
     * A Move Call - any public Move function can be called via
     * this Command. The results can be used that instant to pass
     * into the next Command.
     */
    MoveCall: PROGRAMMABLE_CALL_INNER,
    /**
     * Transfer vector of objects to a receiver.
     */
    TransferObjects: {
      objects: [VECTOR, ARGUMENT],
      address: ARGUMENT,
    },
    /**
     * Split `amount` from a `coin`.
     */
    SplitCoin: { coin: ARGUMENT, amount: ARGUMENT },
    /**
     * Merge Vector of Coins (`sources`) into a `destination`.
     */
    MergeCoins: { destination: ARGUMENT, sources: [VECTOR, ARGUMENT] },
    /**
     * Publish a Move module.
     */
    Publish: [VECTOR, [VECTOR, BCS.U8]],
    /**
     * Build a vector of objects using the input arguments.
     * It is impossible to construct a `vector<T: key>` otherwise,
     * so this call serves a utility function.
     */
    MakeMoveVec: {
      type: ['Option', TYPE_TAG],
      objects: [VECTOR, ARGUMENT],
    },
  });

/**
 * Wrapper around Enum, which transforms any `T` into an object with `kind` property:
 * @example
 * ```
 * let bcsEnum = { TransferObjects: { objects: [], address: ... } }
 * // becomes
 * let translatedEnum = { kind: 'TransferObjects', objects: [], address: ... };
 * ```
 */
builder.registerType(
  [ENUM_KIND, 'T'],
  function encodeCommand(
    this: BCS,
    writer,
    data: { kind: string },
    typeParams,
    typeMap,
  ) {
    const kind = data.kind;
    const invariant = { [kind]: data };
    const [enumType] = typeParams;

    return this.getTypeInterface(enumType as string)._encodeRaw.call(
      this,
      writer,
      invariant,
      typeParams,
      typeMap,
    );
  },
  function decodeCommand(this: BCS, reader, typeParams, typeMap) {
    const [enumType] = typeParams;
    const data = this.getTypeInterface(enumType as string)._decodeRaw.call(
      this,
      reader,
      typeParams,
      typeMap,
    );

    // enum invariant can only have one `key` field
    const kind = Object.keys(data)[0];
    return { kind, ...data[kind] };
  },
  (data: { kind: string }) => {
    if (typeof data !== 'object' && !('kind' in data)) {
      throw new Error(
        `EnumKind: Missing property "kind" in the input ${JSON.stringify(
          data,
        )}`,
      );
    }

    return true;
  },
);
