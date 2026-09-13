function storageAvailable() {
  return typeof window !== 'undefined' && typeof window.localStorage !== 'undefined'
}

export function readLocalValue(key: string) {
  if (!storageAvailable()) return null
  try {
    return window.localStorage.getItem(key)
  } catch {
    return null
  }
}

export function writeLocalValue(key: string, value: string) {
  if (!storageAvailable()) return false
  try {
    window.localStorage.setItem(key, value)
    return true
  } catch {
    return false
  }
}

export function removeLocalValue(key: string) {
  if (!storageAvailable()) return false
  try {
    window.localStorage.removeItem(key)
    return true
  } catch {
    return false
  }
}
