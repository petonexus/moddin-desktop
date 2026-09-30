<script setup lang="ts">
import AppIcon, { type IconName } from './AppIcon.vue'

/**
 * One shape for "nothing here yet" and "still working".
 *
 * Every panel had grown its own version of this box, and they drifted:
 * some had a spinner, some an icon, some neither, and none announced
 * themselves. A busy state is the same box with the message withheld,
 * so the caller only says which one it is.
 */
withDefaults(
  defineProps<{
    /** Spinner instead of a message. */
    busy?: boolean
    icon?: IconName
    title?: string
    description?: string
  }>(),
  { busy: false, icon: undefined, title: '', description: '' },
)
</script>

<template>
  <div class="empty-state" role="status" :aria-busy="busy || undefined">
    <span v-if="busy" class="spinner" />
    <AppIcon v-else-if="icon" :name="icon" :size="28" />
    <strong v-if="title">{{ title }}</strong>
    <span v-if="description">{{ description }}</span>
    <slot />
  </div>
</template>
