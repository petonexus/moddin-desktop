import { enableAutoUnmount, flushPromises, mount } from '@vue/test-utils'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import CapabilityModulesSection from '../CapabilityModulesSection.vue'
import { i18n } from '../../../i18n'
import type { InstallResult } from '../../../types/capability'
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

/**
 * ROADMAP UX-11, call site 1: capability removal, plus the card state
 * the other capability work depends on — a failed check must never read
 * as a healthy module.
 *
 * `../service` is mocked so nothing reaches Tauri; the composable under
 * test is the real one, because that is where the state lives.
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
  getCapabilitySpec,
  installCapability,
  listCapabilities,
  uninstallCapability,
} from '../service'

const mockedList = vi.mocked(listCapabilities)
const mockedSpec = vi.mocked(getCapabilitySpec)
const mockedInstall = vi.mocked(installCapability)
const mockedUninstall = vi.mocked(uninstallCapability)
const mockedVerify = vi.mocked(evaluateCapability)

enableAutoUnmount(afterEach)

const CAPABILITY = {
  id: 'bepinex',
  displayName: 'BepInEx',
  description: 'Loads plugins into the game.',
  category: 'qol',
  status: 'available',
  origin: 'builtIn',
} as const

beforeEach(() => {
  i18n.global.locale.value = 'en'
  mockedList.mockReset()
  mockedSpec.mockReset()
  mockedInstall.mockReset()
  mockedUninstall.mockReset()
  mockedVerify.mockReset()

  mockedList.mockResolvedValue([{ ...CAPABILITY }])
  mockedSpec.mockImplementation(async (id: string) => ({
    id,
    displayName: id,
    category: 'qol',
    status: 'available',
  }))
  mockedInstall.mockResolvedValue({
    capabilityId: 'bepinex',
    transaction: null,
    steps: [],
    affectedPaths: [],
  })
  mockedUninstall.mockResolvedValue(transaction({ id: 'tx-2' }))
  mockedVerify.mockResolvedValue(report([]))
})

function report(
  checks: Array<{ id: string; label: string; passed: boolean; detail?: string }>,
): CapabilityVerificationReport {
  const failing = checks.some((check) => !check.passed)
  return {
    status: failing ? 'failed' : 'ok',
    summary: failing ? '1 check failed' : 'All checks passed',
    checks,
    gameRunning: false,
    installed: true,
  }
}

const INSTALLED: InstallResult = {
  capabilityId: 'bepinex',
  transaction: null,
  steps: [],
  affectedPaths: [],
}

function transaction(overrides: Partial<TransactionRecord> = {}): TransactionRecord {
  return {
    id: 'tx-1',
    createdAt: 1_700_000_000_000,
    kind: 'bepinex',
    label: 'BepInEx',
    gameId: 'stalker-2',
    targetPath: 'C:\\games\\stalker-2\\BepInEx',
    backupPath: 'C:\\backup\\tx-1',
    status: 'applied',
    ...overrides,
  }
}

async function render(transactions: TransactionRecord[] = []) {
  const wrapper = mount(CapabilityModulesSection, {
    props: {
      gameId: 'stalker-2',
      gameName: 'S.T.A.L.K.E.R. 2',
      installDir: 'C:\\games\\stalker-2',
      executableDir: 'C:\\games\\stalker-2\\Binaries\\Win64',
      engine: 'unreal5',
      excludeIds: [],
      transactions,
    },
    attachTo: document.body,
    global: { plugins: [i18n] },
  })
  await flushPromises()
  return wrapper
}

/** The ConfirmDialog renders inline (no Teleport) inside the section. */
function dialog() {
  return document.body.querySelector('[role="dialog"]')
}

function buttonLabelled(wrapper: Awaited<ReturnType<typeof render>>, label: string) {
  return wrapper.findAll('button').find((button) => button.text() === label)
}

describe('capability card rendering', () => {
  it('renders one card per capability with its description, not its id', async () => {
    const wrapper = await render()
    const card = wrapper.find('.module-card')
    expect(card.text()).toContain('BepInEx')
    expect(card.text()).toContain('Loads plugins into the game.')
  })

  it('offers no Remove action while the capability is not installed', async () => {
    const wrapper = await render([])
    expect(buttonLabelled(wrapper, 'Remove')).toBeUndefined()
  })

  it('marks a card Active only because the transaction history says so', async () => {
    expect((await render([])).find('.module-card').classes()).toContain('is-available')
    expect((await render([transaction()])).find('.module-card').classes()).toContain('is-active')
  })
})

describe('card state derived from verification and busy flags', () => {
  it('shows a checking state while the verification is in flight', async () => {
    const gate = deferred<CapabilityVerificationReport>()
    mockedVerify.mockImplementation(() => gate.promise)
    const wrapper = await render([transaction()])

    void buttonLabelled(wrapper, 'Check')?.trigger('click')
    await flushPromises()

    const card = wrapper.find('.module-card')
    expect(card.classes()).toContain('is-checking')
    expect(card.text()).toContain('Checking…')
    // Busy means busy: nothing else on the card may be pressed.
    expect(buttonLabelled(wrapper, 'Reinstall')?.attributes('disabled')).toBeDefined()
    expect(buttonLabelled(wrapper, 'Remove')?.attributes('disabled')).toBeDefined()

    gate.resolve(report([{ id: 'dll', label: 'BepInEx DLL present', passed: true }]))
    await flushPromises()
    expect(wrapper.find('.module-card').classes()).toContain('is-active')
  })

  it('does not render a failed check as a healthy module', async () => {
    mockedVerify.mockResolvedValue(
      report([{ id: 'dll', label: 'BepInEx DLL present', passed: false, detail: 'not found' }]),
    )
    const wrapper = await render([transaction()])

    await buttonLabelled(wrapper, 'Check')?.trigger('click')
    await flushPromises()

    const card = wrapper.find('.module-card')
    expect(card.classes()).toContain('is-attention')
    expect(card.classes()).not.toContain('is-active')
    expect(card.text()).toContain('Needs attention')
    // The failure is surfaced in the card body, not hidden behind the
    // details disclosure.
    expect(card.find('.module-card-issues').text()).toContain('BepInEx DLL present')
    expect(card.find('.checklist li.fail').text()).toContain('not found')
  })

  it('goes back to a healthy state once every check passes', async () => {
    mockedVerify.mockResolvedValue(report([{ id: 'dll', label: 'BepInEx DLL present', passed: true }]))
    const wrapper = await render([transaction()])

    await buttonLabelled(wrapper, 'Check')?.trigger('click')
    await flushPromises()

    const card = wrapper.find('.module-card')
    expect(card.classes()).toContain('is-active')
    expect(card.find('.module-card-issues').exists()).toBe(false)
    expect(card.find('.checklist li.ok').exists()).toBe(true)
  })

  it('leaves the last verdict in place and reports a thrown check as a callout', async () => {
    mockedVerify.mockResolvedValue(report([{ id: 'dll', label: 'BepInEx DLL present', passed: false }]))
    const wrapper = await render([transaction()])
    await buttonLabelled(wrapper, 'Check')?.trigger('click')
    await flushPromises()
    expect(wrapper.find('.module-card').classes()).toContain('is-attention')

    mockedVerify.mockRejectedValue(new Error('capability_evaluate timed out'))
    await buttonLabelled(wrapper, 'Check')?.trigger('click')
    await flushPromises()

    // A thrown check does not clear the previous report, so the card
    // keeps the verdict it already had and the failure surfaces as a
    // callout. That is the safe direction (a stale failure still reads
    // as broken) but see the note on the install/check race below: the
    // reverse — a stale pass — is not covered by this rule.
    const card = wrapper.find('.module-card')
    expect(card.classes()).toContain('is-attention')
    expect(wrapper.find('.callout-danger').text()).toContain('capability_evaluate timed out')
  })

  it('disables install and remove while an install is in flight', async () => {
    const gate = deferred<InstallResult>()
    mockedInstall.mockImplementation(() => gate.promise)
    const wrapper = await render([transaction()])

    void buttonLabelled(wrapper, 'Reinstall')?.trigger('click')
    await flushPromises()

    expect(mockedInstall).toHaveBeenCalledTimes(1)
    expect(buttonLabelled(wrapper, 'Reinstall')?.attributes('disabled')).toBeDefined()
    expect(buttonLabelled(wrapper, 'Remove')?.attributes('disabled')).toBeDefined()

    gate.resolve(INSTALLED)
    await flushPromises()
    expect(buttonLabelled(wrapper, 'Reinstall')?.attributes('disabled')).toBeUndefined()
  })

  it('disables Check while an install is in flight', async () => {
    const gate = deferred<InstallResult>()
    mockedInstall.mockImplementation(() => gate.promise)
    const wrapper = await render([transaction()])

    const check = buttonLabelled(wrapper, 'Check')
    expect(check?.attributes('disabled')).toBeUndefined()

    void buttonLabelled(wrapper, 'Reinstall')?.trigger('click')
    await flushPromises()

    // Checking a module mid-install reports on a state that is still
    // moving. The button used to look live, swallow the click and show
    // nothing, which reads as a broken control rather than a blocked one.
    expect(check?.attributes('disabled')).toBeDefined()
    await check?.trigger('click')
    await flushPromises()
    expect(mockedVerify).not.toHaveBeenCalled()
    expect(wrapper.find('.callout').exists()).toBe(false)

    gate.resolve(INSTALLED)
    await flushPromises()
  })
})

describe('capability removal confirmation', () => {
  it('shows no dialog until Remove is pressed', async () => {
    await render([transaction()])
    expect(dialog()).toBeNull()
  })

  it('names the capability, what is restored and where to undo', async () => {
    const wrapper = await render([transaction()])

    await buttonLabelled(wrapper, 'Remove')?.trigger('click')
    await flushPromises()

    expect(dialog()).not.toBeNull()
    const text = dialog()?.textContent ?? ''
    expect(text).toContain('Remove this mod?')
    expect(text).toContain('BepInEx')
    expect(text).toContain('History')
  })

  it('does not uninstall until the confirmation is accepted', async () => {
    const wrapper = await render([transaction()])
    await buttonLabelled(wrapper, 'Remove')?.trigger('click')
    await flushPromises()

    expect(mockedUninstall).not.toHaveBeenCalled()

    const confirm = Array.from(dialog()?.querySelectorAll('button') ?? []).at(-1)
    ;(confirm as HTMLButtonElement).click()
    await flushPromises()

    expect(mockedUninstall).toHaveBeenCalledWith({
      capabilityId: 'bepinex',
      gameId: 'stalker-2',
      installDir: 'C:\\games\\stalker-2',
    })
    expect(dialog()).toBeNull()
  })

  it('cancels without uninstalling', async () => {
    const wrapper = await render([transaction()])
    await buttonLabelled(wrapper, 'Remove')?.trigger('click')
    await flushPromises()

    ;(dialog()?.querySelector('.dialog-footer .btn') as HTMLButtonElement).click()
    await flushPromises()

    expect(mockedUninstall).not.toHaveBeenCalled()
    expect(dialog()).toBeNull()
  })

  it('removes nothing when Escape is pressed', async () => {
    const wrapper = await render([transaction()])
    await buttonLabelled(wrapper, 'Remove')?.trigger('click')
    await flushPromises()

    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }))
    await flushPromises()

    expect(mockedUninstall).not.toHaveBeenCalled()
  })

  it('clears the pending removal on Escape', async () => {
    const wrapper = await render([transaction()])
    await buttonLabelled(wrapper, 'Remove')?.trigger('click')
    await flushPromises()
    expect(wrapper.find('.dialog-footer').exists()).toBe(true)

    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }))
    await flushPromises()

    // A keyboard user must be able to back out of a destructive prompt.
    // The dialog is unmounted by this component's `v-if`, so the prompt
    // leaving the screen is the observable form of `pendingRemoval`
    // having been cleared.
    expect(wrapper.find('.dialog-footer').exists()).toBe(false)
    expect(mockedUninstall).not.toHaveBeenCalled()
  })
})

describe('capability section loading', () => {
  it('shows a load failure instead of an empty grid', async () => {
    mockedList.mockRejectedValue(new Error('capability_list unavailable'))
    const wrapper = await render()

    expect(wrapper.find('.callout-danger').text()).toContain('capability_list unavailable')
  })
})
