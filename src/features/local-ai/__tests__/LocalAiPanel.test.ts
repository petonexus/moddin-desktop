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
  it('keeps the panel visible until the agent configuration write finishes', async () => {
    let release = () => {}
    mockedRemove.mockReturnValueOnce(new Promise((resolve) => {
      release = () => resolve({
        id: 'claudeDesktop', displayName: 'Claude Desktop', state: 'detectedNotConfigured',
        configPath: 'C:\\Users\\marco\\.claude\\settings.json', binaryPath: 'C:\\tools\\claude.exe', detail: null,
      })
    }))
    await openPanel()
    buttonLabelled(copy.disconnect)?.click()
    await flushPromises()
    ;(confirmation()?.querySelector('.btn-danger-solid') as HTMLButtonElement).click()
    await flushPromises()
    const panel = dialogs()[0]
    expect(panel.getAttribute('aria-busy')).toBe('true')
    expect(buttonLabelled(copy.close)?.disabled).toBe(true)
    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', cancelable: true }))
    await flushPromises()
    expect(dialogs()[0]).toBe(panel)
    release()
    await flushPromises()
    expect(buttonLabelled(copy.close)?.disabled).toBe(false)
  })

  it('allows dismissing the panel during read-only agent detection', async () => {
    let release = () => {}
    mockedDetect.mockReturnValueOnce(new Promise((resolve) => { release = () => resolve([]) }))
    await openPanel()
    expect(dialogs()).toHaveLength(1)
    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', cancelable: true }))
    await flushPromises()
    expect(dialogs()).toHaveLength(0)
    release()
    await flushPromises()
  })

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
    const panel = dialogs()[0]
    expect(dialogs()).toHaveLength(2)

    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }))
    await flushPromises()

    expect(dialogs()).toHaveLength(1)
    expect(dialogs()[0]).toBe(panel)
    expect(mockedRemove).not.toHaveBeenCalled()

    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }))
    await flushPromises()
    expect(dialogs()).toHaveLength(0)
  })
})

/**
 * ROADMAP UX-21 for `agent.detail`, and UX-26/29 for the agent row.
 *
 * `detect` is mocked, so the agents arrive in the shape the real service
 * hands over: the backend's own sentence as `text`, the locale key
 * beside it when the table declares one.
 */
describe('LocalAiPanel agent detail and row actions', () => {
  const NOT_INSTALLED = {
    id: 'cursor' as const,
    displayName: 'Cursor',
    state: 'notInstalled' as const,
    configPath: null,
    binaryPath: null,
    detail: {
      text: 'AI tool is not installed on this PC.',
      key: 'localAiDetailNotInstalled' as const,
    },
  }

  beforeEach(() => {
    mockedDetect.mockResolvedValue([NOT_INSTALLED])
  })

  it('renders the locale sentence for a declared detail', async () => {
    await openPanel()

    expect(document.body.querySelector('.agent-detail')?.textContent)
      .toBe('This AI tool is not installed on this PC.')
  })

  it('carries the interpolated path through the key', async () => {
    mockedDetect.mockResolvedValue([
      {
        ...NOT_INSTALLED,
        state: 'detectedNotConfigured' as const,
        binaryPath: 'C:\\tools\\cursor.exe',
        detail: {
          text: 'AI tool installed at C:\\tools\\cursor.exe. Click Connect to register Moddin.',
          key: 'localAiDetailInstalledAt' as const,
          params: { value: 'C:\\tools\\cursor.exe' },
        },
      },
    ])
    await openPanel()

    expect(document.body.querySelector('.agent-detail')?.textContent)
      .toBe('Installed at C:\\tools\\cursor.exe. Click Connect to register Moddin.')
  })

  it('keeps a real I/O error as the detail', async () => {
    mockedDetect.mockResolvedValue([
      {
        ...NOT_INSTALLED,
        state: 'configError' as const,
        detail: { text: 'EACCES: permission denied (os error 5)' },
      },
    ])
    await openPanel()

    expect(document.body.querySelector('.agent-detail')?.textContent)
      .toBe('EACCES: permission denied (os error 5)')
  })

  it('names the connect button after the agent, and marks the row busy', async () => {
    mockedDetect.mockResolvedValue([{ ...NOT_INSTALLED, state: 'detectedNotConfigured' as const }])
    await openPanel()

    const card = document.body.querySelector('.agent-card') as HTMLElement
    expect(card.getAttribute('aria-busy')).toBe('false')
    const connect = Array.from(card.querySelectorAll('button'))
      .find((button) => button.textContent?.trim() === copy.connect)
    expect(connect?.getAttribute('aria-label')).toBe('Connect Cursor')
  })

  it('names the disconnect button after the agent too', async () => {
    mockedDetect.mockResolvedValue([
      {
        id: 'claudeDesktop' as const,
        displayName: 'Claude Desktop',
        state: 'configured' as const,
        configPath: 'C:\\Users\\marco\\.claude\\settings.json',
        binaryPath: 'C:\\tools\\claude.exe',
        detail: null,
      },
    ])
    await openPanel()

    const disconnect = Array.from(document.body.querySelectorAll('button'))
      .find((button) => button.textContent?.trim() === copy.disconnect)
    expect(disconnect?.getAttribute('aria-label')).toBe('Disconnect Claude Desktop')
  })
})
