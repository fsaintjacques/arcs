import { defineConfig } from 'vitest/config';

export default defineConfig({
  test: {
    // Search agents play whole games in some tests; the default 5s is tight.
    testTimeout: 60_000,
  },
});
