<script setup lang="ts">
import { computed, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import AppIcon from '../../components/ui/AppIcon.vue'
import BaseDialog from '../../components/ui/BaseDialog.vue'
import ConfirmDialog from '../../components/ui/ConfirmDialog.vue'
import ErrorCallout from '../../components/ui/ErrorCallout.vue'
import EmptyState from '../../components/ui/EmptyState.vue'
import { useFriendlyError, type FriendlyErrorRule } from '../../composables/useFriendlyError'
import { useBackendText } from '../../composables/useBackendText'
import { resolveResourceDir, detectAgents, setupAgent, removeAgent } from './service'
import type { AiAgent } from './types'
import { localAiCopyForLocale, formatLocalAiCopy } from './copy'

const { locale } = useI18n()
const copy = computed(() => localAiCopyForLocale(locale.value))
const { text } = useBackendText()

/** UX-26: one "Connect" per agent, so the name carries the agent. */
function agentActionLabel(action: string, name: string): string {
  return formatLocalAiCopy(copy.value.actionNamed, { action, name })
}

const open = ref(false)
const loading = ref(false)
const error = ref<string | null>(null)
/** Which action produced `error`; the raw string alone cannot say. */
const errorContext = ref<string | null>(null)
const busyAgent = ref<string | null>(null)
const pendingDisconnect = ref<AiAgent | null>(null)
const resourceDir = ref<string>('')
const agents = ref<AiAgent[]>([])

const errorRules = computed<FriendlyErrorRule[]>(() => {
  const c = copy.value
  return [
    { match: /denied|permission|access|0x80070005|read-only/i, title: c.errorPermissionTitle, why: c.errorPermissionWhy, showRaw: true },
    { match: /not found|ENOENT|no such file|cannot find/i, context: 'detect', title: c.errorResourceTitle, why: c.errorResourceWhy, showRaw: true },
    { context: 'detect', title: c.errorDetectTitle, why: c.errorDetectWhy, showRaw: true },
    { context: 'write', title: c.errorWriteTitle, why: c.errorWriteWhy, showRaw: true },
  ]
})

const friendlyError = useFriendlyError({ error, rules: () => errorRules.value, context: errorContext })

// Focus, Escape and focus restore are BaseDialog's job; these only say
// whether the dialog is on screen.
function openDialog() {
  open.value = true
}

function closeDialog() {
  open.value = false
}

const installedAgents = computed(() => agents.value.filter((a) => a.binaryPath))
const anyConnected = computed(() => agents.value.some((a) => a.state === 'configured'))

function badgeClass(state: AiAgent['state']): string {
  switch (state) {
    case 'configured':
      return 'badge badge-success'
    case 'detectedNotConfigured':
      return 'badge badge-info'
    case 'configError':
      return 'badge badge-danger'
    default:
      return 'badge badge-plain'
  }
}

function badgeLabel(state: AiAgent['state']): string {
  switch (state) {
    case 'configured':
      return copy.value.configured
    case 'detectedNotConfigured':
      return copy.value.detectedNotConfigured
    case 'configError':
      return copy.value.configError
    default:
      return copy.value.notInstalled
  }
}

async function refresh() {
  loading.value = true
  error.value = null
  errorContext.value = 'detect'
  try {
    if (!resourceDir.value) resourceDir.value = await resolveResourceDir()
    agents.value = await detectAgents(resourceDir.value)
  } catch (err) {
    error.value = err instanceof Error ? err.message : String(err)
  } finally {
    loading.value = false
  }
}

// Detection only runs when the panel opens — never at app startup —
// so launching Moddin stays cheap for users who never touch this.
async function openPanel() {
  await openDialog()
  await refresh()
}

async function connect(agent: AiAgent) {
  busyAgent.value = agent.id
  error.value = null
  errorContext.value = 'write'
  try {
    const result = await setupAgent(agent.id, resourceDir.value)
    agents.value = agents.value.map((a) => (a.id === agent.id ? result : a))
  } catch (err) {
    error.value = err instanceof Error ? err.message : String(err)
  } finally {
    busyAgent.value = null
  }
}

async function disconnect(agent: AiAgent) {
  // Disconnecting rewrites the user's editor config to stop pointing at
  // the Moddin server. That is reversible, but it edits a file the user
  // also edits by hand, so it is worth one question.
  pendingDisconnect.value = agent
}

async function confirmDisconnect() {
  const agent = pendingDisconnect.value
  if (!agent) return
  pendingDisconnect.value = null
  busyAgent.value = agent.id
  error.value = null
  errorContext.value = 'write'
  try {
    const result = await removeAgent(agent.id, resourceDir.value)
    agents.value = agents.value.map((a) => (a.id === agent.id ? result : a))
  } catch (err) {
    error.value = err instanceof Error ? err.message : String(err)
  } finally {
    busyAgent.value = null
  }
}
</script>

<template>
  <button class="nav-item" type="button" @click="openPanel">
    <AppIcon name="terminal" />
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
      <p class="callout callout-info">{{ copy.banner }}</p>

      <div class="agent-toolbar">
        <button
          type="button"
          class="btn btn-ghost btn-sm"
          :class="{ 'is-loading': loading }"
          :disabled="loading"
          @click="refresh"
        >
          <AppIcon v-if="!loading" name="refresh" :size="14" />
          {{ copy.refresh }}
        </button>
      </div>

      <EmptyState v-if="loading" busy :description="copy.detecting" />
      <ErrorCallout :error="friendlyError" />

      <ul class="agent-list">
        <li
          v-for="agent in agents"
          :key="agent.id"
          class="agent-card"
          :aria-busy="busyAgent === agent.id"
        >
          <header>
            <h3>{{ agent.displayName }}</h3>
            <span :class="badgeClass(agent.state)">{{ badgeLabel(agent.state) }}</span>
          </header>

          <dl class="agent-meta">
            <div v-if="agent.binaryPath">
              <dt>{{ copy.binaryPath }}</dt>
              <dd>
                <code>{{ agent.binaryPath }}</code>
              </dd>
            </div>
            <div v-if="agent.configPath">
              <dt>{{ copy.configPath }}</dt>
              <dd>
                <code>{{ agent.configPath }}</code>
              </dd>
            </div>
            <div v-if="resourceDir">
              <dt>{{ copy.resourcePath }}</dt>
              <dd>
                <code>{{ resourceDir }}</code>
              </dd>
            </div>
          </dl>

          <p v-if="agent.detail" class="agent-detail">{{ text(agent.detail) }}</p>

          <footer>
            <button
              v-if="agent.state === 'detectedNotConfigured'"
              type="button"
              class="btn btn-primary btn-sm"
              :class="{ 'is-loading': busyAgent === agent.id }"
              :disabled="busyAgent === agent.id"
              :aria-label="agentActionLabel(copy.connect, agent.displayName)"
              @click="connect(agent)"
            >
              {{ copy.connect }}
            </button>
            <button
              v-else-if="agent.state === 'configured'"
              type="button"
              class="btn btn-ghost btn-sm"
              :disabled="busyAgent === agent.id"
              :aria-label="agentActionLabel(copy.disconnect, agent.displayName)"
              @click="disconnect(agent)"
            >
              {{ copy.disconnect }}
            </button>
          </footer>
        </li>
      </ul>

      <p v-if="!loading && installedAgents.length === 0" class="agent-hint">
        {{ copy.installCursorHint }}
      </p>

      <template #footer>
        <span v-if="anyConnected" class="connected-flag">
          <AppIcon name="check" :size="14" />
          {{ copy.configured }}
        </span>
        <button type="button" class="btn btn-ghost" @click="closeDialog">{{ copy.close }}</button>
      </template>
    </BaseDialog>

    <ConfirmDialog
      v-if="pendingDisconnect"
      :title="copy.disconnectConfirmTitle"
      :description="formatLocalAiCopy(copy.disconnectConfirmDescription, { name: pendingDisconnect.displayName })"
      :confirm-label="copy.disconnect"
      :cancel-label="copy.disconnectCancel"
      :details="[copy.disconnectConfirmDetail]"
      @close="pendingDisconnect = null"
      @confirm="confirmDisconnect"
    />
  </Teleport>
</template>

<style scoped>
.agent-toolbar {
  display: flex;
  justify-content: flex-end;
}

.agent-list {
  list-style: none;
  padding: 0;
  margin: 0;
  display: grid;
  gap: var(--moddin-space-3);
}

.agent-card {
  display: grid;
  gap: var(--moddin-space-2);
  border: 1px solid var(--moddin-line-soft);
  border-radius: var(--moddin-radius-md);
  padding: var(--moddin-space-3) var(--moddin-space-4);
  background: var(--moddin-surface-2);
}

.agent-card header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--moddin-space-3);
}

.agent-card h3 {
  margin: 0;
  font-size: var(--moddin-text-md);
}

.agent-card footer {
  display: flex;
  gap: var(--moddin-space-2);
}

.agent-meta {
  display: grid;
  grid-template-columns: max-content 1fr;
  column-gap: var(--moddin-space-4);
  row-gap: var(--moddin-space-1);
  font-size: var(--moddin-text-sm);
  margin: 0;
}

.agent-meta dt {
  color: var(--moddin-text-muted);
}

.agent-meta dd {
  margin: 0;
  word-break: break-all;
}

.agent-meta code {
  background: var(--moddin-surface-3);
  padding: 1px 6px;
  border-radius: 4px;
  font-size: var(--moddin-text-xs);
}

.agent-detail {
  margin: 0;
  color: var(--moddin-text-muted);
  font-size: var(--moddin-text-sm);
}

.agent-hint {
  color: var(--moddin-text-muted);
  font-size: var(--moddin-text-sm);
}

.connected-flag {
  display: inline-flex;
  align-items: center;
  gap: var(--moddin-space-1);
  margin-right: auto;
  color: var(--moddin-success);
  font-size: var(--moddin-text-sm);
}
</style>
