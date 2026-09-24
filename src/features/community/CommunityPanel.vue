<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import AppIcon from '../../components/ui/AppIcon.vue'
import { useDialogLifecycle } from '../../composables/useDialogLifecycle'
import { dateLocaleFor } from '../../i18n/locale'
import { COMMUNITY_TTL_PRESETS, type CommunityCatalogEntry, type CommunityFetchResult } from '../../types/community'
import { communityCopyForLocale, formatCopy } from './copy'
import {
  fetchCommunityCatalog,
  installCommunityCapability as installCapability,
  listCapabilities,
  setCommunityCatalogTtl,
} from './service'
import type { CapabilitySummary } from './types'

const { locale } = useI18n()
const copy = computed(() => communityCopyForLocale(locale.value))

const open = ref(false)
const loading = ref(false)
const installingId = ref<string | null>(null)
const error = ref<string | null>(null)
const success = ref<string | null>(null)
const { dialogElement, openDialog, closeDialog } = useDialogLifecycle(open)

const fetchResult = ref<CommunityFetchResult | null>(null)
const ttlSeconds = ref<number>(24 * 60 * 60)
const acceptUnsigned = ref<Record<string, boolean>>({})
const capabilities = ref<CapabilitySummary[]>([])

const communityEntries = computed<CommunityCatalogEntry[]>(() => fetchResult.value?.catalog.capabilities ?? [])

async function refreshCapabilities() {
  capabilities.value = await listCapabilities()
}

async function openPanel() {
  await openDialog()
  await Promise.all([refreshCapabilities(), fetchCatalog(false)])
}

// Reset transient messages whenever the dialog closes (button, Esc or backdrop).
watch(open, (isOpen) => {
  if (isOpen) return
  error.value = null
  success.value = null
})

async function fetchCatalog(forceRefresh: boolean) {
  loading.value = true
  error.value = null
  try {
    const result = await fetchCommunityCatalog(forceRefresh, ttlSeconds.value)
    fetchResult.value = result
    if (result.lastError && !result.signatureVerified) error.value = result.lastError
    if (result.ttlSeconds !== ttlSeconds.value && result.ttlSeconds > 0) ttlSeconds.value = result.ttlSeconds
  } catch (err) {
    error.value = err instanceof Error ? err.message : String(err)
  } finally {
    loading.value = false
  }
}

async function setTtl(seconds: number) {
  ttlSeconds.value = await setCommunityCatalogTtl(seconds)
  await fetchCatalog(true)
}

async function install(entry: CommunityCatalogEntry) {
  if (!communityEntries.value.some((item) => item.id === entry.id)) {
    error.value = copy.value.notInCatalog
    return
  }
  if (!entry.signed && !acceptUnsigned.value[entry.id]) {
    error.value = copy.value.needsConsent
    return
  }

  installingId.value = entry.id
  error.value = null
  success.value = null
  try {
    await installCapability({
      capabilityId: entry.id,
      gameId: readSelectedAppId() ?? 'unknown',
      gameName: 'community install',
      installDir: '',
      executableDir: '',
      config: {},
      acceptUnsigned: acceptUnsigned.value[entry.id] ?? false,
    })
    success.value = formatCopy(copy.value.installed, { name: entry.displayName || entry.id })
    await refreshCapabilities()
  } catch (err) {
    error.value = formatCopy(copy.value.installFailed, { error: err instanceof Error ? err.message : String(err) })
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

function ttlLabel(seconds: number) {
  if (seconds === 0) return copy.value.ttlManual
  if (seconds < 24 * 60 * 60) return formatCopy(copy.value.ttlHours, { count: Math.round(seconds / 3600) })
  if (seconds === 24 * 60 * 60) return copy.value.ttlDaily
  return formatCopy(copy.value.ttlDays, { count: Math.round(seconds / 86400) })
}

function setAcceptUnsigned(id: string, value: boolean) {
  acceptUnsigned.value = { ...acceptUnsigned.value, [id]: value }
}

function originLabel(origin: CapabilitySummary['origin']) {
  if (origin === 'builtIn') return copy.value.originBuiltIn
  if (origin === 'local') return copy.value.originLocal
  return copy.value.originCommunity
}

function formatTimestamp(seconds: number): string {
  if (!seconds) return copy.value.never
  return new Intl.DateTimeFormat(dateLocaleFor(locale.value), { dateStyle: 'short', timeStyle: 'short' }).format(new Date(seconds * 1000))
}

onMounted(async () => {
  try {
    const clamped = await setCommunityCatalogTtl(0)
    if (clamped > 0) ttlSeconds.value = clamped
  } catch {
    /* ignored — fall back to the local 24h default */
  }
})
</script>

<template>
  <button class="nav-item" type="button" @click="openPanel">
    <AppIcon name="community" />
    <span>{{ copy.button }}</span>
  </button>

  <Teleport to="body">
    <div v-if="open" class="dialog-backdrop" @click.self="closeDialog">
      <section
        ref="dialogElement"
        class="dialog dialog-lg"
        role="dialog"
        aria-modal="true"
        :aria-label="copy.title"
        tabindex="-1"
      >
        <header class="dialog-header">
          <div>
            <h2>{{ copy.title }}</h2>
            <p class="dialog-description">{{ copy.subtitle }}</p>
          </div>
          <button class="btn btn-icon" type="button" :aria-label="copy.close" @click="closeDialog">
            <AppIcon name="close" :size="18" />
          </button>
        </header>

        <div class="dialog-body">
          <div class="community-toolbar">
            <label class="field">
              <span>{{ copy.refreshEvery }}</span>
              <select class="select" :value="ttlSeconds" @change="setTtl(Number(($event.target as HTMLSelectElement).value))">
                <option v-for="preset in COMMUNITY_TTL_PRESETS" :key="preset.seconds" :value="preset.seconds">
                  {{ ttlLabel(preset.seconds) }}
                </option>
              </select>
            </label>
            <div class="community-refresh">
              <small>{{ formatCopy(copy.lastUpdated, { timestamp: formatTimestamp(fetchResult?.cachedAt ?? 0) }) }}</small>
              <button class="btn btn-sm" :class="{ 'is-loading': loading }" type="button" :disabled="loading" @click="fetchCatalog(true)">
                <AppIcon v-if="!loading" name="refresh" :size="14" />
                {{ copy.refreshNow }}
              </button>
            </div>
          </div>

          <div v-if="fetchResult" class="community-trust" :class="{ bad: !fetchResult.signatureVerified }">
            <AppIcon :name="fetchResult.signatureVerified ? 'shield' : 'alert'" />
            <span>{{ fetchResult.signatureVerified ? copy.catalogVerified : copy.catalogNotVerified }}</span>
            <small :title="fetchResult.bootstrapPublicKeyFingerprint">
              {{ formatCopy(copy.keyFingerprint, { fingerprint: fetchResult.bootstrapPublicKeyFingerprint }) }}
            </small>
          </div>

          <div v-if="error" class="callout callout-danger" role="alert">{{ error }}</div>
          <div v-if="success" class="callout callout-info" role="status">{{ success }}</div>

          <div v-if="loading" class="empty-state">
            <span class="spinner" />
            <span>{{ copy.loading }}</span>
          </div>
          <div v-else-if="communityEntries.length === 0" class="empty-state">
            <AppIcon name="community" :size="28" />
            <span>{{ copy.empty }}</span>
          </div>

          <div v-else class="community-list">
            <article v-for="entry in communityEntries" :key="entry.id" class="community-entry">
              <div class="community-entry-top">
                <div>
                  <strong>{{ entry.displayName || entry.id }}</strong>
                  <small>v{{ entry.version }}</small>
                </div>
                <span class="badge" :class="entry.signed ? 'badge-success' : 'badge-warning'">
                  {{ entry.signed ? copy.signed : copy.unsigned }}
                </span>
              </div>

              <ul v-if="entry.safetyNotes.length" class="note-list community-notes">
                <li v-for="(note, index) in entry.safetyNotes" :key="index">{{ note }}</li>
              </ul>

              <label v-if="!entry.signed" class="community-consent">
                <input
                  type="checkbox"
                  :checked="acceptUnsigned[entry.id] ?? false"
                  @change="setAcceptUnsigned(entry.id, ($event.target as HTMLInputElement).checked)"
                />
                <span>{{ copy.acceptUnsigned }}</span>
              </label>

              <div class="community-entry-actions">
                <button
                  class="btn btn-primary btn-sm"
                  :class="{ 'is-loading': installingId === entry.id }"
                  type="button"
                  :disabled="installingId === entry.id || (!entry.signed && !acceptUnsigned[entry.id])"
                  @click="install(entry)"
                >
                  {{ installingId === entry.id ? copy.installing : copy.install }}
                </button>
              </div>
            </article>
          </div>

          <details v-if="capabilities.length" class="disclosure community-builtin">
            <summary>{{ formatCopy(copy.builtInHeading, { count: capabilities.length }) }}</summary>
            <ul>
              <li v-for="cap in capabilities" :key="`${cap.id}-${cap.origin}`">
                <span>{{ cap.displayName }}</span>
                <span class="badge badge-plain" :class="cap.origin === 'community' ? 'badge-accent' : cap.origin === 'local' ? 'badge-warning' : ''">
                  {{ originLabel(cap.origin) }}
                </span>
              </li>
            </ul>
          </details>
        </div>
      </section>
    </div>
  </Teleport>
</template>

<style scoped>
.community-toolbar { display: flex; flex-wrap: wrap; align-items: flex-end; justify-content: space-between; gap: var(--moddin-space-3); }
.community-toolbar .field { min-width: 200px; }
.community-refresh { display: flex; align-items: center; gap: var(--moddin-space-3); }
.community-refresh small { color: var(--moddin-text-faint); font-size: var(--moddin-text-xs); }

.community-trust {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: var(--moddin-space-2);
  color: var(--moddin-success);
  font-size: var(--moddin-text-sm);
  font-weight: 600;
}
.community-trust.bad { color: var(--moddin-warning); }
.community-trust small { margin-left: auto; color: var(--moddin-text-faint); font-family: var(--moddin-mono); font-size: var(--moddin-text-xs); font-weight: 400; }

.community-list { display: grid; gap: var(--moddin-space-2); }
.community-entry {
  display: grid;
  gap: var(--moddin-space-3);
  border: 1px solid var(--moddin-line-soft);
  border-radius: var(--moddin-radius-md);
  padding: var(--moddin-space-3) var(--moddin-space-4);
  background: var(--moddin-surface-2);
}
.community-entry-top { display: flex; align-items: flex-start; justify-content: space-between; gap: var(--moddin-space-3); }
.community-entry-top strong { display: block; font-size: var(--moddin-text-md); }
.community-entry-top small { color: var(--moddin-text-faint); font-size: var(--moddin-text-xs); }
.community-notes { color: var(--moddin-warning); }
.community-consent { display: flex; align-items: flex-start; gap: var(--moddin-space-2); color: var(--moddin-text-soft); font-size: var(--moddin-text-sm); }
.community-consent input { margin-top: 3px; accent-color: var(--moddin-accent); }
.community-entry-actions { display: flex; justify-content: flex-end; }

.community-builtin > summary { color: var(--moddin-text-muted); font-size: var(--moddin-text-sm); font-weight: 600; }
.community-builtin ul { display: grid; gap: var(--moddin-space-2); margin: var(--moddin-space-3) 0 0; padding: 0; list-style: none; }
.community-builtin li { display: flex; align-items: center; justify-content: space-between; gap: var(--moddin-space-3); color: var(--moddin-text-soft); font-size: var(--moddin-text-sm); }
</style>
