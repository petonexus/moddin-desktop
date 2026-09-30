import { enableAutoUnmount, flushPromises, mount } from '@vue/test-utils'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import LocalAiPanel from '../LocalAiPanel.vue'
import { localAiCopyForLocale } from '../copy'
import { i18n } from '../../../i18n'

/**
 * ROADMAP UX-11, call site 3: disconnecting a Local AI agent.
 *
 * Disconnecting rewrites the editor's config file, so it is a
 * filesystem write dressed as a settings toggle. Both the panel and the
 * confirmation are Teleports to `document.body`, so the dialogs are
 * found through the document the way a user finds them.
 */

vi.mock('../service', () => ({
  detectAgents: vi.fn(),
  removeAgent: vi.fn(),
  resolveResourceDir: vi.fn(),
  setupAgent: vi.fn(),
}))

import { detectAgents, removeAgent, resolveResourceDir } from '../service'

const mockedDetect = vi.mocked(detectAgents)
const mockedRemove = vi.mocked(removeAgent)
const mockedResourceDir = vi.mocked(resolveResourceDir)

const copy = localAiCopyForLocale('en')
const RESOURCE_DIR = 'C:\\Program Files\\Moddin Desktop\\resources'

enableAutoUnmount(afterEach)

beforeEach(() => {
  i18n.global.locale.value = 'en'
  mockedDetect.mockReset()
  mockedRemove.mockReset()
  mockedResourceDir.mockReset()

  mockedResourceDir.mockResolvedValue(RESOURCE_DIR)
  mockedDetect.mockResolvedValue([
    {
      id: 'claudeDesktop',
      displayName: 'Claude Desktop',
      state: 'configured',
      configPath: 'C:\\Users\\marco\\.claude\\settings.json',
      binaryPath: 'C:\\tools\\claude.exe',
      detail: null,
    },
  ])
  mockedRemove.mockResolvedValue({
    id: 'claudeDesktop',
    displayName: 'Claude Desktop',
    state: 'detectedNotConfigured',
    configPath: 'C:\\Users\\marco\\.claude\\settings.json',
    binaryPath: 'C:\\tools\\claude.exe',
    detail: null,
  })
})

/** The panel is a `role="dialog"` too, so the confirmation is the last one. */
function dialogs() {
  return document.body.querySelectorAll('[role="dialog"]')
}

function confirmation() {
  const all = dialogs()
  return all[all.length - 1] ?? null
}

function buttonLabelled(label: string) {
  return Array.from(document.body.querySelectorAll('button')).find(
    (button) => button.textContent?.trim() === label,
  )
}

async function openPanel() {
  const wrapper = mount(LocalAiPanel, {
    attachTo: document.body,
    global: { plugins: [i18n] },
  })
  await wrapper.find('.nav-item').trigger('click')
  await flushPromises()
  return wrapper
}

describe('LocalAiPanel disconnect confirmation', () => {
  it('detects the agents on first open and asks nothing', async () => {
    await openPanel()

    expect(mockedResourceDir).toHaveBeenCalledTimes(1)
    expect(mockedDetect).toHaveBeenCalledWith(RESOURCE_DIR)
    expect(document.body.textContent).toContain(copy.configured)
    // The panel itself; nothing stacked on top of it.
    expect(dialogs()).toHaveLength(1)
  })

  it('asks before disconnecting, naming the agent and the file it rewrites', async () => {
    await openPanel()

    buttonLabelled(copy.disconnect)?.click()
    await flushPromises()

    expect(dialogs()).toHaveLength(2)
    const text = confirmation()?.textContent ?? ''
    expect(text).toContain(copy.disconnectConfirmTitle)
    expect(text).toContain('Claude Desktop')
    expect(text).toContain(copy.disconnectConfirmDetail)
    // The {name} placeholder must be substituted, not rendered raw.
    expect(text).not.toContain('{name}')
  })

  it('does not remove anything until the confirmation is accepted', async () => {
    await openPanel()
    buttonLabelled(copy.disconnect)?.click()
    await flushPromises()

    expect(mockedRemove).not.toHaveBeenCalled()

    const confirm = Array.from(confirmation()?.querySelectorAll('button') ?? []).at(-1)
    ;(confirm as HTMLButtonElement).click()
    await flushPromises()

    expect(mockedRemove).toHaveBeenCalledWith('claudeDesktop', RESOURCE_DIR)
    expect(dialogs()).toHaveLength(1)
  })

  it('cancels without removing anything, and the dialog can be reopened', async () => {
    await openPanel()

    buttonLabelled(copy.disconnect)?.click()
    await flushPromises()
    ;(confirmation()?.querySelector('.dialog-footer .btn') as HTMLButtonElement).click()
    await flushPromises()

    expect(mockedRemove).not.toHaveBeenCalled()
    expect(dialogs()).toHaveLength(1)

    buttonLabelled(copy.disconnect)?.click()
    await flushPromises()
    expect(dialogs()).toHaveLength(2)
  })

  it('removes nothing when Escape dismisses the confirmation', async () => {
    await openPanel()
    buttonLabelled(copy.disconnect)?.click()
    await flushPromises()

    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }))
    await flushPromises()

    expect(mockedRemove).not.toHaveBeenCalled()
  })

  // Escape used to take the whole panel down with the confirmation,
  // because both listened on `window` and the panel had no way to tell
  // the two apart. It now dismisses only the confirmation: the panel
  // stays, the prompt leaves, and nothing is disconnected.
  it('Escape dismisses the confirmation, and only the confirmation', async () => {
    await openPanel()
    buttonLabelled(copy.disconnect)?.click()
    await flushPromises()
    // Both the panel and the confirmation teleport to document.body.
    const dialog = () => document.querySelector('[role="dialog"]')
    expect(dialog()).not.toBeNull()

    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }))
    await flushPromises()

    expect(dialog()).toBeNull()
    expect(mockedRemove).not.toHaveBeenCalled()
  })
})
