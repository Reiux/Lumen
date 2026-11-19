import { reactive } from 'vue'
import { ConfigManager } from '../../bindings/app/core'

export interface PreferencesState {
  useQuota: boolean
  saver: boolean
  autoCheckUpdates: boolean
  autoUpdateIntervalHours: number
  lastUpdateCheck: number
  notifyOnComplete: boolean
  initialized: boolean
}

export const preferencesState = reactive<PreferencesState>({
  useQuota: false,
  saver: false,
  autoCheckUpdates: true,
  autoUpdateIntervalHours: 6,
  lastUpdateCheck: 0,
  notifyOnComplete: true,
  initialized: false,
})

export async function refreshPreferences() {
  try {
    const settings = await ConfigManager.GetSettings()
    if (settings) {
      preferencesState.useQuota = !!settings.useQuota
      preferencesState.saver = !!settings.saver
      preferencesState.notifyOnComplete = settings.notifyOnComplete ?? true
    }
  } catch {
    // Ignore in tests/mock
  }

  try {
    const autoUpdates = await ConfigManager.GetAutoCheckUpdates()
    preferencesState.autoCheckUpdates = autoUpdates ?? true
  } catch {
    // Ignore in tests/mock
  }

  try {
    const interval = await ConfigManager.GetAutoUpdateIntervalHours()
    if (typeof interval === 'number' && interval >= 1) {
      preferencesState.autoUpdateIntervalHours = interval
    }
  } catch {
    // Ignore in tests/mock
  }

  try {
    const ts = await ConfigManager.GetLastUpdateCheck()
    if (typeof ts === 'number' && ts >= 0) {
      preferencesState.lastUpdateCheck = ts
    }
  } catch {
    // Ignore in tests/mock
  }

  preferencesState.initialized = true
}

export async function refreshLastUpdateCheck() {
  try {
    const ts = await ConfigManager.GetLastUpdateCheck()
    if (typeof ts === 'number' && ts >= 0) {
      preferencesState.lastUpdateCheck = ts
    }
  } catch {
    // Ignore in tests/mock
  }
}

const settingSeq: Record<string, number> = {}

function nextSeq(key: string): number {
  const next = (settingSeq[key] ?? 0) + 1
  settingSeq[key] = next
  return next
}

function isLatestSeq(key: string, seq: number): boolean {
  return settingSeq[key] === seq
}

export async function setUseQuota(useQuota: boolean) {
  const seq = nextSeq('storageMode')
  const prevQuota = preferencesState.useQuota
  const prevSaver = preferencesState.saver
  preferencesState.useQuota = useQuota
  if (useQuota) {
    preferencesState.saver = false
  }
  try {
    await ConfigManager.SetUseQuota(useQuota)
  } catch (err) {
    if (isLatestSeq('storageMode', seq)) {
      preferencesState.useQuota = prevQuota
      preferencesState.saver = prevSaver
    }
    console.error('Failed to update useQuota:', err)
    throw err
  }
}

export async function setSaver(saver: boolean) {
  const seq = nextSeq('storageMode')
  const prevSaver = preferencesState.saver
  const prevQuota = preferencesState.useQuota
  preferencesState.saver = saver
  if (saver) {
    preferencesState.useQuota = false
  }
  try {
    await ConfigManager.SetSaver(saver)
  } catch (err) {
    if (isLatestSeq('storageMode', seq)) {
      preferencesState.saver = prevSaver
      preferencesState.useQuota = prevQuota
    }
    console.error('Failed to update saver:', err)
    throw err
  }
}

export async function setAutoCheckUpdates(enabled: boolean) {
  const seq = nextSeq('autoCheckUpdates')
  const prev = preferencesState.autoCheckUpdates
  preferencesState.autoCheckUpdates = enabled
  try {
    await ConfigManager.SetAutoCheckUpdates(enabled)
  } catch (err) {
    if (isLatestSeq('autoCheckUpdates', seq)) {
      preferencesState.autoCheckUpdates = prev
    }
    console.error('Failed to update autoCheckUpdates:', err)
    throw err
  }
}

export async function setAutoUpdateIntervalHours(hours: number) {
  const seq = nextSeq('autoUpdateIntervalHours')
  const prev = preferencesState.autoUpdateIntervalHours
  preferencesState.autoUpdateIntervalHours = hours
  try {
    await ConfigManager.SetAutoUpdateIntervalHours(hours)
  } catch (err) {
    if (isLatestSeq('autoUpdateIntervalHours', seq)) {
      preferencesState.autoUpdateIntervalHours = prev
    }
    console.error('Failed to update autoUpdateIntervalHours:', err)
    throw err
  }
}

export async function setNotifyOnComplete(enabled: boolean) {
  const seq = nextSeq('notifyOnComplete')
  const prev = preferencesState.notifyOnComplete
  preferencesState.notifyOnComplete = enabled
  try {
    await ConfigManager.SetNotifyOnComplete(enabled)
  } catch (err) {
    if (isLatestSeq('notifyOnComplete', seq)) {
      preferencesState.notifyOnComplete = prev
    }
    console.error('Failed to update notifyOnComplete:', err)
    throw err
  }
}
