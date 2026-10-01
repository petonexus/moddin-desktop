import { invokeDebug as invoke } from '../../debug'
import type { TransactionRecord } from '../../types/transaction'
import type { Snapshot } from './types'

/**
 * The four snapshot commands, behind the feature's native boundary.
 *
 * Snapshots are the only path back from several undos in a row, so the
 * first three of these are user-visible mutations and belong in the
 * persistent action log like every other one (see `PERSISTENT_ACTION_COMMANDS`
 * in `src/services/activity-log.ts` — `rollback_snapshot` is not in that
 * set yet).
 *
 * Tauri maps camelCase argument keys onto the snake_case command
 * parameters, so `gameId` reaches Rust's `game_id`.
 */

/** Take a named restore point of every change currently applied to a game. */
export function createSnapshot(name: string, gameId: string) {
  return invoke<Snapshot>('create_snapshot', { name: name.trim(), gameId })
}

/** Every stored snapshot, newest first. */
export function listSnapshots() {
  return invoke<Snapshot[]>('list_snapshots')
}

/**
 * Undo every transaction the snapshot captured, newest first, and hand
 * back the records that were restored. Already-undone transactions are
 * skipped, so re-running a snapshot finishes a partial rollback rather
 * than failing on it.
 */
export function rollbackSnapshot(id: string) {
  return invoke<TransactionRecord[]>('rollback_snapshot', { id })
}

/** Forget a restore point. The transactions it recorded are untouched. */
export async function deleteSnapshot(id: string) {
  await invoke<null>('delete_snapshot', { id })
}
