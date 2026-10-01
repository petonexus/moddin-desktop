import type { LocalizedText } from '../../i18n/backendIds'

export type ActionLogLevel = 'info' | 'success' | 'warning' | 'error'

/** One `list_action_logs` row as Rust writes it (serde camelCase). */
export interface ActionLogEntryWire {
  id: string
  timestamp: number
  level: ActionLogLevel
  /** The Tauri command name. UX-21 turns it into a sentence. */
  action: string
  gameId: string | null
  transactionId: string | null
  message: string
  details: Record<string, string>
}

/**
 * The row the panel renders. UX-21: `action` is a command name, and the
 * log row used to show it raw — `install_optiscaler`, twelve characters
 * of Rust, as the title of the thing that just happened to the user's
 * game. The service layer attaches the locale key; `text` stays as both
 * the documented fallback and the string the search box matches on, so
 * a user who remembers the command name still finds the row.
 */
export type ActionLogEntry = Omit<ActionLogEntryWire, 'action'> & {
  action: LocalizedText
}
