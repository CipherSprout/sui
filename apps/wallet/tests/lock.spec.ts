// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { expect, test } from './fixtures';
import { createWallet, restore } from './utils/auth';

test.only('wallet unlock', async ({ page, context, extensionUrl }) => {
    await restore(context);
    await createWallet(page, extensionUrl);
    await page.getByTestId('menu').click();
    await page.getByRole('button', { name: /Lock Wallet/ }).click();
    await page.getByLabel('Enter Password').fill('mystenlabs');
    await page.getByRole('button', { name: /Unlock Wallet/ }).click();
    await expect(page.getByTestId('coin-page')).toBeVisible();
});
