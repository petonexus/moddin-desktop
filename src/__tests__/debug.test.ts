import { createApp } from 'vue'
import { afterEach, expect, it, vi } from 'vitest'
import { installDebugInstrumentation } from '../debug'
import { showDebugErrorPanel } from '../services/debug-panel'

vi.mock('../services/debug-panel', () => ({ showDebugErrorPanel: vi.fn(), hideDebugErrorPanel: vi.fn() }))
afterEach(() => { vi.unstubAllEnvs(); window.localStorage.clear() })

it('logs runtime errors in production and only opens the debug panel after explicit opt-in', () => {
  vi.stubEnv('DEV', false)
  vi.spyOn(window, 'addEventListener').mockImplementation(() => {})
  vi.spyOn(console, 'error').mockImplementation(() => {})
  window.localStorage.clear()
  const app = createApp({ render: () => null })
  installDebugInstrumentation(app)
  app.config.errorHandler?.(new Error('production error'), null, 'render')
  expect(showDebugErrorPanel).not.toHaveBeenCalled()
  expect(window.__MODDIN_DEBUG__?.logs()).toEqual(expect.arrayContaining([
    expect.objectContaining({ scope: 'runtime', level: 'error' }),
  ]))
  window.__MODDIN_DEBUG__?.enable()
  app.config.errorHandler?.(new Error('debug error'), null, 'render')
  expect(showDebugErrorPanel).toHaveBeenCalledOnce()
})
