import { invokeDebug as invoke } from '../../debug'
import type { AiAgent, AgentId } from './types'

/**
 * Thin Tauri wrappers for the local-AI setup commands.
 *
 * The Tauri side accepts `resourceDir` so we can resolve the bundled
 * `node.exe` + `moddin-agent/src/mcp-server.mjs` runtime shipped with
 * Moddin. On Windows the resource dir is
 * `<install>/resources/`; in development it is the source tree.
 */

/** Return the bundled MCP runtime directory from the Rust side. */
export async function resolveResourceDir(): Promise<string> {
  // We rely on `@tauri-apps/api/path::resourceDir()` to find the
  // production install dir or fall back to the project dev tree.
  // The Rust command takes the resolved path as a string so this
  // stays in lockstep with how the rest of the app passes paths
  // (no `tauri::AppHandle` plumbing in the signature).
  const { resourceDir } = await import('@tauri-apps/api/path')
  return await resourceDir()
}

export function detectAgents(resourceDir: string): Promise<AiAgent[]> {
  return invoke<AiAgent[]>('detect_ai_assistants', { resourceDir })
}

export function setupAgent(
  agentId: AgentId,
  resourceDir: string,
): Promise<AiAgent> {
  return invoke<AiAgent>('setup_ai_assistant', { agentId, resourceDir })
}

export function removeAgent(
  agentId: AgentId,
  resourceDir: string,
): Promise<AiAgent> {
  return invoke<AiAgent>('remove_ai_assistant', { agentId, resourceDir })
}
