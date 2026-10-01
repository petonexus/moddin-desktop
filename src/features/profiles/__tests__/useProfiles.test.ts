import { beforeEach, describe, expect, it, vi } from 'vitest'
import { useProfiles } from '../useProfiles'
import {
  listTransactions,
  loadProfileContext,
  pickProfileFile,
  readProfileFile,
  revealProfileFile,
  saveProfileFile,
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
  pickProfileFile: vi.fn(),
  readProfileFile: vi.fn(),
  revealProfileFile: vi.fn(),
  saveProfileFile: vi.fn(),
}))

vi.mock('../../capability-modules/service', () => ({
  installCapability: vi.fn(),
}))

const mockedList = vi.mocked(listTransactions)
const mockedContext = vi.mocked(loadProfileContext)
const mockedPick = vi.mocked(pickProfileFile)
const mockedRead = vi.mocked(readProfileFile)
const mockedSave = vi.mocked(saveProfileFile)
const mockedReveal = vi.mocked(revealProfileFile)
const mockedInstall = vi.mocked(installCapability)

const SPEC: CapabilitySpec = {
  id: 'reshade',
  displayName: 'ReShade',
  category: 'graphics',
  status: 'available',
  configSchema: [
    { name: 'preset', type: 'string' },
    // A `path` field is the one the secrets rule always withholds, and
    // the round-trip test below is what proves it is named rather than
    // dropped.
    { name: 'modDirectory', type: 'path' },
  ],
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

const APPLIED_TX = {
  id: 'tx-1',
  createdAt: 0,
  kind: 'reshade',
  label: 'ReShade',
  gameId: 'elden-ring',
  targetPath: 'C:\\games\\elden-ring\\dxgi.dll',
  backupPath: 'C:\\backup\\1',
  status: 'applied' as const,
}

beforeEach(() => {
  mockedList.mockReset()
  mockedContext.mockReset()
  mockedPick.mockReset()
  mockedRead.mockReset()
  mockedSave.mockReset()
  mockedReveal.mockReset()
  mockedInstall.mockReset()
  mockedContext.mockResolvedValue(importContext())
  mockedRead.mockResolvedValue({ path: 'C:\\inbox\\profile.json', contents: documentJson() })
  mockedSave.mockResolvedValue({ path: 'C:\\Users\\you\\Desktop\\p.json', bytes: 10 })
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

    // The dialog is given the file name and the contents, and it is the
    // dialog that decides where the file lands. Nothing here picks a
    // directory.
    const [fileName, contents] = mockedSave.mock.calls[0]
    const written = JSON.parse(contents as string)
    expect(fileName).toContain('.json')
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
    expect(mockedSave).not.toHaveBeenCalled()
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
    expect(mockedSave).not.toHaveBeenCalled()
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

    // The path revealed is the one the dialog reported, not a directory
    // Moddin chose: the user picked this file, so this is its location.
    expect(mockedReveal).toHaveBeenCalledWith('C:\\Users\\you\\Desktop\\p.json')
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

/**
 * A cancelled dialog is not a failure.
 *
 * Both of these are the same shape on purpose: the user opened a native
 * dialog and closed it, and "nothing happened" is the whole result. The
 * tests that make that true assert the absence of all three of an error,
 * a notice and a file on disk — a dialog that reported a failure the
 * user can do nothing about is the failure mode, not the fix for it.
 */
describe('a cancelled dialog', () => {
  it('writes nothing, and reports no failure, when the save dialog is dismissed', async () => {
    mockedList.mockResolvedValue([APPLIED_TX])
    mockedSave.mockResolvedValue(null)
    const profiles = useProfiles()

    expect(await profiles.runExport()).toBe(false)

    // The dialog was asked — this is a refusal, not a shortcut.
    expect(mockedSave).toHaveBeenCalledTimes(1)
    expect(profiles.exportResult.value).toBeNull()
    expect(profiles.error.value).toBeNull()
    expect(profiles.notice.value).toBeNull()
  })

  it('keeps the result of an earlier export when a second save is dismissed', async () => {
    mockedList.mockResolvedValue([APPLIED_TX])
    mockedSave.mockResolvedValueOnce({ path: 'C:\\Users\\you\\Desktop\\p.json', bytes: 10 })
    const profiles = useProfiles()
    await profiles.runExport()

    mockedSave.mockResolvedValueOnce(null)
    expect(await profiles.runExport()).toBe(false)

    expect(profiles.exportResult.value?.path).toBe('C:\\Users\\you\\Desktop\\p.json')
    expect(profiles.error.value).toBeNull()
  })

  it('reads nothing, and keeps the path already typed, when the open dialog is dismissed', async () => {
    mockedPick.mockResolvedValue(null)
    const profiles = useProfiles()
    profiles.importPath.value = 'C:\\inbox\\profile.json'

    expect(await profiles.browseImport()).toBe(false)

    expect(mockedRead).not.toHaveBeenCalled()
    expect(profiles.importPath.value).toBe('C:\\inbox\\profile.json')
    expect(profiles.preview.value).toBeNull()
    expect(profiles.error.value).toBeNull()
  })

  it('reads the file the open dialog named, and nothing else', async () => {
    mockedPick.mockResolvedValue('D:\\inbox\\from-another-pc.json')
    const profiles = useProfiles()

    expect(await profiles.browseImport()).toBe(true)

    expect(profiles.importPath.value).toBe('D:\\inbox\\from-another-pc.json')
    expect(mockedRead).toHaveBeenCalledTimes(1)
    expect(mockedRead).toHaveBeenCalledWith('D:\\inbox\\from-another-pc.json')
    expect(profiles.preview.value?.applyCount).toBe(1)
    expect(mockedInstall).not.toHaveBeenCalled()
  })
})

/**
 * The round trip, proved rather than asserted.
 *
 * The document a profile writes is read back through the real parse, the
 * real secrets rule and the real plan — only the two native calls are
 * stubbed, and the stub for the save hands the import exactly the string
 * the export produced. Anything the schema cannot carry therefore has to
 * show up either as a changed value or as a name the plan reports, which
 * is the only two things this feature is allowed to do with it.
 */
describe('a profile that survives its own round trip', () => {
  it('re-imports the file it wrote with the same values and nothing dropped', async () => {
    mockedList.mockResolvedValue([APPLIED_TX])
    const profiles = useProfiles({ configFor: { values: () => ({ preset: 'ultra' }) } })

    expect(await profiles.runExport()).toBe(true)
    const [fileName, contents] = mockedSave.mock.calls[0]

    // Read the file back exactly as it was written.
    mockedRead.mockResolvedValue({ path: `C:\\Users\\you\\Desktop\\${fileName}`, contents })
    const reader = useProfiles()
    reader.importPath.value = `C:\\Users\\you\\Desktop\\${fileName}`

    expect(await reader.readImport()).toBe(true)

    const planned = reader.preview.value?.games[0].capabilities[0]
    expect(planned?.config).toEqual({ preset: 'ultra' })
    expect(planned?.omittedSecrets).toEqual([])
    expect(reader.preview.value?.blockedCount).toBe(0)
    expect(reader.preview.value?.applyCount).toBe(1)
  })

  it('names the settings it cannot carry instead of dropping them silently', async () => {
    mockedList.mockResolvedValue([APPLIED_TX])
    const profiles = useProfiles({
      configFor: { values: () => ({ preset: 'ultra', modDirectory: 'C:\\Users\\you\\mods' }) },
    })

    expect(await profiles.runExport()).toBe(true)
    const [fileName, contents] = mockedSave.mock.calls[0]
    const written = JSON.parse(contents)
    expect(written.games[0].capabilities[0].omittedSecrets).toEqual(['modDirectory'])

    mockedRead.mockResolvedValue({ path: `C:\\Users\\you\\Desktop\\${fileName}`, contents })
    const reader = useProfiles()
    reader.importPath.value = `C:\\Users\\you\\Desktop\\${fileName}`
    await reader.readImport()

    const planned = reader.preview.value?.games[0].capabilities[0]
    // The path is not in the file, and the preview says so by name
    // rather than installing a default the user never chose.
    expect(planned?.config).toEqual({ preset: 'ultra' })
    expect(planned?.omittedSecrets).toEqual(['modDirectory'])
  })
})
