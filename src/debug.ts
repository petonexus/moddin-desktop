import { invoke as tauriInvoke } from '@tauri-apps/api/core'
import type { App } from 'vue'

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
  if (import.meta.env.DEV) return true

  try {
    return window.localStorage.getItem('moddin-debug') === '1'
  } catch {
    return false
  }
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

function showErrorPanel(title: string, details: string) {
  if (typeof document === 'undefined') return

  const panelId = 'moddin-debug-error'
  let panel = document.getElementById(panelId)
  if (!panel) {
    panel = document.createElement('section')
    panel.id = panelId
    panel.setAttribute('role', 'alert')
    panel.style.cssText = [
      'position:fixed',
      'inset:18px',
      'z-index:99999',
      'overflow:auto',
      'padding:22px',
      'border:1px solid #8f3d4d',
      'border-radius:12px',
      'color:#f4d8dc',
      'background:#1d1015',
      'font:13px/1.5 Consolas, monospace',
      'box-shadow:0 20px 80px rgba(0,0,0,.65)',
    ].join(';')
    document.body?.appendChild(panel)
  }

  panel.innerHTML = ''
  const heading = document.createElement('strong')
  heading.textContent = `Moddin debug: ${title}`
  const message = document.createElement('p')
  message.textContent = 'A interface encontrou um erro. Os detalhes abaixo também estão em window.__MODDIN_DEBUG__.logs().'
  const pre = document.createElement('pre')
  pre.style.cssText = 'white-space:pre-wrap;margin:16px 0 0;color:#ffdfe3'
  pre.textContent = details
  panel.append(heading, message, pre)
}

function hideErrorPanel() {
  document.getElementById('moddin-debug-error')?.remove()
}

export const debug = {
  log: (scope: string, message: string, data?: unknown) => writeLog('debug', scope, message, data),
  info: (scope: string, message: string, data?: unknown) => writeLog('info', scope, message, data),
  warn: (scope: string, message: string, data?: unknown) => writeLog('warn', scope, message, data),
  error: (scope: string, message: string, data?: unknown) => writeLog('error', scope, message, data),
}

export async function invokeDebug<T>(command: string, args?: Record<string, unknown>) {
  const startedAt = performance.now()
  debug.info('tauri.invoke', `${command} started`, args)

  try {
    const result = await tauriInvoke<T>(command, args)
    debug.info('tauri.invoke', `${command} completed in ${(performance.now() - startedAt).toFixed(1)}ms`, result)
    return result
  } catch (error) {
    debug.error('tauri.invoke', `${command} failed after ${(performance.now() - startedAt).toFixed(1)}ms`, error)
    throw error
  }
}

export function installDebugInstrumentation(app: App) {
  const report = (title: string, error: unknown) => {
    const details = errorDetails(error)
    debug.error('runtime', title, details)
    showErrorPanel(title, details)
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
      window.localStorage.setItem('moddin-debug', '1')
      debug.info('debug', 'Verbose logging enabled')
    },
    disable() {
      window.localStorage.removeItem('moddin-debug')
      debug.info('debug', 'Verbose logging disabled')
    },
    clear() {
      entries.splice(0, entries.length)
      hideErrorPanel()
    },
    logs: () => [...entries],
    showError: showErrorPanel,
    hideError: hideErrorPanel,
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
