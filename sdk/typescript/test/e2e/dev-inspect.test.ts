// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { describe, it, expect, beforeAll } from 'vitest';
import {
  getObjectId,
  getNewlyCreatedCoinRefsAfterSplit,
  RawSigner,
  Transaction,
  Commands,
  SUI_TYPE_ARG,
} from '../../src';
import {
  DEFAULT_GAS_BUDGET,
  DEFAULT_RECIPIENT,
  publishPackage,
  setup,
  TestToolbox,
} from './utils/setup';

describe('Test dev inspect', () => {
  let toolbox: TestToolbox;
  let signer: RawSigner;
  let packageId: string;

  beforeAll(async () => {
    toolbox = await setup();
    //const version = await toolbox.provider.getRpcApiVersion();
    signer = new RawSigner(toolbox.keypair, toolbox.provider);
    const packagePath = __dirname + '/./data/serializer';
    packageId = await publishPackage(signer, packagePath);
  });

  it('Dev inspect transaction with Pay', async () => {
    const tx = new Transaction();
    tx.setGasBudget(DEFAULT_GAS_BUDGET);
    const coin = tx.add(Commands.SplitCoin(tx.gas, tx.input(1)));
    tx.add(Commands.TransferObjects([coin], tx.input(toolbox.address())));
    const splitTxn = await signer.signAndExecuteTransaction(tx);
    const splitCoins = getNewlyCreatedCoinRefsAfterSplit(splitTxn)!.map((c) =>
      getObjectId(c),
    );

    // TODO: Migrate:
    // await validateDevInspectTransaction(
    //   signer,
    //   {
    //     kind: 'pay',
    //     data: {
    //       inputCoins: splitCoins,
    //       recipients: [DEFAULT_RECIPIENT],
    //       amounts: [4000],
    //       gasBudget: 10000,
    //     },
    //   },
    //   'success',
    // );
  });

  it('Move Call that returns struct', async () => {
    const coins = await toolbox.provider.getGasObjectsOwnedByAddress(
      toolbox.address(),
    );

    const tx = new Transaction();
    tx.setGasBudget(DEFAULT_GAS_BUDGET);
    tx.setGasPayment([coins[1]]);
    tx.add(
      Commands.MoveCall({
        target: `${packageId}::serializer_tests::return_struct`,
        typeArguments: ['0x2::coin::Coin<0x2::sui::SUI>'],
        arguments: [tx.input(coins[0].objectId)],
      }),
    );

    await validateDevInspectTransaction(signer, tx, 'success');
  });

  it('Move Call that aborts', async () => {
    const tx = new Transaction();
    tx.setGasBudget(DEFAULT_GAS_BUDGET);
    tx.add(
      Commands.MoveCall({
        target: `${packageId}::serializer_tests::test_abort`,
        typeArguments: [],
        arguments: [],
      }),
    );

    await validateDevInspectTransaction(signer, tx, 'failure');
  });
});

async function validateDevInspectTransaction(
  signer: RawSigner,
  txn: Transaction,
  status: 'success' | 'failure',
) {
  const result = await signer.devInspectTransaction(txn);
  console.log(result);
  expect(result.effects.status.status).toEqual(status);
}
