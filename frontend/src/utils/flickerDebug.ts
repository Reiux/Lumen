export const FLICKER_FLAGS = [
  'noanim',
  'nobackdrop',
  'noripple',
  'nostate',
  'nolaunch',
  'nowave',
] as const

export type FlickerFlag = (typeof FLICKER_FLAGS)[number]

export function isFlickerFlagActive(flag: FlickerFlag): boolean {
  if (typeof document === 'undefined') return false
  return document.documentElement.hasAttribute(`data-${flag}`)
}

export function setFlickerFlag(flag: FlickerFlag, active: boolean) {
  if (typeof document === 'undefined') return
  if (active) {
    document.documentElement.setAttribute(`data-${flag}`, '1')
  } else {
    document.documentElement.removeAttribute(`data-${flag}`)
  }

  // Persist to localStorage
  try {
    const storageKey = 'lumen_flicker_debug_flags'
    const activeList: string[] = []
    for (const f of FLICKER_FLAGS) {
      if (document.documentElement.hasAttribute(`data-${f}`)) {
        activeList.push(`${f}=1`)
      }
    }
    if (activeList.length > 0) {
      localStorage.setItem(storageKey, activeList.join('&'))
    } else {
      localStorage.removeItem(storageKey)
    }
  } catch {
    // Ignore storage errors
  }
}

export function initFlickerDebug() {
  if (typeof window === 'undefined' || !import.meta.env.DEV) return

  const searchParams = new URLSearchParams(window.location.search)
  const hash = window.location.hash.startsWith('#')
    ? window.location.hash.slice(1)
    : window.location.hash
  const hashParams = new URLSearchParams(hash.includes('?') ? hash.split('?')[1] : hash)

  const storageKey = 'lumen_flicker_debug_flags'

  // If ?reset=1 or ?clear=1 passed, clear all flags
  if (searchParams.get('reset') === '1' || hashParams.get('reset') === '1') {
    try {
      localStorage.removeItem(storageKey)
    } catch {
      // ignore
    }
    for (const flag of FLICKER_FLAGS) {
      document.documentElement.removeAttribute(`data-${flag}`)
    }
    return
  }

  let savedFlags = ''
  try {
    savedFlags = localStorage.getItem(storageKey) || ''
  } catch {
    // ignore
  }
  const savedParams = new URLSearchParams(savedFlags)

  const activeFlags: string[] = []

  for (const flag of FLICKER_FLAGS) {
    const isSet =
      searchParams.get(flag) === '1' ||
      hashParams.get(flag) === '1' ||
      savedParams.get(flag) === '1' ||
      document.documentElement.hasAttribute(`data-${flag}`)

    if (isSet) {
      document.documentElement.setAttribute(`data-${flag}`, '1')
      activeFlags.push(`${flag}=1`)
    }
  }

  // Persist if URL search had parameters
  if (searchParams.toString() || hashParams.toString()) {
    try {
      if (activeFlags.length > 0) {
        localStorage.setItem(storageKey, activeFlags.join('&'))
      }
    } catch {
      // ignore
    }
  }
}
