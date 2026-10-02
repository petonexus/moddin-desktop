import { describe, expect, it, vi } from 'vitest'
import { useAiTopbarActions } from '../useAiTopbarActions'

const { openFor, diagnose } = vi.hoisted(() => ({ openFor: vi.fn(), diagnose: vi.fn() }))
vi.mock('../useAiAssistant', () => ({ useAiAssistantTrigger: () => ({ openFor }) }))
vi.mock('../useAiModuleActions', () => ({ useAiModuleActions: () => ({ openDiagnoseWithAi: diagnose }) }))

describe('AI topbar game context', () => {
  it('uses the current catalog game id for authoring and diagnosis', async () => {
    let gameId: string | null = 'elden-ring'
    const actions = useAiTopbarActions({ selectedGameId: () => gameId,
      selectedGameName: () => 'Selected game', currentError: () => 'failure' })
    await actions.ask()
    expect(openFor).toHaveBeenLastCalledWith(expect.objectContaining({ gameId: 'elden-ring' }))
    gameId = 'cyberpunk-2077'
    actions.diagnose()
    expect(diagnose).toHaveBeenLastCalledWith(expect.objectContaining({ gameId: 'cyberpunk-2077' }))
    gameId = null
    await actions.ask()
    expect(openFor).toHaveBeenLastCalledWith(expect.objectContaining({ gameId: null }))
  })
})
