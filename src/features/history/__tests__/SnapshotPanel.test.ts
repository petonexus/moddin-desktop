import { enableAutoUnmount, flushPromises, mount } from '@vue/test-utils'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import SnapshotPanel from '../SnapshotPanel.vue'
import { i18n } from '../../../i18n'
import type { TransactionRecord } from '../../../types/transaction'
import type { Snapshot } from '../types'

/**
 * ROADMAP F-14: the four snapshot commands.
 *
 * Restoring a snapshot is the most destructive thing the History panel
 * can do — it undoes every change the snapshot captured, in one click,
 * with no redo — so the confirmations are the contract under test, not
 * the styling. Every test here pairs the refusal with the matching
 * success: a guard that passes because the button is dead proves
 * nothing.
 */

vi.mock('../service', () => ({
  createSnapshot: vi.fn(),
  listSnapshots: vi.fn(),
  rollbackSnapshot: vi.fn(),
  deleteSnapshot: vi.fn(),
}))

import { createSnapshot, deleteSnapshot, listSnapshots, rollbackSnapshot } from '../service'

const mockedCreate = vi.mocked(createSnapshot)
const mockedList = vi.mocked(listSnapshots)
const mockedRollback = vi.mocked(rollbackSnapshot)
const mockedDelete = vi.mocked(deleteSnapshot)

enableAutoUnmount(afterEach)

// The panel and its dialogs attach to `document.body`. A dialog left
// behind by one test would be found by the next one's
// `document.body.querySelector('[role="dialog"]')`, which is exactly the
// kind of leak that makes a "no dialog is shown" assertion pass.
afterEach(() => {
  document.body.innerHTML = ''
})

// jsdom reports `navigator.language = en-US`, so the app would boot in
// English here and the assertions below would pass by accident on a
// machine configured differently. Pin it.
beforeEach(() => {
  i18n.global.locale.value = 'en'
  mockedCreate.mockReset()
  mockedList.mockReset()
  mockedRollback.mockReset()
  mockedDelete.mockReset()
  mockedList.mockResolvedValue([])
})

function record(overrides: Partial<TransactionRecord> = {}): TransactionRecord {
  return {
    id: 'tx-1',
    createdAt: 1_757_720_000_000,
    kind: 'obs-vr',
    label: 'OBS VR Capture',
    gameId: 'elden-ring',
    targetPath: 'C:\\games\\elden-ring\\obs',
    backupPath: 'C:\\backup\\tx-1',
    status: 'applied',
    files: [{ targetPath: 'C:\\games\\elden-ring\\obs\\scene.json', backupPath: 'C:\\backup\\1', existedBefore: true }],
    ...overrides,
  }
}

/** Two applied changes to Elden Ring, one to Cyberpunk, 3 files between them. */
const TRANSACTIONS: TransactionRecord[] = [
  record({
    files: [
      { targetPath: 'a.json', backupPath: 'C:\\backup\\1', existedBefore: true },
      { targetPath: 'b.json', backupPath: 'C:\\backup\\2', existedBefore: false },
    ],
  }),
  record({ id: 'tx-2', label: 'Bepinex', files: [{ targetPath: 'c.json', backupPath: 'C:\\backup\\3', existedBefore: false }] }),
  record({ id: 'tx-3', gameId: 'cyberpunk-2077', label: 'UEVR' }),
]

function snapshot(overrides: Partial<Snapshot> = {}): Snapshot {
  return {
    id: '1757720000000-aaaabbbbccccddddeeeeffff00001111',
    name: 'Before the texture overhaul',
    createdAt: 1_757_720_000_000,
    gameId: 'elden-ring',
    transactionIds: ['tx-1', 'tx-2'],
    ...overrides,
  }
}

async function render(items: Snapshot[] = [snapshot()], transactions = TRANSACTIONS) {
  mockedList.mockResolvedValue(items)
  const wrapper = mount(SnapshotPanel, {
    props: {
      transactions,
      gameName: (gameId) => (gameId === 'elden-ring' ? 'Elden Ring' : 'Cyberpunk 2077'),
      formatDate: () => '12/09/2025 10:00',
    },
    // Attached so `BaseDialog`'s focus call on mount is observable
    // against the real document, the way a user's would be.
    attachTo: document.body,
    global: { plugins: [i18n] },
  })
  await flushPromises()
  return wrapper
}

function dialog() {
  return document.body.querySelector('[role="dialog"]')
}

function confirmButton() {
  return dialog()?.querySelector('.dialog-footer .btn-danger-solid') as HTMLButtonElement | null
}

async function askRollback(wrapper: Awaited<ReturnType<typeof render>>) {
  await wrapper.findAll('.snapshot-row button')[0].trigger('click')
  await new Promise((resolve) => setTimeout(resolve, 0))
}

describe('SnapshotPanel — creating', () => {
  it('lists each game that has an active change and preselects none', async () => {
    const wrapper = await render()
    const options = wrapper.findAll('select option').map((option) => option.text())

    // The placeholder first, then the games. `create_snapshot` is
    // per-game, and the panel is handed no selected game, so guessing
    // one would file a restore point under the wrong title.
    expect(options[0]).toBe('Choose a game…')
    expect(options.slice(1)).toEqual(['Cyberpunk 2077', 'Elden Ring'])
    expect(wrapper.find<HTMLSelectElement>('select').element.value).toBe('')
  })

  it('does not create a snapshot until the name is non-empty', async () => {
    const wrapper = await render()

    await wrapper.find('select').setValue('elden-ring')
    await wrapper.find('input').setValue('   ')

    const button = wrapper.find('.snapshot-create-button')
    expect(button.attributes('disabled')).toBeDefined()
    await button.trigger('click')
    await flushPromises()
    expect(mockedCreate).not.toHaveBeenCalled()

    // Control: the same flow with a name does reach the backend, so the
    // assertions above are about the guard and not a dead button.
    await wrapper.find('input').setValue('Before the texture overhaul')
    expect(button.attributes('disabled')).toBeUndefined()
    await wrapper.find('form.snapshot-create').trigger('submit')
    await flushPromises()

    expect(mockedCreate).toHaveBeenCalledTimes(1)
    expect(mockedCreate).toHaveBeenCalledWith('Before the texture overhaul', 'elden-ring')
  })

  it('does not create a snapshot with no game chosen', async () => {
    const wrapper = await render()
    await wrapper.find('input').setValue('Before the texture overhaul')

    expect(wrapper.find('.snapshot-create-button').attributes('disabled')).toBeDefined()
    await wrapper.find('form.snapshot-create').trigger('submit')
    await flushPromises()

    expect(mockedCreate).not.toHaveBeenCalled()
  })
})

describe('SnapshotPanel — listing', () => {
  it('names every snapshot with its game and creation time', async () => {
    const wrapper = await render([
      snapshot(),
      snapshot({ id: '1757720000001-1111', name: 'Clean slate', gameId: 'cyberpunk-2077' }),
    ])

    const rows = wrapper.findAll('.snapshot-row')
    expect(rows).toHaveLength(2)
    expect(rows[0].text()).toContain('Before the texture overhaul')
    expect(rows[0].text()).toContain('Elden Ring')
    expect(rows[0].text()).toContain('12/09/2025 10:00')
    expect(rows[1].text()).toContain('Clean slate')
    expect(rows[1].text()).toContain('Cyberpunk 2077')
  })

  it('renders the shared empty state when there are no snapshots', async () => {
    const wrapper = await render([])

    const empty = wrapper.find('.empty-state')
    expect(empty.exists()).toBe(true)
    expect(empty.text()).toContain('No snapshots yet')
    // `EmptyState` announces itself; a hand-rolled block would not.
    expect(empty.attributes('role')).toBe('status')
    expect(wrapper.find('.snapshot-row').exists()).toBe(false)

    // Control: the empty state is a property of the data, not of the
    // component always rendering one.
    const withSnapshots = await render()
    expect(withSnapshots.find('.empty-state').exists()).toBe(false)
  })

  it('shows the busy empty state while the list is loading', async () => {
    let release: (value: Snapshot[]) => void = () => {}
    mockedList.mockReturnValue(new Promise<Snapshot[]>((resolve) => { release = resolve }))

    const wrapper = mount(SnapshotPanel, {
      props: { transactions: TRANSACTIONS, gameName: () => 'Elden Ring', formatDate: () => '12/09/2025' },
      attachTo: document.body,
      global: { plugins: [i18n] },
    })
    await flushPromises()

    const empty = wrapper.find('.empty-state')
    expect(empty.attributes('aria-busy')).toBe('true')
    expect(empty.text()).toContain('Loading snapshots…')

    release([snapshot()])
    await flushPromises()
    expect(wrapper.find('.empty-state').exists()).toBe(false)
  })
})

describe('SnapshotPanel — rolling back', () => {
  it('asks first, names the snapshot, and never puts the focus on the confirm button', async () => {
    const wrapper = await render()
    await askRollback(wrapper)

    expect(dialog()).not.toBeNull()
    expect(dialog()?.textContent).toContain('Undo the saved changes?')
    expect(dialog()?.textContent).toContain('Before the texture overhaul')
    // Two changes, three files — distinct numbers, so a swapped count
    // cannot pass by accident.
    expect(dialog()?.textContent).toContain('2 change(s) in History are undone by this.')
    expect(dialog()?.textContent).toContain('3 file(s) go back to the copies Moddin backed up')
    expect(dialog()?.textContent).toContain('Changes made after this snapshot are not part of it')
    expect(dialog()?.textContent).toContain('There is no redo.')
    expect(mockedRollback).not.toHaveBeenCalled()

    const confirm = confirmButton()
    expect(confirm).not.toBeNull()
    expect(confirm?.getAttribute('autofocus')).toBeNull()
    expect(document.activeElement).not.toBe(confirm)
  })

  it('rolls back and asks for the transaction list to be reloaded only once confirmed', async () => {
    mockedRollback.mockResolvedValue([])
    const wrapper = await render()
    await askRollback(wrapper)

    confirmButton()?.click()
    await flushPromises()

    expect(mockedRollback).toHaveBeenCalledTimes(1)
    expect(mockedRollback).toHaveBeenCalledWith('1757720000000-aaaabbbbccccddddeeeeffff00001111')
    expect(wrapper.emitted('changed')).toHaveLength(1)
    expect(dialog()).toBeNull()
  })

  it('dismisses the confirmation on Escape without issuing the command', async () => {
    const wrapper = await render()
    await askRollback(wrapper)
    expect(dialog()).not.toBeNull()

    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }))
    await flushPromises()

    expect(dialog()).toBeNull()
    expect(mockedRollback).not.toHaveBeenCalled()

    // Control: the dialog is not stuck shut by the Escape path.
    await askRollback(wrapper)
    expect(dialog()).not.toBeNull()
  })

  it('will not offer to roll back a snapshot that has nothing left to undo', async () => {
    const spent = [
      record({ id: 'tx-1', status: 'rolled_back' }),
      record({ id: 'tx-2', status: 'rolled_back' }),
    ]
    const wrapper = await render([snapshot()], spent)

    const rollback = wrapper.findAll('.snapshot-row button')[0]
    expect(rollback.attributes('disabled')).toBeDefined()
    expect(wrapper.find('.snapshot-row').text()).toContain('0 change(s)')

    await wrapper.findAll('.snapshot-row button')[1].trigger('click')
    await flushPromises()
    confirmButton()?.click()
    await flushPromises()

    // The other action still works, so the row is not simply inert.
    expect(mockedRollback).not.toHaveBeenCalled()
    expect(mockedDelete).toHaveBeenCalledTimes(1)
  })

  it('explains a failed rollback through ErrorCallout instead of dumping err.message', async () => {
    mockedRollback.mockRejectedValue(new Error('Backup no longer exists: C:\\backup\\tx-1'))
    const wrapper = await render()
    await askRollback(wrapper)

    confirmButton()?.click()
    await flushPromises()

    const callout = wrapper.find('.callout-danger')
    expect(callout.exists()).toBe(true)
    // `role="alert"` and the disclosure come from the shared
    // `ErrorCallout`; a `{{ err.message }}` paragraph has neither.
    expect(callout.attributes('role')).toBe('alert')
    expect(callout.find('strong').text()).toBe('The rollback did not finish')
    expect(callout.find('p').text()).toBe(
      'Some changes may already be undone. Check History before rolling back again.',
    )
    expect(callout.find('p').text()).not.toContain('Backup no longer exists')

    // The untranslated string stays reachable, but only under the
    // disclosure — that is where a bug report gets it from.
    expect(callout.find('.callout-raw code').text()).toContain('Backup no longer exists')
    expect(wrapper.emitted('changed')).toHaveLength(1)
    expect(mockedList).toHaveBeenCalledTimes(2)
  })

  it('reloads after a partial failure, keeps the recovery message and uses the updated active count', async () => {
    let release!: () => void
    mockedRollback.mockImplementationOnce(() => new Promise((_resolve, reject) => {
      release = () => reject(new Error('Could not restore second file: C:\\games\\scene.json'))
    }))
    const wrapper = await render()
    await askRollback(wrapper)
    confirmButton()?.click()
    await flushPromises()

    expect(dialog()?.getAttribute('aria-busy')).toBe('true')
    expect(wrapper.emitted('busy')).toEqual([[true]])
    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }))
    await flushPromises()
    expect(dialog()).not.toBeNull()

    release()
    await flushPromises()
    expect(wrapper.emitted('changed')).toHaveLength(1)
    expect(wrapper.emitted('busy')).toEqual([[true], [false]])
    expect(mockedList).toHaveBeenCalledTimes(2)
    expect(dialog()).toBeNull()
    expect(wrapper.find('.callout-danger').text()).toContain('Some changes may already be undone')

    // The host reload discovers that the first captured change finished.
    await wrapper.setProps({ transactions: [
      record({ id: 'tx-1', status: 'rolled_back' }),
      record({ id: 'tx-2' }),
      record({ id: 'tx-3', gameId: 'cyberpunk-2077' }),
    ] })
    expect(wrapper.find('.snapshot-row .badge').text()).toBe('1 change(s)')
    await askRollback(wrapper)
    expect(dialog()?.textContent).toContain('1 change(s) in History are undone by this.')
  })
})

describe('SnapshotPanel — deleting', () => {
  it('asks first and says what deleting costs', async () => {
    mockedDelete.mockResolvedValue(undefined)
    const wrapper = await render()
    await wrapper.findAll('.snapshot-row button')[1].trigger('click')
    await new Promise((resolve) => setTimeout(resolve, 0))

    expect(dialog()?.textContent).toContain('Delete this snapshot?')
    expect(dialog()?.textContent).toContain('Before the texture overhaul')
    expect(dialog()?.textContent).toContain('The 2 change(s) it recorded stay in History')
    expect(dialog()?.textContent).toContain('the saved group for undoing these changes together')
    expect(mockedDelete).not.toHaveBeenCalled()

    confirmButton()?.click()
    await flushPromises()

    expect(mockedDelete).toHaveBeenCalledWith('1757720000000-aaaabbbbccccddddeeeeffff00001111')
    expect(wrapper.findAll('.snapshot-row')).toHaveLength(0)
  })

  it('keeps the snapshot when the delete fails', async () => {
    mockedDelete.mockRejectedValue(new Error('Could not delete snapshot: file in use'))
    const wrapper = await render()
    await wrapper.findAll('.snapshot-row button')[1].trigger('click')
    await new Promise((resolve) => setTimeout(resolve, 0))

    confirmButton()?.click()
    await flushPromises()

    expect(wrapper.findAll('.snapshot-row')).toHaveLength(1)
    expect(wrapper.find('.callout-danger').find('strong').text()).toBe('The snapshot was not deleted')
  })
})
