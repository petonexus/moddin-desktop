<script setup lang="ts">
import { onMounted, onUnmounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'

const { t } = useI18n()
const open = ref(false)

function show() {
  open.value = true
}

function hide() {
  open.value = false
}

// Listen for the global "open contribute" event so the AI topbar split
// button (or any future trigger) can surface the contribute dialog
// without the App.vue having to reach into the trigger component
// directly (keeps the architecture boundary clean).
function onOpenContribute() {
  show()
}

onMounted(() => {
  window.addEventListener('moddin:open-contribute', onOpenContribute)
})

onUnmounted(() => {
  window.removeEventListener('moddin:open-contribute', onOpenContribute)
})

defineExpose({ show, hide })
</script>

<template>
  <button class="ct-fab" type="button" @click="show">
    <span>✨</span>
    {{ t('contributeButton') }}
  </button>
  <Teleport to="body">
    <ContributeDialog :open="open" @close="hide" />
  </Teleport>
</template>

<script lang="ts">
import ContributeDialog from './ContributeDialog.vue'
</script>

<style scoped>
.ct-fab {
  display: inline-flex;
  align-items: center;
  gap: 0.4rem;
  padding: 0.5rem 0.9rem;
  border-radius: 999px;
  border: 1px solid var(--moddin-accent, #7aa2f7);
  background: var(--moddin-surface-2, #1a1f29);
  color: var(--moddin-text, #e8ecf2);
  font-size: 0.85rem;
  cursor: pointer;
}

.ct-fab:hover {
  background: var(--moddin-surface, #141823);
}
</style>
