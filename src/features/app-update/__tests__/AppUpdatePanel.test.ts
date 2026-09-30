import { enableAutoUnmount, flushPromises, mount } from '@vue/test-utils'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import AppUpdatePanel from '../AppUpdatePanel.vue'
import { appUpdateCopyForLocale } from '../copy'
import { i18n } from '../../../i18n'
import type { AppUpdateCheck, AppUpdateStatus } from '../types'

/**
 * ROADMAP `F-04`: the app self-updater's user-visible contract.
 *
 * Four things have to be true, and none of them is "the download works":
 * nothing is asked before the user asks, an update is named before it is
 * installed, a failure is a sentence the user can act on, and turning an
 * update down is a decision rather than a dismissal. The Rust side owns
 * the trust rules — this suite is the half that is easy to break later.
 */

vi.mock('../service', () => ({
  checkForAppUpdate: vi.fn(),
  installAppUpdate: vi.fn(),
  onAppUpdateProgress: vi.fn(),
  readAppUpdateStatus: vi.fn(),
}))

import {
  checkForAppUpdate,
  installAppUpdate,
  onAppUpdateProgress,
  readAppUpdateStatus,
} from '../service'

const mockedCheck = vi.mocked(checkForAppUpdate)
const mockedInstall = vi.mocked(installAppUpdate)
const mockedProgress = vi.mocked(onAppUpdateProgress)
const mockedStatus = vi.mocked(readAppUpdateStatus)

const copy = appUpdateCopyForLocale('en')

const CURRENT: AppUpdateStatus = {
  configured: true,
  currentVersion: '0.1.0',
  channel: 'stable',
  detail: null,
}

function checkResult(overrides: Partial<AppUpdateCheck> = {}): AppUpdateCheck {
  return {
    status: 'up-to-date',
    currentVersion: '0.1.0',
    version: null,
    notes: null,
    publishedAt: null,
    detail: null,
    ...overrides,
  }
}

const AVAILABLE = checkResult({
  status: 'available',
  version: '0.2.0',
  notes: 'Faster library scans.',
})

enableAutoUnmount(afterEach)

beforeEach(() => {
  i18n.global.locale.value = 'en'
  window.localStorage.clear()
  mockedCheck.mockReset()
  mockedInstall.mockReset()
  mockedProgress.mockReset()
  mockedStatus.mockReset()
  mockedStatus.mockResolvedValue(CURRENT)
  mockedProgress.mockResolvedValue(() => {})
  mockedCheck.mockResolvedValue(checkResult())
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
  const wrapper = mount(AppUpdatePanel, {
    attachTo: document.body,
    global: { plugins: [i18n] },
  })
  await wrapper.find('.nav-item').trigger('click')
  await flushPromises()
  return wrapper
}

async function check() {
  buttonLabelled(copy.check)?.click()
  await flushPromises()
}

describe('AppUpdatePanel asks, and only when asked', () => {
  it('does not touch the update feed when the sidebar entry is mounted', async () => {
    mount(AppUpdatePanel, { attachTo: document.body, global: { plugins: [i18n] } })
    await flushPromises()

    expect(mockedStatus).not.toHaveBeenCalled()
    expect(mockedCheck).not.toHaveBeenCalled()
    expect(dialogs()).toHaveLength(0)
  })

  it('reads the build status on open and asks for nothing', async () => {
    await openPanel()

    expect(mockedStatus).toHaveBeenCalledTimes(1)
    expect(mockedCheck).not.toHaveBeenCalled()
  })

  it('renders nothing intrusive when there is no update', async () => {
    await openPanel()
    await check()

    // One box saying so, inside the panel the user opened. No second
    // dialog, no prompt, no callout.
    expect(dialogs()).toHaveLength(1)
    expect(document.body.querySelector('.empty-state')?.textContent).toContain(
      copy.upToDateTitle,
    )
    expect(document.body.querySelector('.callout-danger')).toBeNull()
    expect(document.body.textContent).toContain('0.1.0')
  })
})

describe('AppUpdatePanel offers an update', () => {
  it('names the version and confirms before it installs anything', async () => {
    mockedCheck.mockResolvedValue(AVAILABLE)
    await openPanel()
    await check()

    expect(document.body.textContent).toContain('0.2.0')
    expect(document.body.textContent).toContain('Faster library scans.')
    expect(mockedInstall).not.toHaveBeenCalled()

    buttonLabelled(copy.install)?.click()
    await flushPromises()

    const prompt = confirmation()
    expect(dialogs()).toHaveLength(2)
    expect(prompt?.textContent).toContain(copy.confirmTitle.replace('{version}', '0.2.0'))
    // The confirmation has to state the check and the restart, not just
    // ask for a yes.
    expect(prompt?.textContent).toContain(copy.confirmDetailDownload)
    expect(prompt?.textContent).toContain(copy.confirmDetailRelaunch)
    expect(mockedInstall).not.toHaveBeenCalled()
  })

  it('installs exactly the version that was confirmed', async () => {
    mockedCheck.mockResolvedValue(AVAILABLE)
    mockedInstall.mockResolvedValue({ version: '0.2.0' })
    await openPanel()
    await check()
    buttonLabelled(copy.install)?.click()
    await flushPromises()

    const confirm = Array.from(confirmation()?.querySelectorAll('button') ?? []).at(-1)
    ;(confirm as HTMLButtonElement).click()
    await flushPromises()

    expect(mockedInstall).toHaveBeenCalledWith('0.2.0')
  })
})

describe('AppUpdatePanel failure is a sentence, not a stack', () => {
  it('surfaces a refused signature through ErrorCallout and keeps the offer', async () => {
    mockedCheck.mockResolvedValue(AVAILABLE)
    mockedInstall.mockRejectedValue(
      new Error(
        "The downloaded update is not signed by Moddin's release key, so it was refused. Nothing was installed.",
      ),
    )
    await openPanel()
    await check()
    buttonLabelled(copy.install)?.click()
    await flushPromises()
    ;(Array.from(confirmation()?.querySelectorAll('button') ?? []).at(-1) as HTMLButtonElement).click()
    await flushPromises()

    const callout = document.body.querySelector('.callout-danger')
    expect(callout?.textContent).toContain(copy.errorInstallTitle)
    expect(callout?.textContent).toContain(copy.errorInstallWhy)
    // The failure does not throw the offer away: the user can read the
    // error and decide again, on the same version.
    expect(document.body.textContent).toContain('0.2.0')
    expect(dialogs()).toHaveLength(1)
  })

  it('surfaces a check failure through ErrorCallout', async () => {
    mockedCheck.mockRejectedValue(new Error('Could not fetch a valid release JSON from the remote'))
    await openPanel()
    await check()

    const callout = document.body.querySelector('.callout-danger')
    expect(callout?.textContent).toContain(copy.errorCheckTitle)
    expect(callout?.textContent).toContain(copy.errorCheckWhy)
  })

  it('says a build without a key cannot install, and asks nothing of the feed', async () => {
    mockedStatus.mockResolvedValue({
      configured: false,
      currentVersion: '0.1.0',
      channel: 'stable',
      detail: 'This build has no update signing key, so Moddin cannot verify an update.',
    })
    mockedCheck.mockResolvedValue(checkResult({ status: 'not-configured' }))
    await openPanel()
    await check()

    expect(document.body.textContent).toContain(copy.notConfiguredTitle)
    expect(document.body.textContent).toContain(copy.notConfiguredDescription)
    expect(mockedInstall).not.toHaveBeenCalled()
  })
})

describe('AppUpdatePanel remembers a refusal', () => {
  it('declining installs nothing and is not offered again', async () => {
    mockedCheck.mockResolvedValue(AVAILABLE)
    await openPanel()
    await check()
    buttonLabelled(copy.install)?.click()
    await flushPromises()

    ;(confirmation()?.querySelector('.dialog-footer .btn') as HTMLButtonElement).click()
    await flushPromises()

    expect(mockedInstall).not.toHaveBeenCalled()
    expect(dialogs()).toHaveLength(1)
    expect(document.body.textContent).not.toContain(copy.confirmTitle.replace('{version}', '0.2.0'))

    // The next check sends the declined version back, and the backend is
    // what decides not to offer it: the panel has no prompt to suppress.
    mockedCheck.mockResolvedValue(checkResult({ status: 'declined', version: '0.2.0' }))
    await check()

    expect(mockedCheck).toHaveBeenLastCalledWith('0.2.0')
    expect(document.body.textContent).toContain(
      copy.declinedNotice.replace('{version}', '0.2.0'),
    )
    expect(dialogs()).toHaveLength(1)
  })

  it('lets the user change that answer without asking the feed again', async () => {
    mockedCheck.mockResolvedValue(AVAILABLE)
    await openPanel()
    await check()
    buttonLabelled(copy.install)?.click()
    await flushPromises()
    ;(confirmation()?.querySelector('.dialog-footer .btn') as HTMLButtonElement).click()
    await flushPromises()
    mockedCheck.mockResolvedValue(
      checkResult({ status: 'declined', version: '0.2.0', notes: 'Faster library scans.' }),
    )
    await check()

    buttonLabelled(copy.declinedAgain)?.click()
    await flushPromises()

    // The offer is back, with the notes the last check already read.
    expect(document.body.textContent).toContain('0.2.0')
    expect(document.body.textContent).toContain('Faster library scans.')
    expect(mockedCheck).toHaveBeenCalledTimes(2)

    // And the next check no longer carries the refusal.
    mockedCheck.mockResolvedValue(AVAILABLE)
    await check()
    expect(mockedCheck).toHaveBeenLastCalledWith(null)
  })
})
