import { enableAutoUnmount, flushPromises, mount } from '@vue/test-utils'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import ProfilesPanel from '../ProfilesPanel.vue'
import { i18n } from '../../../i18n'
import {
  listTransactions,
  loadProfileContext,
  pickProfileFile,
  readProfileFile,
  revealProfileFile,
  saveProfileFile,
} from '../service'
import { installCapability } from '../../capability-modules/service'
import { PROFILE_KIND, PROFILE_SCHEMA_VERSION, type ProfileImportContext } from '../types'
import type { CapabilitySpec, CapabilitySummary } from '../../../types/capability'

/**
 * The panel's contract, in the order a user meets it.
 *
 * Reading a profile shows a preview and changes nothing; the preview
 * names what will be skipped and why; Apply is the only way past it,
 * behind a confirmation; and a failure arrives as the shared
 * `ErrorCallout` rather than a raw `err.message`. Every refusal is
 * paired with the matching success, because a guard that passes because
 * the button is dead proves nothing.
 */

vi.mock('../service', () => ({
  listTransactions: vi.fn(),
  loadProfileContext: vi.fn(),
  pickProfileFile: vi.fn(),
  readProfileFile: vi.fn(),
  revealProfileFile: vi.fn(),
  saveProfileFile: vi.fn(),
}))

vi.mock('../../capability-modules/service', () => ({
  installCapability: vi.fn(),
}))

const mockedList = vi.mocked(listTransactions)
const mockedContext = vi.mocked(loadProfileContext)
const mockedPick = vi.mocked(pickProfileFile)
const mockedRead = vi.mocked(readProfileFile)
const mockedSave = vi.mocked(saveProfileFile)
const mockedReveal = vi.mocked(revealProfileFile)
const mockedInstall = vi.mocked(installCapability)

enableAutoUnmount(afterEach)

// The panel and its confirmation attach to `document.body`. A dialog
// left behind by one test would be found by the next one's query, which
// is exactly the kind of leak that makes a "no dialog is shown"
// assertion pass.
afterEach(() => {
  document.body.innerHTML = ''
})

const SPEC: CapabilitySpec = {
  id: 'reshade',
  displayName: 'ReShade',
  category: 'graphics',
  status: 'available',
  configSchema: [{ name: 'preset', type: 'string' }, { name: 'modDirectory', type: 'path' }],
}

const SUMMARY: CapabilitySummary = {
  id: 'reshade',
  displayName: 'ReShade',
  category: 'graphics',
  status: 'available',
  origin: 'builtIn',
  supportedEngines: [],
  engineMatch: { verdict: 'engineAgnostic' },
}

function importContext(overrides: Partial<ProfileImportContext> = {}): ProfileImportContext {
  return {
    capabilities: new Map([['reshade', SUMMARY]]),
    specs: new Map([['reshade', SPEC]]),
    installedByGame: new Map(),
    targets: new Map([['elden-ring', {
      gameId: 'elden-ring',
      gameName: 'Elden Ring',
      installDir: 'C:\\games\\elden-ring',
      executableDir: 'C:\\games\\elden-ring\\Game',
      engine: 'Unreal Engine',
      engineVersion: 'UE5',
    }]]),
    revoked: new Map(),
    ...overrides,
  }
}

function documentJson(capabilities: Array<Record<string, unknown>>) {
  return JSON.stringify({
    kind: PROFILE_KIND,
    schemaVersion: PROFILE_SCHEMA_VERSION,
    appVersion: '0.1.0',
    exportedAt: '2026-01-31T10:00:00.000Z',
    games: [{ gameId: 'elden-ring', gameName: 'Elden Ring', capabilities }],
  })
}

const GOOD_ENTRY = {
  id: 'reshade',
  displayName: 'ReShade',
  config: { values: { preset: 'ultra' } },
  omittedSecrets: [],
}

const GONE_ENTRY = {
  id: 'mod-that-was-removed',
  displayName: 'Mod That Was Removed',
  config: { values: {} },
  omittedSecrets: [],
}

const APPLIED = {
  id: 'tx-1',
  createdAt: 0,
  kind: 'reshade',
  label: 'ReShade',
  gameId: 'elden-ring',
  targetPath: 'C:\\games\\elden-ring\\dxgi.dll',
  backupPath: 'C:\\backup\\1',
  status: 'applied',
}

// jsdom reports `navigator.language = en-US`, so the app would boot in
// English here and the assertions below would pass by accident on a
// machine configured differently. Pin it.
beforeEach(() => {
  i18n.global.locale.value = 'en'
  mockedList.mockReset()
  mockedContext.mockReset()
  mockedRead.mockReset()
  mockedPick.mockReset()
  mockedSave.mockReset()
  mockedReveal.mockReset()
  mockedInstall.mockReset()
  mockedList.mockResolvedValue([])
  mockedContext.mockResolvedValue(importContext())
  mockedRead.mockResolvedValue({ path: 'C:\\inbox\\profile.json', contents: documentJson([GOOD_ENTRY]) })
  mockedSave.mockResolvedValue({ path: 'C:\\Users\\you\\Desktop\\p.json', bytes: 64 })
  mockedReveal.mockResolvedValue(null)
})

async function render() {
  const wrapper = mount(ProfilesPanel, { attachTo: document.body, global: { plugins: [i18n] } })
  await wrapper.find('.nav-item').trigger('click')
  await flushPromises()
  return wrapper
}

/**
 * The panel is teleported to `document.body`, so everything after mount
 * is driven through the real DOM the way a user's events arrive.
 * `.profile-dialog` is the one hook that tells the panel apart from the
 * confirmation stacked on top of it.
 */
function panel() {
  return document.body.querySelector('.profile-dialog [role="dialog"]')
}

function confirmDialog() {
  return [...document.body.querySelectorAll('[role="dialog"]')]
    .find((dialog) => !dialog.closest('.profile-dialog')) ?? null
}

function callouts() {
  return [...document.body.querySelectorAll('.callout-danger')]
}

function panelInputs() {
  return [...(panel()?.querySelectorAll('input') ?? [])] as HTMLInputElement[]
}

function panelRowButtons() {
  return [...(panel()?.querySelectorAll('.profile-row button') ?? [])] as HTMLButtonElement[]
}

function buttonLabelled(label: string, scope: Element | null = panel()) {
  return [...(scope?.querySelectorAll('button') ?? [])]
    .find((button) => button.textContent?.trim() === label) ?? null
}

function footerButton(label: string, scope: Element | null = panel()) {
  return [...(scope?.querySelectorAll('.dialog-footer button') ?? [])]
    .find((button) => button.textContent?.trim() === label) ?? null
}

/** v-model on a real input: set the value, then fire the event. */
async function setInput(element: HTMLInputElement, value: string) {
  element.value = value
  element.dispatchEvent(new Event('input'))
  await flushPromises()
}

async function click(element: Element | null | undefined) {
  element?.dispatchEvent(new MouseEvent('click', { bubbles: true }))
  await new Promise((resolve) => setTimeout(resolve, 0))
  await flushPromises()
}

/**
 * Fill the import path and press "Read file".
 *
 * By label, not by position. The import row holds a "Browse" button and a
 * "Read file" button, so `panelRowButtons()[1]` is whichever of the two was
 * added last, and every test that went through this helper broke the day
 * the other button appeared.
 */
async function readProfile() {
  await setInput(panelInputs()[1], 'C:\\inbox\\profile.json')
  await click(buttonLabelled('Read file'))
}

async function pressApply() {
  await click(footerButton('Apply 1'))
}

describe('ProfilesPanel — opening', () => {
  it('opens as a sidebar entry and states the secrets rule before anything is written', async () => {
    const wrapper = await render()

    expect(wrapper.find('.nav-item').text()).toContain('Profiles')
    expect(panel()).not.toBeNull()
    const text = panel()?.textContent ?? ''
    expect(text).toContain('No folders, no drives, no usernames')
    expect(text).toContain('a setting that points at a place on disk is never written to it')
  })

  it('offers no Apply control until a profile has been read', async () => {
    await render()

    expect(panel()?.textContent).toContain('No profile loaded')
    const footer = [...panel()!.querySelectorAll('.dialog-footer button')]
      .map((button) => button.textContent?.trim())
    expect(footer).toEqual(['Cancel'])
    expect(mockedInstall).not.toHaveBeenCalled()
  })
})

describe('ProfilesPanel — previewing an import', () => {
  it('shows what would change per game, and applies nothing', async () => {
    await render()
    await readProfile()

    const text = panel()?.textContent ?? ''
    expect(text).toContain('What Moddin will do')
    expect(text).toContain('Install ReShade in Elden Ring')
    expect(text).toContain('Elden Ring')
    expect(text).toContain('Installed here')
    expect(text).toContain('C:\\inbox\\profile.json')
    // The shared ChangePreview always carries the backup guarantee.
    expect(text).toContain('A backup is made before any change')
    expect(mockedInstall).not.toHaveBeenCalled()
  })

  it('says what a profile would not touch', async () => {
    await render()
    await readProfile()

    expect(panel()?.textContent).toContain('Only what the file names is touched in Elden Ring')
  })

  it('reports a capability this build does not have instead of offering to install it', async () => {
    mockedRead.mockResolvedValue({
      path: 'C:\\inbox\\profile.json',
      contents: documentJson([GONE_ENTRY, GOOD_ENTRY]),
    })
    await render()
    await readProfile()

    const text = panel()?.textContent ?? ''
    expect(text).toContain('Mod That Was Removed in Elden Ring will be skipped')
    expect(text).toContain('Moddin no longer has this capability')
    // The rest of the file still applies — one withdrawn mod does not
    // cost the user the rest of their setup.
    expect(text).toContain('Install ReShade in Elden Ring')
    expect(mockedInstall).not.toHaveBeenCalled()
  })

  it('refuses a newer format with a message, not a crash', async () => {
    mockedRead.mockResolvedValue({
      path: 'C:\\inbox\\future.json',
      contents: JSON.stringify({
        kind: PROFILE_KIND,
        schemaVersion: PROFILE_SCHEMA_VERSION + 1,
        appVersion: '9.9.9',
        exportedAt: '2027-01-01T00:00:00.000Z',
        games: [],
      }),
    })
    await render()
    await readProfile()

    expect(callouts()).toHaveLength(1)
    expect(callouts()[0].textContent).toContain('written by a newer Moddin')
    expect(callouts()[0].textContent).toContain(String(PROFILE_SCHEMA_VERSION + 1))
    // The refusal is copy, not a stack trace: no technical disclosure.
    expect(panel()?.querySelector('.callout-raw')).toBeNull()
    expect(panel()?.textContent).toContain('No profile loaded')
    expect(mockedInstall).not.toHaveBeenCalled()
  })

  it('changes nothing when the preview is discarded', async () => {
    await render()
    await readProfile()
    expect(panel()?.textContent).toContain('What Moddin will do')

    await click(footerButton('Discard preview'))

    expect(panel()?.textContent).toContain('No profile loaded')
    expect(mockedInstall).not.toHaveBeenCalled()
  })

  it('closes on Escape without installing anything', async () => {
    await render()
    await readProfile()
    expect(panel()).not.toBeNull()

    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }))
    await flushPromises()

    expect(panel()).toBeNull()
    expect(mockedInstall).not.toHaveBeenCalled()
  })
})

describe('ProfilesPanel — applying an import', () => {
  it('keeps the preview visible until an in-flight install completes', async () => {
    let release = () => {}
    mockedInstall.mockReturnValueOnce(new Promise((resolve) => {
      release = () => resolve({ capabilityId: 'reshade', transaction: null, steps: [], affectedPaths: [] })
    }))
    await render()
    await readProfile()
    await pressApply()
    await click(footerButton('Apply 1', confirmDialog()))
    expect(mockedInstall).toHaveBeenCalledTimes(1)
    const activePanel = panel()
    expect((footerButton('Cancel') as HTMLButtonElement).disabled).toBe(true)

    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', cancelable: true }))
    await flushPromises()
    expect(panel()).toBe(activePanel)
    expect(panel()?.getAttribute('aria-busy')).toBe('true')

    release()
    await flushPromises()
    expect(panel()?.textContent).toContain('1 change(s) applied')
    expect((footerButton('Cancel') as HTMLButtonElement).disabled).toBe(false)
  })

  it('asks first, names the change, and keeps the focus off the confirm button', async () => {
    await render()
    await readProfile()
    await pressApply()

    const confirmation = confirmDialog()
    expect(confirmation).not.toBeNull()
    expect(confirmation?.textContent).toContain('Apply this profile?')
    expect(confirmation?.textContent).toContain('1 change(s) are made to your game folders')
    expect(confirmation?.textContent).toContain('Install ReShade in Elden Ring')
    expect(confirmation?.textContent).toContain('undo each change on its own')
    expect(mockedInstall).not.toHaveBeenCalled()

    const confirm = confirmation?.querySelector('.dialog-footer .btn-primary') as HTMLButtonElement
    expect(confirm.getAttribute('autofocus')).toBeNull()
    expect(document.activeElement).not.toBe(confirm)
  })

  it('installs through the existing capability command and reports the change', async () => {
    mockedInstall.mockResolvedValue({ capabilityId: 'reshade', transaction: null, steps: [], affectedPaths: [] })
    const wrapper = await render()
    await readProfile()
    await pressApply()

    await click(footerButton('Apply 1', confirmDialog()))

    expect(mockedInstall).toHaveBeenCalledTimes(1)
    expect(mockedInstall.mock.calls[0][0]).toMatchObject({
      capabilityId: 'reshade',
      gameId: 'elden-ring',
      installDir: 'C:\\games\\elden-ring',
      config: { values: { preset: 'ultra' } },
    })
    expect(wrapper.emitted('changed')).toHaveLength(1)
    expect(panel()?.textContent).toContain('1 change(s) applied')
  })

  it('explains a failed install through ErrorCallout instead of dumping err.message', async () => {
    mockedInstall.mockRejectedValue(new Error('C:\\games\\elden-ring is still running'))
    const wrapper = await render()
    await readProfile()
    await pressApply()

    await click(footerButton('Apply 1', confirmDialog()))

    const callout = panel()?.querySelector('.callout-danger')
    expect(callout).not.toBeNull()
    // `role="alert"` and the disclosure come from the shared
    // `ErrorCallout`; a `{{ err.message }}` paragraph has neither.
    expect(callout?.getAttribute('role')).toBe('alert')
    expect(callout?.querySelector('strong')?.textContent).toBe('The import stopped part-way')
    expect(callout?.querySelector('p')?.textContent).toBe(
      'Some changes may already be installed. Check History before applying again — each one can be undone on its own.',
    )
    expect(callout?.querySelector('p')?.textContent).not.toContain('still running')
    // The untranslated string stays reachable for a bug report.
    expect(callout?.querySelector('.callout-raw code')?.textContent).toContain('still running')
    expect(wrapper.emitted('changed')).toBeUndefined()
  })

  it('explains an unreadable file through ErrorCallout', async () => {
    mockedRead.mockRejectedValue(
      new Error('Could not open that file: The system cannot find the file specified'),
    )
    await render()
    await readProfile()

    const callout = panel()?.querySelector('.callout-danger')
    expect(callout?.querySelector('strong')?.textContent).toBe('The profile was not read')
    expect(callout?.querySelector('p')?.textContent).not.toContain('The system cannot find')
    expect(callout?.querySelector('.callout-raw code')?.textContent).toContain('The system cannot find')
  })
})

describe('ProfilesPanel — exporting a profile', () => {
  it('retains the save result when Escape is pressed while exporting', async () => {
    mockedList.mockResolvedValue([APPLIED])
    let release = () => {}
    mockedSave.mockReturnValueOnce(new Promise((resolve) => {
      release = () => resolve({ path: 'C:\\Users\\you\\Desktop\\p.json', bytes: 64 })
    }))
    await render()
    await click(panelRowButtons()[0])
    const activePanel = panel()
    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', cancelable: true }))
    await flushPromises()
    expect(panel()).toBe(activePanel)
    expect((footerButton('Cancel') as HTMLButtonElement).disabled).toBe(true)
    release()
    await flushPromises()
    expect(panel()?.textContent).toContain('C:\\Users\\you\\Desktop\\p.json')
  })

  it('writes the file and shows where it went', async () => {
    mockedList.mockResolvedValue([APPLIED])
    await render()

    await click(panelRowButtons()[0])

    expect(mockedSave).toHaveBeenCalledTimes(1)
    expect(panel()?.textContent).toContain('C:\\Users\\you\\Desktop\\p.json')
  })

  it('says there is nothing to export rather than writing an empty file', async () => {
    await render()

    await click(panelRowButtons()[0])

    expect(callouts()[0].textContent).toContain('There is nothing to export yet')
    expect(mockedSave).not.toHaveBeenCalled()
  })

  it('will not write a name that is not a .json file', async () => {
    mockedList.mockResolvedValue([APPLIED])
    await render()
    await setInput(panelInputs()[0], 'profile.txt')

    expect(panelRowButtons()[0].disabled).toBe(true)
    await click(panelRowButtons()[0])

    expect(mockedSave).not.toHaveBeenCalled()
  })

  it('opens the folder the file went to on request', async () => {
    mockedList.mockResolvedValue([APPLIED])
    await render()
    await click(panelRowButtons()[0])

    await click(buttonLabelled('Show the file'))

    expect(mockedReveal).toHaveBeenCalledWith('C:\\Users\\you\\Desktop\\p.json')
  })
})
