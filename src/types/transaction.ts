export interface TransactionRecord {
  id: string
  createdAt: number
  kind: string
  label: string
  gameId: string
  targetPath: string
  backupPath: string
  status: 'applied' | 'rolled_back' | string
}
