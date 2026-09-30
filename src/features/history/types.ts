/**
 * One named point-in-time restore point.
 *
 * Mirrors Rust `Snapshot` in `src-tauri/src/transaction.rs`, which is
 * declared `#[serde(rename_all = "camelCase")]` — that is what sends
 * `transaction_ids` over the wire as `transactionIds`. `createdAt` is
 * milliseconds since the epoch, the same as a `TransactionRecord`'s, so
 * the same date formatter applies to both.
 *
 * `transactionIds` is the list of `applied` transactions the snapshot
 * captured when it was taken. Rolling it back replays that list in
 * reverse, which is why the count matters to the user: it is the number
 * of changes about to be undone.
 */
export interface Snapshot {
  id: string
  name: string
  createdAt: number
  gameId: string
  transactionIds: string[]
}
