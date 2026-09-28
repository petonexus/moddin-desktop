/**
 * Types for the "Local AI" feature — wires a local AI tool (Cursor,
 * Claude Desktop, Codex) into Moddin via the bundled MCP server.
 *
 * Mirrors the Rust `src-tauri/src/ai_assistant_setup.rs` shapes so the
 * UI renders fields directly without reshaping.
 */

export type AgentId = 'cursor' | 'claudeDesktop' | 'codex'

export type ConnectionState =
  | 'notInstalled'
  | 'detectedNotConfigured'
  | 'configured'
  | 'configError'

export interface AiAgent {
  id: AgentId
  displayName: string
  state: ConnectionState
  configPath: string | null
  binaryPath: string | null
  detail: string | null
}
