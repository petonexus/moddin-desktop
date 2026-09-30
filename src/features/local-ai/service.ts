import { invokeDebug as invoke } from '../../debug'
import { agentDetailText } from '../../i18n/backendIds'
import type { AiAgent, AiAgentWire, AgentId } from './types'

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
  return invoke<AiAgentWire[]>('detect_ai_assistants', { resourceDir }).then((agents) =>
    agents.map(withDetailKey),
  )
}

export function setupAgent(
  agentId: AgentId,
  resourceDir: string,
): Promise<AiAgent> {
  return invoke<AiAgentWire>('setup_ai_assistant', { agentId, resourceDir }).then(withDetailKey)
}

export function removeAgent(
  agentId: AgentId,
  resourceDir: string,
): Promise<AiAgent> {
  return invoke<AiAgentWire>('remove_ai_assistant', { agentId, resourceDir }).then(withDetailKey)
}

/**
 * UX-21: the one line under the agent's badge is a whole English
 * sentence, and it is the sentence that tells the user what to do next.
 * Every command that returns an agent runs it through the boundary, so
 * none of the three can reintroduce the raw text.
 */
function withDetailKey(agent: AiAgentWire): AiAgent {
  return { ...agent, detail: agent.detail === null ? null : agentDetailText(agent.detail) }
}
