import { invokeDebug as invoke } from '../../debug'
import { actionText } from '../../i18n/backendIds'
import type { ActionLogEntry, ActionLogEntryWire } from './types'

/**
 * UX-21: the row's `action` is a Tauri command name, and it was the log
 * row's title. Mapping it here — not in the panel — is what keeps the
 * boundary one line wide: the panel asks for a sentence, and never
 * learns what the backend called the command.
 */
export async function listActionLogs(limit = 500) {
  const rows = await invoke<ActionLogEntryWire[]>('list_action_logs', { limit })
  return rows.map((row): ActionLogEntry => ({ ...row, action: actionText(row.action) }))
}

export function clearActionLogs() {
  return invoke<void>('clear_action_logs')
}
