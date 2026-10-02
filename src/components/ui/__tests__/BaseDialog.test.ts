import { mount, type VueWrapper } from '@vue/test-utils'
import { defineComponent, h, nextTick } from 'vue'
import { afterEach, describe, expect, it } from 'vitest'
import BaseDialog from '../BaseDialog.vue'
import { i18n } from '../../../i18n'

const wrappers: VueWrapper[] = []

afterEach(async () => {
  wrappers.splice(0).reverse().forEach((wrapper) => wrapper.unmount())
  await nextTick()
  document.body.replaceChildren()
})

function render(props: Record<string, unknown> = {}, content = '') {
  const wrapper = mount(BaseDialog, {
    props: { title: 'Revisar alterações', ...props },
    slots: { default: content },
    attachTo: document.body,
    global: { plugins: [i18n] },
  })
  wrappers.push(wrapper)
  return wrapper
}

function keydown(key: string, shiftKey = false) {
  const event = new KeyboardEvent('keydown', { key, shiftKey, cancelable: true })
  window.dispatchEvent(event)
  return event
}

describe('BaseDialog keyboard lifecycle', () => {
  it('starts on the dialog so its title and description orient the user', async () => {
    const wrapper = render({ description: 'Confira os arquivos antes de aplicar.' })
    await nextTick()
    const dialog = wrapper.get('[role="dialog"]')
    expect(document.activeElement).toBe(dialog.element)
    const descriptionId = dialog.attributes('aria-describedby')
    expect(document.getElementById(descriptionId ?? '')?.textContent).toBe('Confira os arquivos antes de aplicar.')
  })

  it('gives Escape only to the uppermost dialog and returns to its opener', async () => {
    const outer = render({}, '<button class="open-confirm">Remover</button>')
    await nextTick()
    const opener = outer.get('.open-confirm').element as HTMLElement
    opener.focus()
    const inner = render({ title: 'Remover este mod?' })
    await nextTick()
    expect(Number((inner.get('.dialog-backdrop').element as HTMLElement).style.zIndex))
      .toBeGreaterThan(Number((outer.get('.dialog-backdrop').element as HTMLElement).style.zIndex))
    expect(outer.get('[role="dialog"]').attributes('aria-modal')).toBeUndefined()
    expect(inner.get('[role="dialog"]').attributes('aria-modal')).toBe('true')

    keydown('Escape')
    await nextTick()
    expect(inner.emitted('close')).toHaveLength(1)
    expect(outer.emitted('close')).toBeUndefined()
    expect(document.activeElement).toBe(opener)
    expect(outer.get('[role="dialog"]').attributes('aria-modal')).toBe('true')
  })

  it('wraps Tab through visible controls, including a collapsed disclosure summary', async () => {
    const wrapper = render({ busy: true }, `
      <button class="first">Continuar</button>
      <button hidden>Oculto</button>
      <div style="display: none"><button>Oculto por CSS</button></div>
      <div aria-hidden="true"><button>Oculto para acessibilidade</button></div>
      <div inert><button>Inativo</button></div>
      <fieldset disabled><button>Desabilitado</button></fieldset>
      <button tabindex="-1">Fora da ordem</button>
      <details><summary>Local dos arquivos</summary><button>Conteúdo fechado</button></details>
    `)
    await nextTick()
    const first = wrapper.get('.first').element as HTMLElement
    const last = wrapper.get('summary').element as HTMLElement
    first.focus()
    expect(keydown('Tab', true).defaultPrevented).toBe(true)
    expect(document.activeElement).toBe(last)
    expect(keydown('Tab').defaultPrevented).toBe(true)
    expect(document.activeElement).toBe(first)
  })

  it('keeps an initially nested dialog above its parent despite child-first Vue mounting', async () => {
    const wrapper = mount(defineComponent({
      render: () => h(BaseDialog, { title: 'Painel' }, {
        default: () => h(BaseDialog, { title: 'Confirmação' }),
      }),
    }), { attachTo: document.body, global: { plugins: [i18n] } })
    wrappers.push(wrapper)
    await nextTick()
    const dialogs = wrapper.findAllComponents(BaseDialog)
    const overlays = wrapper.findAll('.dialog-backdrop')
    expect(Number((overlays[1].element as HTMLElement).style.zIndex))
      .toBeGreaterThan(Number((overlays[0].element as HTMLElement).style.zIndex))
    expect(document.activeElement).toBe(dialogs[1].get('[role="dialog"]').element)

    keydown('Escape')
    await nextTick()
    expect(dialogs[0].emitted('close')).toBeUndefined()
    expect(dialogs[1].emitted('close')).toHaveLength(1)
  })

  it('keeps focus inside the modal when another control tries to take it', async () => {
    const outside = document.createElement('button')
    document.body.append(outside)
    const wrapper = render()
    await nextTick()
    outside.focus()
    expect(document.activeElement).toBe(wrapper.get('[role="dialog"]').element)
  })

  it('restores the opener when a completed action unmounts the dialog', async () => {
    const opener = document.createElement('button')
    document.body.append(opener)
    opener.focus()
    const wrapper = render()
    await nextTick()
    wrapper.unmount()
    wrappers.splice(wrappers.indexOf(wrapper), 1)
    await nextTick()
    expect(document.activeElement).toBe(opener)
  })

  it('blocks header, backdrop and Escape dismissal until a busy action finishes', async () => {
    const wrapper = render({ busy: true })
    await nextTick()
    expect(wrapper.get('.dialog-header button').attributes('disabled')).toBeDefined()
    await wrapper.get('.dialog-backdrop').trigger('click')
    keydown('Escape')
    await nextTick()
    expect(wrapper.emitted('close')).toBeUndefined()

    await wrapper.setProps({ busy: false })
    keydown('Escape')
    await nextTick()
    expect(wrapper.emitted('close')).toHaveLength(1)
  })
})
