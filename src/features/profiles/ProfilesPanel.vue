<script setup lang="ts">
/*
 * Import/export of user profiles.
 *
 * The panel is a sidebar entry like the Community and OpenXR panels: it
 * owns its own dialog, its own copy and its own service calls rather
 * than emitting native command names up into `App.vue`.
 *
 * The rule the whole feature is built around: reading a profile only
 * ever produces a preview. `readImport` cannot apply anything and
 * `applyPreview` refuses to run without one, so there is no code path
 * where a file the user picked changes a game folder before they have
 * been shown what it would do.
 */
import { computed, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import AppIcon from '../../components/ui/AppIcon.vue'
import BaseDialog from '../../components/ui/BaseDialog.vue'
import ChangePreview from '../../components/ui/ChangePreview.vue'
import ConfirmDialog from '../../components/ui/ConfirmDialog.vue'
import EmptyState from '../../components/ui/EmptyState.vue'
import ErrorCallout from '../../components/ui/ErrorCallout.vue'
import { useFriendlyError, type FriendlyError, type FriendlyErrorRule } from '../../composables/useFriendlyError'
import {
  useProfiles,
  type ProfileConfigLookup,
  type ProfileNotice,
} from './useProfiles'
import type { ProfileCapabilityPlan, ProfileGamePlan } from './types'

const props = defineProps<{
  /**
   * Resolved config for the game the library view currently has open.
   * Optional: without it the export carries recipe defaults, which the
   * export note then says out loud.
   */
  configFor?: ProfileConfigLookup
}>()

/** Fired after an import installs something, so History can reload. */
const emit = defineEmits<{ changed: [] }>()

const { t } = useI18n()

const open = ref(false)

const {
  fileName,
  exportResult,
  importPath,
  loadedPath,
  preview,
  applyResult,
  applying,
  busy,
  error,
  errorContext,
  notice,
  canExport,
  canReadImport,
  canApply,
  schemaVersion,
  runExport,
  browseImport,
  readImport,
  applyPreview,
  cancelImport,
  reveal,
  reset,
} = useProfiles({ configFor: props.configFor })

const errorRules = computed<FriendlyErrorRule[]>(() => [
  {
    context: 'export',
    match: /folder|too large|could not write|permission|denied|0x80070005/i,
    title: t('profileErrorExportTitle'),
    why: t('profileErrorExportWhy'),
    showRaw: true,
  },
  {
    context: 'import',
    match: /could not open|too large|not there|folder|read as text/i,
    title: t('profileErrorImportTitle'),
    why: t('profileErrorImportWhy'),
    showRaw: true,
  },
  {
    context: 'apply',
    title: t('profileErrorApplyTitle'),
    why: t('profileErrorApplyWhy'),
    showRaw: true,
  },
  {
    context: 'reveal',
    title: t('profileErrorRevealTitle'),
    why: t('profileErrorRevealWhy'),
    showRaw: true,
  },
  {
    title: t('profileErrorGenericTitle'),
    why: t('profileErrorGenericWhy'),
    showRaw: true,
  },
])

/**
 * A refusal the parse already decided on, phrased in the user's
 * language. It is not a backend failure, so it does not go through
 * `useFriendlyError` — that classifier exists to title a raw command
 * error, and re-titling "this file is from a newer Moddin" as a crash
 * would bury the one thing the user needs to read. It still lands in
 * the shared `ErrorCallout`, and the version is named in the message
 * rather than hidden behind a disclosure.
 */
const noticeError = computed<FriendlyError | null>(() => {
  const current = notice.value
  if (!current) return null
  return { title: '', why: noticeText(current), showRaw: false, raw: '' }
})

const friendlyError = useFriendlyError({
  error,
  rules: () => errorRules.value,
  context: errorContext,
})

const NOTICE_KEYS = {
  'not-json': 'profileImportErrorNotJson',
  'not-profile': 'profileImportErrorNotProfile',
  'future-version': 'profileImportErrorFutureVersion',
  'old-version': 'profileImportErrorOldVersion',
  invalid: 'profileImportErrorInvalid',
  'export-empty': 'profileExportEmpty',
} as const satisfies Record<ProfileNotice['code'], string>

function noticeText(current: ProfileNotice): string {
  const key = NOTICE_KEYS[current.code]
  // The version is named in the message rather than hidden behind the
  // disclosure: "this file is from a newer Moddin" is the whole answer.
  if (current.code === 'future-version') {
    return t(key, { version: current.detail, supported: schemaVersion })
  }
  return t(key)
}

/** The sentences `ChangePreview` renders for a plan. */
const previewChanges = computed(() =>
  (preview.value?.games ?? []).flatMap((game) =>
    game.capabilities
      .filter((capability) => !capability.blocked)
      .map((capability) => changeLine(game, capability)),
  ),
)

const previewWarnings = computed(() => {
  const lines: string[] = []
  for (const game of preview.value?.games ?? []) {
    if (game.capabilities.some((capability) => !capability.blocked)) {
      lines.push(t('profilePreviewAdditive', { game: game.gameName }))
    }
    for (const capability of game.capabilities.filter((item) => item.blocked)) {
      lines.push(
        t('profilePreviewBlocked', {
          game: game.gameName,
          capability: capability.displayName,
          reason: blockedReason(capability),
        }),
      )
    }
    for (const capability of game.capabilities.filter((item) => item.omittedSecrets.length)) {
      lines.push(
        t('profilePreviewOmitted', {
          game: game.gameName,
          capability: capability.displayName,
          fields: capability.omittedSecrets.join(', '),
        }),
      )
    }
  }
  return lines
})

const perGameSummary = computed(() =>
  (preview.value?.games ?? []).map((game) => ({
    game,
    apply: game.capabilities.filter((capability) => !capability.blocked).length,
    blocked: game.capabilities.filter((capability) => capability.blocked).length,
  })),
)

const BLOCK_KEYS = {
  'game-not-installed': 'profileBlockGameNotInstalled',
  'unknown-capability': 'profileBlockUnknownCapability',
  'community-capability': 'profileBlockCommunity',
  revoked: 'profileBlockRevoked',
  'engine-mismatch': 'profileBlockEngineMismatch',
  'engine-unknown': 'profileBlockEngineUnknown',
} as const satisfies Record<ProfileCapabilityPlan['reasons'][number], string>

function blockedReason(capability: ProfileCapabilityPlan): string {
  return capability.reasons
    .map((reason) => t(BLOCK_KEYS[reason]))
    .join(' ')
}

function changeLine(game: ProfileGamePlan, capability: ProfileCapabilityPlan): string {
  return capability.action === 'reinstall'
    ? t('profilePreviewReinstall', { game: game.gameName, capability: capability.displayName })
    : t('profilePreviewInstall', { game: game.gameName, capability: capability.displayName })
}

const pendingApply = ref(false)

function askApply() {
  pendingApply.value = true
}

async function confirmApply() {
  pendingApply.value = false
  const result = await applyPreview(() => emit('changed'))
  if (result && result.failed.length) {
    // The run stopped part-way. Say what landed, because every one of
    // those installs is a transaction the user can undo on its own.
    error.value = result.failed[0].message
    errorContext.value = 'apply'
  }
}

// Reset transient state whenever the dialog closes (button, Esc or
// backdrop), so a half-finished import is not waiting behind the next
// open.
watch(open, (isOpen) => {
  if (!isOpen) reset()
})

function openPanel() {
  open.value = true
}

function closePanel() {
  open.value = false
}
</script>

<template>
  <button class="nav-item" type="button" @click="openPanel">
    <AppIcon name="folder" />
    <span>{{ t('profilePanelTitle') }}</span>
  </button>

  <Teleport to="body">
    <BaseDialog
      v-if="open"
      class="profile-dialog"
      size="lg"
      :title="t('profilePanelTitle')"
      :description="t('profilePanelSubtitle')"
      @close="closePanel"
    >
      <ErrorCallout :error="friendlyError ?? noticeError" />

      <section class="dialog-section">
        <h3>{{ t('profileExportHeading') }}</h3>
        <p>{{ t('profileExportHint') }}</p>

        <!-- The secrets rule, stated where the file is produced. A
             profile is a file that leaves this machine, so the one
             thing a user must not have to guess is what is not in it. -->
        <div class="callout callout-info">
          <AppIcon class="callout-icon" name="shield" />
          <div>
            <strong>{{ t('profileExportSecretsTitle') }}</strong>
            <p>{{ t('profileExportSecretsWhy') }}</p>
          </div>
        </div>

        <div class="profile-row">
          <label class="field">
            <span>{{ t('profileExportFileName') }}</span>
            <input
              v-model="fileName"
              class="input"
              type="text"
              :aria-required="true"
              :placeholder="'moddin-profile-2026-01-31.json'"
            />
          </label>
          <button
            class="btn btn-primary"
            type="button"
            :class="{ 'is-loading': busy }"
            :disabled="!canExport || busy"
            @click="runExport"
          >
            <AppIcon v-if="!busy" name="folder" :size="14" />
            {{ t('profileExportAction') }}
          </button>
        </div>

        <div v-if="exportResult" class="callout callout-info" role="status">
          <strong>{{ t('profileExportDone') }}</strong>
          <p class="path-text">{{ exportResult.path }}</p>
          <button class="btn btn-sm" type="button" :disabled="busy" @click="reveal">
            <AppIcon name="external" :size="14" />
            {{ t('profileExportReveal') }}
          </button>
        </div>
      </section>

      <section class="dialog-section">
        <h3>{{ t('profileImportHeading') }}</h3>
        <p>{{ t('profileImportHint') }}</p>

        <div class="profile-row">
          <label class="field">
            <span>{{ t('profileImportPath') }}</span>
            <input
              v-model="importPath"
              class="input"
              type="text"
              :placeholder="'C:\\Users\\you\\Downloads\\moddin-profile-2026-01-31.json'"
            />
          </label>
          <button
            class="btn"
            type="button"
            :disabled="busy"
            :title="t('profileImportBrowseTitle')"
            @click="browseImport"
          >
            <AppIcon name="folder" :size="14" />
            {{ t('profileImportBrowse') }}
          </button>
          <button
            class="btn"
            type="button"
            :class="{ 'is-loading': busy }"
            :disabled="!canReadImport || busy"
            @click="readImport"
          >
            <AppIcon v-if="!busy" name="search" :size="14" />
            {{ t('profileImportRead') }}
          </button>
        </div>

        <EmptyState
          v-if="!preview"
          icon="folder"
          :title="t('profileImportEmptyTitle')"
          :description="t('profileImportEmptyHint')"
        />

        <!-- Nothing below this block runs before the user presses
             Apply. The preview is the only thing reading a file
             produces. -->
        <template v-else>
          <ChangePreview
            :changes="previewChanges"
            :warnings="previewWarnings"
            :location="loadedPath"
            :empty-text="t('profilePreviewNothing')"
          >
            <ul class="profile-summary">
              <li v-for="row in perGameSummary" :key="row.game.gameId" class="profile-game">
                <strong>{{ row.game.gameName }}</strong>
                <span class="badge" :class="row.game.installed ? 'badge-success' : 'badge-warning'">
                  {{ row.game.installed ? t('profileGameInstalled') : t('profileGameMissing') }}
                </span>
                <small>{{ t('profileGameCounts', { apply: row.apply, blocked: row.blocked }) }}</small>
              </li>
            </ul>
          </ChangePreview>

          <div v-if="applyResult" class="callout callout-info" role="status">
            {{ t('profileApplyResult', { count: applyResult.installed.length }) }}
          </div>
        </template>
      </section>

      <template #footer>
        <button class="btn" type="button" @click="closePanel">{{ t('cancel') }}</button>
        <template v-if="preview">
          <button class="btn" type="button" :disabled="busy" @click="cancelImport">
            {{ t('profileImportDiscard') }}
          </button>
          <button
            class="btn btn-primary"
            type="button"
            :class="{ 'is-loading': applying }"
            :disabled="!canApply"
            @click="askApply"
          >
            {{ applying ? t('profileApplyWorking') : t('profileApplyAction', { count: preview.applyCount }) }}
          </button>
        </template>
      </template>
    </BaseDialog>
  </Teleport>

  <ConfirmDialog
    v-if="pendingApply"
    :title="t('profileApplyConfirmTitle')"
    :description="t('profileApplyConfirmDescription', { count: preview?.applyCount ?? 0 })"
    :confirm-label="t('profileApplyAction', { count: preview?.applyCount ?? 0 })"
    :cancel-label="t('cancel')"
    tone="default"
    :details="previewChanges"
    :footnote="t('profileApplyConfirmFootnote')"
    @close="pendingApply = false"
    @confirm="confirmApply"
  />
</template>

<style scoped>
.profile-row {
  display: grid;
  grid-template-columns: minmax(0, 1fr) auto;
  align-items: end;
  gap: var(--moddin-space-3);
}

.profile-row .btn { white-space: nowrap; }

.callout .path-text { margin: var(--moddin-space-2) 0; }

.profile-summary { display: grid; gap: var(--moddin-space-2); margin: 0; padding: 0; list-style: none; }
.profile-game {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: var(--moddin-space-2);
  padding: var(--moddin-space-2) 0;
}
.profile-game + .profile-game { border-top: 1px solid var(--moddin-line-soft); }
.profile-game small { color: var(--moddin-text-faint); font-size: var(--moddin-text-xs); }
</style>
