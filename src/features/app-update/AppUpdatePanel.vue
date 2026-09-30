<script setup lang="ts">
import { computed, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import AppIcon from '../../components/ui/AppIcon.vue'
import BaseDialog from '../../components/ui/BaseDialog.vue'
import ConfirmDialog from '../../components/ui/ConfirmDialog.vue'
import ErrorCallout from '../../components/ui/ErrorCallout.vue'
import EmptyState from '../../components/ui/EmptyState.vue'
import { useFriendlyError, type FriendlyErrorRule } from '../../composables/useFriendlyError'
import { dateLocaleFor } from '../../i18n/locale'
import { appUpdateCopyForLocale, formatAppUpdateCopy } from './copy'
import { useAppUpdate } from './useAppUpdate'

/**
 * The app self-updater, as one sidebar entry (ROADMAP `F-04`).
 *
 * The panel is the whole feature: open it, ask, read, confirm. Nothing
 * checks at startup, nothing polls, and a version the user turns down is
 * remembered so the next manual check reports it as declined instead of
 * asking again.
 *
 * Nothing about the update is decided here. Which releases exist, which
 * of them is newer, and whether one is a preview are the backend's
 * answers (`src-tauri/src/app_update.rs`); this component renders them and
 * passes a version back.
 */
const { locale } = useI18n()
const copy = computed(() => appUpdateCopyForLocale(locale.value))

const {
  phase,
  error,
  errorContext,
  status,
  offeredVersion,
  notes,
  publishedAt,
  detail,
  declinedVersion,
  percent,
  percentLabel,
  load,
  check,
  decline,
  showDeclinedAgain,
  install,
} = useAppUpdate()

const open = ref(false)
const confirming = ref(false)

const errorRules = computed<FriendlyErrorRule[]>(() => {
  const c = copy.value
  return [
    {
      match: /no update signing key/i,
      context: 'install',
      title: c.notConfiguredTitle,
      why: c.notConfiguredDescription,
    },
    {
      match: /not signed|signature|not verified/i,
      context: 'install',
      title: c.errorInstallTitle,
      why: c.errorInstallWhy,
      showRaw: true,
    },
    { context: 'install', title: c.errorInstallTitle, why: c.errorInstallWhy, showRaw: true },
    { context: 'check', title: c.errorCheckTitle, why: c.errorCheckWhy, showRaw: true },
  ]
})

const friendlyError = useFriendlyError({
  error,
  rules: () => errorRules.value,
  context: errorContext,
})

const installing = computed(() => phase.value === 'installing' || phase.value === 'finishing')
const confirmText = (template: string, version: string | null) =>
  formatAppUpdateCopy(template, { version: version ?? '' })

/** The release date, in the reader's locale. RFC 3339 from the manifest. */
const published = computed(() => {
  if (!publishedAt.value) return null
  const date = new Date(publishedAt.value)
  if (Number.isNaN(date.getTime())) return null
  return formatAppUpdateCopy(copy.value.released, {
    date: new Intl.DateTimeFormat(dateLocaleFor(locale.value), { dateStyle: 'long' }).format(
      date,
    ),
  })
})

/**
 * The status of this build is read when the panel opens, not before:
 * asking at startup is the background check this feature does not do.
 */
async function openPanel() {
  open.value = true
  await load()
}

function closePanel() {
  open.value = false
}

function confirmInstall() {
  confirming.value = false
  void install()
}

/** "Not now" is a decision, not a dismissal: it is remembered. */
function declineInstall() {
  confirming.value = false
  decline()
}
</script>

<template>
  <button class="nav-item" type="button" @click="openPanel">
    <AppIcon name="arrow-up" />
    <span>{{ copy.button }}</span>
  </button>

  <Teleport to="body">
    <BaseDialog
      v-if="open"
      size="lg"
      :title="copy.title"
      :description="copy.subtitle"
      @close="closePanel"
    >
      <ErrorCallout :error="friendlyError" />

      <dl class="update-meta">
        <div>
          <dt>{{ copy.installedVersion }}</dt>
          <dd>{{ status?.currentVersion ?? '—' }}</dd>
        </div>
        <div v-if="status">
          <dt>{{ copy.channelLabel }}</dt>
          <dd>{{ status.channel }}</dd>
        </div>
      </dl>

      <EmptyState v-if="phase === 'checking'" busy :description="copy.checking" />

      <EmptyState
        v-else-if="phase === 'up-to-date'"
        icon="check"
        :title="copy.upToDateTitle"
        :description="copy.upToDateDescription"
      />

      <div v-else-if="phase === 'not-configured'" class="callout callout-warning">
        <strong>{{ copy.notConfiguredTitle }}</strong>
        <p>{{ copy.notConfiguredDescription }}</p>
      </div>

      <div v-else-if="phase === 'unavailable'" class="callout callout-info">
        <strong>{{ copy.unavailableTitle }}</strong>
        <p v-if="detail">{{ detail }}</p>
      </div>

      <div v-else-if="phase === 'declined'" class="update-declined">
        <p class="callout callout-info">
          {{ confirmText(copy.declinedNotice, offeredVersion ?? declinedVersion) }}
        </p>
        <button type="button" class="btn btn-ghost btn-sm" @click="showDeclinedAgain">
          {{ copy.declinedAgain }}
        </button>
      </div>

      <div v-else-if="phase === 'available' || installing" class="update-offer">
        <h3 class="update-version">
          {{ offeredVersion }}
          <span class="badge badge-info">{{ status?.channel }}</span>
        </h3>

        <p v-if="published" class="update-released">{{ published }}</p>
        <p v-if="notes" class="update-notes">{{ notes }}</p>

        <template v-if="phase === 'available'">
          <p class="update-signed">
            <AppIcon name="shield" :size="14" />
            {{ copy.confirmDetailDownload }}
          </p>
          <button type="button" class="btn btn-primary" @click="confirming = true">
            {{ copy.install }}
          </button>
        </template>

        <div v-else class="update-progress">
          <progress
            v-if="percent !== null"
            class="update-bar"
            :value="percent"
            max="100"
            aria-hidden="true"
          />
          <p role="status">
            {{
              phase === 'finishing'
                ? copy.finishing
                : percentLabel
                  ? formatAppUpdateCopy(copy.downloading, { percent: percentLabel })
                  : copy.verifying
            }}
          </p>
        </div>
      </div>

      <template #footer>
        <span v-if="phase === 'available'" class="update-hint">{{ copy.confirmDetailRelaunch }}</span>
        <button
          type="button"
          class="btn btn-ghost"
          :disabled="installing"
          @click="closePanel"
        >
          {{ copy.close }}
        </button>
        <button
          type="button"
          class="btn btn-primary"
          :class="{ 'is-loading': phase === 'checking' }"
          :disabled="installing || phase === 'checking'"
          @click="check"
        >
          {{ phase === 'checking' ? copy.checking : copy.check }}
        </button>
      </template>
    </BaseDialog>

    <ConfirmDialog
      v-if="confirming"
      tone="default"
      :title="confirmText(copy.confirmTitle, offeredVersion)"
      :description="confirmText(copy.confirmDescription, offeredVersion)"
      :details="[copy.confirmDetailDownload, copy.confirmDetailRelaunch]"
      :confirm-label="copy.install"
      :cancel-label="copy.installCancel"
      @close="declineInstall"
      @confirm="confirmInstall"
    />
  </Teleport>
</template>

<style scoped>
.update-meta {
  display: grid;
  grid-template-columns: max-content 1fr;
  column-gap: var(--moddin-space-4);
  row-gap: var(--moddin-space-1);
  margin: 0 0 var(--moddin-space-4);
  font-size: var(--moddin-text-sm);
}

.update-meta dt {
  color: var(--moddin-text-muted);
}

.update-meta dd {
  margin: 0;
}

.update-offer {
  display: grid;
  gap: var(--moddin-space-3);
  justify-items: start;
}

.update-version {
  display: flex;
  align-items: center;
  gap: var(--moddin-space-2);
  margin: 0;
  font-size: var(--moddin-text-lg);
}

.update-released {
  margin: 0;
  color: var(--moddin-text-muted);
  font-size: var(--moddin-text-xs);
}

.update-notes {
  margin: 0;
  white-space: pre-line;
  color: var(--moddin-text-muted);
  font-size: var(--moddin-text-sm);
  max-height: 12rem;
  overflow-y: auto;
}

.update-signed {
  display: flex;
  align-items: flex-start;
  gap: var(--moddin-space-2);
  margin: 0;
  color: var(--moddin-text-muted);
  font-size: var(--moddin-text-sm);
}

.update-progress {
  display: grid;
  gap: var(--moddin-space-2);
  width: 100%;
  font-size: var(--moddin-text-sm);
  color: var(--moddin-text-muted);
}

.update-declined {
  display: grid;
  gap: var(--moddin-space-2);
  justify-items: start;
}
.update-bar {
  width: 100%;
  height: 6px;
}

.update-hint {
  margin-right: auto;
  align-self: center;
  color: var(--moddin-text-muted);
  font-size: var(--moddin-text-sm);
  max-width: 26rem;
}
</style>
