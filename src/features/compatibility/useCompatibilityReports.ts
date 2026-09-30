import { ref, watch, type Ref } from 'vue'
import { readLocalValue, writeLocalValue } from '../../services/storage'
import type { CompatibilityReport, CompatibilityStatus } from '../../types/compatibility'
import type { ToolModuleDefinition } from '../../types/game'
import { moduleVersion, type Translate } from '../modules/module-registry'

/**
 * Compatibility reports: what the player found out by trying the mod.
 *
 * Moddin can tell you whether a recipe *can* apply; it cannot tell you
 * whether the result is playable, so the answer is the player's. This
 * owns that answer — the vocabulary, the draft dialog and the storage —
 * so nothing else has to know that an unverified status is stored as an
 * absent key rather than as the word "unverified".
 *
 * A report is tied to the version it was recorded against. A recipe that
 * moves to a new version keeps the old note on disk and stops showing
 * it, because "works on 0.3.4" is not a claim about 0.4.0.
 */

export const COMPATIBILITY_STATUSES: CompatibilityStatus[] = [
  'unverified',
  'experimental',
  'proven',
  'risky',
  'not_working',
]

const REPORTS_STORAGE_KEY = 'moddin-compatibility-reports'

export function isCompatibilityStatus(value: unknown): value is CompatibilityStatus {
  return typeof value === 'string' && COMPATIBILITY_STATUSES.includes(value as CompatibilityStatus)
}

export interface CompatibilityReportsHost {
  t: Translate
  /** Key this module's report is stored under, per game. */
  stateKey: (module: ToolModuleDefinition) => string
  /** The game the report is about, named for the dialog. */
  gameLabel: () => string
  formatDate: (timestamp: number) => string
  success: Ref<string | null>
}

export function useCompatibilityReports(host: CompatibilityReportsHost) {
  const reports = ref<Record<string, CompatibilityReport>>({})
  const dialog = ref<{ module: ToolModuleDefinition; gameName: string; version: string } | null>(null)
  const draftStatus = ref<CompatibilityStatus>('unverified')
  const draftNote = ref('')

  /** What the catalog declares, when the player has recorded nothing. */
  function declaredStatus(module: ToolModuleDefinition): CompatibilityStatus {
    const configured = module.config?.compatibilityStatus
    return isCompatibilityStatus(configured) ? configured : 'unverified'
  }

  /** The recorded report, but only while it still matches the recipe. */
  function reportFor(module: ToolModuleDefinition) {
    const report = reports.value[host.stateKey(module)]
    return report?.testedVersion === moduleVersion(module) ? report : undefined
  }

  function statusFor(module: ToolModuleDefinition): CompatibilityStatus {
    return reportFor(module)?.status ?? declaredStatus(module)
  }

  function statusLabel(status: CompatibilityStatus) {
    if (status === 'experimental') return host.t('compatibilityExperimental')
    if (status === 'proven') return host.t('compatibilityProven')
    if (status === 'risky') return host.t('compatibilityRisky')
    if (status === 'not_working') return host.t('compatibilityNotWorking')
    return host.t('compatibilityUnverified')
  }

  /** Open the record dialog, pre-filled with whatever is known today. */
  function openRecord(module: ToolModuleDefinition) {
    const report = reportFor(module)
    draftStatus.value = report?.status ?? declaredStatus(module)
    draftNote.value = report?.note ?? ''
    dialog.value = {
      module,
      gameName: host.gameLabel(),
      version: moduleVersion(module),
    }
  }

  /**
   * Saving "unverified" deletes the report instead of storing it, so a
   * cleared note cannot keep claiming a result the player just withdrew.
   */
  function save() {
    const open = dialog.value
    if (!open) return

    const key = host.stateKey(open.module)
    const nextReports = { ...reports.value }
    if (draftStatus.value === 'unverified') {
      delete nextReports[key]
    } else {
      nextReports[key] = {
        status: draftStatus.value,
        note: draftNote.value.trim(),
        testedVersion: open.version,
        updatedAt: Date.now(),
      }
    }
    reports.value = nextReports
    dialog.value = null
    host.success.value = host.t('compatibilityTestSaved')
  }

  /**
   * Read what is on disk, keeping only whole reports. Storage can be
   * unavailable, private-mode restricted or edited by hand, and a half a
   * report is worse than none: it would render as a claim nobody made.
   */
  function load() {
    const raw = readLocalValue(REPORTS_STORAGE_KEY)
    if (!raw) return
    let parsed: Record<string, unknown>
    try {
      parsed = JSON.parse(raw) as Record<string, unknown>
    } catch {
      // Malformed storage is not worth an error the player can do nothing
      // about; the reports simply start empty.
      return
    }
    const validReports: Record<string, CompatibilityReport> = {}
    for (const [key, value] of Object.entries(parsed)) {
      if (!value || typeof value !== 'object') continue
      const report = value as Partial<CompatibilityReport>
      if (!isCompatibilityStatus(report.status)) continue
      if (typeof report.note !== 'string') continue
      if (typeof report.testedVersion !== 'string') continue
      if (typeof report.updatedAt !== 'number') continue
      validReports[key] = {
        status: report.status,
        note: report.note,
        testedVersion: report.testedVersion,
        updatedAt: report.updatedAt,
      }
    }
    reports.value = validReports
  }

  watch(reports, (value) => {
    writeLocalValue(REPORTS_STORAGE_KEY, JSON.stringify(value))
  }, { deep: true })

  return {
    reports,
    dialog,
    draftStatus,
    draftNote,
    reportFor,
    statusFor,
    statusLabel,
    openRecord,
    save,
    load,
  }
}
