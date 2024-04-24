// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { execSync } from 'child_process';
import * as fs from 'fs';
import path from 'path';
import type { TransactionBlock } from '@mysten/sui.js/transactions';
import { clone } from 'isomorphic-git';
import http from 'isomorphic-git/http/node';
import tmp from 'tmp';

import type { TestToolbox } from '../../kiosk/test/e2e/setup.js';
import type { Constants } from '../src/types.js';

const SUI_BIN =
	//@ts-ignore-next-line
	import.meta.env.VITE_SUI_BIN ?? path.resolve(`${__dirname}/../../../target/debug/sui`);

const SUINS_REPO = `https://github.com/MystenLabs/suins-contracts.git`;
const SUINS_REPO_BRANCH = `ml/easier-testing`;

/**
 * Creates a temp folder (only lives through a single test run)
 * and clones the `suins-contracts` repository (contracts + scripts).
 */
export async function cloneSuinsContracts(): Promise<string> {
	tmp.setGracefulCleanup();
	const tmpobj = tmp.dirSync({ unsafeCleanup: true });

	// get the repository.
	await clone({
		fs,
		http,
		dir: tmpobj.name,
		url: SUINS_REPO,
		ref: SUINS_REPO_BRANCH,
		singleBranch: true,
		noTags: true,
		depth: 1,
	});

	return tmpobj.name;
}

/**
 * Publishes the contracts and does the initial setups needed.
 * Returns the constants from the contracts.
 * */
export async function publishAndSetupSuinsContracts(
	toolbox: TestToolbox,
	contractsFolder: string,
): Promise<Constants> {
	// installs dependencies
	execSync(`cd ${contractsFolder}/scripts && pnpm i`);

	// publishes & sets-up the contracts on our localnet.
	execSync(`cd ${contractsFolder}/scripts && pnpm publish-and-setup`, {
		env: {
			...process.env,
			PRIVATE_KEY: toolbox.keypair.getSecretKey(),
			SUI_BINARY: SUI_BIN,
			NETWORK: 'localnet',
		},
	});

	console.log('SuiNS Contract published & set up successfully.');

	return JSON.parse(fs.readFileSync(`${contractsFolder}/scripts/constants.sdk.json`, 'utf8'));
}

export async function execute(toolbox: TestToolbox, transactionBlock: TransactionBlock) {
	return toolbox.client.signAndExecuteTransactionBlock({
		transactionBlock,
		signer: toolbox.keypair,
		options: {
			showEffects: true,
			showObjectChanges: true,
		},
	});
}
