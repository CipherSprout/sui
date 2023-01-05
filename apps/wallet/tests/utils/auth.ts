import fs from 'fs';
import { BrowserContext, Page } from '@playwright/test';

export const PASSWORD = 'mystenlabs';

export async function createWallet(page: Page, extensionUrl: string) {
    await page.goto(extensionUrl);
    await page.getByRole('link', { name: /Get Started/ }).click();
    await page.getByRole('link', { name: /Create a New Wallet/ }).click();
    await page.getByLabel('Create Password').fill('mystenlabs');
    await page.getByLabel('Confirm Password').fill('mystenlabs');
    // TODO: Clicking checkbox should be improved:
    await page
        .locator('label', { has: page.locator('input[type=checkbox]') })
        .locator('span')
        .nth(0)
        .click();
    await page.getByRole('button', { name: /Create Wallet/ }).click();
    await page.getByRole('button', { name: /Open Sui Wallet/ }).click();
}

export async function restore(context: BrowserContext) {
    const authString = await fs.promises.readFile('./auth.json', 'utf-8');
    const [background] = context.serviceWorkers();
    await background.evaluate(
        async (authString) => {
            const auth = JSON.parse(authString);
            await chrome.storage.session.set(auth.session);
            await chrome.storage.local.set(auth.local);
        },
        [authString]
    );
}
