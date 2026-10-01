import { invokeDebug as invoke } from '../../debug'
import type { CheekyFoveatedDlssPreview, CheekyFoveatedDlssRequest, CheekyFoveatedDlssResult } from '../../types/cheeky'
import type { ModuleUpdate } from '../../types/module-update'
import type { ObsVrPreview, ObsVrRequest } from '../../types/obs'
import type { OfxrPreview, OfxrRequest, OfxrResult } from '../../types/ofxr'
import type { OptiScalerPreview, OptiScalerRequest } from '../../types/optiscaler'
import type { TransactionRecord } from '../../types/transaction'
import type { UevrPreview, UevrRequest, UevrResult } from '../../types/uevr'
import type { VrLaunchPreview, VrLaunchRequest, VrLaunchResult } from '../../types/vr-launch'

/**
 * The library grid's native boundary: the only place in the `modules`
 * feature that reaches a Tauri command.
 *
 * Everything the grid does to a game folder is a preview first and a
 * mutation second, so the pairs below are deliberately kept together —
 * `previewObsVr` and `configureObsVr` are the same recipe seen from
 * either side, and a reader should be able to check that the command the
 * card advertises is the command that runs. `module-registry.ts` names
 * the same commands as data; its test fails if the two drift apart.
 */

export function previewVrLaunch(request: VrLaunchRequest) {
  return invoke<VrLaunchPreview>('preview_vr_launch', { request })
}

/**
 * The OFXR bridge is armed as part of the launch rather than installed
 * here, so this command takes both requests. `ofxrRequest` is null when
 * the selected game has no OFXR module, which is the normal case.
 */
export function launchVrGame(request: VrLaunchRequest, ofxrRequest: OfxrRequest | null) {
  return invoke<VrLaunchResult>('launch_vr_game', { request, ofxrRequest })
}

export function previewObsVr(request: ObsVrRequest) {
  return invoke<ObsVrPreview>('preview_obs_vr', { request })
}

export function configureObsVr(request: ObsVrRequest) {
  return invoke<TransactionRecord>('configure_obs_vr', { request })
}

export function uninstallObsVr(request: ObsVrRequest) {
  return invoke<TransactionRecord>('uninstall_obs_vr', { request })
}

export function previewOptiScaler(request: OptiScalerRequest) {
  return invoke<OptiScalerPreview>('preview_optiscaler', { request })
}

export function installOptiScaler(request: OptiScalerRequest) {
  return invoke<TransactionRecord>('install_optiscaler', { request })
}

export function uninstallOptiScaler(request: OptiScalerRequest) {
  return invoke<TransactionRecord>('uninstall_optiscaler', { request })
}

export function previewOfxr(request: OfxrRequest) {
  return invoke<OfxrPreview>('preview_ofxr', { request })
}

export function installOfxr(request: OfxrRequest) {
  return invoke<OfxrResult>('install_ofxr', { request })
}

export function uninstallOfxr(request: OfxrRequest) {
  return invoke<TransactionRecord>('uninstall_ofxr', { request })
}

export function previewCheekyFoveatedDlss(request: CheekyFoveatedDlssRequest) {
  return invoke<CheekyFoveatedDlssPreview>('preview_cheeky_foveated_dlss', { request })
}

export function installCheekyFoveatedDlss(request: CheekyFoveatedDlssRequest) {
  return invoke<CheekyFoveatedDlssResult>('install_cheeky_foveated_dlss', { request })
}

export function uninstallCheekyFoveatedDlss(request: CheekyFoveatedDlssRequest) {
  return invoke<TransactionRecord>('uninstall_cheeky_foveated_dlss', { request })
}

export function previewUevr(request: UevrRequest) {
  return invoke<UevrPreview>('preview_uevr', { request })
}

export function installUevr(request: UevrRequest) {
  return invoke<UevrResult>('install_uevr', { request })
}

export function uninstallUevr(request: UevrRequest) {
  return invoke<TransactionRecord>('uninstall_uevr', { request })
}

/**
 * Undo for a module with no dedicated uninstall: roll back the last
 * applied transaction of that kind for that game. Used by the VR launch
 * profile and the desktop shortcut, whose only recorded change is one
 * transaction.
 */
export function rollbackLatestModuleTransaction(gameId: string, kind: string) {
  return invoke<TransactionRecord>('rollback_latest_module_transaction', { gameId, kind })
}

/** Ask the update checker whether `updateUrl` has something newer. */
export function checkModuleUpdate(request: { currentVersion: string | null; updateUrl: string }) {
  return invoke<ModuleUpdate>('check_module_update', { request })
}
