import { defineConfig } from '@playwright/test';

export default defineConfig({
  testDir: '.',
  use: { baseURL: 'http://localhost:8088', browserName: 'chromium' },
  webServer: {
    command: 'python3 -m http.server 8088 --bind 127.0.0.1 --directory ../../lit-static',
    url: 'http://localhost:8088',
  },
});
