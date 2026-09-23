<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { invokeDebug as invoke } from '../../debug'
import {
  COMMUNITY_TTL_PRESETS,
  type CommunityCatalog,
  type CommunityCatalogEntry,
  type CommunityFetchResult,
} from '../../types/community'
import { useAiAssistantTrigger } from '../../composables/useAiAssistant'
import type { AuthorPromptContext } from '../../types/ai-assistant'
import CollectionsTab from './CollectionsTab.vue'

const { t, locale } = useI18n()
const { openFor } = useAiAssistantTrigger()

type CommunityTab = 'capabilities' | 'collections'
const activeTab = ref<CommunityTab>('capabilities')

function openAiAssistant() {
  // The community panel already lists curated capabilities; the
  // "Add mod with AI" entry point opens the AI dialog with author
  // mode pre-selected. The user can switch to improve / diagnose
  // from inside the dialog if they prefer.
  const ctx: AuthorPromptContext = { mode: 'author' }
  openFor(ctx)
}

interface CapabilitySummary {
  id: string
  displayName: string
  category: string
  status: string
  origin: 'builtIn' | 'local' | 'community'
}

interface InstallResult {
  capabilityId: string
  transaction: { id: string } | null
  steps: Array<{ kind: string; affectedPaths: string[] }>
  affectedPaths: string[]
}

const open = ref(false)
const loading = ref(false)
const installingId = ref<string | null>(null)
const error = ref<string | null>(null)
const success = ref<string | null>(null)
const dialogElement = ref<HTMLElement | null>(null)

const fetchResult = ref<CommunityFetchResult | null>(null)
const ttlSeconds = ref<number>(24 * 60 * 60)
const acceptUnsigned = ref<Record<string, boolean>>({})

const capabilities = ref<CapabilitySummary[]>([])

async function refreshCapabilities() {
  const list = await invoke<CapabilitySummary[]>('capability_list')
  capabilities.value = list
}

async function openPanel() {
  open.value = true
  await Promise.all([refreshCapabilities(), fetchCatalog({ forceRefresh: false })])
}

function closePanel() {
  open.value = false
  error.value = null
  success.value = null
}

async function fetchCatalog(opts: { forceRefresh: boolean } = { forceRefresh: false }) {
  loading.value = true
  error.value = null
  try {
    const result = await invoke<CommunityFetchResult>('community_catalog_fetch', {
      forceRefresh: opts.forceRefresh,
      ttlSeconds: ttlSeconds.value,
    })
    fetchResult.value = result
    if (result.lastError && !result.signatureVerified) {
      error.value = result.lastError
    }
    if (result.ttlSeconds !== ttlSeconds.value && result.ttlSeconds > 0) {
      ttlSeconds.value = result.ttlSeconds
    }
  } catch (err) {
    error.value = err instanceof Error ? err.message : String(err)
  } finally {
    loading.value = false
  }
}

async function setTtl(seconds: number) {
  const clamped = await invoke<number>('community_catalog_set_ttl', { ttlSeconds: seconds })
  ttlSeconds.value = clamped
  await fetchCatalog({ forceRefresh: true })
}

function findCatalogEntry(id: string): CommunityCatalogEntry | undefined {
  return fetchResult.value?.catalog.capabilities.find((entry) => entry.id === id)
}

async function installCommunityCapability(capabilityId: string) {
  const entry = findCatalogEntry(capabilityId)
  if (!entry) {
    error.value = `Capability '${capabilityId}' is not in the cached community catalog.`
    return
  }
  if (!entry.signed && !acceptUnsigned.value[capabilityId]) {
    error.value = `Capability '${capabilityId}' is unsigned. Tick 'Accept unsigned' to install.`
    return
  }

  installingId.value = capabilityId
  error.value = null
  success.value = null
  try {
    const selectedAppId = readSelectedAppId()
    const result = await invoke<InstallResult>('community_capability_install', {
      capabilityId,
      gameId: selectedAppId ?? 'unknown',
      gameName: 'community install',
      installDir: '',
      executableDir: '',
      config: {},
      acceptUnsigned: acceptUnsigned.value[capabilityId] ?? false,
    })
    success.value = `Installed ${capabilityId} (transaction ${result.transaction?.id ?? 'none'})`
    await refreshCapabilities()
  } catch (err) {
    error.value = `Install failed: ${err instanceof Error ? err.message : String(err)}`
  } finally {
    installingId.value = null
  }
}

function readSelectedAppId(): string | null {
  try {
    return window.localStorage.getItem('moddin-selected-appId')
  } catch {
    return null
  }
}

const ttlLabel = computed(() => {
  if (ttlSeconds.value === 0) return t('communityTtlManual')
  if (ttlSeconds.value < 60 * 60) {
    return `${Math.max(1, Math.round(ttlSeconds.value / 60))} min`
  }
  if (ttlSeconds.value < 24 * 60 * 60) {
    return `${Math.round(ttlSeconds.value / (60 * 60))} h`
  }
  return `${(ttlSeconds.value / (24 * 60 * 60)).toFixed(1)} d`
})

const communityEntries = computed<CommunityCatalogEntry[]>(() =>
  fetchResult.value?.catalog.capabilities ?? [],
)

function setAcceptUnsigned(id: string, value: boolean) {
  acceptUnsigned.value = { ...acceptUnsigned.value, [id]: value }
}

function originLabel(origin: CapabilitySummary['origin']) {
  if (origin === 'builtIn') return t('communityOriginBuiltIn')
  if (origin === 'local') return t('communityOriginLocal')
  return t('communityOriginCommunity')
}

function badgeClass(signed: boolean) {
  return signed ? 'community-badge verified' : 'community-badge unsigned'
}

function badgeLabel(signed: boolean) {
  return signed ? t('communitySigned') : t('communityUnsigned')
}

function formatTimestamp(seconds: number): string {
  if (!seconds) return '—'
  return new Intl.DateTimeFormat(locale.value, {
    dateStyle: 'short',
    timeStyle: 'medium',
  }).format(new Date(seconds * 1000))
}

function closeOnBackdrop(event: MouseEvent) {
  if (event.target === event.currentTarget) {
    closePanel()
  }
}

onMounted(async () => {
  try {
    const clamped = await invoke<number>('community_catalog_set_ttl', { ttlSeconds: 0 })
    if (clamped > 0) ttlSeconds.value = clamped
  } catch {
    /* ignored — we'll fall back to the local 24h default */
  }
  await refreshCapabilities()
})
</script>

<template>
  <button class="community-fab" type="button" @click="openPanel">
    <span>⇆</span>
    {{ t('communityButton') }}
  </button>

  <div v-if="open" class="community-backdrop" @click="closeOnBackdrop">
    <section
      ref="dialogElement"
      class="community-panel"
      role="dialog"
      aria-modal="true"
      :aria-label="t('communityTitle')"
      tabindex="-1"
    >
      <header class="community-header">
        <div>
          <small>COMMUNITY</small>
          <h2>{{ t('communityTitle') }}</h2>
          <p>{{ t('communitySubtitle') }}</p>
        </div>
        <div class="community-header-actions">
          <button
            class="community-ai-button"
            type="button"
            @click="openAiAssistant"
          >
            <span>✨</span>
            {{ t('aiAssistantButton') }}
          </button>
          <button class="community-icon-button" type="button" :aria-label="t('communityClose')" @click="closePanel">×</button>
        </div>
      </header>

      <div class="community-tabs" role="tablist">
        <button
          type="button"
          role="tab"
          :class="{ active: activeTab === 'capabilities' }"
          :aria-selected="activeTab === 'capabilities'"
          @click="activeTab = 'capabilities'"
        >
          {{ t('communityTabCapabilities') }}
        </button>
        <button
          type="button"
          role="tab"
          :class="{ active: activeTab === 'collections' }"
          :aria-selected="activeTab === 'collections'"
          @click="activeTab = 'collections'"
        >
          {{ t('communityTabCollections') }}
        </button>
      </div>

      <div v-show="activeTab === 'collections'" role="tabpanel">
        <CollectionsTab />
      </div>

      <div v-if="activeTab === 'capabilities'">
      <div class="community-toolbar">
        <label class="community-ttl">
          <span>{{ t('communityRefreshEvery') }}</span>
          <select
            :value="ttlSeconds"
            @change="setTtl(Number(($event.target as HTMLSelectElement).value))"
          >
            <option v-for="preset in COMMUNITY_TTL_PRESETS" :key="preset.label" :value="preset.seconds">
              {{ preset.label }}
            </option>
          </select>
        </label>
        <span class="community-cache-hint">
          {{ t('communityCachedUntil', { timestamp: formatTimestamp(fetchResult?.cachedAt ?? 0) }) }}
        </span>
        <button class="community-secondary" type="button" :disabled="loading" @click="fetchCatalog({ forceRefresh: true })">
          {{ t('communityRefreshNow') }}
        </button>
      </div>

      <div v-if="fetchResult" class="community-meta">
        <span class="community-meta-key">
          {{ t('communityBootstrapKey') }}: <code>{{ fetchResult.bootstrapPublicKeyFingerprint }}</code>
        </span>
        <span :class="badgeClass(fetchResult.signatureVerified)">
          {{ fetchResult.signatureVerified ? t('communitySignatureVerified') : t('communitySignatureFailed') }}
        </span>
      </div>

      <div v-if="error" class="community-error">{{ error }}</div>
      <div v-if="success" class="community-success">{{ success }}</div>

      <div v-if="communityEntries.length === 0 && !loading" class="community-empty">
        {{ t('communityEmpty') }}
      </div>
      <div v-else-if="loading" class="community-empty">{{ t('communityLoading') }}</div>

      <div v-else class="community-list">
        <article
          v-for="entry in communityEntries"
          :key="entry.id"
          class="community-entry"
        >
          <div class="community-entry-top">
            <div>
              <strong>{{ entry.displayName || entry.id }}</strong>
              <span class="community-entry-id">{{ entry.id }} · v{{ entry.version }}</span>
            </div>
            <span :class="badgeClass(entry.signed)">{{ badgeLabel(entry.signed) }}</span>
          </div>

          <p v-if="entry.safetyNotes.length > 0" class="community-notes">
            <span v-for="(note, index) in entry.safetyNotes" :key="index">⚠ {{ note }}</span>
          </p>

          <label v-if="!entry.signed" class="community-accept-unsigned">
            <input
              type="checkbox"
              :checked="acceptUnsigned[entry.id] ?? false"
              @change="setAcceptUnsigned(entry.id, ($event.target as HTMLInputElement).checked)"
            />
            <span>{{ t('communityAcceptUnsigned') }}</span>
          </label>

          <button
            class="community-primary"
            type="button"
            :disabled="installingId === entry.id"
            @click="installCommunityCapability(entry.id)"
          >
            {{ installingId === entry.id ? t('communityInstalling') : t('communityInstall') }}
          </button>
        </article>
      </div>

      <details v-if="capabilities.length > 0" class="community-builtin">
        <summary>{{ t('communityBuiltinHeading', { count: capabilities.length }) }}</summary>
        <ul>
          <li v-for="cap in capabilities" :key="`${cap.id}-${cap.origin}`">
            <strong>{{ cap.displayName }}</strong>
            <code>{{ cap.id }}</code>
            <span :class="`community-origin-tag origin-${cap.origin}`">{{ originLabel(cap.origin) }}</span>
          </li>
        </ul>
      </details>
      </div>
    </section>
  </div>
</template>

<style scoped>
.community-fab {
  display: inline-flex;
  align-items: center;
  gap: 0.4rem;
  padding: 0.5rem 0.9rem;
  border-radius: 999px;
  border: 1px solid var(--moddin-border, #3a4252);
  background: var(--moddin-surface-2, #1a1f29);
  color: var(--moddin-text, #e8ecf2);
  font-size: 0.85rem;
  cursor: pointer;
}

.community-backdrop {
  position: fixed;
  inset: 0;
  background: rgba(0, 0, 0, 0.55);
  z-index: 9000;
  display: flex;
  align-items: center;
  justify-content: center;
}

.community-panel {
  width: min(720px, 96vw);
  max-height: 88vh;
  overflow: auto;
  background: var(--moddin-surface, #141823);
  color: var(--moddin-text, #e8ecf2);
  border-radius: 12px;
  padding: 1.25rem 1.5rem 1.5rem;
  box-shadow: 0 24px 48px rgba(0, 0, 0, 0.6);
  display: flex;
  flex-direction: column;
  gap: 0.75rem;
}

.community-header {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 1rem;
}

.community-header small {
  letter-spacing: 0.08em;
  color: var(--moddin-accent, #7aa2f7);
  font-size: 0.75rem;
  font-weight: 700;
}

.community-icon-button {
  background: transparent;
  border: 1px solid var(--moddin-border, #3a4252);
  color: inherit;
  width: 32px;
  height: 32px;
  border-radius: 50%;
  cursor: pointer;
  font-size: 1.1rem;
}

.community-header-actions {
  display: flex;
  align-items: center;
  gap: 0.5rem;
}

.community-ai-button {
  display: inline-flex;
  align-items: center;
  gap: 0.35rem;
  background: transparent;
  border: 1px solid var(--moddin-accent, #7aa2f7);
  color: var(--moddin-accent, #7aa2f7);
  border-radius: 999px;
  padding: 0.3rem 0.7rem;
  font-size: 0.78rem;
  cursor: pointer;
}

.community-ai-button:hover {
  background: rgba(122, 162, 247, 0.1);
}

.community-tabs {
  display: flex;
  gap: 0.25rem;
  border-bottom: 1px solid var(--moddin-border, #3a4252);
}

.community-tabs button {
  background: transparent;
  border: 0;
  color: var(--moddin-muted, #8c93a3);
  padding: 0.5rem 0.9rem;
  font-size: 0.85rem;
  cursor: pointer;
  border-bottom: 2px solid transparent;
}

.community-tabs button.active {
  color: var(--moddin-accent, #7aa2f7);
  border-bottom-color: var(--moddin-accent, #7aa2f7);
  font-weight: 600;
}

.community-toolbar {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 0.75rem;
  padding-bottom: 0.5rem;
  border-bottom: 1px solid var(--moddin-border, #3a4252);
}

.community-ttl {
  display: inline-flex;
  align-items: center;
  gap: 0.4rem;
  font-size: 0.85rem;
}

.community-ttl select {
  background: var(--moddin-surface-2, #1a1f29);
  color: inherit;
  border: 1px solid var(--moddin-border, #3a4252);
  border-radius: 6px;
  padding: 0.3rem 0.5rem;
}

.community-cache-hint {
  font-size: 0.78rem;
  color: var(--moddin-muted, #8c93a3);
  flex: 1;
  text-align: right;
}

.community-secondary {
  background: transparent;
  border: 1px solid var(--moddin-accent, #7aa2f7);
  color: var(--moddin-accent, #7aa2f7);
  border-radius: 6px;
  padding: 0.35rem 0.85rem;
  cursor: pointer;
}

.community-secondary:disabled {
  opacity: 0.55;
  cursor: not-allowed;
}

.community-meta {
  display: flex;
  flex-wrap: wrap;
  gap: 0.75rem;
  font-size: 0.78rem;
  color: var(--moddin-muted, #8c93a3);
}

.community-meta code {
  background: var(--moddin-surface-2, #1a1f29);
  padding: 0 0.3rem;
  border-radius: 4px;
}

.community-badge {
  display: inline-flex;
  align-items: center;
  border-radius: 999px;
  padding: 0.15rem 0.6rem;
  font-size: 0.7rem;
  font-weight: 600;
  text-transform: uppercase;
  letter-spacing: 0.06em;
}

.community-badge.verified {
  background: rgba(40, 200, 120, 0.18);
  color: #2ecf86;
  border: 1px solid rgba(40, 200, 120, 0.4);
}

.community-badge.unsigned {
  background: rgba(240, 180, 50, 0.18);
  color: #f0b432;
  border: 1px solid rgba(240, 180, 50, 0.4);
}

.community-error {
  background: rgba(255, 90, 90, 0.12);
  border: 1px solid rgba(255, 90, 90, 0.4);
  color: #ff8c8c;
  padding: 0.5rem 0.7rem;
  border-radius: 6px;
}

.community-success {
  background: rgba(40, 200, 120, 0.12);
  border: 1px solid rgba(40, 200, 120, 0.4);
  color: #2ecf86;
  padding: 0.5rem 0.7rem;
  border-radius: 6px;
}

.community-list {
  display: flex;
  flex-direction: column;
  gap: 0.6rem;
}

.community-entry {
  background: var(--moddin-surface-2, #1a1f29);
  border: 1px solid var(--moddin-border, #3a4252);
  border-radius: 8px;
  padding: 0.7rem 0.9rem;
  display: flex;
  flex-direction: column;
  gap: 0.5rem;
}

.community-entry-top {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 0.75rem;
}

.community-entry-id {
  display: block;
  font-size: 0.78rem;
  color: var(--moddin-muted, #8c93a3);
  margin-top: 0.1rem;
}

.community-notes {
  margin: 0;
  display: flex;
  flex-direction: column;
  gap: 0.2rem;
  font-size: 0.78rem;
  color: var(--moddin-muted, #8c93a3);
}

.community-accept-unsigned {
  display: inline-flex;
  align-items: center;
  gap: 0.4rem;
  font-size: 0.78rem;
  color: var(--moddin-warning, #f0b432);
}

.community-primary {
  align-self: flex-end;
  background: var(--moddin-accent, #7aa2f7);
  color: #0c0e15;
  border: none;
  border-radius: 6px;
  padding: 0.4rem 1rem;
  font-weight: 600;
  cursor: pointer;
}

.community-primary:disabled {
  opacity: 0.55;
  cursor: not-allowed;
}

.community-empty {
  text-align: center;
  padding: 1.25rem;
  color: var(--moddin-muted, #8c93a3);
}

.community-builtin summary {
  cursor: pointer;
  padding: 0.5rem 0.25rem;
  font-weight: 600;
  color: var(--moddin-muted, #8c93a3);
}

.community-builtin ul {
  margin: 0.5rem 0 0;
  padding: 0 0 0 1rem;
  display: flex;
  flex-direction: column;
  gap: 0.25rem;
  font-size: 0.8rem;
}

.community-origin-tag {
  margin-left: 0.4rem;
  padding: 0.05rem 0.4rem;
  border-radius: 4px;
  font-size: 0.7rem;
  background: var(--moddin-surface, #141823);
  border: 1px solid var(--moddin-border, #3a4252);
}

.community-origin-tag.origin-local {
  border-color: #f0b432;
  color: #f0b432;
}

.community-origin-tag.origin-community {
  border-color: #7aa2f7;
  color: #7aa2f7;
}
</style>
