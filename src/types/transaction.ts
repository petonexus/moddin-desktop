export interface TransactionFile {
  targetPath: string
  backupPath: string | null
  existedBefore: boolean
}

export interface TransactionRecord {
  id: string
  createdAt: number
  kind: string
  label: string
  gameId: string
  targetPath: string
  backupPath: string
  status: 'prepared' | 'applied' | 'rolled_back' | string
  files?: TransactionFile[]
  createdDirectories?: string[]
  metadata?: Record<string, string>
}
