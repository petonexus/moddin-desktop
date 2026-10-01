/**
 * Transport shapes for the app self-updater (ROADMAP `F-04`).
 *
 * Mirrors `src-tauri/src/app_update.rs`. The status vocabulary is a
 * closed set because every one of these is a different thing to show the
 * user, and "something went wrong" is not one of them.
 */

/** Mirrors `crate::app_update::AppUpdateStatus`. */
export interface AppUpdateStatus {
  /**
   * `false` while `tauri.conf.json` still holds the pubkey placeholder.
   * The updater is wired; it cannot be trusted yet, and the panel says so
   * instead of offering a check that can only fail.
   */
  configured: boolean
  currentVersion: string
  /** Always `stable`. Printed so the channel is never implied. */
  channel: string
  detail: string | null
}

/** Mirrors `crate::app_update::CheckStatus`. */
export type AppUpdateCheckStatus =
  | 'up-to-date'
  | 'available'
  | 'declined'
  | 'unavailable'
  | 'not-configured'

/** Mirrors `crate::app_update::AppUpdateCheck`. */
export interface AppUpdateCheck {
  status: AppUpdateCheckStatus
  currentVersion: string
  /** The offered version, or the declined one so the panel can name it. */
  version: string | null
  /** Release notes, as published. Untranslated, because they are data. */
  notes: string | null
  publishedAt: string | null
  detail: string | null
}

/** Mirrors `crate::app_update::AppUpdateProgress`. */
export interface AppUpdateProgress {
  phase: 'downloading' | 'installing'
  downloaded: number
  total: number | null
}
