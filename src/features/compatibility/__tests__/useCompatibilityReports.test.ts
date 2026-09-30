import { beforeEach, describe, expect, it, vi } from 'vitest'
import { ref } from 'vue'
import type { CompatibilityReport } from '../../../types/compatibility'
import type { ToolModuleDefinition } from '../../../types/game'
import { useCompatibilityReports } from '../useCompatibilityReports'

/**
 * The player's own verdict on a mod, which is the one thing Moddin cannot
 * check for itself: whether the result is playable.
 *
 * What matters here is that a stored claim is a whole claim. A report
 * half-written by a browser that died mid-save, or edited by hand, must
 * not render as "proven" — and a report recorded against one version must
 * not keep speaking for the next one.
 */

const STORAGE_KEY = 'moddin-compatibility-reports'

function module(overrides: Partial<ToolModuleDefinition> = {}): ToolModuleDefinition {
  return {
    id: 'cheeky-foveated-dlss',
    name: 'Cheeky Foveated DLSS',
    description: 'Concentrates DLSS quality in the centre of the image.',
    category: 'graphics',
    status: 'available',
    config: { version: '0.3.4', compatibilityStatus: 'unverified' },
    ...overrides,
  }
}

function host(success = ref<string | null>(null)) {
  return {
    t: (key: string) => key,
    stateKey: (entry: ToolModuleDefinition) => `elden-ring:${entry.id}`,
    gameLabel: () => 'Elden Ring',
    formatDate: (timestamp: number) => `at ${timestamp}`,
    success,
  }
}

function storedReport(overrides: Partial<CompatibilityReport> = {}): CompatibilityReport {
  return {
    status: 'proven',
    note: '30 minutes, no artefacts.',
    testedVersion: '0.3.4',
    updatedAt: 1_700_000_000_000,
    ...overrides,
  }
}

beforeEach(() => {
  window.localStorage.clear()
})

describe('reading what is on disk', () => {
  it('starts empty when there is nothing stored', () => {
    const reports = useCompatibilityReports(host())

    reports.load()

    expect(reports.reports.value).toEqual({})
  })

  it('drops a report that is missing any part of itself', () => {
    // Storage can be unavailable, private-mode restricted or edited by
    // hand. Half a report would render as a claim nobody made.
    window.localStorage.setItem(STORAGE_KEY, JSON.stringify({
      'elden-ring:cheeky-foveated-dlss': storedReport(),
      'elden-ring:optiscaler': { status: 'proven' },
      'elden-ring:uevr': { ...storedReport(), status: 'probably' },
      'elden-ring:obs-vr': { ...storedReport(), updatedAt: 'yesterday' },
      'elden-ring:vr-launch': null,
    }))

    const reports = useCompatibilityReports(host())
    reports.load()

    expect(Object.keys(reports.reports.value)).toEqual(['elden-ring:cheeky-foveated-dlss'])
  })

  it('ignores storage it cannot read rather than failing to start', () => {
    window.localStorage.setItem(STORAGE_KEY, '{not json')
    const reports = useCompatibilityReports(host())

    expect(() => reports.load()).not.toThrow()
    expect(reports.reports.value).toEqual({})
  })
})

describe('what a report is allowed to claim', () => {
  it('only speaks for the version it was recorded against', () => {
    const reports = useCompatibilityReports(host())
    reports.load()
    reports.reports.value = {
      'elden-ring:cheeky-foveated-dlss': storedReport({ testedVersion: '0.3.3' }),
    }

    // The recipe moved to 0.3.4; "works on 0.3.3" is not a claim about it.
    expect(reports.reportFor(module())).toBeUndefined()
    expect(reports.statusFor(module())).toBe('unverified')
  })

  it('falls back to the status the recipe declares', () => {
    const reports = useCompatibilityReports(host())

    expect(reports.statusFor(module({ config: { compatibilityStatus: 'risky' } }))).toBe('risky')
    expect(reports.statusFor(module({ config: { compatibilityStatus: 'invented' } }))).toBe('unverified')
  })
})

describe('recording a result', () => {
  it('keeps what the player typed, against the version on the card', () => {
    const reports = useCompatibilityReports(host())
    reports.openRecord(module())
    reports.draftStatus.value = 'proven'
    reports.draftNote.value = '  30 minutes, no artefacts.  '

    reports.save()

    expect(reports.reports.value['elden-ring:cheeky-foveated-dlss']).toMatchObject({
      status: 'proven',
      note: '30 minutes, no artefacts.',
      testedVersion: '0.3.4',
    })
    expect(reports.dialog.value).toBeNull()
  })

  it('deletes the report when the player withdraws it', () => {
    // Storing "unverified" as a row would keep claiming a result the
    // player just took back.
    const reports = useCompatibilityReports(host())
    reports.reports.value = { 'elden-ring:cheeky-foveated-dlss': storedReport() }
    reports.openRecord(module())
    reports.draftStatus.value = 'unverified'

    reports.save()

    expect(reports.reports.value).toEqual({})
  })

  it('opens pre-filled with what is already known', () => {
    const reports = useCompatibilityReports(host())
    reports.reports.value = { 'elden-ring:cheeky-foveated-dlss': storedReport({ status: 'risky' }) }

    reports.openRecord(module())

    expect(reports.dialog.value).toMatchObject({ gameName: 'Elden Ring', version: '0.3.4' })
    expect(reports.draftStatus.value).toBe('risky')
    expect(reports.draftNote.value).toBe('30 minutes, no artefacts.')
  })

  it('says something when a result is saved', () => {
    const success = ref<string | null>(null)
    const reports = useCompatibilityReports(host(success))
    reports.openRecord(module())
    reports.draftStatus.value = 'proven'

    reports.save()

    expect(success.value).toBe('compatibilityTestSaved')
  })

  it('writes every change back to storage', async () => {
    const reports = useCompatibilityReports(host())
    reports.openRecord(module())
    reports.draftStatus.value = 'experimental'
    reports.save()

    // The watcher is what keeps a note alive across restarts; without
    // this the record only lives until the window closes.
    await vi.waitFor(() => {
      expect(JSON.parse(window.localStorage.getItem(STORAGE_KEY) ?? '{}'))
        .toHaveProperty('elden-ring:cheeky-foveated-dlss')
    })
  })
})
