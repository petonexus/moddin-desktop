import { beforeEach, describe, expect, it, vi } from 'vitest'
import { hasCompatibilityConstraint, useCapabilityModules } from '../useCapabilityModules'
import type { UseCapabilityModulesOptions } from '../types'
import type { CapabilitySpec, CapabilitySummary } from '../../../types/capability'
import type { CapabilityVerificationReport } from '../types'
import type { TransactionRecord } from '../../../types/transaction'

/** A promise whose resolution the test controls, so "in flight" is real. */
function deferred<T>() {
  let resolve!: (value: T) => void
  const promise = new Promise<T>((settle) => {
    resolve = settle
  })
  return { promise, resolve }
}

function verification(
  checks: Array<{ id: string; label: string; passed: boolean; detail?: string }> = [],
  overrides: Partial<CapabilityVerificationReport> = {},
): CapabilityVerificationReport {
  const failing = checks.some((check) => !check.passed)
  return {
    status: failing ? 'failed' : 'ok',
    summary: failing ? '1 check failed' : 'All checks passed',
    checks,
    gameRunning: false,
    installed: false,
    ...overrides,
  }
}

/**
 * The composable is the only thing standing between a Tauri command and
 * the capability card, so the mocks here mirror the real command names
 * and payloads. `src/features/capability-modules/service.ts` is not
 * loaded at all — it is the only place allowed to reach the Tauri core
 * API, and mocking it keeps that boundary out of the test as well.
 */
vi.mock('../service', () => ({
  listCapabilities: vi.fn(),
  getCapabilitySpec: vi.fn(),
  getCapabilityCompatibility: vi.fn(),
  installCapability: vi.fn(),
  uninstallCapability: vi.fn(),
  evaluateCapability: vi.fn(),
}))

import {
  evaluateCapability,
  getCapabilityCompatibility,
  getCapabilitySpec,
  installCapability,
  listCapabilities,
  uninstallCapability,
} from '../service'

const mockedList = vi.mocked(listCapabilities)
const mockedSpec = vi.mocked(getCapabilitySpec)
const mockedCompatibility = vi.mocked(getCapabilityCompatibility)
const mockedInstall = vi.mocked(installCapability)
const mockedUninstall = vi.mocked(uninstallCapability)
const mockedVerify = vi.mocked(evaluateCapability)

function summary(id: string): CapabilitySummary {
  return {
    id,
    displayName: `Mod ${id}`,
    description: `Does ${id} things`,
    category: 'graphics',
    status: 'available',
    origin: 'builtIn',
    // Engine-neutral: the engine-filtering tests build their own
    // capability with a real verdict, and these ones are about the
    // rest of the card.
    supportedEngines: [],
    engineMatch: { verdict: 'engineAgnostic' },
  }
}

function spec(id: string, overrides: Partial<CapabilitySpec> = {}): CapabilitySpec {
  return {
    id,
    displayName: `Mod ${id}`,
    category: 'graphics',
    status: 'available',
    ...overrides,
  }
}

function transaction(overrides: Partial<TransactionRecord> = {}): TransactionRecord {
  return {
    id: 'tx-1',
    createdAt: 1_700_000_000_000,
    kind: 'community-mod',
    label: 'Mod community-mod',
    gameId: 'elden-ring',
    targetPath: 'C:\\games\\elden-ring\\bepinex',
    backupPath: 'C:\\backup\\tx-1',
    status: 'applied',
    ...overrides,
  }
}

interface Harness {
  modules: ReturnType<typeof useCapabilityModules>
  onChanged: ReturnType<typeof vi.fn>
}

function harness(overrides: Partial<UseCapabilityModulesOptions> = {}): Harness {
  const onChanged = vi.fn()
  const modules = useCapabilityModules({
    gameId: () => 'elden-ring',
    gameName: () => 'Elden Ring',
    installDir: () => 'C:\\games\\elden-ring',
    executableDir: () => 'C:\\games\\elden-ring\\Game',
    engine: () => null,
    excludeIds: () => [],
    transactions: () => [],
    onChanged,
    ...overrides,
  })
  return { modules, onChanged }
}

beforeEach(() => {
  mockedList.mockReset()
  mockedSpec.mockReset()
  mockedCompatibility.mockReset()
  mockedInstall.mockReset()
  mockedUninstall.mockReset()
  mockedVerify.mockReset()

  mockedList.mockResolvedValue([])
  mockedSpec.mockImplementation(async (id: string) => spec(id))
  mockedCompatibility.mockResolvedValue(null)
  mockedInstall.mockResolvedValue({
    capabilityId: 'community-mod',
    transaction: null,
    steps: [],
    affectedPaths: [],
  })
  mockedUninstall.mockResolvedValue(transaction())
  mockedVerify.mockResolvedValue(verification())
})

describe('hasCompatibilityConstraint', () => {
  it('is false without a spec or without a compatibility block', () => {
    expect(hasCompatibilityConstraint(null)).toBe(false)
    expect(hasCompatibilityConstraint(undefined)).toBe(false)
    expect(hasCompatibilityConstraint(spec('a'))).toBe(false)
  })

  it('is false for a compatibility block that constrains nothing', () => {
    // Mirrors Rust `CompatibilitySpec::has_constraints`: an empty block
    // must not cost the card an exe probe.
    expect(hasCompatibilityConstraint(spec('a', { compatibility: {} }))).toBe(false)
    expect(
      hasCompatibilityConstraint(spec('a', { compatibility: { gameExe: 'game.exe' } })),
    ).toBe(false)
    expect(
      hasCompatibilityConstraint(spec('a', { compatibility: { blockedExeVersions: [] } })),
    ).toBe(false)
  })

  it('is true when any bound is declared', () => {
    expect(hasCompatibilityConstraint(spec('a', { compatibility: { minExeVersion: '1.0' } }))).toBe(true)
    expect(hasCompatibilityConstraint(spec('a', { compatibility: { maxExeVersion: '2.0' } }))).toBe(true)
    expect(
      hasCompatibilityConstraint(spec('a', { compatibility: { blockedExeVersions: ['1.5'] } })),
    ).toBe(true)
  })
})

describe('loading', () => {
  it('lists capabilities and prefetches each visible spec once', async () => {
    mockedList.mockResolvedValue([summary('alpha'), summary('beta')])
    const { modules } = harness()

    await modules.ensureLoaded()

    expect(modules.loaded.value).toBe(true)
    expect(modules.visibleCapabilities.value.map((item) => item.id)).toEqual(['alpha', 'beta'])
    expect(modules.stateFor('alpha').spec?.id).toBe('alpha')
    expect(mockedSpec).toHaveBeenCalledTimes(2)
  })

  it('is idempotent: a second ensureLoaded does not refetch', async () => {
    mockedList.mockResolvedValue([summary('alpha')])
    const { modules } = harness()

    await modules.ensureLoaded()
    await modules.ensureLoaded()

    expect(mockedList).toHaveBeenCalledTimes(1)
  })

  it('hides ids the host already owns in its library grid', async () => {
    mockedList.mockResolvedValue([summary('alpha'), summary('bepinex')])
    const { modules } = harness({ excludeIds: () => ['bepinex'] })

    await modules.ensureLoaded()

    expect(modules.visibleCapabilities.value.map((item) => item.id)).toEqual(['alpha'])
    // The excluded card is not on screen, so its spec is not fetched.
    expect(mockedSpec).toHaveBeenCalledTimes(1)
  })

  it('records the failure and stays unloaded when the list call throws', async () => {
    mockedList.mockRejectedValue(new Error('backend is down'))
    const { modules } = harness()

    await modules.ensureLoaded()

    expect(modules.loaded.value).toBe(false)
    expect(modules.loadError.value).toBe('backend is down')
    expect(modules.loading.value).toBe(false)
  })

  it('records a spec failure on the card without failing the whole load', async () => {
    mockedList.mockResolvedValue([summary('alpha')])
    mockedSpec.mockRejectedValue(new Error('capability_get exploded'))
    const { modules } = harness()

    await modules.ensureLoaded()

    expect(modules.loadError.value).toBeNull()
    const state = modules.stateFor('alpha')
    expect(state.error).toBe('capability_get exploded')
    expect(state.errorKind).toBe('action')
    expect(state.specLoading).toBe(false)
  })
})

describe('stateFor', () => {
  it('creates one card per capability with a clean initial state', () => {
    const { modules } = harness()
    const state = modules.stateFor('alpha')

    // `cards` is a `reactive` record, so the second lookup is the proxy
    // for the object the first one returned — same contents, and a write
    // through either is visible through the other.
    const again = modules.stateFor('alpha')
    state.error = 'written through the first handle'
    expect(again.error).toBe('written through the first handle')
    again.error = 'written through the proxy'
    expect(state.error).toBe('written through the proxy')

    expect(modules.stateFor('beta')).not.toBe(again)
    expect(modules.stateFor('beta')).toMatchObject({
      busy: false,
      verifyBusy: false,
      error: null,
      errorKind: null,
      spec: null,
      specLoading: false,
      verification: null,
      compatibility: null,
      compatibilityBusy: false,
      installedDependencies: [],
    })
  })

  it('seeds each config field from the schema when the spec arrives', async () => {
    mockedList.mockResolvedValue([summary('alpha')])
    mockedSpec.mockResolvedValue(
      spec('alpha', {
        configSchema: [
          { name: 'url', type: 'url' },
          { name: 'count', type: 'number', default: 3 },
          { name: 'flag', type: 'boolean' },
          { name: 'named', type: 'string', default: 'preset' },
        ],
      }),
    )
    const { modules } = harness()

    await modules.ensureLoaded()

    expect(modules.stateFor('alpha').configValues).toEqual({
      url: '',
      count: 3,
      flag: false,
      named: 'preset',
    })
  })

  it('does not overwrite a value the user already typed', async () => {
    mockedList.mockResolvedValue([summary('alpha')])
    mockedSpec.mockResolvedValue(
      spec('alpha', { configSchema: [{ name: 'count', type: 'number', default: 3 }] }),
    )
    const { modules } = harness()
    modules.stateFor('alpha').configValues.count = 9

    await modules.ensureLoaded()

    expect(modules.stateFor('alpha').configValues.count).toBe(9)
  })

  it('pre-fills a required field from the catalogue for the selected game', async () => {
    // The card used to render every required field blank and wait for the
    // user to paste a download URL and a digest nobody can be expected to
    // know. The per-game `config:` block already holds both — the same
    // data the game page's own module card shows.
    mockedList.mockResolvedValue([summary('optiscaler')])
    mockedSpec.mockResolvedValue(
      spec('optiscaler', {
        configSchema: [
          { name: 'downloadUrl', type: 'url', required: true },
          { name: 'sha256', type: 'sha256', required: true },
          { name: 'version', type: 'string', required: true },
        ],
      }),
    )
    const { modules } = harness({ gameId: () => 'cyberpunk-2077', gameName: () => 'Cyberpunk 2077' })

    await modules.ensureLoaded()
    await modules.install(summary('optiscaler'))

    expect(mockedInstall).toHaveBeenCalledWith(
      expect.objectContaining({
        capabilityId: 'optiscaler',
        config: {
          values: {
            downloadUrl: 'https://github.com/optiscaler/OptiScaler/releases/download/v0.9.4/Optiscaler_0.9.4-final.20260718._MM.7z',
            sha256: '575cb4df866116093df75af607e37fd70e10f5163e0f23fd5c804142e80ef0ad',
            version: '0.9.4',
          },
        },
      }),
    )
    // The values the catalogue filled are not reported as the user's
    // unfinished work.
    expect(modules.missingRequiredFields('optiscaler')).toEqual([])
  })

  it('still asks for a required field the catalogue has no value for', async () => {
    // The shipped `optiscaler` recipe: `proxy` is a required path, and it
    // is the one field the catalogue cannot know — it depends on which
    // DLLs already live in the game folder. A default is not invented for
    // it, and it is not quietly made optional.
    mockedList.mockResolvedValue([summary('optiscaler')])
    mockedSpec.mockResolvedValue(
      spec('optiscaler', {
        configSchema: [
          { name: 'downloadUrl', type: 'url', required: true },
          { name: 'proxy', type: 'path', required: true },
        ],
      }),
    )
    const { modules } = harness({ gameId: () => 'cyberpunk-2077' })

    await modules.ensureLoaded()

    expect(modules.stateFor('optiscaler').configValues.downloadUrl).toContain('Optiscaler_0.9.4')
    expect(modules.missingRequiredFields('optiscaler')).toEqual(['proxy'])
  })

  it('fills from the catalogue of the game that is selected, not another one', async () => {
    mockedList.mockResolvedValue([summary('obs-vr')])
    mockedSpec.mockResolvedValue(
      spec('obs-vr', { configSchema: [{ name: 'sourceName', type: 'string', required: true }] }),
    )
    const { modules } = harness({ gameId: () => 'dawnwalker' })

    await modules.ensureLoaded()

    expect(modules.stateFor('obs-vr').configValues.sourceName).toBe('The Blood of Dawnwalker VR')
    expect(modules.missingRequiredFields('obs-vr')).toEqual([])
  })
})

describe('isInstalled', () => {
  it('is false with no game selected', () => {
    const { modules } = harness({ gameId: () => null, transactions: () => [transaction()] })
    expect(modules.isInstalled('community-mod')).toBe(false)
  })

  it('counts only applied transactions for the selected game', () => {
    const applied = transaction()
    const { modules } = harness({
      transactions: () => [
        applied,
        transaction({ id: 'tx-2', status: 'rolled_back' }),
        transaction({ id: 'tx-3', gameId: 'doom-2016' }),
        transaction({ id: 'tx-4', kind: 'optiscaler' }),
      ],
    })

    expect(modules.isInstalled('community-mod')).toBe(true)
    // No transaction of kind `bepinex` exists, so it stays uninstalled
    // even though an unrelated capability is installed for this game.
    expect(modules.isInstalled('bepinex')).toBe(false)
  })
})

describe('missingRequiredFields', () => {
  it('names the required fields that are still empty', async () => {
    mockedList.mockResolvedValue([summary('alpha')])
    mockedSpec.mockResolvedValue(
      spec('alpha', {
        configSchema: [
          { name: 'url', type: 'url', required: true },
          { name: 'note', type: 'string' },
          { name: 'count', type: 'number', required: true },
        ],
      }),
    )
    const { modules } = harness()

    await modules.ensureLoaded()
    const state = modules.stateFor('alpha')
    state.configValues.url = ''
    state.configValues.count = 0

    // `count` is filled (0 is a value), `url` is still blank; `note` is
    // optional so it is never reported.
    expect(modules.missingRequiredFields('alpha')).toEqual(['url'])
  })
})

describe('compatibility gate', () => {
  it('probes the build only when the spec constrains it', async () => {
    mockedList.mockResolvedValue([summary('alpha'), summary('beta')])
    mockedSpec.mockImplementation(async (id: string) =>
      id === 'beta' ? spec('beta', { compatibility: { minExeVersion: '1.0' } }) : spec('alpha'),
    )
    const { modules } = harness()

    await modules.ensureLoaded()

    expect(mockedCompatibility).toHaveBeenCalledTimes(1)
    expect(mockedCompatibility).toHaveBeenCalledWith({
      capabilityId: 'beta',
      executableDir: 'C:\\games\\elden-ring\\Game',
    })
    expect(modules.compatibilityBlocks('alpha')).toBe(false)
  })

  it('blocks a plain install on a failing probe and not on a passing one', async () => {
    mockedCompatibility.mockResolvedValue({ label: 'exe-version', passed: true })
    mockedList.mockResolvedValue([summary('beta')])
    mockedSpec.mockResolvedValue(spec('beta', { compatibility: { minExeVersion: '1.0' } }))
    const { modules } = harness()
    await modules.ensureLoaded()

    expect(modules.stateFor('beta').compatibility?.passed).toBe(true)
    expect(modules.compatibilityBlocks('beta')).toBe(false)

    mockedCompatibility.mockResolvedValue({
      label: 'exe-version',
      passed: false,
      detail: 'requires 1.0.0.0, found 0.9.0.0',
    })
    await modules.refreshCompatibility(summary('beta'))

    expect(modules.compatibilityBlocks('beta')).toBe(true)
  })

  it('does not probe when no executable directory is known', async () => {
    mockedList.mockResolvedValue([summary('beta')])
    mockedSpec.mockResolvedValue(spec('beta', { compatibility: { minExeVersion: '1.0' } }))
    const { modules } = harness({ executableDir: () => null })

    await modules.ensureLoaded()

    expect(mockedCompatibility).not.toHaveBeenCalled()
    expect(modules.stateFor('beta').compatibility).toBeNull()
  })

  it('fails open on a probe error, so a broken check cannot hide Apply', async () => {
    mockedCompatibility.mockRejectedValue(new Error('cannot read FileVersion'))
    mockedList.mockResolvedValue([summary('beta')])
    mockedSpec.mockResolvedValue(spec('beta', { compatibility: { minExeVersion: '1.0' } }))
    const { modules } = harness()

    await modules.ensureLoaded()

    const state = modules.stateFor('beta')
    expect(state.compatibility).toBeNull()
    expect(state.compatibilityBusy).toBe(false)
    expect(state.error).toBe('cannot read FileVersion')
    expect(modules.compatibilityBlocks('beta')).toBe(false)
  })
})

describe('verify', () => {
  it('raises and lowers verifyBusy around the check', async () => {
    const gate = deferred<CapabilityVerificationReport>()
    mockedVerify.mockImplementation(() => gate.promise)
    const { modules } = harness()
    const capability = summary('alpha')

    const pending = modules.verify(capability)
    expect(modules.stateFor('alpha').verifyBusy).toBe(true)

    gate.resolve(verification())
    await pending
    expect(modules.stateFor('alpha').verifyBusy).toBe(false)
  })

  it('passes the selected game directories and the card config', async () => {
    mockedList.mockResolvedValue([summary('alpha')])
    mockedSpec.mockResolvedValue(
      spec('alpha', { configSchema: [{ name: 'count', type: 'number', default: 2 }] }),
    )
    const { modules } = harness()
    await modules.ensureLoaded()

    await modules.verify(summary('alpha'))

    expect(mockedVerify).toHaveBeenCalledWith({
      capabilityId: 'alpha',
      executableDir: 'C:\\games\\elden-ring\\Game',
      config: { values: { count: 2 } },
    })
  })

  it('surfaces a failing check as a failing check, not as an empty card', async () => {
    mockedVerify.mockResolvedValue(
      verification([{ id: 'bepinex', label: 'BepInEx present', passed: false, detail: 'missing' }]),
    )
    const { modules } = harness()

    await modules.verify(summary('alpha'))

    const report = modules.stateFor('alpha').verification
    expect(report?.status).toBe('failed')
    expect(report?.checks.filter((check) => !check.passed)).toHaveLength(1)
    expect(modules.stateFor('alpha').error).toBeNull()
  })

  it('leaves no verification behind when the check itself throws', async () => {
    // A thrown check must not read as "verified": `verification` stays
    // null and the error is recorded, so a card can never show a clean
    // report for a check that never completed.
    mockedVerify.mockRejectedValue(new Error('capability_evaluate timed out'))
    const { modules } = harness()

    await modules.verify(summary('alpha'))

    const state = modules.stateFor('alpha')
    expect(state.verification).toBeNull()
    expect(state.error).toBe('capability_evaluate timed out')
    expect(state.errorKind).toBe('action')
    expect(state.verifyBusy).toBe(false)
  })
})

describe('install', () => {
  it('sends the resolved install target and reports auto-installed dependencies', async () => {
    mockedInstall.mockResolvedValue({
      capabilityId: 'alpha',
      transaction: null,
      steps: [],
      affectedPaths: [],
      installedDependencies: ['loader', 'runtime'],
      compatibility: { label: 'exe-version', passed: true },
    })
    const { modules, onChanged } = harness()

    await modules.install(summary('alpha'))

    expect(mockedInstall).toHaveBeenCalledWith(
      expect.objectContaining({
        capabilityId: 'alpha',
        gameId: 'elden-ring',
        gameName: 'Elden Ring',
        installDir: 'C:\\games\\elden-ring',
        executableDir: 'C:\\games\\elden-ring\\Game',
      }),
    )
    const state = modules.stateFor('alpha')
    expect(state.installedDependencies).toEqual(['loader', 'runtime'])
    expect(state.compatibility?.passed).toBe(true)
    expect(onChanged).toHaveBeenCalledTimes(1)
    expect(state.busy).toBe(false)
  })

  it('clears a stale verification so the card cannot claim a pre-install result', async () => {
    const { modules } = harness()
    modules.stateFor('alpha').verification = {
      status: 'ok',
      summary: 'ok',
      checks: [],
      gameRunning: false,
      installed: true,
    }

    await modules.install(summary('alpha'))

    expect(modules.stateFor('alpha').verification).toBeNull()
  })

  it('passes force through for an explicit compatibility override', async () => {
    const { modules } = harness()

    await modules.install(summary('alpha'), { force: true })

    expect(mockedInstall).toHaveBeenCalledWith(expect.objectContaining({ force: true }))
  })

  it('marks a failed install with errorKind "install"', async () => {
    mockedInstall.mockRejectedValue(new Error('download 404'))
    const { modules, onChanged } = harness()

    await modules.install(summary('alpha'))

    const state = modules.stateFor('alpha')
    expect(state.error).toBe('download 404')
    expect(state.errorKind).toBe('install')
    expect(state.busy).toBe(false)
    expect(onChanged).not.toHaveBeenCalled()
  })
})

describe('uninstall', () => {
  it('clears the card state and asks the host to refresh', async () => {
    const { modules, onChanged } = harness()
    const state = modules.stateFor('alpha')
    state.verification = { status: 'ok', summary: 'ok', checks: [], gameRunning: false, installed: true }
    state.installedDependencies = ['loader']

    await modules.uninstall(summary('alpha'))

    expect(mockedUninstall).toHaveBeenCalledWith({
      capabilityId: 'alpha',
      gameId: 'elden-ring',
      installDir: 'C:\\games\\elden-ring',
    })
    expect(state.verification).toBeNull()
    expect(state.installedDependencies).toEqual([])
    expect(onChanged).toHaveBeenCalledTimes(1)
  })

  it('marks a failed removal with errorKind "action"', async () => {
    mockedUninstall.mockRejectedValue(new Error('file is locked'))
    const { modules } = harness()

    await modules.uninstall(summary('alpha'))

    expect(modules.stateFor('alpha').errorKind).toBe('action')
    expect(modules.stateFor('alpha').error).toBe('file is locked')
  })
})

describe('refresh', () => {
  it('re-reads the list and the specs', async () => {
    mockedList.mockResolvedValue([summary('alpha')])
    const { modules } = harness()
    await modules.ensureLoaded()
    expect(modules.loaded.value).toBe(true)

    mockedList.mockResolvedValue([summary('alpha'), summary('gamma')])
    await modules.refresh()

    expect(mockedList).toHaveBeenCalledTimes(2)
    expect(modules.visibleCapabilities.value.map((item) => item.id)).toEqual(['alpha', 'gamma'])
  })
})
