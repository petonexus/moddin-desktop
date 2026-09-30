import { beforeEach, describe, expect, it, vi } from 'vitest'
import { useProfiles } from '../useProfiles'
import {
  listTransactions,
  loadProfileContext,
  readProfileFile,
  revealProfileFile,
  writeProfileFile,
} from '../service'
import { installCapability } from '../../capability-modules/service'
import {
  PROFILE_KIND,
  PROFILE_SCHEMA_VERSION,
  type ProfileImportContext,
} from '../types'
import type { CapabilitySpec, CapabilitySummary } from '../../../types/capability'

/**
 * The import flow's contract: reading a file produces a preview and
 * nothing else, and applying walks exactly the plan the user read
 * through the install command the capability cards already use.
 *
 * Only the native boundary is mocked. The document schema, the secrets
 * rule and the plan are the real ones, so these tests would notice if
 * one of them changed underneath the flow.
 */

vi.mock('../service', () => ({
  listTransactions: vi.fn(),
  loadProfileContext: vi.fn(),
  readProfileFile: vi.fn(),
  revealProfileFile: vi.fn(),
  writeProfileFile: vi.fn(),
}))

vi.mock('../../capability-modules/service', () => ({
  installCapability: vi.fn(),
}))

const mockedList = vi.mocked(listTransactions)
const mockedContext = vi.mocked(loadProfileContext)
const mockedRead = vi.mocked(readProfileFile)
const mockedWrite = vi.mocked(writeProfileFile)
const mockedReveal = vi.mocked(revealProfileFile)
const mockedInstall = vi.mocked(installCapability)

const SPEC: CapabilitySpec = {
  id: 'reshade',
  displayName: 'ReShade',
  category: 'graphics',
  status: 'available',
  configSchema: [{ name: 'preset', type: 'string' }],
}

const SUMMARY: CapabilitySummary = {
  id: 'reshade',
  displayName: 'ReShade',
  category: 'graphics',
  status: 'available',
  origin: 'builtIn',
  supportedEngines: [],
  engineMatch: { verdict: 'engineAgnostic' },
}

function importContext(overrides: Partial<ProfileImportContext> = {}): ProfileImportContext {
  return {
    capabilities: new Map([['reshade', SUMMARY]]),
    specs: new Map([['reshade', SPEC]]),
    installedByGame: new Map(),
    targets: new Map([['elden-ring', {
      gameId: 'elden-ring',
      gameName: 'Elden Ring',
      installDir: 'C:\\games\\elden-ring',
      executableDir: 'C:\\games\\elden-ring\\Game',
      engine: 'Unreal Engine',
      engineVersion: 'UE5',
    }]]),
    revoked: new Map(),
    ...overrides,
  }
}

function documentJson(overrides: Record<string, unknown> = {}) {
  return JSON.stringify({
    kind: PROFILE_KIND,
    schemaVersion: PROFILE_SCHEMA_VERSION,
    appVersion: '0.1.0',
    exportedAt: '2026-01-31T10:00:00.000Z',
    games: [{
      gameId: 'elden-ring',
      gameName: 'Elden Ring',
      capabilities: [{
        id: 'reshade',
        displayName: 'ReShade',
        config: { values: { preset: 'ultra' } },
        omittedSecrets: [],
      }],
    }],
    ...overrides,
  })
}

/** A composable with the import path already filled in, as the panel leaves it. */
function withPath() {
  const profiles = useProfiles()
  profiles.importPath.value = 'C:\\inbox\\profile.json'
  return profiles
}

beforeEach(() => {
  mockedList.mockReset()
  mockedContext.mockReset()
  mockedRead.mockReset()
  mockedWrite.mockReset()
  mockedReveal.mockReset()
  mockedInstall.mockReset()
  mockedContext.mockResolvedValue(importContext())
  mockedRead.mockResolvedValue({ path: 'C:\\inbox\\profile.json', contents: documentJson() })
  mockedWrite.mockResolvedValue({ path: 'C:\\localappdata\\Moddin\\profiles\\exports\\p.json', bytes: 10 })
  mockedList.mockResolvedValue([])
})

describe('exporting a profile', () => {
  it('writes what the transaction log says is installed, with the values it was given', async () => {
    mockedList.mockResolvedValue([
      {
        id: 'tx-1',
        createdAt: 0,
        kind: 'reshade',
        label: 'ReShade',
        gameId: 'elden-ring',
        targetPath: 'C:\\games\\elden-ring\\dxgi.dll',
        backupPath: 'C:\\backup\\1',
        status: 'applied',
      },
      {
        // A legacy module install: no capability recipe exists for it,
        // so there is nothing an import could re-run.
        id: 'tx-2',
        createdAt: 0,
        kind: 'obs-vr',
        label: 'OBS VR Capture',
        gameId: 'elden-ring',
        targetPath: 'C:\\obs\\scenes.json',
        backupPath: 'C:\\backup\\2',
        status: 'applied',
      },
    ])
    const profiles = useProfiles({ configFor: { values: () => ({ preset: 'ultra' }) } })

    expect(await profiles.runExport()).toBe(true)

    const written = JSON.parse(mockedWrite.mock.calls[0][1])
    expect(mockedWrite.mock.calls[0][0]).toContain('.json')
    expect(written.kind).toBe('moddin-profile')
    expect(written.schemaVersion).toBe(PROFILE_SCHEMA_VERSION)
    // obs-vr is not in the registry, so it is left out entirely rather
    // than written as an entry no import could act on.
    expect(written.games).toHaveLength(1)
    expect(written.games[0].gameId).toBe('elden-ring')
    expect(written.games[0].capabilities).toEqual([
      { id: 'reshade', displayName: 'ReShade', config: { values: { preset: 'ultra' } }, omittedSecrets: [] },
    ])
  })

  it('says there is nothing to export instead of writing an empty file', async () => {
    const profiles = useProfiles()

    expect(await profiles.runExport()).toBe(false)
    expect(mockedWrite).not.toHaveBeenCalled()
    expect(profiles.notice.value).toEqual({ code: 'export-empty', detail: '' })
  })

  it('refuses a file name that is not a .json file before touching the disk', async () => {
    const profiles = useProfiles()
    mockedList.mockResolvedValue([
      {
        id: 'tx-1', createdAt: 0, kind: 'reshade', label: 'ReShade', gameId: 'elden-ring',
        targetPath: 'a', backupPath: 'b', status: 'applied',
      },
    ])
    profiles.fileName.value = 'profile.txt'

    expect(profiles.canExport.value).toBe(false)
    expect(await profiles.runExport()).toBe(false)
    expect(mockedWrite).not.toHaveBeenCalled()
  })

  it('opens the exported file in the file browser on request', async () => {
    mockedReveal.mockResolvedValue(null)
    mockedList.mockResolvedValue([
      {
        id: 'tx-1', createdAt: 0, kind: 'reshade', label: 'ReShade', gameId: 'elden-ring',
        targetPath: 'a', backupPath: 'b', status: 'applied',
      },
    ])
    const profiles = useProfiles()
    await profiles.runExport()

    await profiles.reveal()

    expect(mockedReveal).toHaveBeenCalledWith('C:\\localappdata\\Moddin\\profiles\\exports\\p.json')
  })
})

describe('reading a profile', () => {
  it('produces a preview and installs nothing', async () => {
    const profiles = withPath()

    expect(await profiles.readImport()).toBe(true)

    expect(profiles.preview.value?.applyCount).toBe(1)
    expect(profiles.loadedPath.value).toBe('C:\\inbox\\profile.json')
    expect(mockedInstall).not.toHaveBeenCalled()
  })

  it('refuses a newer format with the version named', async () => {
    mockedRead.mockResolvedValue({
      path: 'C:\\inbox\\future.json',
      contents: documentJson({ schemaVersion: PROFILE_SCHEMA_VERSION + 1 }),
    })
    const profiles = withPath()

    expect(await profiles.readImport()).toBe(false)

    expect(profiles.notice.value).toEqual({
      code: 'future-version',
      detail: String(PROFILE_SCHEMA_VERSION + 1),
    })
    expect(profiles.preview.value).toBeNull()
    expect(mockedInstall).not.toHaveBeenCalled()
  })

  it('reports a capability the build does not have, in the preview', async () => {
    mockedRead.mockResolvedValue({
      path: 'C:\\inbox\\profile.json',
      contents: documentJson({
        games: [{
          gameId: 'elden-ring',
          gameName: 'Elden Ring',
          capabilities: [{
            id: 'mod-that-was-removed',
            displayName: 'Mod That Was Removed',
            config: { values: {} },
            omittedSecrets: [],
          }],
        }],
      }),
    })
    const profiles = withPath()

    await profiles.readImport()

    expect(profiles.preview.value?.applyCount).toBe(0)
    expect(profiles.preview.value?.blockedCount).toBe(1)
    expect(profiles.canApply.value).toBe(false)
    expect(mockedInstall).not.toHaveBeenCalled()
  })

  it('changes nothing when the preview is discarded', async () => {
    const profiles = withPath()
    await profiles.readImport()

    profiles.cancelImport()

    expect(profiles.preview.value).toBeNull()
    expect(profiles.loadedPath.value).toBeNull()
    expect(mockedInstall).not.toHaveBeenCalled()
  })

  it('will not apply without a preview to apply', async () => {
    const profiles = withPath()

    expect(await profiles.applyPreview()).toBeNull()
    expect(mockedInstall).not.toHaveBeenCalled()
  })
})

describe('applying a preview', () => {
  it('installs through the capability install command, once per entry', async () => {
    mockedInstall.mockResolvedValue({
      capabilityId: 'reshade',
      transaction: null,
      steps: [],
      affectedPaths: [],
    })
    const onChanged = vi.fn()
    const profiles = withPath()
    await profiles.readImport()

    const result = await profiles.applyPreview(onChanged)

    expect(result?.installed).toEqual(['reshade'])
    expect(mockedInstall).toHaveBeenCalledTimes(1)
    expect(mockedInstall).toHaveBeenCalledWith({
      capabilityId: 'reshade',
      gameId: 'elden-ring',
      gameName: 'Elden Ring',
      installDir: 'C:\\games\\elden-ring',
      executableDir: 'C:\\games\\elden-ring\\Game',
      config: { values: { preset: 'ultra' } },
    })
    expect(onChanged).toHaveBeenCalledTimes(1)
  })

  it('stops at the first failure and says what already went in', async () => {
    mockedRead.mockResolvedValue({
      path: 'C:\\inbox\\profile.json',
      contents: documentJson({
        games: [{
          gameId: 'elden-ring',
          gameName: 'Elden Ring',
          capabilities: [
            { id: 'reshade', displayName: 'ReShade', config: { values: {} }, omittedSecrets: [] },
            { id: 'bepinex', displayName: 'BepInEx', config: { values: {} }, omittedSecrets: [] },
          ],
        }],
      }),
    })
    mockedContext.mockResolvedValue(importContext({
      capabilities: new Map([
        ['reshade', SUMMARY],
        ['bepinex', { ...SUMMARY, id: 'bepinex', displayName: 'BepInEx' }],
      ]),
      specs: new Map([
        ['reshade', SPEC],
        ['bepinex', { ...SPEC, id: 'bepinex', displayName: 'BepInEx' }],
      ]),
    }))
    mockedInstall
      .mockResolvedValueOnce({ capabilityId: 'reshade', transaction: null, steps: [], affectedPaths: [] })
      .mockRejectedValueOnce(new Error('game is running'))

    const profiles = withPath()
    await profiles.readImport()
    const result = await profiles.applyPreview()

    expect(result?.installed).toEqual(['reshade'])
    expect(result?.failed).toEqual([{ id: 'bepinex', message: 'game is running' }])
    // BepInEx was never reached, and the run did not keep going past it.
    expect(mockedInstall).toHaveBeenCalledTimes(2)
  })

  it('never reaches a capability the preview blocked', async () => {
    mockedRead.mockResolvedValue({
      path: 'C:\\inbox\\profile.json',
      contents: documentJson({
        games: [{
          gameId: 'elden-ring',
          gameName: 'Elden Ring',
          capabilities: [
            { id: 'mod-that-was-removed', displayName: 'Gone', config: { values: {} }, omittedSecrets: [] },
            { id: 'reshade', displayName: 'ReShade', config: { values: {} }, omittedSecrets: [] },
          ],
        }],
      }),
    })
    mockedInstall.mockResolvedValue({
      capabilityId: 'reshade',
      transaction: null,
      steps: [],
      affectedPaths: [],
    })
    const profiles = withPath()
    await profiles.readImport()

    await profiles.applyPreview()

    expect(mockedInstall).toHaveBeenCalledTimes(1)
    expect(mockedInstall.mock.calls[0][0].capabilityId).toBe('reshade')
  })

  it('refuses to apply a profile for a game that is not installed', async () => {
    mockedContext.mockResolvedValue(importContext({
      targets: new Map([['elden-ring', null]]),
    }))
    const profiles = withPath()
    await profiles.readImport()
    expect(profiles.canApply.value).toBe(false)

    expect(await profiles.applyPreview()).toBeNull()
    expect(mockedInstall).not.toHaveBeenCalled()
  })
})
