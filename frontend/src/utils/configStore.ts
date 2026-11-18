import { reactive } from 'vue'
import { ConfigManager } from '../../bindings/app/core'

export interface PreferencesState {
  useQuota: boolean
  saver: boolean
  autoCheckUpdates: boolean
  notifyOnComplete: boolean
  initialized: boolean
}

export const preferencesState = reactive<PreferencesState>({
  useQuota: false,
  saver: false,
  autoCheckUpdates: true,
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

  preferencesState.initialized = true
}

export async function setUseQuota(useQuota: boolean) {
  const prevQuota = preferencesState.useQuota
  const prevSaver = preferencesState.saver
  preferencesState.useQuota = useQuota
  if (useQuota) {
    preferencesState.saver = false
  }
  try {
    await ConfigManager.SetUseQuota(useQuota)
  } catch (err) {
    preferencesState.useQuota = prevQuota
    preferencesState.saver = prevSaver
    console.error('Failed to update useQuota:', err)
    throw err
  }
}

export async function setSaver(saver: boolean) {
  const prevSaver = preferencesState.saver
  const prevQuota = preferencesState.useQuota
  preferencesState.saver = saver
  if (saver) {
    preferencesState.useQuota = false
  }
  try {
    await ConfigManager.SetSaver(saver)
  } catch (err) {
    preferencesState.saver = prevSaver
    preferencesState.useQuota = prevQuota
    console.error('Failed to update saver:', err)
    throw err
  }
}

export async function setAutoCheckUpdates(enabled: boolean) {
  const prev = preferencesState.autoCheckUpdates
  preferencesState.autoCheckUpdates = enabled
  try {
    await ConfigManager.SetAutoCheckUpdates(enabled)
  } catch (err) {
    preferencesState.autoCheckUpdates = prev
    console.error('Failed to update autoCheckUpdates:', err)
    throw err
  }
}

export async function setNotifyOnComplete(enabled: boolean) {
  const prev = preferencesState.notifyOnComplete
  preferencesState.notifyOnComplete = enabled
  try {
    await ConfigManager.SetNotifyOnComplete(enabled)
  } catch (err) {
    preferencesState.notifyOnComplete = prev
    console.error('Failed to update notifyOnComplete:', err)
    throw err
  }
}
