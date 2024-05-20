// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { afterAll, beforeAll, beforeEach, describe, expect, it, Mock, vi } from 'vitest';

import { SuiClient } from '../../src/client';
import { Ed25519Keypair } from '../../src/keypairs/ed25519';
import { ParallelExecutor, TransactionBlock } from '../../src/transactions';
import { setup, TestToolbox } from './utils/setup';

let toolbox: TestToolbox;
beforeAll(async () => {
	toolbox = await setup();

	vi.spyOn(toolbox.client, 'multiGetObjects');
	vi.spyOn(toolbox.client, 'getCoins');
	vi.spyOn(toolbox.client, 'executeTransactionBlock');
});

afterAll(() => {
	vi.restoreAllMocks();
});

describe('ParallelExecutor', () => {
	beforeEach(() => {
		vi.clearAllMocks();
	});

	it('Executes multiple transactions in parallel', async () => {
		const executor = new ParallelExecutor({
			client: toolbox.client,
			signer: toolbox.keypair,
			maxPoolSize: 3,
			coinBatchSize: 2,
		});

		let concurrentRequests = 0;
		let maxConcurrentRequests = 0;
		let totalTransactions = 0;

		(toolbox.client.executeTransactionBlock as Mock).mockImplementation(async function (
			this: SuiClient,
			input,
		) {
			totalTransactions++;
			concurrentRequests++;
			maxConcurrentRequests = Math.max(maxConcurrentRequests, concurrentRequests);
			const promise = SuiClient.prototype.executeTransactionBlock.call(this, input);

			return promise.finally(() => {
				concurrentRequests--;
			});
		});

		const txbs = Array.from({ length: 10 }, () => {
			const txb = new TransactionBlock();
			txb.transferObjects([txb.splitCoins(txb.gas, [1])[0]], toolbox.address());
			return txb;
		});

		const results = await Promise.all(txbs.map((txb) => executor.executeTransactionBlock(txb)));

		expect(maxConcurrentRequests).toBe(3);
		// 10 + initial coin split + 1 refill to reach concurrency limit
		expect(totalTransactions).toBe(12);

		const digest = new Set(results.map((result) => result.digest));
		expect(digest.size).toBe(results.length);
	});

	it('handles gas coin transfers', async () => {
		const executor = new ParallelExecutor({
			client: toolbox.client,
			signer: toolbox.keypair,
			maxPoolSize: 3,
			coinBatchSize: 2,
		});

		let concurrentRequests = 0;
		let maxConcurrentRequests = 0;

		(toolbox.client.executeTransactionBlock as Mock).mockImplementation(async function (
			this: SuiClient,
			input,
		) {
			concurrentRequests++;
			maxConcurrentRequests = Math.max(maxConcurrentRequests, concurrentRequests);
			const promise = SuiClient.prototype.executeTransactionBlock.call(this, input);

			return promise.finally(() => {
				concurrentRequests--;
			});
		});

		const receiver = new Ed25519Keypair();

		const txbs = Array.from({ length: 10 }, () => {
			const txb = new TransactionBlock();
			txb.transferObjects([txb.gas], receiver.toSuiAddress());
			return txb;
		});

		const results = await Promise.all(txbs.map((txb) => executor.executeTransactionBlock(txb)));

		expect(maxConcurrentRequests).toBe(3);

		const digest = new Set(results.map((result) => result.digest));
		expect(digest.size).toBe(results.length);

		const returnFunds = new TransactionBlock();
		returnFunds.transferObjects([returnFunds.gas], toolbox.address());

		await toolbox.client.signAndExecuteTransactionBlock({
			transactionBlock: returnFunds,
			signer: receiver,
		});
	});

	it('handles errors', async () => {
		const executor = new ParallelExecutor({
			client: toolbox.client,
			signer: toolbox.keypair,
			maxPoolSize: 3,
			coinBatchSize: 2,
		});

		let concurrentRequests = 0;
		let maxConcurrentRequests = 0;

		(toolbox.client.executeTransactionBlock as Mock).mockImplementation(async function (
			this: SuiClient,
			input,
		) {
			concurrentRequests++;
			maxConcurrentRequests = Math.max(maxConcurrentRequests, concurrentRequests);
			const promise = SuiClient.prototype.executeTransactionBlock.call(this, input);

			return promise.finally(() => {
				concurrentRequests--;
			});
		});

		const txbs = Array.from({ length: 10 }, (_, i) => {
			const txb = new TransactionBlock();

			if (i % 2 === 0) {
				txb.transferObjects([txb.splitCoins(txb.gas, [1])[0]], toolbox.address());
			} else {
				txb.moveCall({
					target: '0x123::foo::bar',
					arguments: [],
				});
			}

			return txb;
		});

		const results = await Promise.allSettled(
			txbs.map((txb) => executor.executeTransactionBlock(txb)),
		);

		const failed = results.filter((result) => result.status === 'rejected');
		const succeeded = new Set(
			results
				.filter((result) => result.status === 'fulfilled')
				.map((r) => (r.status === 'fulfilled' ? r.value.digest : null)),
		);

		expect(failed.length).toBe(5);
		expect(succeeded.size).toBe(5);
	});
});
