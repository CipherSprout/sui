// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { describe, it, expect } from 'vitest';
import {
	messageWithIntent,
	IntentScope,
	AppId,
	IntentVersion,
} from '../../../src/cryptography/intent';

describe('Intent', () => {
	it('`messageWithIntent()` should combine intent with message correctly', async () => {
		const data = new Uint8Array([0, 0, 0, 5, 72, 101, 108, 108, 111]);
		const intentMessage = messageWithIntent(IntentScope.PersonalMessage, data);

		expect(intentMessage[0]).toEqual(IntentScope.PersonalMessage);
		expect(intentMessage[1]).toEqual(IntentVersion.V0);
		expect(intentMessage[2]).toEqual(AppId.Sui);

		const intent = new Uint8Array([IntentScope.PersonalMessage, IntentVersion.V0, AppId.Sui]);
		const expectedData = new Uint8Array(data.length + 3);
		expectedData.set(intent);
		expectedData.set(data, intent.length);

		expect(intentMessage).toEqual(expectedData);
	});
});
