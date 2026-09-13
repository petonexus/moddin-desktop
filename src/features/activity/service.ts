import { invokeDebug as invoke } from '../../debug'
import type { ActionLogEntry } from './types'

export function listActionLogs(limit = 500) {
  return invoke<ActionLogEntry[]>('list_action_logs', { limit })
}

export function clearActionLogs() {
  return invoke<void>('clear_action_logs')
}
