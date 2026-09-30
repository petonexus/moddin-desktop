import { computed, reactive, ref } from 'vue'
import type { CapabilitySpec, CapabilitySummary } from '../../types/capability'
import {
  evaluateCapability,
  getCapabilityCompatibility,
  getCapabilitySpec,
  installCapability,
  listCapabilities,
  uninstallCapability,
} from './service'
import type { CapabilityCardState, CapabilityConfigValue, UseCapabilityModulesOptions } from './types'

function messageOf(error: unknown) {
  return error instanceof Error ? error.message : String(error)
}

function defaultFor(fieldType: string, fallback: string | number | boolean | undefined): CapabilityConfigValue {
  if (fallback !== undefined) return fallback
  if (fieldType === 'boolean') return false
  if (fieldType === 'number') return 0
  return ''
}

/**
 * Mirrors Rust `CompatibilitySpec::has_constraints`: a block that sets no
 * bound constrains nothing, so the card must not even probe the exe. Keeps
 * the common case (no `compatibility` key at all) free.
 */
export function hasCompatibilityConstraint(spec: CapabilitySpec | null | undefined): boolean {
  const compatibility = spec?.compatibility
  if (!compatibility) return false
  return Boolean(
    compatibility.minExeVersion || compatibility.maxExeVersion || compatibility.blockedExeVersions?.length,
  )
}

/**
 * Turns saved capability recipes (AI-authored local YAML, community
 * specs, and the shipped data-driven ones) into installable module cards
 * for the selected game.
 *
 * The host decides what appears here by passing `excludeIds` — the
 * catalog modules its library grid already owns because it has a
 * dedicated installer for them. Everything else the registry knows,
 * including `builtIn` specs like BepInEx, surfaces as a card.
 *
 * Engine gating happens in the backend, not here. It used not to happen
 * at all, because `capability_list` returned summaries with no
 * `supportedEngines` and a filter invented on this side would have
 * hidden cards at random. The summaries now carry an `engineMatch`
 * verdict the runner computed while it already had the spec in hand, and
 * the gate is `CapabilitySpec::engine_match` in Rust: a recipe that
 * names engines and does not name the game's is held back; a recipe
 * that names none, or a game that declares no engine, is shown. The
 * reason matters here, because it is why `engine()` is passed instead
 * of filtered afterwards — the "no engine means everything" case is a
 * decision about the user's data, not a detail of the filter.
 */
export function useCapabilityModules(options: UseCapabilityModulesOptions) {
  const capabilities = ref<CapabilitySummary[]>([])
  const loaded = ref(false)
  const loading = ref(false)
  const loadError = ref<string | null>(null)
  const cards = reactive<Record<string, CapabilityCardState>>({})

  const visibleCapabilities = computed<CapabilitySummary[]>(() =>
    capabilities.value.filter((capability) => !options.excludeIds().includes(capability.id)),
  )

  function stateFor(capabilityId: string): CapabilityCardState {
    let state = cards[capabilityId]
    if (!state) {
      state = {
        busy: false,
        verifyBusy: false,
        error: null,
        errorKind: null,
        spec: null,
        specLoading: false,
        verification: null,
        configValues: {},
        compatibility: null,
        compatibilityBusy: false,
        installedDependencies: [],
      }
      cards[capabilityId] = state
    }
    return state
  }

  function applySpecDefaults(state: CapabilityCardState) {
    const schema = state.spec?.configSchema ?? []
    for (const field of schema) {
      if (state.configValues[field.name] === undefined) {
        state.configValues[field.name] = defaultFor(field.type, field.default)
      }
    }
  }

  /** Fetch (once) and cache the full spec; needed for the config form. */
  async function ensureSpec(capability: CapabilitySummary) {
    const state = stateFor(capability.id)
    if (state.spec || state.specLoading) return
    state.specLoading = true
    try {
      state.spec = await getCapabilitySpec(capability.id)
      applySpecDefaults(state)
      // Specs that pin a game build get probed right away so the card can
      // warn (and offer a forced install) before the user clicks Apply.
      if (hasCompatibilityConstraint(state.spec)) await refreshCompatibility(capability)
    } catch (error) {
      state.error = messageOf(error)
      state.errorKind = 'action'
    } finally {
      state.specLoading = false
    }
  }

  /**
   * Re-probe the spec's compatibility block. Cheap on its own (it reads
   * one exe's FileVersion) and re-runs whenever the selected game changes,
   * so switching games never leaves a verdict from the previous one.
   */
  async function refreshCompatibility(capability: CapabilitySummary) {
    const state = stateFor(capability.id)
    const executableDir = options.executableDir() ?? ''
    if (!hasCompatibilityConstraint(state.spec) || !executableDir) {
      state.compatibility = null
      return
    }
    state.compatibilityBusy = true
    try {
      state.compatibility = await getCapabilityCompatibility({
        capabilityId: capability.id,
        executableDir,
      })
    } catch (error) {
      // A failed probe must not hide the install button: the backend
      // re-evaluates the gate on install anyway, so a stale "unknown"
      // here is cosmetic at worst.
      state.compatibility = null
      state.error = messageOf(error)
      state.errorKind = 'action'
    } finally {
      state.compatibilityBusy = false
    }
  }

  /** True when the card must offer a forced install instead of a plain one. */
  function compatibilityBlocks(capabilityId: string) {
    const compatibility = stateFor(capabilityId).compatibility
    return Boolean(compatibility && !compatibility.passed)
  }

  let loadPromise: Promise<void> | null = null

  async function ensureLoaded() {
    if (loaded.value || loadPromise) return loadPromise ?? Promise.resolve()
    loading.value = true
    loadPromise = (async () => {
      try {
        capabilities.value = await listCapabilities(options.engine())
        loadError.value = null
        loaded.value = true
        // Prefetch full specs so the per-card config forms are ready when
        // the user expands a card; results are cached per capability id.
        await Promise.all(visibleCapabilities.value.map((capability) => ensureSpec(capability)))
      } catch (error) {
        loadError.value = messageOf(error)
      } finally {
        loading.value = false
        loadPromise = null
      }
    })()
    return loadPromise
  }

  async function refresh() {
    loaded.value = false
    await ensureLoaded()
  }

  function isInstalled(capabilityId: string) {
    const gameId = options.gameId()
    if (!gameId) return false
    return options
      .transactions()
      .some(
        (transaction) =>
          transaction.status === 'applied'
          && transaction.gameId === gameId
          && transaction.kind === capabilityId,
      )
  }

  /** Names of required config fields the user has not filled in yet. */
  function missingRequiredFields(capabilityId: string): string[] {
    const state = stateFor(capabilityId)
    return (state.spec?.configSchema ?? [])
      .filter((field) => {
        if (!field.required) return false
        const value = state.configValues[field.name]
        return value === undefined || value === ''
      })
      .map((field) => field.name)
  }

  /**
   * Install a capability. `force` overrides the spec's compatibility gate
   * for this capability only — the backend never cascades it to a
   * dependency. The returned outcome is kept on the card so the view can
   * show what was auto-installed and, after a forced install, exactly
   * which build constraint the user overrode.
   */
  async function install(capability: CapabilitySummary, installOptions: { force?: boolean } = {}) {
    const state = stateFor(capability.id)
    state.busy = true
    state.error = null
    state.errorKind = null
    state.installedDependencies = []
    try {
      const result = await installCapability({
        capabilityId: capability.id,
        gameId: options.gameId() ?? '',
        gameName: options.gameName() ?? '',
        installDir: options.installDir() ?? '',
        executableDir: options.executableDir() ?? '',
        config: { values: { ...state.configValues } },
        force: installOptions.force,
      })
      state.installedDependencies = result.installedDependencies ?? []
      if (result.compatibility) state.compatibility = result.compatibility
      state.verification = null
      await options.onChanged()
    } catch (error) {
      state.error = messageOf(error)
      state.errorKind = 'install'
    } finally {
      state.busy = false
    }
  }

  async function uninstall(capability: CapabilitySummary) {
    const state = stateFor(capability.id)
    state.busy = true
    state.error = null
    state.errorKind = null
    try {
      await uninstallCapability({
        capabilityId: capability.id,
        gameId: options.gameId() ?? '',
        installDir: options.installDir() ?? '',
      })
      state.verification = null
      state.installedDependencies = []
      await options.onChanged()
    } catch (error) {
      state.error = messageOf(error)
      state.errorKind = 'action'
    } finally {
      state.busy = false
    }
  }

  async function verify(capability: CapabilitySummary) {
    const state = stateFor(capability.id)
    state.verifyBusy = true
    state.error = null
    state.errorKind = null
    try {
      state.verification = await evaluateCapability({
        capabilityId: capability.id,
        executableDir: options.executableDir() ?? '',
        config: { values: { ...state.configValues } },
      })
    } catch (error) {
      state.error = messageOf(error)
      state.errorKind = 'action'
    } finally {
      state.verifyBusy = false
    }
  }

  return {
    capabilities,
    visibleCapabilities,
    loaded,
    loading,
    loadError,
    ensureLoaded,
    refresh,
    stateFor,
    ensureSpec,
    isInstalled,
    missingRequiredFields,
    compatibilityBlocks,
    refreshCompatibility,
    install,
    uninstall,
    verify,
  }
}
