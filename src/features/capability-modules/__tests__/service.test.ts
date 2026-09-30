import { describe, expect, it } from 'vitest'
import ptBR from '../../../i18n/locales/pt-BR'
import type { CapabilitySummary } from '../../../types/capability'
import { toCapabilityCheckView, toCapabilitySummaryView } from '../service'

/**
 * ROADMAP UX-21, the service half of the boundary.
 *
 * The point of mapping in the service layer is that the panel cannot get
 * it wrong: there is no component holding a backend id and remembering to
 * translate it, because the sentence arrives already resolved.
 *
 * These are the boundary's own two functions rather than the Tauri
 * commands around them, and that is deliberate. They are pure, they are
 * where the id becomes a key, and a test that stood up the native bridge
 * to reach them would be asserting that `invoke` is called — which is
 * not what this item is about. `check:architecture` keeps the bridge in
 * one place, and this file stays on the right side of it.
 */

const BEPINEX_DESCRIPTION = 'Lets Unity games load mods and plugins, and is the base most other Unity mods need.'

function summary(overrides: Partial<CapabilitySummary> = {}): CapabilitySummary {
  return {
    id: 'bepinex',
    displayName: 'BepInEx',
    description: BEPINEX_DESCRIPTION,
    category: 'qol',
    status: 'available',
    origin: 'builtIn',
    supportedEngines: [],
    engineMatch: { verdict: 'engineAgnostic' },
    ...overrides,
  }
}

describe('capability summaries cross the boundary with a locale key', () => {
  it('attaches the description key for a shipped capability', () => {
    const card = toCapabilitySummaryView(summary())

    expect(card.descriptionKey).toBe('capabilityDescriptionBepinex')
    expect(card.description).toBe(BEPINEX_DESCRIPTION)
    // The sentence a pt-BR user reads is the locale's, not the recipe's.
    expect(ptBR.capabilityDescriptionBepinex).not.toBe(BEPINEX_DESCRIPTION)
  })

  it('attaches nothing for a recipe this table does not declare', () => {
    const card = toCapabilitySummaryView(
      summary({ id: 'community-authored-mod', description: 'Written by whoever made the recipe.' }),
    )

    // The fallback, decided in `src/i18n/backendIds.ts`: the author's own
    // sentence, not a blank card.
    expect(card.descriptionKey).toBeUndefined()
    expect(card.description).toBe('Written by whoever made the recipe.')
  })

  it('leaves the summary otherwise untouched', () => {
    const original = summary()
    const card = toCapabilitySummaryView(original)

    expect(card.id).toBe('bepinex')
    expect(card.displayName).toBe('BepInEx')
    expect(card.category).toBe('qol')
    expect(card.origin).toBe('builtIn')
  })
})

describe('check outcomes cross the boundary with a locale key', () => {
  it('attaches the label key for a check the recipes declare', () => {
    const check = toCapabilityCheckView({
      id: 'preloader-present',
      label: 'BepInEx preloader installed',
      passed: true,
      detail: null,
    })

    expect(check.labelKey).toBe('capabilityCheckPreloaderPresent')
    expect(check.label).toBe('BepInEx preloader installed')
    expect(ptBR.capabilityCheckPreloaderPresent).not.toBe('BepInEx preloader installed')
  })

  it('attaches nothing for a community recipe\'s own check', () => {
    const check = toCapabilityCheckView({
      id: 'their-own-check',
      label: 'Does the thing',
      passed: false,
      detail: null,
    })

    expect(check.labelKey).toBeUndefined()
    expect(check.label).toBe('Does the thing')
  })

  it('leaves the verdict and the backend detail untouched', () => {
    const check = toCapabilityCheckView({
      id: 'game-build',
      label: 'Game build',
      passed: false,
      detail: '1.2.3 is outside the supported range.',
    })

    expect(check.passed).toBe(false)
    expect(check.detail).toBe('1.2.3 is outside the supported range.')
  })
})
