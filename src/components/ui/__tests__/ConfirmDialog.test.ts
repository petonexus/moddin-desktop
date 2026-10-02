import { enableAutoUnmount, mount } from '@vue/test-utils'
import { afterEach, describe, expect, it } from 'vitest'
import ConfirmDialog from '../ConfirmDialog.vue'
import { i18n } from '../../../i18n'

// The wrappers attach to `document.body`, so they have to come back off
// it or a later test sees the previous test's dialog.
enableAutoUnmount(afterEach)

/**
 * `ConfirmDialog` is the one place ROADMAP UX-11 enforces "ask first"
 * for a destructive action, so the contract the four call sites depend
 * on is pinned here: cancel is offered before confirm, confirm is not
 * autofocused, and the accessibility affordances are inherited from
 * `BaseDialog` rather than re-implemented.
 */
function render(props: Record<string, unknown> = {}) {
  return mount(ConfirmDialog, {
    props: {
      title: 'Remover este mod?',
      description: 'O mod será removido da pasta do jogo.',
      confirmLabel: 'Remover',
      ...props,
    },
    // Attached so `BaseDialog`'s `dialogElement.focus()` on mount is
    // observable — the "confirm is not autofocused" guarantee is about
    // the real document, not a detached tree.
    attachTo: document.body,
    global: { plugins: [i18n] },
  })
}

describe('ConfirmDialog', () => {
  it('renders the title, description and confirm label', () => {
    const wrapper = render()
    expect(wrapper.text()).toContain('Remover este mod?')
    expect(wrapper.text()).toContain('O mod será removido da pasta do jogo.')
    expect(wrapper.text()).toContain('Remover')
  })

  it('lists every detail line and the footnote', () => {
    const wrapper = render({
      details: ['Os arquivos voltam ao estado original.', 'Feche o jogo antes.'],
      footnote: 'Você pode desfazer isso depois em Histórico.',
    })
    const items = wrapper.findAll('.confirm-details li')
    expect(items.map((item) => item.text())).toEqual([
      'Os arquivos voltam ao estado original.',
      'Feche o jogo antes.',
    ])
    expect(wrapper.find('.confirm-footnote').text()).toBe('Você pode desfazer isso depois em Histórico.')
  })

  it('renders no details list when the caller supplies none', () => {
    expect(render().find('.confirm-details').exists()).toBe(false)
  })

  it('offers cancel before confirm so the safe choice is first', () => {
    const labels = render({ cancelLabel: 'Cancelar' })
      .findAll('.dialog-footer button')
      .map((button) => button.text())
    expect(labels[0]).toContain('Cancelar')
    expect(labels[1]).toContain('Remover')
  })

  it('labels the default cancel button with a translated string, not the raw key', () => {
    const labels = render()
      .findAll('.dialog-footer button')
      .map((button) => button.text())
    // The default label used to render the literal "actionCancel",
    // because the key existed in no locale file.
    expect(labels[0]).toBe(i18n.global.t('actionCancel'))
    expect(labels[0]).not.toBe('actionCancel')
  })

  it('never puts the focus on the confirm button', async () => {
    // A dialog opening over a game folder must not remove a mod
    // because the user pressed Enter. Where the focus lands exactly is
    // `BaseDialog`'s business; that it is *not* on the destructive
    // action is this dialog's business.
    const wrapper = render()
    await new Promise((resolve) => setTimeout(resolve, 0))

    const confirm = wrapper.find('.btn-danger-solid')
    expect(confirm.attributes('autofocus')).toBeUndefined()
    expect(document.activeElement).not.toBe(confirm.element)
    expect(document.activeElement).toBe(wrapper.find('.dialog-footer .btn').element)
  })

  it('blocks repeat confirmation and cancellation while an action is busy', async () => {
    const wrapper = render({ busy: true })
    const buttons = wrapper.findAll('.dialog-footer button')
    expect(buttons.every((button) => button.attributes('disabled') !== undefined)).toBe(true)
    await buttons[1].trigger('click')
    await buttons[0].trigger('click')
    expect(wrapper.emitted('confirm')).toBeUndefined()
    expect(wrapper.emitted('close')).toBeUndefined()
  })

  it('emits confirm only from the confirm button', async () => {
    const wrapper = render()
    await wrapper.find('.btn-danger-solid').trigger('click')
    expect(wrapper.emitted('confirm')).toHaveLength(1)
    expect(wrapper.emitted('close')).toBeUndefined()
  })

  it('emits close from the cancel button, never confirm', async () => {
    const wrapper = render()
    await wrapper.find('.dialog-footer .btn').trigger('click')

    expect(wrapper.emitted('close')).toHaveLength(1)
    expect(wrapper.emitted('confirm')).toBeUndefined()
  })

  it('emits close on Escape', async () => {
    const wrapper = render()
    // The dialog is mounted only while it is open, so there is no
    // `open` prop to set: it is open by construction.
    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }))
    await new Promise((resolve) => setTimeout(resolve, 0))

    expect(wrapper.emitted('close')).toHaveLength(1)
    expect(wrapper.emitted('confirm')).toBeUndefined()
  })

  // The dialog is unmounted by whoever opened it, so "does it disappear"
  // is not this component's contract — `emit('close')` is, and that is
  // what lets a caller clear its pending state. The call sites assert
  // the rest: CapabilityModulesSection and LocalAiPanel each test that
  // Escape clears the pending removal or disconnect.

  it('inherits the modal semantics from BaseDialog', () => {
    const dialog = render().find('[role="dialog"]')
    expect(dialog.attributes('aria-modal')).toBe('true')
    expect(dialog.attributes('aria-labelledby')).toBeTruthy()
  })

  it('uses the danger tone by default and drops the icon for a plain confirmation', () => {
    expect(render().find('.btn-danger-solid').exists()).toBe(true)
    const plain = render({ tone: 'default', confirmLabel: 'Continuar' })
    expect(plain.find('.btn-danger-solid').exists()).toBe(false)
    expect(plain.find('.btn-primary').exists()).toBe(true)
  })
})
