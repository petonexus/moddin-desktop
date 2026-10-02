import { mount, type VueWrapper } from '@vue/test-utils'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import ToastStack from '../ToastStack.vue'
import { i18n } from '../../../i18n'

let wrapper: VueWrapper | null = null

beforeEach(() => vi.useFakeTimers())
afterEach(() => {
  wrapper?.unmount()
  wrapper = null
  vi.useRealTimers()
})

function render(success: string | null = 'Mod instalado.') {
  wrapper = mount(ToastStack, {
    props: { success, error: null },
    attachTo: document.body,
    global: { plugins: [i18n] },
  })
  return wrapper
}

describe('ToastStack message lifetime', () => {
  it('dismisses success already present on mount after six seconds', () => {
    const toast = render()
    vi.advanceTimersByTime(5999)
    expect(toast.emitted('dismiss-success')).toBeUndefined()
    vi.advanceTimersByTime(1)
    expect(toast.emitted('dismiss-success')).toHaveLength(1)
  })

  it('gives a replacement success message a fresh reading period', async () => {
    const toast = render()
    vi.advanceTimersByTime(5000)
    await toast.setProps({ success: 'Backup criado.' })
    vi.advanceTimersByTime(5000)
    expect(toast.emitted('dismiss-success')).toBeUndefined()
    vi.advanceTimersByTime(1000)
    expect(toast.emitted('dismiss-success')).toHaveLength(1)
  })

  it('pauses while the pointer is reading the toast and resumes the remaining time', async () => {
    const toast = render()
    vi.advanceTimersByTime(2000)
    await toast.get('.toast-success').trigger('pointerenter')
    vi.advanceTimersByTime(20000)
    expect(toast.emitted('dismiss-success')).toBeUndefined()
    await toast.get('.toast-success').trigger('pointerleave')
    vi.advanceTimersByTime(3999)
    expect(toast.emitted('dismiss-success')).toBeUndefined()
    vi.advanceTimersByTime(1)
    expect(toast.emitted('dismiss-success')).toHaveLength(1)
  })

  it('keeps a keyboard-focused dismiss button available and cancels its timer on manual dismissal', async () => {
    const toast = render()
    const dismiss = toast.get('.toast-success button')
    ;(dismiss.element as HTMLElement).focus()
    vi.advanceTimersByTime(20000)
    expect(toast.emitted('dismiss-success')).toBeUndefined()
    await dismiss.trigger('click')
    vi.advanceTimersByTime(20000)
    expect(toast.emitted('dismiss-success')).toHaveLength(1)
  })

  it('keeps errors until the user dismisses them', async () => {
    const toast = render(null)
    await toast.setProps({ error: 'Não foi possível instalar.' })
    vi.advanceTimersByTime(60000)
    expect(toast.emitted('dismiss-error')).toBeUndefined()
    await toast.get('.toast-error button').trigger('click')
    expect(toast.emitted('dismiss-error')).toHaveLength(1)
  })

  it('preserves reading time while the app is in the background', () => {
    const toast = render()
    vi.advanceTimersByTime(1000)
    const hidden = vi.spyOn(document, 'hidden', 'get').mockReturnValue(true)
    document.dispatchEvent(new Event('visibilitychange'))
    vi.advanceTimersByTime(60000)
    expect(toast.emitted('dismiss-success')).toBeUndefined()
    hidden.mockReturnValue(false)
    document.dispatchEvent(new Event('visibilitychange'))
    vi.advanceTimersByTime(4999)
    expect(toast.emitted('dismiss-success')).toBeUndefined()
    vi.advanceTimersByTime(1)
    expect(toast.emitted('dismiss-success')).toHaveLength(1)
  })
})
