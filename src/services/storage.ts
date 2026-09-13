function localStorageOrNull() {
  if (typeof window === 'undefined') return null
  try {
    return window.localStorage
  } catch {
    return null
  }
}

export function readLocalValue(key: string) {
  const storage = localStorageOrNull()
  if (!storage) return null
  try {
    return storage.getItem(key)
  } catch {
    return null
  }
}

export function writeLocalValue(key: string, value: string) {
  const storage = localStorageOrNull()
  if (!storage) return false
  try {
    storage.setItem(key, value)
    return true
  } catch {
    return false
  }
}

export function removeLocalValue(key: string) {
  const storage = localStorageOrNull()
  if (!storage) return false
  try {
    storage.removeItem(key)
    return true
  } catch {
    return false
  }
}
