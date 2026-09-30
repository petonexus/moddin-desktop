import { enableAutoUnmount, flushPromises, mount } from '@vue/test-utils'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import OpenXrManager from '../OpenXrManager.vue'
import { openXrCopyForLocale } from '../copy'
import { i18n } from '../../../i18n'
import type { OpenXrState } from '../types'

/**
 * ROADMAP UX-11, call site 2: making a runtime the Windows-wide
 * default. This one writes to HKLM, prompts for elevation, and changes
 * every VR game on the machine — not just the one on screen. The dialog
 * has to say so.
 */

vi.mock('../service', () => ({
  detectOpenXrGames: vi.fn(),
  inspectOpenXr: vi.fn(),
  setGameOpenXrRuntime: vi.fn(),
  setSystemOpenXrRuntime: vi.fn(),
}))

import { inspectOpenXr, setSystemOpenXrRuntime } from '../service'

const mockedInspect = vi.mocked(inspectOpenXr)
const mockedSetSystem = vi.mocked(setSystemOpenXrRuntime)

const copy = openXrCopyForLocale('en')
const MANIFEST = 'D:\\OpenXR\\oculus\\openxr_manifest.json'

const STATE: OpenXrState = {
  activeRuntime: 'steamvr',
  activeRuntimeName: 'SteamVR',
  gameOverride: null,
  gameOverrideName: null,
  effectiveRuntime: 'steamvr',
  effectiveRuntimeName: 'SteamVR',
  effectiveSource: 'system',
  runtimes: [
    {
      name: 'SteamVR',
      manifestPath: 'C:\\Steam\\steamxr\\openxr_manifest.json',
      libraryPath: 'C:\\Steam\\steamxr',
      manifestExists: true,
      libraryExists: true,
      enabled: true,
      active: true,
    },
    {
      name: 'Oculus',
      manifestPath: MANIFEST,
      libraryPath: 'D:\\OpenXR\\oculus',
      manifestExists: true,
      libraryExists: true,
      enabled: true,
      active: false,
    },
  ],
  warnings: [],
}

enableAutoUnmount(afterEach)

beforeEach(() => {
  window.localStorage.clear()
  i18n.global.locale.value = 'en'
  mockedInspect.mockReset()
  mockedSetSystem.mockReset()
  mockedInspect.mockResolvedValue(STATE)
  mockedSetSystem.mockResolvedValue(STATE)
})

/** The manager panel is a `role="dialog"` too; the confirmation is last. */
function dialogs() {
  return document.body.querySelectorAll('[role="dialog"]')
}

function confirmation() {
  const all = dialogs()
  return all[all.length - 1] ?? null
}

/** One "Make default" button per runtime; the active runtime's is disabled. */
async function openManager() {
  const wrapper = mount(OpenXrManager, {
    attachTo: document.body,
    global: { plugins: [i18n] },
  })
  await wrapper.find('.nav-item').trigger('click')
  await flushPromises()
  return wrapper
}

/** One "Make default" button per runtime; the active runtime's is disabled. */
function makeDefaultButton() {
  return Array.from(document.body.querySelectorAll('button')).find(
    (button) => button.textContent?.trim() === copy.makeSystem && !button.disabled,
  )
}

describe('OpenXrManager Windows-wide runtime confirmation', () => {
  it('inspects on open and asks nothing', async () => {
    await openManager()

    expect(mockedInspect).toHaveBeenCalled()
    expect(document.body.textContent).toContain('Oculus')
    expect(dialogs()).toHaveLength(1)
  })

  it('states the Windows-wide scope and the elevation prompt before applying', async () => {
    await openManager()

    makeDefaultButton()?.click()
    await flushPromises()

    expect(dialogs()).toHaveLength(2)
    const text = confirmation()?.textContent ?? ''
    expect(text).toContain(copy.makeSystemConfirmTitle)
    expect(text).toContain(copy.makeSystemConfirmDescription)
    expect(text).toContain(copy.makeSystemConfirmScope)
    expect(text).toContain(copy.makeSystemConfirmAdmin)
  })

  it('does not write HKLM until the confirmation is accepted', async () => {
    await openManager()
    makeDefaultButton()?.click()
    await flushPromises()

    expect(mockedSetSystem).not.toHaveBeenCalled()

    const confirm = Array.from(confirmation()?.querySelectorAll('button') ?? []).at(-1)
    ;(confirm as HTMLButtonElement).click()
    await flushPromises()

    expect(mockedSetSystem).toHaveBeenCalledTimes(1)
    expect(String(mockedSetSystem.mock.calls[0][0])).toContain('openxr_manifest.json')
    expect(dialogs()).toHaveLength(1)
  })

  it('cancels without writing anything, and the dialog can be reopened', async () => {
    await openManager()

    makeDefaultButton()?.click()
    await flushPromises()
    ;(confirmation()?.querySelector('.dialog-footer .btn') as HTMLButtonElement).click()
    await flushPromises()

    expect(mockedSetSystem).not.toHaveBeenCalled()
    expect(dialogs()).toHaveLength(1)

    makeDefaultButton()?.click()
    await flushPromises()
    expect(dialogs()).toHaveLength(2)
  })

  it('removes nothing when Escape dismisses the confirmation', async () => {
    await openManager()
    makeDefaultButton()?.click()
    await flushPromises()

    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }))
    await flushPromises()

    expect(mockedSetSystem).not.toHaveBeenCalled()
  })
})
