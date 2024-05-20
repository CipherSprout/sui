// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { afterAll, beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';

import { bcs } from '../../src/bcs';
import { Ed25519Keypair } from '../../src/keypairs/ed25519';
import { SerialTransactionBlockExecutor, TransactionBlock } from '../../src/transactions';
import { setup, TestToolbox } from './utils/setup';

let toolbox: TestToolbox;
beforeAll(async () => {
	toolbox = await setup();

	vi.spyOn(toolbox.client, 'multiGetObjects');
	vi.spyOn(toolbox.client, 'getCoins');
});

afterAll(() => {
	vi.restoreAllMocks();
});

describe('SerialExecutor', () => {
	beforeEach(() => {
		vi.clearAllMocks();
	});

	it('Executes multiple transactions using the same objects', async () => {
		const executor = new SerialTransactionBlockExecutor({
			client: toolbox.client,
			signer: toolbox.keypair,
		});
		const txb = new TransactionBlock();
		const [coin] = txb.splitCoins(txb.gas, [1]);
		txb.transferObjects([coin], toolbox.address());
		expect(toolbox.client.getCoins).toHaveBeenCalledTimes(0);

		const result = await executor.executeTransactionBlock(txb);

		const effects = bcs.TransactionEffects.fromBase64(result.effects);

		const newCoinId = effects.V2?.changedObjects.find(
			([_id, { outputState }], index) =>
				index !== effects.V2.gasObjectIndex && outputState.ObjectWrite,
		)?.[0]!;

		expect(toolbox.client.getCoins).toHaveBeenCalledTimes(1);

		const txb2 = new TransactionBlock();
		txb2.transferObjects([newCoinId], toolbox.address());
		const txb3 = new TransactionBlock();
		txb3.transferObjects([newCoinId], toolbox.address());
		const txb4 = new TransactionBlock();
		txb4.transferObjects([newCoinId], toolbox.address());

		const results = await Promise.all([
			executor.executeTransactionBlock(txb2),
			executor.executeTransactionBlock(txb3),
			executor.executeTransactionBlock(txb4),
		]);

		expect(results[0].digest).not.toEqual(results[1].digest);
		expect(results[1].digest).not.toEqual(results[2].digest);
		expect(toolbox.client.multiGetObjects).toHaveBeenCalledTimes(0);
		expect(toolbox.client.getCoins).toHaveBeenCalledTimes(1);
	});

	it('Resets cache on errors', async () => {
		const executor = new SerialTransactionBlockExecutor({
			client: toolbox.client,
			signer: toolbox.keypair,
		});
		const txb = new TransactionBlock();
		const [coin] = txb.splitCoins(txb.gas, [1]);
		txb.transferObjects([coin], toolbox.address());

		const result = await executor.executeTransactionBlock(txb);
		const effects = bcs.TransactionEffects.fromBase64(result.effects);

		const newCoinId = effects.V2?.changedObjects.find(
			([_id, { outputState }], index) =>
				index !== effects.V2.gasObjectIndex && outputState.ObjectWrite,
		)?.[0]!;

		expect(toolbox.client.getCoins).toHaveBeenCalledTimes(1);

		const txb2 = new TransactionBlock();
		txb2.transferObjects([newCoinId], toolbox.address());
		const txb3 = new TransactionBlock();
		txb3.transferObjects([newCoinId], new Ed25519Keypair().toSuiAddress());

		await toolbox.client.signAndExecuteTransactionBlock({
			signer: toolbox.keypair,
			transactionBlock: txb2,
		});

		await expect(() => executor.executeTransactionBlock(txb3)).rejects.toThrowError();

		// // Transaction should succeed after cache reset/error
		const result2 = await executor.executeTransactionBlock(txb3);

		expect(result2.digest).not.toEqual(result.digest);
		expect(bcs.TransactionEffects.fromBase64(result2.effects).V2?.status.Success).toEqual(true);
	});
});
