import { onUnmounted, type Ref } from 'vue'

export function useDialogLifecycle(open: Ref<boolean>) {
  let opener: HTMLElement | null = null

  function rememberOpener() {
    if (typeof document === 'undefined') return
    opener = document.activeElement instanceof HTMLElement ? document.activeElement : null
  }

  function openDialog() {
    rememberOpener()
    open.value = true
  }

  function closeDialog() {
    if (!open.value) return
    open.value = false

    const target = opener
    opener = null
    if (target) queueMicrotask(() => target.focus())
  }

  function handleKeydown(event: KeyboardEvent) {
    if (event.key !== 'Escape' || !open.value) return
    event.preventDefault()
    closeDialog()
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
    openDialog,
    closeDialog,
  }
}
