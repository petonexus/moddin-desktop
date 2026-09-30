import { enableAutoUnmount, flushPromises, mount } from '@vue/test-utils'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import ContributeTrigger from '../ContributeTrigger.vue'
import GlobalTools from '../GlobalTools.vue'
import { i18n } from '../../../i18n'

/**
 * The Contribute dialog used to be reachable only through a window event
 * that nothing was listening for, so the topbar button did nothing. The
 * point of this file is that the trigger owns the dialog: one click
 * opens it, and every verb ends in `useAiAssistant().openFor` — the same
 * entry point the topbar and the sidebar already use.
 */

vi.mock('../../../composables/useAiAssistant', () => ({
  useAiAssistant: vi.fn(),
  useAiAssistantTrigger: vi.fn(() => ({ openFor: vi.fn() })),
}))
vi.mock('../../../features/ai-assistant/service', () => ({ listCapabilitiesForAssistant: vi.fn() }))
vi.mock('../../../services/catalog', () => ({ findCatalogGameById: vi.fn(() => null) }))

import { useAiAssistant } from '../../../composables/useAiAssistant'
import { listCapabilitiesForAssistant } from '../../../features/ai-assistant/service'
import { findCatalogGameById } from '../../../services/catalog'

const openFor = vi.fn()
const mockedList = vi.mocked(listCapabilitiesForAssistant)
const mockedFindGame = vi.mocked(findCatalogGameById)

enableAutoUnmount(afterEach)

afterEach(() => {
  document.body.innerHTML = ''
})

beforeEach(() => {
  i18n.global.locale.value = 'en'
  openFor.mockReset()
  mockedList.mockReset()
  mockedFindGame.mockReset()

  vi.mocked(useAiAssistant).mockReturnValue({
    openFor,
  } as unknown as ReturnType<typeof useAiAssistant>)

  window.localStorage.setItem('moddin-selected-appId', 'elden-ring')
  mockedFindGame.mockReturnValue({
    id: 'elden-ring',
    name: 'Elden Ring',
    executable: 'eldenring.exe',
    modules: [],
  })
  mockedList.mockResolvedValue([])
})

function dialog() {
  return document.body.querySelector('[role="dialog"]')
}

async function openFromSidebar() {
  const wrapper = mount(ContributeTrigger, {
    attachTo: document.body,
    global: { plugins: [i18n] },
  })
  await wrapper.find('button.nav-item').trigger('click')
  await flushPromises()
  return wrapper
}

describe('ContributeTrigger', () => {
  it('is mounted in the sidebar', async () => {
    // The other four sidebar panels are not what this test is about, and
    // each of them calls the backend on mount; stubbing them keeps the
    // assertion about GlobalTools' own markup.
    const wrapper = mount(GlobalTools, {
      attachTo: document.body,
      global: {
        plugins: [i18n],
        stubs: {
          ActivityLogPanel: true,
          CommunityPanel: true,
          LocalAiPanel: true,
          OpenXrManager: true,
        },
      },
    })
    await flushPromises()

    const labels = wrapper.findAll('button.nav-item').map((button) => button.text())
    expect(labels).toContain('Contribute')
    expect(labels).toContain('See collections')
  })

  it('opens the dialog on click — no window event in between', async () => {
    const events: string[] = []
    const listener = (event: Event) => events.push(event.type)
    window.addEventListener('moddin:open-contribute', listener)

    expect(dialog()).toBeNull()
    await openFromSidebar()

    expect(dialog()).not.toBeNull()
    expect(dialog()?.textContent).toContain('Contribute to Moddin')
    // The removed event bus is not part of the path any more.
    expect(events).toEqual([])

    window.removeEventListener('moddin:open-contribute', listener)
  })

  it('sends the author verb to the assistant and closes', async () => {
    await openFromSidebar()
    const card = Array.from(dialog()?.querySelectorAll('.contribute-card') ?? []).find((element) =>
      element.textContent?.includes('Author a new mod'),
    ) as HTMLButtonElement
    card.click()
    await flushPromises()

    expect(openFor).toHaveBeenCalledWith(
      expect.objectContaining({ mode: 'author', gameId: 'elden-ring', gameName: 'Elden Ring' }),
    )
    expect(dialog()).toBeNull()
  })

  it('asks which mod to improve before it will send the improve verb', async () => {
    mockedList.mockResolvedValue([
      {
        id: 'uevr',
        displayName: 'UEVR',
        category: 'vr',
        status: 'available',
        origin: 'community',
        supportedEngines: [],
        engineMatch: { verdict: 'engineAgnostic' },
      },
    ])
    await openFromSidebar()
    const card = Array.from(dialog()?.querySelectorAll('.contribute-card') ?? []).find((element) =>
      element.textContent?.includes('Improve an existing mod'),
    ) as HTMLButtonElement
    card.click()
    await flushPromises()

    // Still in the picker, and nothing was sent to the assistant yet.
    expect(openFor).not.toHaveBeenCalled()
    const options = Array.from(dialog()?.querySelectorAll('option') ?? []).map((option) => option.textContent)
    expect(options).toContain('UEVR')

    const select = dialog()?.querySelector('select') as HTMLSelectElement
    select.value = 'uevr'
    select.dispatchEvent(new Event('change'))
    await flushPromises()

    const submit = Array.from(dialog()?.querySelectorAll('.dialog-footer button') ?? []).find((element) =>
      element.textContent?.includes('Ask AI to improve it'),
    ) as HTMLButtonElement
    submit.click()
    await flushPromises()

    expect(openFor).toHaveBeenCalledWith(
      expect.objectContaining({ mode: 'improve', capabilityId: 'uevr', gameId: 'elden-ring' }),
    )
  })

  it('closes on Escape and sends nothing', async () => {
    await openFromSidebar()
    expect(dialog()).not.toBeNull()

    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }))
    await flushPromises()

    expect(dialog()).toBeNull()
    expect(openFor).not.toHaveBeenCalled()
  })
})
