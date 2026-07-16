import { defineConfig } from '@playwright/test';

export default defineConfig({
  testDir: './tests',
  outputDir: './test-results',
  use: {
    baseURL: 'http://127.0.0.1:4173',
    viewport: { width: 440, height: 660 },
    deviceScaleFactor: 1,
    launchOptions: { args: ['--disable-gpu'] },
  },
  webServer: {
    command: 'npm run dev -- --host 127.0.0.1 --port 4173',
    url: 'http://127.0.0.1:4173',
    reuseExistingServer: false,
  },
});
