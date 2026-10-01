import { beforeEach, describe, expect, it, vi } from 'vitest'
import { ref } from 'vue'
import type { ToolModuleDefinition } from '../../../types/game'
import type { GameCatalogEntry, InstalledGame } from '../../../types/game'
import type { ObsVrPreview } from '../../../types/obs'
import type { OptiScalerPreview } from '../../../types/optiscaler'
import type { UevrPreview } from '../../../types/uevr'
import type { ModuleUpdate } from '../../../types/module-update'
import type { DesktopShortcutPreview } from '../../desktop-shortcut/types'
import type { ModuleRequestContext } from '../module-registry'

/**
 * The scheduling around a module's checks, which is where a stale answer
 * is decided from.
 *
 * Verification and update checks both run in the background after a
 * selection settles, so by the time an answer arrives the player may
 * have moved on. Getting that wrong is not a crash: it is a card that
 * confidently reports the state of the game they were looking at two
 * clicks ago. The tests below are about the bookkeeping — the busy flag,
 * the TTL, and above all refusing an answer that belongs to a selection
 * that no longer exists.
 */

vi.mock('../service', () => ({
  previewObsVr: vi.fn(),
  previewOptiScaler: vi.fn(),
  previewOfxr: vi.fn(),
  previewCheekyFoveatedDlss: vi.fn(),
  previewUevr: vi.fn(),
  previewVrLaunch: vi.fn(),
  launchVrGame: vi.fn(),
  configureObsVr: vi.fn(),
  installOptiScaler: vi.fn(),
  installOfxr: vi.fn(),
  installCheekyFoveatedDlss: vi.fn(),
  installUevr: vi.fn(),
  uninstallObsVr: vi.fn(),
  uninstallOptiScaler: vi.fn(),
  uninstallOfxr: vi.fn(),
  uninstallCheekyFoveatedDlss: vi.fn(),
  uninstallUevr: vi.fn(),
  rollbackLatestModuleTransaction: vi.fn(),
  checkModuleUpdate: vi.fn(),
}))

import { checkModuleUpdate as checkModuleUpdateCommand, previewObsVr, previewOptiScaler, previewUevr } from '../service'
import { useModuleVerification } from '../useModuleVerification'
import en from '../../../i18n/locales/en'
import es from '../../../i18n/locales/es'
import ptBR from '../../../i18n/locales/pt-BR'

const mockedPreviewObsVr = vi.mocked(previewObsVr)
const mockedPreviewOptiScaler = vi.mocked(previewOptiScaler)
const mockedPreviewUevr = vi.mocked(previewUevr)
const mockedCheckModuleUpdate = vi.mocked(checkModuleUpdateCommand)

const installed: InstalledGame = {
  store: 'steam',
  appId: '1245620',
  name: 'ELDEN RING',
  installDir: 'C:\\Games\\ELDEN RING',
  libraryPath: 'C:\\Games\\steamapps',
}

function catalog(overrides: Partial<GameCatalogEntry> = {}): GameCatalogEntry {
  return {
    id: 'elden-ring',
    name: 'Elden Ring',
    steamAppId: '1245620',
    executable: 'Game/eldenring.exe',
    modules: [],
    ...overrides,
  }
}

function obsModule(overrides: Partial<ToolModuleDefinition> = {}): ToolModuleDefinition {
  return {
    id: 'obs-vr',
    name: 'OBS VR Capture',
    description: 'Capture the VR scene in OBS.',
    category: 'vr',
    status: 'available',
    config: { collectionName: 'Sem nome', sceneName: 'vr', sourceName: 'Elden Ring VR' },
    ...overrides,
  }
}

function optiscalerModule(): ToolModuleDefinition {
  return {
    id: 'optiscaler',
    name: 'OptiScaler',
    description: 'Install the official release.',
    category: 'graphics',
    status: 'available',
    config: {
      version: '0.9.4',
      downloadUrl: 'https://example.test/optiscaler.7z',
      sha256: 'abc',
      proxyCandidates: 'dxgi.dll,version.dll',
      updateUrl: 'https://api.github.com/repos/optiscaler/OptiScaler/releases/latest',
    },
  }
}

function uevrModule(): ToolModuleDefinition {
  return {
    id: 'uevr',
    name: 'UEVR',
    description: 'Inject the UEVR adapter.',
    category: 'vr',
    status: 'available',
    config: {
      versionPolicy: 'latest',
      releaseApiUrl: 'https://api.github.com/repos/praydog/UEVR-nightly/releases/latest',
    },
  }
}

function uevrPreview(overrides: Partial<UevrPreview> = {}): UevrPreview {
  return {
    canApply: true,
    gameRunning: false,
    uevrRunning: false,
    executableExists: true,
    executablePath: 'C:\\Games\\Game\\Game.exe',
    executableDirectory: 'C:\\Games\\Game',
    engine: 'Unreal Engine',
    engineVersion: '5.4',
    engineConfidence: 'high',
    engineEvidence: [],
    backend: 'nightly',
    backendLabel: 'UEVR-nightly',
    selectedVersion: 'v1.16.3',
    installed: false,
    installedVersion: null,
    installedBackend: null,
    installDirectory: '%LOCALAPPDATA%\\UEVR',
    manualInstallDetected: false,
    changes: [],
    warnings: [],
    ...overrides,
  }
}

function obsPreview(overrides: Partial<ObsVrPreview> = {}): ObsVrPreview {
  return {
    gameRunning: false,
    canApply: true,
    installed: false,
    collectionName: 'Sem nome',
    collectionFile: 'C:\\obs\\scenes\\json',
    sceneFound: true,
    sourceExists: true,
    sourceTargetMatches: true,
    sourceInScene: true,
    changes: [],
    warnings: [],
    ...overrides,
  } as ObsVrPreview
}

interface Harness {
  store: ReturnType<typeof useModuleVerification>
  host: Parameters<typeof useModuleVerification>[0]
  select: (appId: string | null) => void
}

/**
 * A host that behaves like the app: a selected game, a generation that
 * moves on every selection, and one module list per catalog id.
 */
function harness(games: Record<string, ToolModuleDefinition[]> = { 'elden-ring': [obsModule()] }): Harness {
  const selectedAppId = ref<string | null>('1245620')
  const generation = ref(0)
  const actionError = ref<string | null>(null)
  const success = ref<string | null>(null)
  const desktopShortcutPreview = vi.fn(async (): Promise<DesktopShortcutPreview> => ({
    canApply: true,
    shortcutPath: 'C:\\Desktop\\Game.lnk',
    targetPath: 'game.exe',
    iconPath: 'game.exe',
    willReplace: false,
  }))

  const host: Parameters<typeof useModuleVerification>[0] = {
    t: (key) => key,
    selectedAppId: () => selectedAppId.value,
    selectionGeneration: () => generation.value,
    gameContext: (appId) => {
      if (!appId) return null
      const modules = games['elden-ring']
      if (!modules || appId !== '1245620') return null
      return { installed, catalog: catalog({ modules }) }
    },
    requestContext: (module): ModuleRequestContext => ({
      module,
      game: { installed, catalog: catalog({ modules: games['elden-ring'] }) },
      t: (key) => key,
      uevrBackend: () => 'nightly',
    }),
    stateKey: (module, appId) => `${appId ?? 'unknown'}:${module.id}`,
    activeTransactionId: () => null,
    desktopShortcutPreview,
    actionError,
    success,
  }

  return {
    store: useModuleVerification(host),
    host,
    select: (appId) => {
      selectedAppId.value = appId
      generation.value += 1
    },
  }
}

beforeEach(() => {
  mockedPreviewObsVr.mockReset()
  mockedPreviewOptiScaler.mockReset()
  mockedPreviewUevr.mockReset()
  mockedCheckModuleUpdate.mockReset()
  // An update check asks the module's own preview for the version that is
  // actually installed, so every optiscaler check starts here.
  mockedPreviewOptiScaler.mockResolvedValue({ installedVersion: null } as unknown as OptiScalerPreview)
})

describe('verifying one module', () => {
  it('records what the preview says, keyed by game and module', async () => {
    mockedPreviewObsVr.mockResolvedValue(obsPreview({ installed: true }))
    const { store } = harness()

    await store.verifyModule(obsModule())

    const verification = store.verificationFor(obsModule())
    expect(verification?.status).toBe('installed')
    expect(verification?.gameRunning).toBe(false)
    expect(verification?.checks.map((check) => check.label)).toContain('checkObsScene')
  })

  it('reports a failure instead of leaving the card spinning', async () => {
    mockedPreviewObsVr.mockRejectedValue(new Error('OBS is not installed'))
    const { store, host } = harness()

    await store.verifyModule(obsModule())

    expect(host.actionError.value).toBe('OBS is not installed')
    expect(store.verificationFor(obsModule())).toBeUndefined()
    expect(store.isVerifying(obsModule())).toBe(false)
  })

  it('ignores a module the player cannot install yet', async () => {
    const { store } = harness()
    const planned = obsModule({ status: 'planned' })

    await store.verifyModule(planned)

    expect(mockedPreviewObsVr).not.toHaveBeenCalled()
  })

  it('asks the shortcut feature for the one module it owns', async () => {
    const shortcut = obsModule({ id: 'desktop-shortcut', name: 'Desktop shortcut' })
    const { store, host } = harness({ 'elden-ring': [shortcut] })

    await store.verifyModule(shortcut)

    expect(host.desktopShortcutPreview).toHaveBeenCalledWith(shortcut)
    expect(store.verificationFor(shortcut)?.status).toBe('ready')
  })

  it('discards an answer that belongs to a selection the player left', async () => {
    // The preflight finishes after the player clicked another game. Its
    // answer is about a directory they are no longer looking at.
    const harnessRef = harness()
    const module = obsModule()
    mockedPreviewObsVr.mockImplementation(async () => {
      harnessRef.select('9999')
      return obsPreview({ installed: true })
    })

    await harnessRef.store.verifyModule(module)

    expect(harnessRef.store.verificationFor(module)).toBeUndefined()
  })
})

describe('verifying every module of the selected game', () => {
  it('verifies UEVR like any other available module', async () => {
    // It used to be skipped here, which left it the one available card
    // sitting on "unknown" until the player pressed Verify — with no gate
    // hiding the card to justify it. `preview_uevr` is the one preview
    // that can reach the network, and only for a game that passes the
    // local Unreal preflight, but the update check on the same selection
    // already fetches the same release endpoint, so the skip bought
    // nothing and cost the card its answer.
    mockedPreviewUevr.mockResolvedValue(uevrPreview())
    const uevr = uevrModule()
    const { store } = harness({ 'elden-ring': [obsModule(), uevr] })

    await store.verifyAvailableModules()

    expect(mockedPreviewUevr).toHaveBeenCalledTimes(1)
    expect(store.verificationFor(uevr)?.status).toBe('ready')
    expect(store.verificationFor(uevr)?.checks.map((check) => check.label)).toContain('checkUevrEngine')
  })

  it('records what UEVR actually reports rather than assuming the engine', async () => {
    // Elden Ring declares UEVR available on a non-Unreal engine. The
    // check is the honest answer there: the engine check fails and the
    // card says so, instead of the card quietly staying unknown.
    mockedPreviewUevr.mockResolvedValue(uevrPreview({ engine: null, canApply: false, selectedVersion: null }))
    const uevr = uevrModule()
    const { store } = harness({ 'elden-ring': [uevr] })

    await store.verifyAvailableModules()

    const verification = store.verificationFor(uevr)
    expect(verification?.status).toBe('attention')
    expect(verification?.checks.find((check) => check.label === 'checkUevrEngine')?.passed).toBe(false)
  })

  it('reuses an answer that is still fresh', async () => {
    mockedPreviewObsVr.mockResolvedValue(obsPreview())
    const { store } = harness()
    const module = obsModule()

    await store.verifyAvailableModules()
    await store.verifyAvailableModules()

    expect(mockedPreviewObsVr).toHaveBeenCalledTimes(1)
  })

  it('re-checks when the answer has gone stale', async () => {
    mockedPreviewObsVr.mockResolvedValue(obsPreview())
    const { store } = harness()
    const module = obsModule()

    await store.verifyAvailableModules()
    // The TTL is 15 seconds; nothing should have expired yet.
    await store.verifyAvailableModules()
    expect(mockedPreviewObsVr).toHaveBeenCalledTimes(1)

    await store.verifyAvailableModules('1245620', 0, true)
    expect(mockedPreviewObsVr).toHaveBeenCalledTimes(2)
  })

  it('does nothing for a game the catalog does not know', async () => {
    const { store, select } = harness()
    select('9999')

    await store.verifyAvailableModules()

    expect(mockedPreviewObsVr).not.toHaveBeenCalled()
  })
})

describe('checking for a newer version', () => {
  it('records nothing for a recipe with no source, and never calls the backend', async () => {
    const { store } = harness()
    const module = obsModule()

    await store.checkModuleUpdate(module)

    expect(mockedCheckModuleUpdate).not.toHaveBeenCalled()
    // `hasUpdateSource` is the same predicate `checkModuleUpdate` uses, so
    // the card hides its whole update area. There was nothing on screen for
    // a record to explain: writing one only put a hardcoded English
    // sentence in a store no renderer reads, and the localized answer the
    // card would have shown is `updateNoSource` either way.
    expect(store.hasUpdateSource(module)).toBe(false)
    expect(store.updateFor(module)).toBeUndefined()
    expect(store.updateSummary(module)).toBe('updateNoSource')
  })

  it('says "no update source" in every language, because that is the answer the card gives', () => {
    // The removed hardcoded sentence duplicated this key's job. If the
    // three locales ever drift, the one string that still has to be
    // translated is the one the card actually renders.
    for (const [locale, messages] of Object.entries({ en, es, 'pt-BR': ptBR })) {
      expect(messages.updateNoSource, `${locale} must translate updateNoSource`).toBeTruthy()
      expect(messages.updateNoSource, `${locale} must not repeat the English sentence`)
        .not.toBe('This module recipe does not define an update source.')
    }
  })

  it('compares against the version the backend reports, not the recipe', async () => {
    mockedPreviewOptiScaler.mockResolvedValue({
      installedVersion: '0.9.1',
      canApply: true,
      gameRunning: false,
      selectedProxy: 'dxgi.dll',
      executableExists: true,
      installed: true,
      manualInstallDetected: false,
      changes: [],
      warnings: [],
    } as unknown as OptiScalerPreview)
    mockedCheckModuleUpdate.mockResolvedValue({
      status: 'available',
      currentVersion: '0.9.1',
      latestVersion: '0.9.4',
      releaseUrl: 'https://example.test/release',
      checkedAt: 0,
    } satisfies ModuleUpdate)
    const { store } = harness({ 'elden-ring': [optiscalerModule()] })
    const module = optiscalerModule()

    await store.checkModuleUpdate(module)

    expect(mockedCheckModuleUpdate).toHaveBeenCalledWith({
      currentVersion: '0.9.1',
      updateUrl: 'https://api.github.com/repos/optiscaler/OptiScaler/releases/latest',
    })
    expect(store.updateFor(module)?.status).toBe('available')
  })

  it('falls back to the recipe version when the preview has none', async () => {
    mockedCheckModuleUpdate.mockResolvedValue({
      status: 'current',
      currentVersion: '0.9.4',
      latestVersion: '0.9.4',
      releaseUrl: null,
      checkedAt: 0,
    } satisfies ModuleUpdate)
    const { store } = harness({ 'elden-ring': [optiscalerModule()] })
    const module = optiscalerModule()

    await store.checkModuleUpdate(module)

    expect(mockedCheckModuleUpdate).toHaveBeenCalledWith(expect.objectContaining({ currentVersion: '0.9.4' }))
  })

  it('keeps a failed check on the card instead of claiming there is none', async () => {
    mockedCheckModuleUpdate.mockRejectedValue(new Error('The release API timed out'))
    const { store, host } = harness({ 'elden-ring': [optiscalerModule()] })
    const module = optiscalerModule()

    await store.checkModuleUpdate(module)

    expect(store.updateFor(module)).toMatchObject({ status: 'error', detail: 'The release API timed out' })
    expect(host.actionError.value).toBe('The release API timed out')
  })

  it('says what it knows about an update in one line', async () => {
    const { store } = harness({ 'elden-ring': [optiscalerModule()] })
    const module = optiscalerModule()

    expect(store.updateSummary(module)).toBe('updateNotChecked')
    store.updates.value = { '1245620:optiscaler': {
      status: 'available',
      currentVersion: '0.9.1',
      latestVersion: '0.9.4',
      releaseUrl: null,
      checkedAt: 0,
    } satisfies ModuleUpdate }
    expect(store.updateSummary(module)).toBe('updateAvailableSummary')
  })

  it('only checks modules that declare where to look, and not twice', async () => {
    mockedCheckModuleUpdate.mockResolvedValue({
      status: 'current',
      currentVersion: '0.9.4',
      latestVersion: '0.9.4',
      releaseUrl: null,
      checkedAt: Date.now(),
    } satisfies ModuleUpdate)
    const { store } = harness({ 'elden-ring': [obsModule(), optiscalerModule()] })

    await store.checkAvailableModuleUpdates()
    await store.checkAvailableModuleUpdates()

    // OBS has no update source, so it is never asked; OptiScaler is.
    expect(mockedCheckModuleUpdate).toHaveBeenCalledTimes(1)
  })
})
