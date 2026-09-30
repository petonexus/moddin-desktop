import type { ModuleVerification, ModuleVerificationCheck, ModuleVerificationStatus } from '../../types/module-verification'
import type {
  ModulePreviewByDialog,
  ModuleRequestByDialog,
  ModuleDialogName,
  Translate,
} from './module-registry'

/**
 * How each module's preview becomes a checklist.
 *
 * Every module answers "can I apply this?" differently — OptiScaler also
 * has to say which proxy it would load, UEVR has to pass an engine gate
 * — so the checks are per module and cannot be derived from a shared
 * shape. What *is* shared is the outcome: a status, one summary line and
 * the checks themselves, which is what the card and the store keep.
 *
 * These are pure functions over a preview the backend already returned.
 * They were an if-chain inside `verifyModule`, where the only way to
 * tell a module's checks from another's was to read the branch it took.
 */

/** What a module's verification records once its preview is in. */
export interface VerificationOutcome {
  status: ModuleVerificationStatus
  summary: string
  checks: ModuleVerificationCheck[]
  gameRunning: boolean
}

export type VerificationProbe<D extends ModuleDialogName> = (
  preview: ModulePreviewByDialog[D],
  request: ModuleRequestByDialog[D],
  t: Translate,
) => VerificationOutcome

function check(label: string, passed: boolean, detail?: string): ModuleVerificationCheck {
  return { label, passed, detail }
}

/** Installed beats ready beats attention, for every module. */
function statusFor(installed: boolean, available: boolean): ModuleVerificationStatus {
  if (installed) return 'installed'
  return available ? 'ready' : 'attention'
}

export function verificationSummary(status: ModuleVerificationStatus, t: Translate) {
  if (status === 'installed') return t('verificationSummaryInstalled')
  if (status === 'ready') return t('verificationSummaryReady')
  if (status === 'attention') return t('verificationSummaryAttention')
  return t('verificationSummaryUnknown')
}

/**
 * The "this is the version I installed" detail. Shared by every module
 * that pins a version, and deliberately empty when nothing is installed
 * — an empty detail is not a failed check.
 */
function installedVersionDetail(installedVersion: string | null | undefined, t: Translate) {
  return installedVersion ? t('installedVersion', { version: installedVersion }) : undefined
}

export const VERIFICATION_PROBES: { [D in ModuleDialogName]: VerificationProbe<D> } = {
  'vr-launch': (preview, request, t) => {
    const filesPresent = preview.missingFiles.length === 0
    const configMatches = !request.configPath
      || (preview.configExists && preview.settings.every((setting) => !setting.willChange))
    const status = statusFor(filesPresent && configMatches, preview.canLaunch)
    return {
      status,
      summary: verificationSummary(status, t),
      checks: [
        check(t('checkExecutable'), preview.executableExists),
        check(t('checkVrFiles'), filesPresent),
        check(t('checkVrConfig'), configMatches),
        check(t('checkOpenXrRuntime'), Boolean(preview.activeOpenXrRuntime)),
        check(t('checkGameClosed'), !preview.gameRunning),
      ],
      gameRunning: preview.gameRunning,
    }
  },

  'obs-vr': (preview, _request, t) => {
    const status = statusFor(preview.installed, preview.canApply)
    return {
      status,
      summary: verificationSummary(status, t),
      checks: [
        check(t('checkObsCollection'), Boolean(preview.collectionFile)),
        check(t('checkObsScene'), preview.sceneFound),
        check(t('checkObsSource'), preview.sourceExists),
        check(t('checkObsTarget'), preview.sourceTargetMatches),
        check(t('checkObsSceneLink'), preview.sourceInScene),
        check(t('checkGameClosed'), !preview.gameRunning),
      ],
      gameRunning: preview.gameRunning,
    }
  },

  optiscaler: (preview, request, t) => {
    const installed = preview.installed && preview.installedVersion === request.version
    const status = statusFor(installed, preview.canApply)
    return {
      status,
      summary: verificationSummary(status, t),
      checks: [
        check(t('checkExecutable'), preview.executableExists),
        check(t('checkGameClosed'), !preview.gameRunning),
        check(t('checkManagedInstall'), !preview.manualInstallDetected),
        check(t('checkProxyAvailable'), Boolean(preview.selectedProxy)),
        check(
          t('checkVersion'),
          installed,
          preview.installed ? installedVersionDetail(preview.installedVersion, t) : undefined,
        ),
      ],
      gameRunning: preview.gameRunning,
    }
  },

  ofxr: (preview, request, t) => {
    // Arming is part of being installed: a tray that is running but not
    // armed still needs the user to start the game from Moddin.
    const installed = preview.installed && preview.configured && preview.trayRunning && preview.armed
    const status = statusFor(installed, preview.canApply && preview.executableExists)
    return {
      status,
      summary: verificationSummary(status, t),
      checks: [
        check(t('checkExecutable'), preview.executableExists),
        check(t('checkOfxrInstalled'), preview.installed && preview.trayInstalled),
        check(
          t('checkVersion'),
          preview.installed && preview.installedVersion === request.version,
          installedVersionDetail(preview.installedVersion, t),
        ),
        check(t('checkOfxrConfig'), preview.configured),
        check(t('checkOfxrTray'), preview.trayRunning),
        check(t('checkOfxrArmed'), preview.armed),
        check(t('checkGameClosed'), !preview.gameRunning),
      ],
      gameRunning: preview.gameRunning,
    }
  },

  cheeky: (preview, request, t) => {
    const installed = preview.installed && preview.installedVersion === request.version
    const status = statusFor(installed, preview.canApply)
    return {
      status,
      summary: verificationSummary(status, t),
      checks: [
        check(t('checkExecutable'), preview.executableExists),
        check(t('checkGameClosed'), !preview.gameRunning),
        check(t('checkManagedInstall'), !preview.manualInstallDetected),
        check(t('checkCheekyAddon'), preview.installed),
        check(
          t('checkVersion'),
          installed,
          preview.installed ? installedVersionDetail(preview.installedVersion, t) : undefined,
        ),
      ],
      gameRunning: preview.gameRunning,
    }
  },

  uevr: (preview, request, t) => {
    const installed = preview.installed
      && preview.installedBackend === request.backend
      && preview.selectedVersion !== null
      && preview.installedVersion === preview.selectedVersion
    const status = statusFor(installed, preview.canApply)
    return {
      status,
      summary: verificationSummary(status, t),
      checks: [
        check(t('checkExecutable'), preview.executableExists),
        // Unknown engine is never "Unreal": the gate hides the card
        // rather than letting a non-Unreal game through.
        check(t('checkUevrEngine'), preview.engine === 'Unreal Engine', preview.engine ?? t('uevrUnknownEngine')),
        check(t('checkUevrBackend'), preview.selectedVersion !== null, preview.backendLabel),
        check(t('checkGameClosed'), !preview.gameRunning && !preview.uevrRunning),
        check(t('checkManagedInstall'), !preview.manualInstallDetected),
        check(
          t('checkVersion'),
          installed,
          installedVersionDetail(preview.installedVersion, t),
        ),
      ],
      gameRunning: preview.gameRunning || preview.uevrRunning,
    }
  },
}
