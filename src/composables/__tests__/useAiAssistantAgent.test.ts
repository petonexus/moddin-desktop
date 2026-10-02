import { afterEach, describe, expect, it, vi } from 'vitest'
import { useAiAssistant } from '../useAiAssistant'

const { run, cancel, save } = vi.hoisted(() => ({ run: vi.fn(), cancel: vi.fn(), save: vi.fn() }))
vi.mock('../../features/ai-assistant/service', async (original) => ({
  ...await original<typeof import('../../features/ai-assistant/service')>(),
  runAiAgentPrompt: run,
  cancelAiAgentPrompt: cancel,
  saveCapabilityYaml: save,
}))

afterEach(() => { vi.clearAllMocks() })

describe('AI provider consent and cancellation', () => {
  it('requires explicit consent before submitting context', async () => {
    const assistant = useAiAssistant()
    assistant.promptText.value = 'private game context'
    await assistant.runWithAgent('codex')
    expect(run).not.toHaveBeenCalled()
  })

  it('keeps a running request visible, cancels it and never saves the result', async () => {
    let finish!: (result: { status: string; output: string; yaml: string | null }) => void
    run.mockImplementation(() => new Promise(resolve => { finish = resolve }))
    cancel.mockResolvedValue(undefined)
    const assistant = useAiAssistant()
    assistant.open.value = true
    assistant.promptText.value = 'game context'
    const pending = assistant.runWithAgent('codex', true)
    expect(run).toHaveBeenCalledWith('codex', expect.stringContaining('game context'), true)
    expect(assistant.agentBusy.value).toBe(true)
    assistant.close()
    expect(assistant.open.value).toBe(true)
    await assistant.cancelAgentRun()
    expect(cancel).toHaveBeenCalledOnce()
    finish({ status: 'ok', output: 'late answer', yaml: 'id: discard-after-cancel' })
    await pending
    expect(assistant.agentBusy.value).toBe(false)
    expect(assistant.yamlInput.value).not.toBe('id: discard-after-cancel')
    expect(save).not.toHaveBeenCalled()
    assistant.close()
    expect(assistant.open.value).toBe(false)
  })
})
