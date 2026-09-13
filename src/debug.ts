import { invoke as tauriInvoke } from '@tauri-apps/api/core'
import type { App } from 'vue'
import { persistActionLog, type PersistentActionLevel } from './services/activity-log'
import { hideDebugErrorPanel, showDebugErrorPanel } from './services/debug-panel'
import { readLocalValue, removeLocalValue, writeLocalValue } from './services/storage'

type DebugLevel = 'debug' | 'info' | 'warn' | 'error'

export interface DebugEntry {
  timestamp: string
  level: DebugLevel
  scope: string
  message: string
  data?: unknown
}

interface ModdinDebugApi {
  enable: () => void
  disable: () => void
  clear: () => void
  logs: () => DebugEntry[]
  showError: (title: string, details: string) => void
  hideError: () => void
}

declare global {
  interface Window {
    __MODDIN_DEBUG__?: ModdinDebugApi
    __MODDIN_BOOT_ERRORS__?: Array<{ title: string; details: string }>
  }
}

const MAX_LOG_ENTRIES = 300
const entries: DebugEntry[] = []

function isDebugEnabled() {
  return import.meta.env.DEV || readLocalValue('moddin-debug') === '1'
}

function errorDetails(error: unknown) {
  if (error instanceof Error) return error.stack || `${error.name}: ${error.message}`
  if (typeof error === 'string') return error

  try {
    return JSON.stringify(error, null, 2)
  } catch {
    return String(error)
  }
}

function printableData(data: unknown) {
  if (data === undefined) return undefined
  if (data instanceof Error) return errorDetails(data)
  return data
}

function writeLog(level: DebugLevel, scope: string, message: string, data?: unknown) {
  const entry: DebugEntry = {
    timestamp: new Date().toISOString(),
    level,
    scope,
    message,
    data: printableData(data),
  }

  entries.push(entry)
  if (entries.length > MAX_LOG_ENTRIES) entries.shift()

  const prefix = `[Moddin][${scope}] ${message}`
  if (level === 'error') console.error(prefix, data)
  else if (level === 'warn') console.warn(prefix, data)
  else if (isDebugEnabled()) console[level](prefix, data ?? '')
}

export const debug = {
  log: (scope: string, message: string, data?: unknown) => writeLog('debug', scope, message, data),
  info: (scope: string, message: string, data?: unknown) => writeLog('info', scope, message, data),
  warn: (scope: string, message: string, data?: unknown) => writeLog('warn', scope, message, data),
  error: (scope: string, message: string, data?: unknown) => writeLog('error', scope, message, data),
}

async function persistActionSafely(
  level: PersistentActionLevel,
  command: string,
  durationMs: number,
  args?: Record<string, unknown>,
  result?: unknown,
  error?: unknown,
) {
  try {
    await persistActionLog(level, command, durationMs, args, result, error)
  } catch (logError) {
    // Persistent diagnostics must never turn a successful game/mod action into a failure.
    debug.warn('activity', 'Persistent action log could not be written', logError)
  }
}

export async function invokeDebug<T>(command: string, args?: Record<string, unknown>) {
  const startedAt = performance.now()
  debug.info('tauri.invoke', `${command} started`, args)

  try {
    const result = await tauriInvoke<T>(command, args)
    const durationMs = performance.now() - startedAt
    debug.info('tauri.invoke', `${command} completed in ${durationMs.toFixed(1)}ms`, result)
    await persistActionSafely('success', command, durationMs, args, result)
    return result
  } catch (error) {
    const durationMs = performance.now() - startedAt
    debug.error('tauri.invoke', `${command} failed after ${durationMs.toFixed(1)}ms`, error)
    await persistActionSafely('error', command, durationMs, args, undefined, error)
    throw error
  }
}

export function installDebugInstrumentation(app: App) {
  const report = (title: string, error: unknown) => {
    const details = errorDetails(error)
    debug.error('runtime', title, details)
    showDebugErrorPanel(title, details)
  }

  window.addEventListener('error', (event) => {
    report(event.message || 'Unhandled window error', event.error || event.message)
  })
  window.addEventListener('unhandledrejection', (event) => {
    report('Unhandled promise rejection', event.reason)
  })

  app.config.errorHandler = (error, _instance, info) => {
    report(`Vue error (${info})`, error)
  }

  window.__MODDIN_DEBUG__ = {
    enable() {
      writeLocalValue('moddin-debug', '1')
      debug.info('debug', 'Verbose logging enabled')
    },
    disable() {
      removeLocalValue('moddin-debug')
      debug.info('debug', 'Verbose logging disabled')
    },
    clear() {
      entries.splice(0, entries.length)
      hideDebugErrorPanel()
    },
    logs: () => [...entries],
    showError: showDebugErrorPanel,
    hideError: hideDebugErrorPanel,
  }

  for (const bootError of window.__MODDIN_BOOT_ERRORS__ ?? []) {
    report(bootError.title, bootError.details)
  }
  delete window.__MODDIN_BOOT_ERRORS__

  debug.info('app', 'Debug instrumentation installed', {
    development: import.meta.env.DEV,
    userAgent: navigator.userAgent,
  })
}
