<script setup lang="ts">
import { computed, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import AppIcon from '../../components/ui/AppIcon.vue'
import EmptyState from '../../components/ui/EmptyState.vue'
import type { InstalledGame } from '../../types/game'
import { formatLibraryCopy, libraryCopyForLocale } from './copy'

const props = defineProps<{
  games: InstalledGame[]
  selectedAppId: string | null
  loading: boolean
  selectionBusy?: boolean
  totalCount: number
  supportedCount: number
  modCount: (game: InstalledGame) => number
  displayName: (game: InstalledGame) => string
}>()

const search = defineModel<string>('search', { required: true })
const supportedOnly = defineModel<boolean>('supportedOnly', { required: true })
const emit = defineEmits<{ select: [appId: string]; rescan: [] }>()
const { t, locale } = useI18n()
const copy = computed(() => libraryCopyForLocale(locale.value))
const searchId = 'library-search'
const searchInput = ref<HTMLInputElement | null>(null)
const selectionLockId = 'library-selection-lock'
const rescanLabel = computed(() => props.selectionBusy ? copy.value.selectionBusyHint : props.loading ? t('scanning') : t('rescanLibraries'))
const hasSearch = computed(() => Boolean(search.value.trim()))
const emptyKind = computed(() => {
  if (props.totalCount === 0) return 'library'
  if (supportedOnly.value && props.supportedCount === 0) return 'supported'
  return hasSearch.value ? 'search' : 'supported'
})
const emptyTitle = computed(() => {
  if (emptyKind.value === 'library') return copy.value.emptyLibraryTitle
  if (emptyKind.value === 'supported') return copy.value.emptySupportedTitle
  return copy.value.emptySearchTitle
})
const emptyHint = computed(() => {
  if (emptyKind.value === 'library') return copy.value.emptyLibraryHint
  if (emptyKind.value === 'supported') return copy.value.emptySupportedHint
  return supportedOnly.value ? copy.value.emptySupportedSearchHint : copy.value.emptySearchHint
})

function clearSearch() {
  search.value = ''
  searchInput.value?.focus()
}

function resetFilters() {
  supportedOnly.value = false
  clearSearch()
}

function storeLabel(store: InstalledGame['store']) {
  return t(store === 'epic' ? 'storeEpic' : store === 'gog' ? 'storeGog' : 'storeSteam')
}

/** Arrow keys move focus; native Enter/Space still select the focused game. */
function navigateGames(event: KeyboardEvent, index: number) {
  if (props.selectionBusy) return
  if (!['ArrowDown', 'ArrowUp', 'Home', 'End'].includes(event.key)) return
  event.preventDefault()
  const buttons = (event.currentTarget as HTMLElement).parentElement?.querySelectorAll<HTMLButtonElement>('.game-row')
  if (!buttons?.length) return
  const next = event.key === 'Home' ? 0
    : event.key === 'End' ? buttons.length - 1
      : Math.max(0, Math.min(buttons.length - 1, index + (event.key === 'ArrowDown' ? 1 : -1)))
  buttons[next]?.focus()
}
</script>

<template>
  <aside class="panel game-list" :aria-label="t('libraryTitle')">
    <div class="game-list-header">
      <div class="library-heading">
        <div>
          <h2>{{ copy.title }}</h2>
          <p>{{ copy.subtitle }}</p>
        </div>
        <button
          class="btn btn-ghost btn-icon rescan-button"
          :class="{ 'is-loading': loading }"
          type="button"
          :disabled="loading || selectionBusy"
          :title="rescanLabel"
          :aria-label="rescanLabel"
          :aria-describedby="selectionBusy ? selectionLockId : undefined"
          @click="emit('rescan')"
        >
          <AppIcon v-if="!loading" name="refresh" />
        </button>
      </div>
      <div class="search-field">
        <AppIcon name="search" />
        <label class="sr-only" :for="searchId">{{ t('searchPlaceholder') }}</label>
        <input
          :id="searchId"
          ref="searchInput"
          v-model="search"
          class="input"
          type="search"
          :placeholder="t('searchPlaceholder')"
          @keydown.esc.prevent="clearSearch"
        />
        <button v-if="search" class="clear-search" type="button" :aria-label="copy.clearSearch" :title="copy.clearSearch" @click="clearSearch">
          <AppIcon name="close" :size="14" />
        </button>
      </div>
      <div class="game-list-toolbar">
        <div class="segmented" role="group" :aria-label="t('libraryTitle')">
          <button type="button" :aria-pressed="supportedOnly" @click="supportedOnly = true">
            {{ t('filterWithMods') }} <span class="count">{{ supportedCount }}</span>
          </button>
          <button type="button" :aria-pressed="!supportedOnly" @click="supportedOnly = false">
            {{ t('filterAll') }} <span class="count">{{ totalCount }}</span>
          </button>
        </div>
      </div>
      <div class="library-results" role="status" aria-live="polite" aria-atomic="true">
        <span>{{ loading ? t('scanning') : formatLibraryCopy(copy.resultCount, { shown: games.length, total: totalCount }) }}</span>
        <button v-if="hasSearch && games.length" class="reset-search" type="button" @click="clearSearch">{{ copy.clearSearch }}</button>
      </div>
    </div>

    <div class="game-list-body" :aria-busy="loading">
      <EmptyState
        v-if="loading && !games.length"
        busy
        :description="copy.scanningLibraries"
      />
      <EmptyState
        v-else-if="!games.length"
        :icon="emptyKind === 'search' ? 'search' : 'gamepad'"
        :title="emptyTitle"
        :description="emptyHint"
      >
        <div class="empty-actions">
          <button v-if="emptyKind === 'library'" class="btn btn-primary btn-sm" type="button" :disabled="selectionBusy" :aria-describedby="selectionBusy ? selectionLockId : undefined" @click="emit('rescan')">
            <AppIcon name="refresh" :size="14" />
            {{ t('rescanLibraries') }}
          </button>
          <button v-else-if="hasSearch" class="btn btn-primary btn-sm" type="button" @click="clearSearch">{{ copy.clearSearch }}</button>
          <button v-if="emptyKind !== 'library' && supportedOnly" class="btn btn-sm" type="button" @click="resetFilters">{{ copy.resetFilters }}</button>
        </div>
      </EmptyState>

      <button
        v-for="(game, index) in games"
        :key="game.appId"
        type="button"
        class="game-row"
        :aria-current="selectedAppId === game.appId ? 'true' : undefined"
        :disabled="selectionBusy"
        :aria-describedby="selectionBusy ? selectionLockId : undefined"
        :title="displayName(game)"
        @click="emit('select', game.appId)"
        @keydown="navigateGames($event, index)"
      >
        <span class="game-avatar" :class="{ muted: !modCount(game) }">{{ displayName(game).slice(0, 1).toUpperCase() }}</span>
        <span class="game-row-copy">
          <strong>{{ displayName(game) }}</strong>
          <span class="game-row-meta">
            <small class="game-store">{{ storeLabel(game.store) }}</small>
            <span aria-hidden="true">·</span>
            <small v-if="modCount(game)" class="game-mods">{{ t('gameModCount', { count: modCount(game) }, modCount(game)) }}</small>
            <small v-else>{{ copy.noCompatibleMods }}</small>
          </span>
        </span>
        <AppIcon v-if="selectedAppId === game.appId" class="game-selected" name="check" :size="15" />
      </button>
    </div>
    <p v-if="selectionBusy" :id="selectionLockId" class="game-list-footnote is-busy" role="status">{{ copy.selectionBusyHint }}</p>
    <p v-else-if="games.length" class="game-list-footnote">{{ copy.keyboardHint }}</p>
  </aside>
</template>

<style scoped>
.game-list { display: flex; flex-direction: column; overflow: hidden; }

.game-list-header {
  display: grid;
  gap: var(--moddin-space-3);
  border-bottom: 1px solid var(--moddin-line-soft);
  padding: var(--moddin-space-4);
}

.library-heading { display: flex; align-items: flex-start; justify-content: space-between; gap: var(--moddin-space-2); }
.library-heading h2 { font-size: var(--moddin-text-lg); }
.library-heading p { margin-top: var(--moddin-space-1); color: var(--moddin-text-muted); font-size: var(--moddin-text-sm); }
.search-field { position: relative; display: block; }
.search-field > .app-icon { position: absolute; top: 50%; left: 11px; color: var(--moddin-text-muted); transform: translateY(-50%); pointer-events: none; }
.search-field .input { padding-left: 34px; padding-right: 36px; }
.search-field .input::-webkit-search-cancel-button { -webkit-appearance: none; }
.clear-search { position: absolute; top: 50%; right: 6px; display: grid; place-items: center; width: 28px; height: 28px; padding: 0; border: 0; border-radius: var(--moddin-radius-sm); color: var(--moddin-text-muted); background: transparent; transform: translateY(-50%); cursor: pointer; }
.clear-search:hover { color: var(--moddin-text); background: var(--moddin-surface-3); }

.game-list-toolbar { display: flex; align-items: center; justify-content: space-between; gap: var(--moddin-space-2); }
.game-list-toolbar .segmented { flex: 1; flex-wrap: wrap; }
.game-list-toolbar .segmented button { flex: 1 1 auto; white-space: nowrap; }
.rescan-button { flex: 0 0 auto; }
.rescan-button.is-loading::before { margin: 0; }
.library-results { display: flex; align-items: center; justify-content: space-between; gap: var(--moddin-space-2); min-height: 18px; color: var(--moddin-text-muted); font-size: var(--moddin-text-sm); }
.reset-search { padding: 0; border: 0; color: var(--moddin-accent-text); background: transparent; font-size: var(--moddin-text-xs); cursor: pointer; }
.reset-search:hover { text-decoration: underline; }
.empty-actions { display: flex; flex-wrap: wrap; justify-content: center; gap: var(--moddin-space-2); margin-top: var(--moddin-space-2); }

.game-list-body { flex: 1; min-height: 0; overflow-y: auto; padding: var(--moddin-space-2); }
.game-list-body :deep(.empty-state) { padding: var(--moddin-space-6) var(--moddin-space-3); }

.game-row {
  display: flex;
  align-items: center;
  gap: var(--moddin-space-3);
  width: 100%;
  border: 1px solid transparent;
  border-radius: var(--moddin-radius-md);
  min-height: 72px;
  padding: var(--moddin-space-3);
  background: transparent;
  text-align: left;
  cursor: pointer;
  transition: background-color var(--moddin-fast) var(--moddin-ease), border-color var(--moddin-fast) var(--moddin-ease);
}
.game-row + .game-row { margin-top: 2px; }
.game-row:hover:not(:disabled) { background: var(--moddin-surface-2); }
.game-row:disabled { cursor: wait; }
.game-row[aria-current='true'] { border-color: var(--moddin-accent-ring); background: var(--moddin-accent-soft); box-shadow: inset 3px 0 0 var(--moddin-accent); }

.game-avatar {
  display: grid;
  place-items: center;
  width: 40px;
  height: 40px;
  flex: 0 0 auto;
  border-radius: var(--moddin-radius-md);
  color: var(--moddin-accent-text);
  background: var(--moddin-surface-3);
  font-weight: 750;
}
.game-avatar.muted { color: var(--moddin-text-muted); }

.game-row-copy { display: grid; flex: 1; min-width: 0; gap: 4px; }
.game-row-copy strong { overflow: hidden; color: var(--moddin-text); font-size: var(--moddin-text-md); font-weight: 600; text-overflow: ellipsis; white-space: nowrap; }
.game-row-meta { display: flex; flex-wrap: wrap; align-items: baseline; gap: 5px; color: var(--moddin-text-muted); font-size: var(--moddin-text-xs); line-height: 1.45; }
.game-row-meta small { font-size: inherit; }
.game-store { color: var(--moddin-text-soft); }
.game-mods { color: var(--moddin-accent-text); }
.game-selected { color: var(--moddin-accent-text); }
.game-list-footnote { border-top: 1px solid var(--moddin-line-soft); padding: var(--moddin-space-3) var(--moddin-space-4); color: var(--moddin-text-muted); font-size: var(--moddin-text-xs); }
.game-list-footnote.is-busy { color: var(--moddin-warning); background: var(--moddin-warning-bg); }
</style>
