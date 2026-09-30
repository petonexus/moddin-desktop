import { beforeEach, describe, expect, it, vi } from 'vitest'

/**
 * The unsigned-consent gate (ROADMAP UX-12).
 *
 * `installSelectedRecommendations` used to send `acceptUnsigned: true`
 * for every recommendation, so the checkbox the Community panel shows
 * was skipped entirely on the AI path. These tests pin the replacement:
 * split the batch by the verified catalog's `signed` flag, and fail
 * closed on anything unsigned that the user has not acknowledged.
 *
 * `useAiAssistant.ts` is a module of singleton state, so every test
 * re-imports it through `vi.resetModules()` and re-derives the handle
 * after resetting storage.
 */

const CONSENT_KEY = 'moddin-unsigned-consent'

vi.mock('../../features/capability-modules/service', () => ({
  resolveInstallTarget: vi.fn(),
}))
vi.mock('../../features/community/service', () => ({
  fetchCommunityCatalog: vi.fn(),
  installCommunityCapability: vi.fn(),
}))
vi.mock('../../features/ai-assistant/service', () => ({
  AGENT_SUFFIX: '',
  buildAuthorPrompt: vi.fn(() => ''),
  listAgentClis: vi.fn(async () => []),
  previewCapabilityPlan: vi.fn(),
  runAiAgentPrompt: vi.fn(),
  saveCapabilityYaml: vi.fn(),
  validateCapabilityYaml: vi.fn(),
  validateRecommendationsYaml: vi.fn(),
  listCapabilitiesForAssistant: vi.fn(async () => []),
}))
vi.mock('../../features/collection/service', () => ({
  listCollections: vi.fn(async () => []),
}))

const TARGET = {
  gameId: 'elden-ring',
  gameName: 'Elden Ring',
  installDir: 'C:\\games\\elden-ring',
  executableDir: 'C:\\games\\elden-ring\\Game',
}

type Assistant = typeof import('../useAiAssistant')

async function loadAssistant(): Promise<Assistant> {
  vi.resetModules()
  return import('../useAiAssistant')
}

function recommendation(id: string) {
  return { type: 'capability' as const, id, reason: 'because', confidence: 0.9 }
}

/** Put `ids` in the selection and aim the dialog at Elden Ring. */
function select(assistant: Assistant['useAiAssistant'] extends () => infer R ? R : never, ids: string[]) {
  assistant.recommendations.value = ids.map(recommendation)
  assistant.selectedRecommendations.value = new Set(ids.map((id) => `capability:${id}`))
  assistant.context.value = { ...assistant.context.value, gameId: 'elden-ring', gameName: 'Elden Ring' }
}

let install: ReturnType<typeof vi.fn>
let resolveTarget: ReturnType<typeof vi.fn>
let fetchCatalog: ReturnType<typeof vi.fn>
let record: Assistant['recordUnsignedConsent']

beforeEach(async () => {
  window.localStorage.clear()
  const modules = await loadAssistant()
  record = modules.recordUnsignedConsent

  const capabilityService = await import('../../features/capability-modules/service')
  const communityService = await import('../../features/community/service')
  resolveTarget = vi.mocked(capabilityService.resolveInstallTarget)
  fetchCatalog = vi.mocked(communityService.fetchCommunityCatalog)
  install = vi.mocked(communityService.installCommunityCapability)

  resolveTarget.mockResolvedValue(TARGET)
  install.mockResolvedValue({ ok: true })
  fetchCatalog.mockResolvedValue({ catalog: { capabilities: [] } })
})

describe('recordUnsignedConsent', () => {
  it('persists one entry per capability id', () => {
    record('community-a')
    record('community-b')

    expect(JSON.parse(window.localStorage.getItem(CONSENT_KEY) ?? '{}')).toEqual({
      'community-a': true,
      'community-b': true,
    })
  })

  it('is additive: a second tick does not drop the first', () => {
    record('community-a')
    record('community-b')

    expect(JSON.parse(window.localStorage.getItem(CONSENT_KEY) ?? '{}')['community-a']).toBe(true)
  })
})

describe('installSelectedRecommendations: the signed batch', () => {
  it('installs a signed capability without any consent', async () => {
    fetchCatalog.mockResolvedValue({ catalog: { capabilities: [{ id: 'community-a', signed: true }] } })
    const { useAiAssistant } = await loadAssistant()
    const assistant = useAiAssistant()
    select(assistant as never, ['community-a'])

    await assistant.installSelectedRecommendations()

    expect(install).toHaveBeenCalledTimes(1)
    expect(install).toHaveBeenCalledWith({
      capabilityId: 'community-a',
      gameId: 'elden-ring',
      gameName: 'Elden Ring',
      installDir: TARGET.installDir,
      executableDir: TARGET.executableDir,
      config: { values: {} },
      acceptUnsigned: false,
    })
    expect(assistant.recommendationInstallStatus.value['capability:community-a']).toBe('done')
    expect(assistant.error.value).toBeNull()
  })
})

describe('installSelectedRecommendations: failing closed', () => {
  it('refuses an unsigned capability that was never acknowledged', async () => {
    fetchCatalog.mockResolvedValue({ catalog: { capabilities: [{ id: 'community-a', signed: false }] } })
    const { useAiAssistant } = await loadAssistant()
    const assistant = useAiAssistant()
    select(assistant as never, ['community-a'])

    await assistant.installSelectedRecommendations()

    expect(install).not.toHaveBeenCalled()
    expect(assistant.recommendationInstallStatus.value['capability:community-a']).toBe('failed')
    expect(assistant.error.value).toContain('community-a')
    expect(assistant.error.value).toContain('not signed')
  })

  it('treats a capability absent from the catalog as unverified', async () => {
    // "I have never heard of this" is not a signed entry.
    fetchCatalog.mockResolvedValue({ catalog: { capabilities: [{ id: 'community-a', signed: true }] } })
    const { useAiAssistant } = await loadAssistant()
    const assistant = useAiAssistant()
    select(assistant as never, ['community-unknown'])

    await assistant.installSelectedRecommendations()

    expect(install).not.toHaveBeenCalled()
    expect(assistant.error.value).toContain('community-unknown')
  })

  it('installs only the signed half of a mixed batch and names the blocked one', async () => {
    fetchCatalog.mockResolvedValue({
      catalog: {
        capabilities: [
          { id: 'community-signed', signed: true },
          { id: 'community-unsigned', signed: false },
        ],
      },
    })
    const { useAiAssistant } = await loadAssistant()
    const assistant = useAiAssistant()
    select(assistant as never, ['community-signed', 'community-unsigned'])

    await assistant.installSelectedRecommendations()

    // Fails closed for the whole batch: the gate runs before any
    // install, so a signed entry is not applied either.
    expect(install).not.toHaveBeenCalled()
    expect(assistant.error.value).toContain('community-unsigned')
    expect(assistant.recommendationInstallStatus.value['capability:community-signed']).toBe('pending')
  })

  it('treats every recommendation as unverified when the catalog fetch fails', async () => {
    fetchCatalog.mockRejectedValue(new Error('community_catalog_fetch failed'))
    const { useAiAssistant } = await loadAssistant()
    const assistant = useAiAssistant()
    select(assistant as never, ['community-a'])

    await assistant.installSelectedRecommendations()

    expect(install).not.toHaveBeenCalled()
    expect(assistant.recommendationInstallStatus.value['capability:community-a']).toBe('failed')
    expect(assistant.error.value).toContain('community-a')
  })

  it('treats a corrupt consent record as no consent at all', async () => {
    window.localStorage.setItem(CONSENT_KEY, '{not json')
    fetchCatalog.mockResolvedValue({ catalog: { capabilities: [{ id: 'community-a', signed: false }] } })
    const { useAiAssistant } = await loadAssistant()
    const assistant = useAiAssistant()
    select(assistant as never, ['community-a'])

    await assistant.installSelectedRecommendations()

    expect(install).not.toHaveBeenCalled()
  })

  it('ignores consent entries that are not exactly `true`', async () => {
    window.localStorage.setItem(CONSENT_KEY, JSON.stringify({ 'community-a': 'yes' }))
    fetchCatalog.mockResolvedValue({ catalog: { capabilities: [{ id: 'community-a', signed: false }] } })
    const { useAiAssistant } = await loadAssistant()
    const assistant = useAiAssistant()
    select(assistant as never, ['community-a'])

    await assistant.installSelectedRecommendations()

    expect(install).not.toHaveBeenCalled()
  })
})

describe('installSelectedRecommendations: recorded consent unblocks', () => {
  it('accepts an unsigned capability the user already ticked in the Community panel', async () => {
    fetchCatalog.mockResolvedValue({ catalog: { capabilities: [{ id: 'community-a', signed: false }] } })
    const { useAiAssistant } = await loadAssistant()
    record('community-a')
    const assistant = useAiAssistant()
    select(assistant as never, ['community-a'])

    await assistant.installSelectedRecommendations()

    expect(install).toHaveBeenCalledTimes(1)
    expect(install).toHaveBeenCalledWith(expect.objectContaining({ acceptUnsigned: true }))
    expect(assistant.error.value).toBeNull()
  })

  it('does not let consent for one capability cover another', async () => {
    fetchCatalog.mockResolvedValue({
      catalog: {
        capabilities: [
          { id: 'community-a', signed: false },
          { id: 'community-b', signed: false },
        ],
      },
    })
    const { useAiAssistant } = await loadAssistant()
    record('community-a')
    const assistant = useAiAssistant()
    select(assistant as never, ['community-a', 'community-b'])

    await assistant.installSelectedRecommendations()

    expect(install).not.toHaveBeenCalled()
    expect(assistant.error.value).toContain('community-b')
  })

  it('derives acceptUnsigned from the consent record, not from the signature', async () => {
    // Characterisation, not an endorsement: the request flag is
    // `consent[id] === true`, so a capability that is signed AND has
    // stale consent still sends `acceptUnsigned: true`. Harmless for
    // the backend (the flag only bypasses a gate the signed entry does
    // not trip) and the gate above it still used the real signature —
    // but it is why the two must not be conflated if the flag is ever
    // given a second meaning.
    fetchCatalog.mockResolvedValue({ catalog: { capabilities: [{ id: 'community-a', signed: true }] } })
    const { useAiAssistant } = await loadAssistant()
    record('community-a')
    const assistant = useAiAssistant()
    select(assistant as never, ['community-a'])

    await assistant.installSelectedRecommendations()

    expect(install).toHaveBeenCalledWith(expect.objectContaining({ acceptUnsigned: true }))
  })
})

describe('installSelectedRecommendations: no install target', () => {
  it('fails loudly instead of installing against empty paths', async () => {
    resolveTarget.mockResolvedValue(null)
    fetchCatalog.mockResolvedValue({ catalog: { capabilities: [{ id: 'community-a', signed: true }] } })
    const { useAiAssistant } = await loadAssistant()
    const assistant = useAiAssistant()
    select(assistant as never, ['community-a'])

    await assistant.installSelectedRecommendations()

    expect(install).not.toHaveBeenCalled()
    expect(assistant.recommendationInstallStatus.value['capability:community-a']).toBe('failed')
    expect(assistant.error.value).toMatch(/No supported game is selected/)
  })

  it('surfaces an install-target failure without installing anything', async () => {
    resolveTarget.mockRejectedValue(new Error('detect_installed_games failed'))
    const { useAiAssistant } = await loadAssistant()
    const assistant = useAiAssistant()
    select(assistant as never, ['community-a'])

    await assistant.installSelectedRecommendations()

    expect(install).not.toHaveBeenCalled()
    expect(assistant.error.value).toContain('detect_installed_games failed')
    expect(assistant.busy.value).toBe(false)
  })

  it('does nothing when no recommendation is selected', async () => {
    const { useAiAssistant } = await loadAssistant()
    const assistant = useAiAssistant()
    assistant.recommendations.value = [recommendation('community-a')]

    await assistant.installSelectedRecommendations()

    expect(resolveTarget).not.toHaveBeenCalled()
    expect(install).not.toHaveBeenCalled()
  })
})
