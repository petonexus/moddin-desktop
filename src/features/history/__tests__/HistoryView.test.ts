import { enableAutoUnmount, mount } from '@vue/test-utils'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import HistoryView from '../HistoryView.vue'
import { i18n } from '../../../i18n'
import type { TransactionRecord } from '../../../types/transaction'

/**
 * ROADMAP UX-11, call site 4: history undo.
 *
 * Every row's button is labelled "Desfazer", so the list alone does not
 * tell the user what they are about to revert. The dialog is what names
 * the change and counts the files.
 */

// `HistoryView` renders the snapshot panel, which loads on mount. Left
// unmocked it would reach for the Tauri bridge from jsdom and park a
// load failure in the panel; the snapshot behaviour is covered in
// `SnapshotPanel.test.ts`.
vi.mock('../service', () => ({
  createSnapshot: vi.fn(),
  listSnapshots: vi.fn().mockResolvedValue([]),
  rollbackSnapshot: vi.fn(),
  deleteSnapshot: vi.fn(),
}))

enableAutoUnmount(afterEach)

// jsdom reports `navigator.language = en-US`, so the app would boot in
// English here and the assertions below would pass by accident on a
// machine configured differently. Pin it.
beforeEach(() => {
  i18n.global.locale.value = 'en'
})

function record(overrides: Partial<TransactionRecord> = {}): TransactionRecord {
  return {
    id: 'tx-1',
    createdAt: 1_700_000_000_000,
    kind: 'obs-vr',
    label: 'OBS VR Capture',
    gameId: 'elden-ring',
    targetPath: 'C:\\games\\elden-ring\\obs',
    backupPath: 'C:\\backup\\tx-1',
    status: 'applied',
    files: [
      { targetPath: 'C:\\games\\elden-ring\\obs\\scene.json', backupPath: 'C:\\backup\\1', existedBefore: true },
      { targetPath: 'C:\\games\\elden-ring\\obs\\source.json', backupPath: 'C:\\backup\\2', existedBefore: false },
    ],
    ...overrides,
  }
}

function render(transactions: TransactionRecord[] = [record()]) {
  return mount(HistoryView, {
    props: {
      transactions,
      loading: false,
      busyId: null,
      gameName: () => 'Elden Ring',
      kindLabel: () => 'Módulo',
      formatDate: () => '29/09/2026',
      blockedReason: () => undefined,
    },
    // Attached so the dialog this component renders inline is reachable
    // from the document the way a user's would be.
    attachTo: document.body,
    global: { plugins: [i18n] },
  })
}

function dialog() {
  return document.body.querySelector('[role="dialog"]')
}

describe('HistoryView undo confirmation', () => {
  it('shows no dialog until the user asks for one', () => {
    render()
    expect(dialog()).toBeNull()
  })

  it('asks first, and names the change and the number of files', async () => {
    const wrapper = render()

    await wrapper.findAll('.history-row button')[0].trigger('click')

    expect(dialog()).not.toBeNull()
    expect(dialog()?.textContent).toContain('Undo this change?')
    expect(dialog()?.textContent).toContain('OBS VR Capture')
    expect(dialog()?.textContent).toContain('2 file(s)')
  })

  it('does not emit undo until the confirmation is accepted', async () => {
    const wrapper = render()

    await wrapper.findAll('.history-row button')[0].trigger('click')
    expect(wrapper.emitted('undo')).toBeUndefined()

    const confirm = Array.from(dialog()?.querySelectorAll('button') ?? []).at(-1)
    ;(confirm as HTMLButtonElement).click()
    await wrapper.vm.$nextTick()

    expect(wrapper.emitted('undo')).toHaveLength(1)
    expect((wrapper.emitted('undo')?.[0]?.[0] as TransactionRecord).id).toBe('tx-1')
    expect(dialog()).toBeNull()
  })

  it('cancels without emitting, and the dialog can be reopened', async () => {
    const wrapper = render()
    await wrapper.findAll('.history-row button')[0].trigger('click')

    const cancel = dialog()?.querySelector('.dialog-footer .btn') as HTMLButtonElement
    cancel.click()
    await wrapper.vm.$nextTick()

    expect(wrapper.emitted('undo')).toBeUndefined()
    expect(dialog()).toBeNull()

    await wrapper.findAll('.history-row button')[0].trigger('click')
    expect(dialog()).not.toBeNull()
  })

  it('counts zero rather than crashing for a transaction with no file list', async () => {
    const wrapper = render([record({ files: undefined })])

    await wrapper.findAll('.history-row button')[0].trigger('click')

    expect(dialog()?.textContent).toContain('0 file(s)')
  })

  it('offers no undo button for an already-undone transaction', () => {
    const wrapper = render([record({ status: 'rolled_back' })])
    expect(wrapper.findAll('.history-row button')).toHaveLength(0)
    expect(dialog()).toBeNull()
  })
})
