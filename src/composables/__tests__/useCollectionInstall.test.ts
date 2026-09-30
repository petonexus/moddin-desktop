import { beforeEach, describe, expect, it, vi } from 'vitest'
import type { CollectionSummary } from '../../types/collection'
import type { CapabilitySpec } from '../../types/capability'

/**
 * The collection run, against a mocked backend.
 *
 * Two of the three install paths in this app shipped unable to install
 * anything, and this is the file whose absence let them: every member was
 * sent as `config: {}` to `community_capability_install`, which is both
 * the wrong command for a built-in recipe and a config that fails
 * preflight on the first required field. These tests pin the two things
 * that make a collection installable at all — the right command per
 * member, and the per-game config from the catalogue — plus the refusal
 * that replaces guessing when a required field has no value anywhere.
 *
 * `services/catalog` is *not* mocked: the config a member installs with
 * has to come from the real shipped catalogue, or this test would only
 * prove that a mock reached a mock.
 */
vi.mock('../../features/collection/service', () => ({ listCollections: vi.fn() }))
vi.mock('../../features/community/service', () => ({ installCommunityCapability: vi.fn() }))
vi.mock('../../features/capability-modules/service', () => ({
  getCapabilitySpec: vi.fn(),
  installCapability: vi.fn(),
  listCapabilities: vi.fn(),
  resolveInstallTarget: vi.fn(),
  uninstallCapability: vi.fn(),
}))

import { useCollectionInstall } from '../useCollectionInstall'
import { installCommunityCapability } from '../../features/community/service'
import {
  getCapabilitySpec,
  installCapability,
  listCapabilities,
  resolveInstallTarget,
} from '../../features/capability-modules/service'

const mockedCommunityInstall = vi.mocked(installCommunityCapability)
const mockedInstall = vi.mocked(installCapability)
const mockedList = vi.mocked(listCapabilities)
const mockedSpec = vi.mocked(getCapabilitySpec)
const mockedResolve = vi.mocked(resolveInstallTarget)

const TARGET = {
  gameId: 'cyberpunk-2077',
  gameName: 'Cyberpunk 2077',
  installDir: 'C:\\games\\cyberpunk\\Mods',
  executableDir: 'C:\\games\\cyberpunk',
}

function summary(members: string[]): CollectionSummary {
  return {
    id: 'vr-stack',
    displayName: 'VR stack',
    category: 'vr',
    description: 'A set written by a test.',
    targetGame: 'cyberpunk-2077',
    capabilityCount: members.length,
    requiredCount: members.length,
    preset: {
      blocked: [],
      capabilities: members.map((id) => ({ id, displayName: id, rationale: 'Because.' })),
    },
  }
}

function spec(id: string, overrides: Partial<CapabilitySpec> = {}): CapabilitySpec {
  return { id, displayName: id, category: 'graphics', status: 'available', ...overrides }
}

/** The shipped `optiscaler` fields, minus the two the catalogue has no answer for. */
function optiscalerSpec(): CapabilitySpec {
  return spec('optiscaler', {
    configSchema: [
      { name: 'downloadUrl', type: 'url', required: true },
      { name: 'sha256', type: 'sha256', required: true },
      { name: 'version', type: 'string', required: true },
    ],
  })
}

beforeEach(() => {
  mockedCommunityInstall.mockReset()
  mockedInstall.mockReset()
  mockedList.mockReset()
  mockedSpec.mockReset()
  mockedResolve.mockReset()

  mockedResolve.mockResolvedValue(TARGET)
  mockedList.mockResolvedValue([
    { id: 'optiscaler', displayName: 'OptiScaler', category: 'graphics', status: 'available', origin: 'builtIn' },
    { id: 'cheeky-foveated-dlss', displayName: 'Cheeky', category: 'graphics', status: 'available', origin: 'builtIn' },
    { id: 'ofxr-bridge', displayName: 'OFXR Bridge', category: 'vr', status: 'available', origin: 'builtIn' },
  ])
  mockedSpec.mockImplementation(async (id: string) => {
    if (id === 'cheeky-foveated-dlss') {
      return spec(id, {
        configSchema: [
          { name: 'downloadUrl', type: 'url', required: true },
          { name: 'addonFile', type: 'string', required: true },
        ],
      })
    }
    return optiscalerSpec()
  })
  mockedInstall.mockResolvedValue({
    capabilityId: 'optiscaler',
    transaction: { id: 'tx-1', createdAt: 0, kind: 'optiscaler', label: 'OptiScaler', gameId: 'cyberpunk-2077', targetPath: '', backupPath: '', status: 'applied' },
    steps: [],
    affectedPaths: [],
  })
  mockedCommunityInstall.mockResolvedValue({ capabilityId: 'community-x', transaction: { id: 'tx-9' }, steps: [], affectedPaths: [] })
})

describe('a collection of built-in recipes installs', () => {
  it('runs every member through capability_install with the catalogue config', async () => {
    const install = useCollectionInstall()
    await install.start(summary(['optiscaler', 'cheeky-foveated-dlss']), 'cyberpunk-2077')

    expect(install.phase.value).toBe('completed')
    expect(mockedInstall).toHaveBeenCalledTimes(2)
    // Not the community command: a built-in recipe is a registered
    // capability, and the community command is for recipes that are
    // fetched and signature-checked at install time.
    expect(mockedCommunityInstall).not.toHaveBeenCalled()

    // Values, not an empty config. These are the real strings out of
    // `src/catalog/games/cyberpunk-2077.yaml`, so a renamed catalogue
    // key fails here rather than at an install.
    expect(mockedInstall).toHaveBeenNthCalledWith(1, {
      capabilityId: 'optiscaler',
      gameId: 'cyberpunk-2077',
      gameName: 'Cyberpunk 2077',
      installDir: 'C:\\games\\cyberpunk\\Mods',
      executableDir: 'C:\\games\\cyberpunk',
      config: {
        values: {
          downloadUrl: 'https://github.com/optiscaler/OptiScaler/releases/download/v0.9.4/Optiscaler_0.9.4-final.20260718._MM.7z',
          sha256: '575cb4df866116093df75af607e37fd70e10f5163e0f23fd5c804142e80ef0ad',
          version: '0.9.4',
        },
      },
    })
    // In preset order, each with its own module's config.
    expect(mockedInstall).toHaveBeenNthCalledWith(
      2,
      expect.objectContaining({
        capabilityId: 'cheeky-foveated-dlss',
        config: {
          values: {
            downloadUrl: 'https://github.com/ClarkCheekyKent/CheekyFoveatedDLSS/releases/download/v0.3.4/CheekyFoveatedDLSS.addon64',
            addonFile: 'CheekyFoveatedDLSS.addon64',
          },
        },
      }),
    )
    expect(install.error.value).toBeNull()
  })

  it('reverts through the ordinary capability uninstall, not a collection command', async () => {
    const install = useCollectionInstall()
    await install.start(summary(['optiscaler', 'cheeky-foveated-dlss']), 'cyberpunk-2077')
    await install.abort()

    const { uninstallCapability } = await import('../../features/capability-modules/service')
    expect(vi.mocked(uninstallCapability)).toHaveBeenCalledTimes(2)
  })
})

describe('a member whose required config has no value', () => {
  // The shipped truth, not a hypothetical: `ofxr-bridge` declares
  // `runtimePath`, `trayExe`, `trayIniPath` and `extractedDir` as
  // required paths, and no catalogue entry carries them — they only exist
  // after OptiScaler has been extracted into the game folder. There is no
  // honest value to send, so the run stops before it starts rather than
  // failing inside a step with a field name the user never saw.
  it('is reported as needing input before any install call', async () => {
    mockedSpec.mockImplementation(async (id: string) =>
      id === 'optiscaler'
        ? optiscalerSpec()
        : spec(id, {
            configSchema: [
              { name: 'runtimePath', type: 'path', required: true },
              { name: 'trayExe', type: 'path', required: true },
              { name: 'nvidiaPreset', type: 'enum', required: false, default: 'medium', enumValues: ['low', 'medium'] },
            ],
          }),
    )

    const install = useCollectionInstall()
    await install.start(summary(['optiscaler', 'ofxr-bridge']), 'cyberpunk-2077')

    expect(mockedInstall).not.toHaveBeenCalled()
    expect(mockedCommunityInstall).not.toHaveBeenCalled()
    expect(install.phase.value).toBe('error')
    expect(install.errorCode.value).toBe('needsConfig')
    expect(install.missingConfig.value).toEqual([
      { capabilityId: 'ofxr-bridge', fields: ['runtimePath', 'trayExe'] },
    ])
  })

  it('a defaulted optional field is not treated as missing', async () => {
    // The control for the refusal above: a field with a default is filled
    // in, so the same member with no required field installs.
    mockedSpec.mockImplementation(async (id: string) =>
      id === 'optiscaler'
        ? optiscalerSpec()
        : spec(id, {
            configSchema: [
              { name: 'nvidiaPreset', type: 'enum', required: false, default: 'medium', enumValues: ['low', 'medium'] },
            ],
          }),
    )

    const install = useCollectionInstall()
    await install.start(summary(['optiscaler', 'ofxr-bridge']), 'cyberpunk-2077')

    expect(install.phase.value).toBe('completed')
    expect(mockedInstall).toHaveBeenNthCalledWith(
      2,
      expect.objectContaining({ capabilityId: 'ofxr-bridge', config: { values: { nvidiaPreset: 'medium' } } }),
    )
  })
})

describe('routing', () => {
  it('sends a member the registry does not know to the community command, unsigned', async () => {
    // The defensive half of the routing: a community id cannot reach a
    // collection today (the loader refuses it at load time), but if one
    // ever did it must not be installed as a built-in — and it must not be
    // waved through unsigned either. `getCapabilitySpec` fails for it
    // exactly as the real backend does, and that is not a reason to
    // pretend the run cannot be planned.
    mockedList.mockResolvedValue([
      { id: 'optiscaler', displayName: 'OptiScaler', category: 'graphics', status: 'available', origin: 'builtIn' },
    ])
    mockedSpec.mockImplementation(async (id: string) => {
      if (id === 'community-some-mod') throw new Error('no such capability')
      return optiscalerSpec()
    })

    const install = useCollectionInstall()
    await install.start(summary(['community-some-mod']), 'cyberpunk-2077')

    expect(install.errorCode.value).toBeNull()
    expect(mockedCommunityInstall).toHaveBeenCalledWith(
      expect.objectContaining({
        capabilityId: 'community-some-mod',
        config: { values: {} },
        acceptUnsigned: false,
      }),
    )
    expect(mockedInstall).not.toHaveBeenCalled()
  })

  it('a registered recipe the backend will not describe stops the run', async () => {
    // The other direction: a built-in we cannot read is one whose required
    // fields nobody can check, so nothing is installed on a guess.
    mockedSpec.mockRejectedValue(new Error('capability `optiscaler` is not a registered capability'))

    const install = useCollectionInstall()
    await install.start(summary(['optiscaler']), 'cyberpunk-2077')

    expect(mockedInstall).not.toHaveBeenCalled()
    expect(install.errorCode.value).toBe('needsConfig')
    expect(install.missingConfig.value[0].capabilityId).toBe('optiscaler')
  })
})
