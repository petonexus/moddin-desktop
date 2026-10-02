import { nextTick, onUnmounted, ref, type Ref } from 'vue'

const FOCUSABLE_SELECTOR = [
  'a[href]',
  'button',
  'input:not([type="hidden"])',
  'select',
  'textarea',
  'summary',
  '[contenteditable="true"]',
  '[tabindex]',
].join(',')

interface ActiveDialog {
  element: Ref<HTMLElement | null>
  layer: Ref<number>
  active: Ref<boolean>
  focus: () => void
}

// One keyboard event belongs to the uppermost dialog, including a
// confirmation opened from another modal.
const activeDialogs: ActiveDialog[] = []

function updateDialogLayers() {
  activeDialogs.forEach((dialog, index) => {
    dialog.layer.value = 100 + index
    dialog.active.value = index === activeDialogs.length - 1
  })
}

function isVisible(element: HTMLElement) {
  if (!element.isConnected || element.closest('[hidden], [inert], [aria-hidden="true"]')) return false

  for (let ancestor: HTMLElement | null = element; ancestor; ancestor = ancestor.parentElement) {
    const style = window.getComputedStyle(ancestor)
    if (style.display === 'none' || style.visibility === 'hidden') return false
    if (ancestor instanceof HTMLDetailsElement && !ancestor.open) {
      const summary = ancestor.querySelector('summary')
      if (!summary?.contains(element)) return false
    }
  }
  return true
}

export function useDialogLifecycle(open: Ref<boolean>, options: { canClose?: () => boolean } = {}) {
  const dialogElement = ref<HTMLElement | null>(null)
  const dialogLayer = ref(100)
  const dialogActive = ref(true)
  let opener: HTMLElement | null = null
  let lastFocused: HTMLElement | null = null
  const controller: ActiveDialog = {
    element: dialogElement, layer: dialogLayer, active: dialogActive, focus: focusDialog,
  }

  function isTopDialog() {
    return activeDialogs.at(-1) === controller
  }

  function rememberOpener() {
    if (typeof document === 'undefined') return
    opener = document.activeElement instanceof HTMLElement ? document.activeElement : null
  }

  function focusableElements() {
    const dialog = dialogElement.value
    if (!dialog) return []
    return Array.from(dialog.querySelectorAll<HTMLElement>(FOCUSABLE_SELECTOR))
      .filter((element) => element.tabIndex >= 0 && !element.matches(':disabled') && isVisible(element))
  }

  function focusDialog() {
    const dialog = dialogElement.value
    if (!dialog?.isConnected) return
    const preferred = dialog.querySelector<HTMLElement>('[data-dialog-initial-focus]')
    const target = lastFocused && dialog.contains(lastFocused) && isVisible(lastFocused)
      && !lastFocused.matches(':disabled') ? lastFocused
      : preferred && isVisible(preferred) && !preferred.matches(':disabled') ? preferred : dialog
    target.focus({ preventScroll: true })
  }

  async function openDialog() {
    if (activeDialogs.includes(controller)) return
    rememberOpener()
    open.value = true
    // Vue mounts children before parents. An initially nested dialog must
    // still remain above its parent in both the keyboard and paint stacks.
    const descendantIndex = activeDialogs.findIndex((entry) =>
      entry.element.value && dialogElement.value?.contains(entry.element.value),
    )
    if (descendantIndex === -1) activeDialogs.push(controller)
    else activeDialogs.splice(descendantIndex, 0, controller)
    updateDialogLayers()
    await nextTick()
    if (open.value && isTopDialog()) focusDialog()
  }

  function restoreFocus() {
    const target = opener
    opener = null
    void nextTick(() => {
      const current = activeDialogs.at(-1)
      if (target && isVisible(target) && !target.matches(':disabled')
        && (!current || current.element.value?.contains(target))) {
        target.focus({ preventScroll: true })
      } else {
        current?.focus()
      }
    })
  }

  function releaseDialog() {
    const wasTop = isTopDialog()
    const index = activeDialogs.indexOf(controller)
    if (index !== -1) activeDialogs.splice(index, 1)
    controller.active.value = false
    updateDialogLayers()
    if (wasTop) restoreFocus()
  }

  function closeDialog() {
    if (!open.value || !isTopDialog() || options.canClose?.() === false) return
    open.value = false
    releaseDialog()
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
    if (!open.value || !isTopDialog() || event.defaultPrevented) return

    if (event.key === 'Escape') {
      event.preventDefault()
      closeDialog()
      return
    }

    if (event.key === 'Tab') trapTab(event)
  }

  function handleFocus(event: FocusEvent) {
    if (!open.value || !isTopDialog()) return
    const dialog = dialogElement.value
    if (!dialog?.isConnected) return
    if (event.target instanceof HTMLElement && dialog.contains(event.target)) {
      lastFocused = event.target
    } else {
      focusDialog()
    }
  }

  if (typeof window !== 'undefined') {
    window.addEventListener('keydown', handleKeydown)
    window.addEventListener('focusin', handleFocus)
  }

  onUnmounted(() => {
    if (typeof window !== 'undefined') {
      window.removeEventListener('keydown', handleKeydown)
      window.removeEventListener('focusin', handleFocus)
    }
    // A dialog can also disappear without going through `closeDialog` — a
    // confirmed action unmounts it — and dropping focus on <body> there
    // loses the user's place entirely.
    releaseDialog()
  })

  return {
    dialogElement,
    dialogLayer,
    dialogActive,
    openDialog,
    closeDialog,
  }
}
