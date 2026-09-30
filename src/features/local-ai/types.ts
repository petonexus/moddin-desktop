/**
 * Types for the "Local AI" feature — wires a local AI tool (Cursor,
 * Claude Desktop, Codex) into Moddin via the bundled MCP server.
 *
 * Mirrors the Rust `src-tauri/src/ai_assistant_setup.rs` shapes so the
 * UI renders fields directly without reshaping.
 */

import type { LocalizedText } from '../../i18n/backendIds'

export type AgentId = 'cursor' | 'claudeDesktop' | 'codex'

export type ConnectionState =
  | 'notInstalled'
  | 'detectedNotConfigured'
  | 'configured'
  | 'configError'

/** One agent row as `detect_ai_assistants` writes it (serde camelCase). */
export interface AiAgentWire {
  id: AgentId
  displayName: string
  state: ConnectionState
  configPath: string | null
  binaryPath: string | null
  /** UX-21: an English sentence the backend writes, or null. */
  detail: string | null
}

/**
 * What the panel renders. UX-21: `detail` is a whole English sentence
 * under the agent's badge — "AI tool installed at C:\… Click Connect to
 * register Moddin." — and the service layer attaches the locale key, or
 * nothing when the backend string is not a declared id.
 *
 * A read failure (`detect_moddin_entry` returning `Err`) keeps the
 * backend's message: it is an I/O error with a real path in it, not
 * prose. See the fallback note in `src/i18n/backendIds.ts`.
 */
export type AiAgent = Omit<AiAgentWire, 'detail'> & {
  detail: LocalizedText | null
}
