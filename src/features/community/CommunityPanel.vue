<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import AppIcon from '../../components/ui/AppIcon.vue'
import BaseDialog from '../../components/ui/BaseDialog.vue'
import ErrorCallout from '../../components/ui/ErrorCallout.vue'
import EmptyState from '../../components/ui/EmptyState.vue'
import { useFriendlyError, type FriendlyErrorRule } from '../../composables/useFriendlyError'
import { dateLocaleFor } from '../../i18n/locale'
import { COMMUNITY_TTL_PRESETS, type CommunityCatalogEntry, type CommunityFetchResult } from '../../types/community'
import { communityCopyForLocale, formatCopy } from './copy'
import { resolveInstallTarget } from '../capability-modules/service'
import { recordUnsignedConsent } from '../../composables/useAiAssistant'
import {
  fetchCommunityCatalog,
  installCommunityCapability as installCapability,
  listCapabilities,
  reloadCapabilities,
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
/** Which action produced `error` — the raw string alone cannot say. */
const errorContext = ref<string | null>(null)

/**
 * Raw backend errors turned into a summary plus a 'why' line the user can
 * act on, with the original kept one disclosure away for power users.
 */
const errorRules = computed<FriendlyErrorRule[]>(() => {
  const c = copy.value
  return [
    { match: /signature|Verification equation/i, title: c.errorSigTitle, why: c.errorSigWhy, showRaw: true },
    { match: /network|fetch|timeout|ENOTFOUND|ETIMEDOUT|Could not reach/i, context: 'fetch', title: c.errorNetworkTitle, why: c.errorNetworkWhy, showRaw: true },
    { match: /not found|404|missing catalog|catalog\.json/i, context: 'fetch', title: c.errorMissingTitle, why: c.errorMissingWhy, showRaw: true },
    { context: 'fetch', title: c.errorGenericTitle, why: c.errorGenericWhy, showRaw: true },
    { match: /no space|ENOSPC|disk|quota/i, context: 'install', title: c.errorDiskTitle, why: c.errorDiskWhy, showRaw: true },
    { match: /denied|permission|0x80070005|access/i, context: 'install', title: c.errorPermissionTitle, why: c.errorPermissionWhy, showRaw: true },
    { context: 'install', title: c.errorInstallTitle, why: c.errorInstallWhy, showRaw: true },
  ]
})

/** Copy the panel already wrote; it needs no translation, only surfacing. */
const alreadyLocalized = computed(() => [
  copy.value.notInCatalog,
  copy.value.needsConsent,
  copy.value.noGameSelected,
])

const friendlyError = useFriendlyError({ error, rules: () => errorRules.value, context: errorContext, verbatim: alreadyLocalized })

const fetchResult = ref<CommunityFetchResult | null>(null)
const ttlSeconds = ref<number>(24 * 60 * 60)
const acceptUnsigned = ref<Record<string, boolean>>({})
const capabilities = ref<CapabilitySummary[]>([])

const communityEntries = computed<CommunityCatalogEntry[]>(() => fetchResult.value?.catalog.capabilities ?? [])

// Focus, Escape and focus restore are BaseDialog's job; these only say
// whether the dialog is on screen.
function openDialog() {
  open.value = true
}

function closeDialog() {
  open.value = false
}

/**
 * The maintainers' kill switch, keyed by capability id. It arrives inside
 * the signed catalog — the same bytes that carried the capability list —
 * so a revoked entry stays in the list and is flagged rather than
 * hidden: the user asked what the community catalogue offers, and "this
 * one was withdrawn, here is why" is more useful than a silently shorter
 * list. An absent list is the maintainer saying nothing is revoked, and
 * is read the same as an empty one.
 */
const revocations = computed<Map<string, string>>(() => {
  const map = new Map<string, string>()
  for (const entry of fetchResult.value?.catalog.revoked ?? []) map.set(entry.id, entry.reason)
  return map
})

function revocationFor(id: string): string | null {
  return revocations.value.has(id) ? revocations.value.get(id) ?? '' : null
}

/**
 * A capability is installable unless the maintainers revoked it. There is
 * no second "could not check the kill switch" case to guard here: the
 * list and the signature are one thing now, so a catalogue that did not
 * verify carries no capabilities at all (`communityEntries` is empty)
 * and the backend refuses the install anyway.
 */
function isBlocked(entry: CommunityCatalogEntry): boolean {
  return revocations.value.has(entry.id)
}

async function refreshCapabilities() {
  capabilities.value = await listCapabilities()
}

async function openPanel() {
  await openDialog()
  // `capability_reload` rather than `capability_list`: it re-reads the
  // local override directory, so a recipe the user just dropped in shows
  // up in this panel's built-in list without an app restart.
  const [reloaded] = await Promise.all([reloadCapabilities(), fetchCatalog(false)])
  capabilities.value = reloaded
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
  errorContext.value = 'fetch'
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
  const revoked = revocationFor(entry.id)
  if (revoked !== null) {
    error.value = formatCopy(copy.value.revokedReason, { name: entry.displayName || entry.id, reason: revoked || copy.value.revokedNoReason })
    return
  }
  if (!entry.signed && !acceptUnsigned.value[entry.id]) {
    error.value = copy.value.needsConsent
    return
  }

  installingId.value = entry.id
  error.value = null
  errorContext.value = 'install'
  success.value = null
  try {
    // Installing a community mod still means installing it *for a game*:
    // recipes extract archives and write files, and the compatibility
    // gate reads the game exe. Without a resolved target both are
    // silently skipped, so refuse instead of pretending it worked.
    const target = await resolveInstallTarget(readSelectedAppId())
    if (!target) {
      error.value = copy.value.noGameSelected
      return
    }
    await installCapability({
      capabilityId: entry.id,
      gameId: target.gameId,
      gameName: target.gameName,
      installDir: target.installDir,
      executableDir: target.executableDir,
      config: {},
      acceptUnsigned: acceptUnsigned.value[entry.id] ?? false,
    })
    success.value = formatCopy(copy.value.installed, { name: entry.displayName || entry.id })
    await refreshCapabilities()
  } catch (err) {
    error.value = err instanceof Error ? err.message : String(err)
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
  // Persist it: the AI assistant's install flow reads the same record,
  // so agreeing to an unsigned mod here also allows installing the very
  // same mod from an AI recommendation instead of silently blocking it.
  if (value) recordUnsignedConsent(id)
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
    <BaseDialog
      v-if="open"
      size="lg"
      :title="copy.title"
      :description="copy.subtitle"
      @close="closeDialog"
    >
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

      <!-- One trust fact, not two. The revocation list rides inside the
           signed bytes, so an unverified signature means Moddin has
           neither a usable capability list nor a usable kill switch. -->
      <div v-if="fetchResult" class="community-trust" :class="{ bad: !fetchResult.signatureVerified }">
        <AppIcon :name="fetchResult.signatureVerified ? 'shield' : 'alert'" />
        <span v-if="!fetchResult.signatureVerified">{{ copy.catalogNotVerified }}</span>
        <span v-else>{{ copy.catalogVerified }}</span>
        <small :title="fetchResult.bootstrapPublicKeyFingerprint">
          {{ formatCopy(copy.keyFingerprint, { fingerprint: fetchResult.bootstrapPublicKeyFingerprint }) }}
        </small>
      </div>

      <ErrorCallout :error="friendlyError" />
      <div v-if="success" class="callout callout-info" role="status">{{ success }}</div>

      <EmptyState v-if="loading" busy :description="copy.loading" />
      <EmptyState
        v-else-if="communityEntries.length === 0"
        icon="community"
        :description="copy.empty"
      />

      <div v-else class="community-list">
        <article v-for="entry in communityEntries" :key="entry.id" class="community-entry">
          <div class="community-entry-top">
            <div>
              <strong>{{ entry.displayName || entry.id }}</strong>
              <small>v{{ entry.version }}</small>
            </div>
            <span v-if="revocationFor(entry.id) !== null" class="badge badge-danger">
              {{ copy.revoked }}
            </span>
            <span v-else class="badge" :class="entry.signed ? 'badge-success' : 'badge-warning'">
              {{ entry.signed ? copy.signed : copy.unsigned }}
            </span>
          </div>

          <p v-if="revocationFor(entry.id) !== null" class="callout callout-danger community-revoked">
            {{ formatCopy(copy.revokedReason, { name: entry.displayName || entry.id, reason: revocationFor(entry.id) || copy.revokedNoReason }) }}
          </p>

          <ul v-if="entry.safetyNotes.length" class="note-list community-notes">
            <li v-for="(note, index) in entry.safetyNotes" :key="index">{{ note }}</li>
          </ul>

          <label v-if="!entry.signed && revocationFor(entry.id) === null" class="community-consent">
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
              :disabled="installingId === entry.id || isBlocked(entry) || (!entry.signed && !acceptUnsigned[entry.id])"
              :title="isBlocked(entry) ? copy.revoked : undefined"
              @click="install(entry)"
            >
              {{ installingId === entry.id ? copy.installing : isBlocked(entry) ? copy.unavailable : copy.install }}
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
    </BaseDialog>
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
