import vue from '@vitejs/plugin-vue'
import { defineConfig } from 'vitest/config'

/**
 * Frontend test runner (ROADMAP F-12).
 *
 * Kept separate from `vite.config.ts` on purpose: the dev server config
 * (fixed port, watch ignores) is irrelevant to a test run, and the
 * project's build must not start depending on a devDependency that only
 * the test runner needs.
 *
 * jsdom is the environment for everything, not just the component tests:
 * the unsigned-consent gate persists to `localStorage` through
 * `src/services/storage.ts`, and testing it against Node's storage-less
 * global would test a different module than the app ships.
 */
export default defineConfig({
  plugins: [vue()],
  test: {
    environment: 'jsdom',
    // Only `src/**`. The test files live next to the code they cover so
    // the import paths read the same as the source's own.
    include: ['src/**/*.test.ts'],
    // Component tests mount real dialogs that add window listeners; a
    // leaked spy between files hides real failures.
    restoreMocks: true,
  },
})
