import { describe, expect, it } from 'vitest'
import { shouldPersistAction } from '../activity-log'

describe('capability activity history', () => {
  it.each(['capability_install', 'capability_uninstall', 'community_capability_install'])(
    'records %s in persistent history', (command) => {
      expect(shouldPersistAction(command)).toBe(true)
    })
  it('keeps previews out of persistent history', () => {
    expect(shouldPersistAction('capability_evaluate')).toBe(false)
  })
})
