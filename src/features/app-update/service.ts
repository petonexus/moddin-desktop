import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { invokeDebug as invoke } from '../../debug'
import type { AppUpdateCheck, AppUpdateProgress, AppUpdateStatus } from './types'

/** Mirrors `crate::app_update::app_update_status`. */
export function readAppUpdateStatus() {
  return invoke<AppUpdateStatus>('app_update_status')
}

/**
 * Ask the endpoint what it has.
 *
 * `declinedVersion` is the last version the user turned down. The backend
 * is the one that decides not to offer it again, so the rule cannot be
 * skipped by a caller that forgets the argument.
 */
export function checkForAppUpdate(declinedVersion: string | null) {
  return invoke<AppUpdateCheck>('app_update_check', {
    request: { declinedVersion },
  })
}

/**
 * Download, verify and install the version the user confirmed.
 *
 * The backend refuses a release that no longer matches `version`, so the
 * artifact that lands on disk is the one that was named in the
 * confirmation. On Windows this promise does not resolve: the plugin
 * hands the verified installer to Windows and the installer restarts
 * Moddin. Every failure does resolve, as a rejected call.
 */
export function installAppUpdate(version: string) {
  return invoke<{ version: string }>('app_update_install', { request: { version } })
}

/**
 * Download progress, as the Rust side reports it.
 *
 * A failure to subscribe is not worth surfacing: the install is already
 * running and reports its own outcome, and a listener is not part of it.
 */
export async function onAppUpdateProgress(
  handler: (progress: AppUpdateProgress) => void,
): Promise<UnlistenFn> {
  try {
    return await listen<AppUpdateProgress>('app-update://progress', (event) => {
      handler(event.payload)
    })
  } catch {
    return () => {}
  }
}
