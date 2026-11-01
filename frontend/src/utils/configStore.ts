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
  preferencesState.useQuota = useQuota
  if (useQuota) {
    preferencesState.saver = false
  }
  try {
    await ConfigManager.SetUseQuota(useQuota)
  } catch {
    // Ignore in tests/mock
  }
}

export async function setSaver(saver: boolean) {
  preferencesState.saver = saver
  if (saver) {
    preferencesState.useQuota = false
  }
  try {
    await ConfigManager.SetSaver(saver)
  } catch {
    // Ignore in tests/mock
  }
}

export async function setAutoCheckUpdates(enabled: boolean) {
  preferencesState.autoCheckUpdates = enabled
  try {
    await ConfigManager.SetAutoCheckUpdates(enabled)
  } catch {
    // Ignore in tests/mock
  }
}

export async function setNotifyOnComplete(enabled: boolean) {
  preferencesState.notifyOnComplete = enabled
  try {
    await ConfigManager.SetNotifyOnComplete(enabled)
  } catch {
    // Ignore in tests/mock
  }
}
