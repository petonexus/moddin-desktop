import { enableAutoUnmount, mount } from '@vue/test-utils'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import { i18n } from '../../../i18n'
import ModuleCard from '../ModuleCard.vue'
import { libraryCopyForLocale } from '../copy'

enableAutoUnmount(afterEach)

function mountCard(overrides: Partial<InstanceType<typeof ModuleCard>['$props']> = {}) {
  return mount(ModuleCard, {
    props: {
      name: 'OptiScaler',
      description: 'Upscaling options for this game.',
      state: 'unknown',
      actionLabel: 'Install',
      actionPrimary: true,
      actionBusy: false,
      actionDisabled: false,
      verifyBusy: false,
      ...overrides,
    },
    global: { plugins: [i18n] },
  })
}

beforeEach(() => { i18n.global.locale.value = 'en' })

describe('module-card status and action context', () => {
  it('explains an unchecked state and labels the card from its visible title', () => {
    const wrapper = mountCard()
    const title = wrapper.find('h4')
    const stateHint = wrapper.find('.module-card-status p')

    expect(wrapper.find('.module-card').attributes('aria-labelledby')).toBe(title.attributes('id'))
    expect(wrapper.find('.module-card').attributes('aria-describedby')).toBe(stateHint.attributes('id'))
    expect(stateHint.text()).toBe(libraryCopyForLocale('en').cardUnknownHint)
    expect(wrapper.find('.module-card-actions button').attributes('aria-label')).toContain('OptiScaler')
  })

  it('shows the reason for disabled controls in readable text and links every affected button to it', async () => {
    const wrapper = mountCard({
      state: 'active',
      actionDisabled: true,
      blockedReason: 'Close the game before changing mods.',
      removeLabel: 'Remove',
    })
    const reason = wrapper.find('.module-card-blocked')
    const buttons = wrapper.findAll('.module-card-actions button')

    expect(reason.text()).toBe('Close the game before changing mods.')
    expect(buttons).toHaveLength(3)
    for (const button of buttons) {
      expect(button.attributes('disabled')).toBeDefined()
      expect(button.attributes('aria-describedby')).toBe(reason.attributes('id'))
      await button.trigger('click')
    }
    expect(wrapper.emitted('action')).toBeUndefined()
    expect(wrapper.emitted('verify')).toBeUndefined()
    expect(wrapper.emitted('remove')).toBeUndefined()
  })

  it('keeps verification metadata in details and avoids repeating guidance on healthy cards', () => {
    const wrapper = mountCard({ state: 'active', checkedAtLabel: 'Checked today at 14:00' })

    expect(wrapper.find('.module-card-status').exists()).toBe(false)
    expect(wrapper.find('.module-card-details').text()).toContain('Checked today at 14:00')
    expect(wrapper.find('.module-card').attributes('aria-describedby')).toBeUndefined()
  })

  it('marks checking as busy and explains planned modules without showing installation controls', () => {
    const checking = mountCard({ state: 'checking' })
    const planned = mountCard({ state: 'planned' })

    expect(checking.find('.module-card').attributes('aria-busy')).toBe('true')
    expect(planned.find('.module-card-status').text()).toContain(libraryCopyForLocale('en').cardPlannedHint)
    expect(planned.find('.module-card-actions').exists()).toBe(false)
  })

  it('keeps failed check reasons visible and exposes the full checklist in details', () => {
    const wrapper = mountCard({
      state: 'attention',
      verification: {
        checks: [
          { label: 'Game files', passed: true },
          { label: 'Missing mod file', passed: false, detail: 'Install the missing file again.' },
        ],
      },
    })

    expect(wrapper.find('.module-card-issues').text()).toContain('Missing mod file')
    expect(wrapper.find('.module-card-issues').text()).not.toContain('Game files')
    expect(wrapper.find('.checklist').text()).toContain('Install the missing file again.')
    expect(wrapper.find('.module-card-details summary').text()).toContain('1 of 2 items OK')
  })

  it('shows an update as text when it has no release URL, and opens an actual release when present', async () => {
    const wrapper = mountCard({
      update: { hasSource: true, available: true, summary: 'Version 2 available', releaseUrl: null, busy: false },
    })

    expect(wrapper.find('.update-pill').exists()).toBe(false)
    expect(wrapper.find('.module-card-tags').text()).toContain('Version 2 available')
    await wrapper.setProps({
      update: { hasSource: true, available: true, summary: 'Version 2 available', releaseUrl: 'https://github.com/example/mod/releases', busy: false },
    })
    const release = wrapper.find('.update-pill')
    expect(release.attributes('aria-label')).toContain('OptiScaler')
    await release.trigger('click')
    expect(wrapper.emitted('open-release')).toEqual([[]])
  })

  it.each(['pt-BR', 'es'])('uses localized guidance when the language changes to %s', async (locale) => {
    const wrapper = mountCard({ state: 'unknown' })
    i18n.global.locale.value = locale as 'pt-BR' | 'es'
    await wrapper.vm.$nextTick()

    expect(wrapper.find('.module-card-status p').text()).toBe(libraryCopyForLocale(locale).cardUnknownHint)
  })
})
