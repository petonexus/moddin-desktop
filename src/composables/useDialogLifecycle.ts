import { nextTick, onUnmounted, ref, type Ref } from 'vue'

const FOCUSABLE_SELECTOR = [
  'a[href]',
  'button:not([disabled])',
  'input:not([disabled])',
  'select:not([disabled])',
  'textarea:not([disabled])',
  '[tabindex]:not([tabindex="-1"])',
].join(',')

export function useDialogLifecycle(open: Ref<boolean>) {
  const dialogElement = ref<HTMLElement | null>(null)
  let opener: HTMLElement | null = null

  function rememberOpener() {
    if (typeof document === 'undefined') return
    opener = document.activeElement instanceof HTMLElement ? document.activeElement : null
  }

  function focusableElements() {
    const dialog = dialogElement.value
    if (!dialog) return []
    return Array.from(dialog.querySelectorAll<HTMLElement>(FOCUSABLE_SELECTOR))
      .filter((element) => element.getAttribute('aria-hidden') !== 'true')
  }

  async function openDialog() {
    rememberOpener()
    open.value = true
    await nextTick()

    const dialog = dialogElement.value
    if (!dialog) return
    const firstFocusable = focusableElements()[0]
    ;(firstFocusable ?? dialog).focus()
  }

  function closeDialog() {
    if (!open.value) return
    open.value = false

    const target = opener
    opener = null
    if (target) queueMicrotask(() => target.focus())
  }

  function trapTab(event: KeyboardEvent) {
    const dialog = dialogElement.value
    if (!dialog) return

    const focusable = focusableElements()
    if (!focusable.length) {
      event.preventDefault()
      dialog.focus()
      return
    }

    const first = focusable[0]
    const last = focusable[focusable.length - 1]
    const active = document.activeElement
    const outsideDialog = !(active instanceof Node) || !dialog.contains(active)

    if (event.shiftKey && (active === first || active === dialog || outsideDialog)) {
      event.preventDefault()
      last.focus()
      return
    }

    if (!event.shiftKey && (active === last || active === dialog || outsideDialog)) {
      event.preventDefault()
      first.focus()
    }
  }

  function handleKeydown(event: KeyboardEvent) {
    if (!open.value) return

    if (event.key === 'Escape') {
      event.preventDefault()
      closeDialog()
      return
    }

    if (event.key === 'Tab') trapTab(event)
  }

  if (typeof window !== 'undefined') {
    window.addEventListener('keydown', handleKeydown)
  }

  onUnmounted(() => {
    if (typeof window !== 'undefined') {
      window.removeEventListener('keydown', handleKeydown)
    }
  })

  return {
    dialogElement,
    openDialog,
    closeDialog,
  }
}
