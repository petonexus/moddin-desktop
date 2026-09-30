<script setup lang="ts">
import { computed, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import AppIcon from '../ui/AppIcon.vue'
import BaseDialog from '../ui/BaseDialog.vue'
import EmptyState from '../ui/EmptyState.vue'
import { useAiAssistant } from '../../composables/useAiAssistant'
import { listCapabilitiesForAssistant } from '../../features/ai-assistant/service'
import { readLocalValue } from '../../services/storage'
import { findCatalogGameById } from '../../services/catalog'
import type { CatalogCapability } from '../../types/ai-assistant'

/**
 * The three ways to reach the assistant from a game.
 *
 * Every verb ends in the same place: `useAiAssistant().openFor`, which is
 * the single entry point the topbar and the sidebar already use. The
 * improve flow collects a target first because "improve this mod" is
 * meaningless without a mod to improve, and because the assistant can
 * only be asked about a mod that is actually installed.
 */
const { t } = useI18n()
const emit = defineEmits<{ close: [] }>()
const ai = useAiAssistant()

const mode = ref<'menu' | 'improve'>('menu')
const intent = ref('')
const selectedId = ref('')
const capabilities = ref<CatalogCapability[]>([])
const loading = ref(false)
const loadFailed = ref(false)

/**
 * The selected game lives in local storage so the panels that need it do
 * not have to be handed a prop; the catalog turns that id into the name
 * the prompt shows, so the assistant still says which game it means.
 */
const game = computed(() => {
  const gameId = readLocalValue('moddin-selected-appId')
  if (!gameId) return { gameId: null, gameName: null }
  return { gameId, gameName: findCatalogGameById(gameId)?.name ?? null }
})

async function openImprove() {
  mode.value = 'improve'
  loading.value = true
  loadFailed.value = false
  try {
    const installed = await listCapabilitiesForAssistant()
    capabilities.value = installed
      .filter((capability) => capability.origin === 'community' || capability.origin === 'local')
      .map((capability) => ({
        id: capability.id,
        displayName: capability.displayName,
        category: capability.category,
        description: capability.description ?? '',
      }))
    loadFailed.value = false
  } catch {
    loadFailed.value = true
  } finally {
    loading.value = false
  }
}

function submitImprove() {
  const capabilityId = selectedId.value
  if (!capabilityId) return
  void ai.openFor({
    mode: 'improve',
    capabilityId,
    gameId: game.value.gameId,
    gameName: game.value.gameName,
    intent: intent.value,
  })
  emit('close')
}

function start(target: 'author' | 'improve' | 'diagnose') {
  if (target === 'improve') {
    void openImprove()
    return
  }
  void ai.openFor({
    mode: target,
    gameId: game.value.gameId,
    gameName: game.value.gameName,
    intent: target === 'author' ? t('contributeAuthorIntent') : t('contributeDiagnoseIntent'),
  })
  emit('close')
}
</script>

<template>
  <BaseDialog :title="t('contributeTitle')" :eyebrow="t('navToolsMods')" size="lg" @close="emit('close')">
    <div class="contribute">
      <p class="contribute-subtitle">{{ t('contributeSubtitle') }}</p>

      <template v-if="mode === 'menu'">
        <div class="contribute-grid">
          <button class="contribute-card" type="button" @click="start('author')">
            <AppIcon name="ai" />
            <strong>{{ t('contributeAuthorTitle') }}</strong>
            <span>{{ t('contributeAuthorBody') }}</span>
          </button>
          <button class="contribute-card" type="button" @click="start('improve')">
            <AppIcon name="refresh" />
            <strong>{{ t('contributeImproveTitle') }}</strong>
            <span>{{ t('contributeImproveBody') }}</span>
          </button>
          <button class="contribute-card" type="button" @click="start('diagnose')">
            <AppIcon name="terminal" />
            <strong>{{ t('contributeDiagnoseTitle') }}</strong>
            <span>{{ t('contributeDiagnoseBody') }}</span>
          </button>
        </div>
      </template>

      <form v-else class="contribute-improve" @submit.prevent="submitImprove">
        <EmptyState v-if="loading" busy />

        <label class="field">
          <span>{{ t('contributeImprovePickerLabel') }}</span>
          <select v-model="selectedId" :disabled="loading">
            <option value="" disabled>{{ t('contributeImprovePickerPlaceholder') }}</option>
            <option v-for="capability in capabilities" :key="capability.id" :value="capability.id">
              {{ capability.displayName }}
            </option>
          </select>
        </label>

        <p v-if="loadFailed" class="contribute-improve-empty">{{ t('contributeNoCapabilities') }}</p>

        <label class="field">
          <span>{{ t('contributeImproveIntentLabel') }}</span>
          <textarea
            v-model="intent"
            rows="3"
            :placeholder="t('contributeImproveIntentPlaceholder')"
          />
        </label>
      </form>
    </div>

    <template #footer>
      <button v-if="mode === 'improve'" class="btn" type="button" @click="mode = 'menu'">
        {{ t('contributeBack') }}
      </button>
      <button v-if="mode === 'improve'" class="btn btn-primary" type="button" :disabled="!selectedId" @click="submitImprove">
        {{ t('contributeImproveContinue') }}
      </button>
    </template>
  </BaseDialog>
</template>

<style scoped>
.contribute { display: grid; gap: var(--moddin-space-4); }
.contribute-subtitle { margin: 0; color: var(--moddin-text-soft); font-size: var(--moddin-text-sm); }

.contribute-grid { display: grid; gap: var(--moddin-space-3); }
.contribute-card {
  display: grid;
  gap: var(--moddin-space-1);
  justify-items: start;
  text-align: left;
  border: 1px solid var(--moddin-line-soft);
  border-radius: var(--moddin-radius-md);
  background: var(--moddin-surface-2);
  padding: var(--moddin-space-3) var(--moddin-space-4);
  cursor: pointer;
}
.contribute-card:hover { border-color: var(--moddin-accent); }
.contribute-card strong { font-size: var(--moddin-text-md); }
.contribute-card span { color: var(--moddin-text-soft); font-size: var(--moddin-text-sm); }

.contribute-improve { display: grid; gap: var(--moddin-space-3); }
.contribute-improve-empty { margin: 0; color: var(--moddin-text-muted); font-size: var(--moddin-text-sm); }
</style>
