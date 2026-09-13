import { invoke as tauriInvoke } from '@tauri-apps/api/core'

export type PersistentActionLevel = 'info' | 'success' | 'warning' | 'error'

// Keep this list limited to user-visible operations that mutate state or launch
// a configured game. Preview/inspection commands intentionally remain ephemeral.
const PERSISTENT_ACTION_COMMANDS = new Set([
  'configure_obs_vr',
  'uninstall_obs_vr',
  'install_optiscaler',
  'uninstall_optiscaler',
  'install_ofxr',
  'uninstall_ofxr',
  'install_cheeky_foveated_dlss',
  'uninstall_cheeky_foveated_dlss',
  'install_uevr',
  'uninstall_uevr',
  'rollback_latest_module_transaction',
  'rollback_transaction',
  'launch_vr_game',
  'set_game_openxr_runtime',
  'set_system_openxr_runtime',
])

function objectValue(value: unknown): Record<string, unknown> | null {
  return value !== null && typeof value === 'object' && !Array.isArray(value)
    ? value as Record<string, unknown>
    : null
}

function compactErrorDetails(error: unknown) {
  if (error instanceof Error) return error.stack || `${error.name}: ${error.message}`
  if (typeof error === 'string') return error

  try {
    return JSON.stringify(error)
  } catch {
    return String(error)
  }
}

function actionGameId(args?: Record<string, unknown>, result?: unknown) {
  const request = objectValue(args?.request)
  const resultObject = objectValue(result)
  const transaction = objectValue(resultObject?.transaction)
  const candidates = [
    args?.gameId,
    request?.gameId,
    resultObject?.gameId,
    transaction?.gameId,
  ]
  return candidates.find((value): value is string => typeof value === 'string' && value.length > 0)
}

function actionTransactionId(result?: unknown) {
  const resultObject = objectValue(result)
  if (!resultObject) return undefined
  if (typeof resultObject.id === 'string') return resultObject.id
  const transaction = objectValue(resultObject.transaction)
  return typeof transaction?.id === 'string' ? transaction.id : undefined
}

export function shouldPersistAction(command: string) {
  return PERSISTENT_ACTION_COMMANDS.has(command)
}

export async function persistActionLog(
  level: PersistentActionLevel,
  command: string,
  durationMs: number,
  args?: Record<string, unknown>,
  result?: unknown,
  error?: unknown,
) {
  if (!shouldPersistAction(command)) return

  const details: Record<string, string> = {
    durationMs: durationMs.toFixed(1),
  }
  const resultObject = objectValue(result)
  if (typeof resultObject?.status === 'string') details.status = resultObject.status
  if (typeof resultObject?.processId === 'number') details.processId = String(resultObject.processId)

  const compactError = error
    ? compactErrorDetails(error).replace(/\s+/g, ' ').slice(0, 1_200)
    : undefined
  const message = compactError
    ? `${command} failed: ${compactError}`
    : `${command} completed successfully`

  await tauriInvoke('record_ui_action_log', {
    level,
    action: command,
    gameId: actionGameId(args, result) ?? null,
    transactionId: actionTransactionId(result) ?? null,
    message,
    details,
  })
}
