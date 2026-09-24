<script setup lang="ts">
import { useI18n } from 'vue-i18n'
import AppIcon from '../../components/ui/AppIcon.vue'
import type { InstalledGame } from '../../types/game'

defineProps<{
  games: InstalledGame[]
  selectedAppId: string | null
  loading: boolean
  totalCount: number
  supportedCount: number
  modCount: (game: InstalledGame) => number
  displayName: (game: InstalledGame) => string
}>()

const search = defineModel<string>('search', { required: true })
const supportedOnly = defineModel<boolean>('supportedOnly', { required: true })
const emit = defineEmits<{ select: [appId: string]; rescan: [] }>()
const { t } = useI18n()
</script>

<template>
  <aside class="panel game-list" :aria-label="t('libraryTitle')">
    <div class="game-list-header">
      <label class="search-field">
        <AppIcon name="search" />
        <span class="sr-only">{{ t('searchPlaceholder') }}</span>
        <input v-model="search" class="input" type="search" :placeholder="t('searchPlaceholder')" />
      </label>
      <div class="game-list-toolbar">
        <div class="segmented" role="group" :aria-label="t('libraryTitle')">
          <button type="button" :aria-pressed="supportedOnly" @click="supportedOnly = true">
            {{ t('filterWithMods') }} <span class="count">{{ supportedCount }}</span>
          </button>
          <button type="button" :aria-pressed="!supportedOnly" @click="supportedOnly = false">
            {{ t('filterAll') }} <span class="count">{{ totalCount }}</span>
          </button>
        </div>
        <button
          class="btn btn-ghost btn-icon rescan-button"
          :class="{ 'is-loading': loading }"
          type="button"
          :disabled="loading"
          :title="loading ? t('scanning') : t('rescanLibraries')"
          :aria-label="loading ? t('scanning') : t('rescanLibraries')"
          @click="emit('rescan')"
        >
          <AppIcon v-if="!loading" name="refresh" />
        </button>
      </div>
    </div>

    <div class="game-list-body">
      <div v-if="loading && !games.length" class="empty-state">
        <span class="spinner" />
        <span>{{ t('scanningLibraries') }}</span>
      </div>
      <div v-else-if="!games.length" class="empty-state">
        <strong>{{ t('noGamesFound') }}</strong>
        <span>{{ t('noGamesHint') }}</span>
      </div>

      <button
        v-for="game in games"
        :key="game.appId"
        type="button"
        class="game-row"
        :aria-current="selectedAppId === game.appId ? 'true' : undefined"
        @click="emit('select', game.appId)"
      >
        <span class="game-avatar" :class="{ muted: !modCount(game) }">{{ displayName(game).slice(0, 1).toUpperCase() }}</span>
        <span class="game-row-copy">
          <strong>{{ displayName(game) }}</strong>
          <small v-if="modCount(game)">{{ t('gameModCount', { count: modCount(game) }, modCount(game)) }}</small>
          <small v-else class="text-faint">{{ t('gameNoMods') }}</small>
        </span>
      </button>
    </div>
  </aside>
</template>

<style scoped>
.game-list { display: flex; flex-direction: column; overflow: hidden; }

.game-list-header {
  display: grid;
  gap: var(--moddin-space-3);
  border-bottom: 1px solid var(--moddin-line-soft);
  padding: var(--moddin-space-3);
}

.search-field { position: relative; display: block; }
.search-field .app-icon { position: absolute; top: 50%; left: 11px; color: var(--moddin-text-faint); transform: translateY(-50%); pointer-events: none; }
.search-field .input { padding-left: 34px; }

.game-list-toolbar { display: flex; align-items: center; justify-content: space-between; gap: var(--moddin-space-2); }
.game-list-toolbar .segmented { flex-wrap: nowrap; }
.game-list-toolbar .segmented button { white-space: nowrap; }
.rescan-button { flex: 0 0 auto; }
.rescan-button.is-loading::before { margin: 0; }

.game-list-body { flex: 1; min-height: 0; overflow-y: auto; padding: var(--moddin-space-2); }

.game-row {
  display: flex;
  align-items: center;
  gap: var(--moddin-space-3);
  width: 100%;
  border: 1px solid transparent;
  border-radius: var(--moddin-radius-md);
  padding: var(--moddin-space-2);
  background: transparent;
  text-align: left;
  cursor: pointer;
  transition: background-color var(--moddin-fast) var(--moddin-ease), border-color var(--moddin-fast) var(--moddin-ease);
}
.game-row + .game-row { margin-top: 2px; }
.game-row:hover { background: var(--moddin-surface-2); }
.game-row[aria-current='true'] { border-color: var(--moddin-accent-ring); background: var(--moddin-accent-soft); }

.game-avatar {
  display: grid;
  place-items: center;
  width: 36px;
  height: 36px;
  flex: 0 0 auto;
  border-radius: var(--moddin-radius-md);
  color: var(--moddin-accent-text);
  background: var(--moddin-surface-3);
  font-weight: 750;
}
.game-avatar.muted { color: var(--moddin-text-faint); }

.game-row-copy { display: grid; min-width: 0; gap: 1px; }
.game-row-copy strong { overflow: hidden; color: var(--moddin-text); font-size: var(--moddin-text-md); font-weight: 600; text-overflow: ellipsis; white-space: nowrap; }
.game-row-copy small { color: var(--moddin-accent-text); font-size: var(--moddin-text-xs); }
.game-row-copy small.text-faint { color: var(--moddin-text-faint); }
</style>
