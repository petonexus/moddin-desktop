import { describe, expect, it } from 'vitest'
import type { CompatibilityStatus } from '../../../types/compatibility'
import type { ToolModuleDefinition } from '../../../types/game'
import type { ModuleUpdate } from '../../../types/module-update'
import type { ModuleVerification } from '../../../types/module-verification'
import {
  availableModuleCategories,
  countModulesInCategory,
  moduleActionLabel,
  moduleActionsBlocked,
  moduleCardView,
  selectModuleGroups,
  summariseModuleProgress,
  type ModuleCardViewInput,
} from '../module-card-view'
import { isCapabilityBackedModule } from '../module-registry'

/**
 * The questions a module card exists to answer, and the answers it gives.
 *
 * Every one of these was a decision inside the template's owner, spread
 * across the component that renders it. That is how "unknown" and
 * "checking" drifted apart: one meant "a check is running" and the other
 * meant "nobody has looked yet", and only the first is something the
 * player can wait for. Putting the decisions here makes each one a claim
 * that can be checked.
 */

function module(overrides: Partial<ToolModuleDefinition> = {}): ToolModuleDefinition {
  return {
    id: 'obs-vr',
    name: 'OBS VR Capture',
    description: 'Capture the VR scene in OBS.',
    category: 'vr',
    status: 'available',
    ...overrides,
  }
}

function verification(overrides: Partial<ModuleVerification> = {}): ModuleVerification {
  return {
    status: 'ready',
    summary: 'Ready',
    checks: [],
    gameRunning: false,
    checkedAt: 1_700_000_000_000,
    activeTransactionId: null,
    ...overrides,
  }
}

function update(overrides: Partial<ModuleUpdate> = {}): ModuleUpdate {
  return {
    status: 'current',
    currentVersion: '1.0.0',
    latestVersion: '1.0.0',
    releaseUrl: null,
    checkedAt: 1_700_000_000_000,
    ...overrides,
  }
}

function input(overrides: Partial<ModuleCardViewInput> = {}): ModuleCardViewInput {
  return {
    module: module(),
    verifying: false,
    loading: false,
    busy: false,
    actionBusy: false,
    gameRunning: false,
    installed: false,
    updateBusy: false,
    hasUpdateSource: false,
    updateSummary: 'updateNotChecked',
    compatibility: 'unverified',
    t: (key) => key,
    formatDate: (timestamp) => `at ${timestamp}`,
    ...overrides,
  }
}

describe('what state the card shows', () => {
  it('calls a module nobody can install yet "planned"', () => {
    expect(moduleCardView(input({ module: module({ status: 'planned' }) })).state).toBe('planned')
  })

  it('says "checking" only while something is actually running', () => {
    const loading = moduleCardView(input({ loading: true }))
    const verifying = moduleCardView(input({ verifying: true }))
    const idle = moduleCardView(input())

    expect(loading.state).toBe('checking')
    expect(verifying.state).toBe('checking')
    // "Unknown" is honest: nobody has looked, and nobody is looking now.
    expect(idle.state).toBe('unknown')
  })

  it('reports the last verification when there is one', () => {
    expect(moduleCardView(input({ verification: verification({ status: 'installed' }) })).state).toBe('active')
    expect(moduleCardView(input({ verification: verification({ status: 'ready' }) })).state).toBe('available')
    expect(moduleCardView(input({ verification: verification({ status: 'attention' }) })).state).toBe('attention')
    expect(moduleCardView(input({ verification: verification({ status: 'unknown' }) })).state).toBe('unknown')
  })

  it('stops saying "checking" once a verification is in', () => {
    const view = moduleCardView(input({ verification: verification({ status: 'installed' }), loading: true }))

    expect(view.state).toBe('active')
  })
})

describe('the compatibility chip', () => {
  const chipFor = (module: ToolModuleDefinition, compatibility: CompatibilityStatus) =>
    moduleCardView(input({ module, compatibility })).tag

  it('shows one for any recipe that declares a status of its own', () => {
    const chip = chipFor(module({ id: 'optiscaler', config: { compatibilityStatus: 'proven' } }), 'proven')

    expect(chip).toEqual({ label: 'compatibilityProven', tone: 'success' })
  })

  it('reads the chip from the recorded status, not the recipe', () => {
    const chip = chipFor(module({ id: 'optiscaler', config: { compatibilityStatus: 'unverified' } }), 'risky')

    expect(chip).toEqual({ label: 'compatibilityRisky', tone: 'danger' })
  })

  it('keeps the chip for Cheeky, whose recipes predate the config key', () => {
    expect(chipFor(module({ id: 'cheeky-foveated-dlss' }), 'experimental'))
      .toEqual({ label: 'compatibilityExperimental', tone: 'warning' })
  })

  it('shows nothing for a module with nothing to report', () => {
    expect(chipFor(module({ id: 'optiscaler' }), 'unverified')).toBeNull()
  })
})

describe('the update area', () => {
  it('stays hidden when the recipe has nowhere to look', () => {
    // "No update source" on every card was noise the player cannot act on.
    expect(moduleCardView(input({ hasUpdateSource: false })).update).toBeNull()
    expect(moduleCardView(input({ hasUpdateSource: true, update: update() })).update)
      .toMatchObject({ hasSource: true, available: false })
  })

  it('leads with the new version when there is one', () => {
    const view = moduleCardView(input({
      hasUpdateSource: true,
      update: update({ status: 'available', latestVersion: '0.9.5', releaseUrl: 'https://example.test/r' }),
    }))

    expect(view.update).toMatchObject({
      available: true,
      summary: 'updateAvailableShort',
      releaseUrl: 'https://example.test/r',
    })
  })

  it('never offers an update check on a module that is not installed yet', () => {
    const view = moduleCardView(input({
      module: module({ status: 'planned' }),
      hasUpdateSource: true,
      update: update({ status: 'available' }),
    }))

    expect(view.update).toBeNull()
  })
})

describe('the one button', () => {
  it('names the action the module actually has', () => {
    const label = (module: ToolModuleDefinition, overrides: Partial<ModuleCardViewInput> = {}) =>
      moduleActionLabel(input({ module, ...overrides }))

    expect(label(module({ status: 'planned' }))).toBe('statePlanned')
    expect(label(module(), { actionBusy: true })).toBe('actionOpening')
    expect(label(module({ id: 'vr-launch' }))).toBe('actionLaunchVr')
    expect(label(module({ id: 'desktop-shortcut' }))).toBe('actionCreateShortcut')
    expect(label(module())).toBe('actionInstall')
  })

  it('offers a reinstall for the two modules that swap versions in place', () => {
    const installed = verification({ status: 'installed' })

    expect(moduleActionLabel(input({ module: module({ id: 'optiscaler' }), verification: installed })))
      .toBe('actionReinstall')
    expect(moduleActionLabel(input({ module: module({ id: 'uevr' }), verification: installed })))
      .toBe('actionReinstall')
    // OBS is a scene edit, not a swap: re-running it applies again.
    expect(moduleActionLabel(input({ module: module(), verification: installed }))).toBe('actionApplyAgain')
  })

  it('is the primary gesture only while there is something to do', () => {
    expect(moduleCardView(input()).actionPrimary).toBe(true)
    expect(moduleCardView(input({ verification: verification({ status: 'installed' }) })).actionPrimary).toBe(false)
    // Launching in VR is always the thing the player came for.
    expect(moduleCardView(input({
      module: module({ id: 'vr-launch' }),
      verification: verification({ status: 'installed' }),
    })).actionPrimary).toBe(true)
  })

  it('is disabled while the game runs, the UI is busy, or the module is planned', () => {
    expect(moduleActionsBlocked(input())).toBe(false)
    expect(moduleActionsBlocked(input({ gameRunning: true }))).toBe(true)
    expect(moduleActionsBlocked(input({ busy: true }))).toBe(true)
    expect(moduleActionsBlocked(input({ loading: true }))).toBe(true)
    expect(moduleActionsBlocked(input({ module: module({ status: 'planned' }) }))).toBe(true)
  })
})

describe('what the card says about the rest of the module', () => {
  it('offers removal only while a transaction is on record', () => {
    expect(moduleCardView(input()).removeLabel).toBeNull()
    expect(moduleCardView(input({ installed: true })).removeLabel).toBe('actionRemove')
    // A planned module has nothing to remove, whatever the store says.
    expect(moduleCardView(input({ module: module({ status: 'planned' }), installed: true })).removeLabel).toBeNull()
  })

  it('dates the verification it is showing', () => {
    const view = moduleCardView(input({ verification: verification() }))

    expect(view.checkedAtLabel).toBe('checkedAt')
    expect(moduleCardView(input()).checkedAtLabel).toBeUndefined()
  })

  it('uses the catalog sentence for a module with no localized copy', () => {
    const view = moduleCardView(input({
      module: module({ id: 'graphics-profile', name: 'Perfil gráfico', description: 'Aplica os presets do jogo.' }),
    }))

    expect(view.name).toBe('Perfil gráfico')
    expect(view.description).toBe('Aplica os presets do jogo.')
  })
})

describe('how the grid is grouped', () => {
  const modules: ToolModuleDefinition[] = [
    module({ id: 'obs-vr', category: 'vr' }),
    module({ id: 'optiscaler', category: 'graphics' }),
    module({ id: 'reshade', category: 'graphics' }),
    module({ id: 'graphics-profile', category: 'graphics', status: 'planned' }),
    module({ id: 'vortex-migration', category: 'system', status: 'planned' }),
  ]
  const owned = modules.filter((entry) => !isCapabilityBackedModule(entry))

  it('offers a tab for every category the game has something in', () => {
    // Including the ones only the capability section fills: the tab has
    // to be there for the card below it.
    expect(availableModuleCategories(modules)).toEqual(['vr', 'graphics', 'system'])
    expect(countModulesInCategory(modules, 'graphics')).toBe(3)
  })

  it('lists only the modules the grid owns', () => {
    const groups = selectModuleGroups(modules, owned, 'all')

    // `vortex-migration` is planned and unimplemented, but it is still a
    // grid card: planned modules render as "Planned", not as nothing.
    expect(groups.map((group) => group.category)).toEqual(['vr', 'graphics', 'system'])
    expect(groups.flatMap((group) => group.modules.map((entry) => entry.id)))
      .not.toContain('reshade')
  })

  it('narrows to one category when the player picks one', () => {
    expect(selectModuleGroups(modules, owned, 'graphics')).toEqual([
      { category: 'graphics', modules: [modules[1], modules[3]] },
    ])
  })

  it('drops a category with nothing left in it', () => {
    // A tab can outlive its contents: the filter is remembered per
    // selection, and a group with no cards would render an empty heading.
    expect(selectModuleGroups(modules, [modules[0]], 'all')).toEqual([
      { category: 'vr', modules: [modules[0]] },
    ])
  })
})

describe('the progress bar above the grid', () => {
  // S.T.A.L.K.E.R. 2's shape: five available modules, two of which the
  // capability section renders and the grid never verifies.
  const available = [
    module({ id: 'obs-vr', category: 'vr' }),
    module({ id: 'uevr', category: 'vr' }),
    module({ id: 'ofxr-bridge', category: 'vr' }),
    module({ id: 'reshade', category: 'graphics' }),
    module({ id: 'optiscaler', category: 'graphics' }),
  ]
  const stateOf = (entry: ToolModuleDefinition) =>
    entry.id === 'obs-vr' ? 'active' : entry.id === 'uevr' ? 'attention' : 'unknown'

  it('counts working modules out of the available ones', () => {
    const progress = summariseModuleProgress(available, stateOf)

    expect(progress).toEqual({ total: 3, active: 1, attention: 1, checking: false })
  })

  it('leaves the capability-backed modules out of the total, so the bar can be finished', () => {
    // `ofxr-bridge` and `reshade` are available in the catalogue but
    // rendered by the capability section, which owns its own progress.
    // Counting them is what made the total unreachable: installing
    // everything the grid owns still left the bar short, forever.
    const progress = summariseModuleProgress(available, stateOf)

    expect(available.filter(isCapabilityBackedModule).map((entry) => entry.id))
      .toEqual(['ofxr-bridge', 'reshade'])
    expect(progress.total).toBe(available.length - 2)
    expect(progress.active + progress.attention).toBeLessThanOrEqual(progress.total)
  })

  it('counts a module that can be finished at all: a planned one is never active', () => {
    const withPlanned = [...available, module({ id: 'graphics-profile', category: 'graphics', status: 'planned' })]

    expect(summariseModuleProgress(withPlanned, () => 'active'))
      .toEqual({ total: 3, active: 3, attention: 0, checking: false })
  })

  it('says nothing is running when nothing is', () => {
    const progress = summariseModuleProgress([module({ id: 'obs-vr' })], () => 'active')

    expect(progress).toEqual({ total: 1, active: 1, attention: 0, checking: false })
  })

  it('reports a check still running, so the bar is not read as settled', () => {
    expect(summariseModuleProgress([module({ id: 'obs-vr' })], () => 'checking').checking).toBe(true)
  })
})
