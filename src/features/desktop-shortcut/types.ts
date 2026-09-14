import type { TransactionRecord } from '../../types/transaction'

export interface DesktopShortcutRequest {
  gameId: string
  gameName: string
  installDir: string
  executable: string
}

export interface DesktopShortcutPreview {
  canApply: boolean
  shortcutPath: string
  targetPath: string
  iconPath: string
  willReplace: boolean
}

export interface DesktopShortcutResult {
  shortcutPath: string
  transaction: TransactionRecord
}
