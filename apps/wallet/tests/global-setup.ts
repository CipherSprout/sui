// global-setup.ts
import fs from 'fs';
import { chromium, FullConfig } from '@playwright/test';
import { LAUNCH_ARGS } from './fixtures';
import { createWallet } from './utils/auth';

async function globalSetup(config: FullConfig) {
    const context = await chromium.launchPersistentContext('', {
        headless: false,
        args: LAUNCH_ARGS,
    });
    let [background] = context.serviceWorkers();
    if (!background) {
        background = await context.waitForEvent('serviceworker');
    }

    const extensionId = background.url().split('/')[2];
    const extensionUrl = `chrome-extension://${extensionId}/ui.html`;
    const page = await context.newPage();
    await createWallet(page, extensionUrl);
    const storage = await background.evaluate(async () => ({
        local: await chrome.storage.local.get(),
        session: await chrome.storage.session.get(),
    }));
    fs.promises.writeFile('./auth.json', JSON.stringify(storage));
    await context.close();
}

export default globalSetup;
