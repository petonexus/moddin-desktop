export type ActionLogLevel = 'info' | 'success' | 'warning' | 'error'

export interface ActionLogEntry {
  id: string
  timestamp: number
  level: ActionLogLevel
  action: string
  gameId: string | null
  transactionId: string | null
  message: string
  details: Record<string, string>
}
