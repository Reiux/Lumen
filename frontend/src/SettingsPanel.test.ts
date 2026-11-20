import { describe, expect, it, vi } from 'vitest'
import { flushPromises, mount } from '@vue/test-utils'
import { i18n } from './i18n'
import SettingsPanel from './SettingsPanel.vue'

vi.mock('../bindings/app/core', () => ({
  ConfigManager: {
    GetSettings: vi.fn().mockResolvedValue({
      proxy: '',
      useQuota: false,
      saver: false,
      recursive: false,
      forceUpload: false,
      pairLivePhotos: false,
      skipIncompleteLivePhotos: true,
      updateExistingPhotosToLive: false,
      deleteFromHost: false,
      disableUnsupportedFilesFilter: false,
      setDateFromFilename: false,
      excludePattern: '',
      uploadThreads: 4,
      minimizeToTray: false,
      launchOnStartup: false,
      notifyOnComplete: true,
      useLocalHashCache: true,
      uploadSpeedLimitKBps: 0,
      watchFoldersEnabled: false,
      watchedFolders: [],
      scheduleEnabled: false,
      scheduleStartHour: 1,
      scheduleEndHour: 7,
      pauseOnBattery: false,
      dryRun: false,
      locale: 'en',
    }),
    GetHashCacheCount: vi.fn().mockResolvedValue(0),
    IsTrayAvailable: vi.fn().mockResolvedValue(true),
    HasBattery: vi.fn().mockResolvedValue(false),
    GetCredentials: vi.fn().mockResolvedValue({ credentials: [], selected: '' }),
  },
}))

describe('SettingsPanel', () => {
  it('mounts and renders all settings sections without runtime errors', async () => {
    const wrapper = mount(SettingsPanel, {
      global: {
        plugins: [i18n],
        stubs: {
          Teleport: true,
        },
      },
    })

    await flushPromises()

    expect(wrapper.find('h1').text()).toBeTruthy()
    expect(wrapper.find('#section-appearance').exists()).toBe(true)
    expect(wrapper.find('#section-uploads').exists()).toBe(true)
    expect(wrapper.find('#section-performance').exists()).toBe(true)
    expect(wrapper.find('#section-background').exists()).toBe(true)
    expect(wrapper.find('#section-danger').exists()).toBe(true)
    expect(wrapper.find('#exclude-pattern-input').exists()).toBe(true)
    expect(wrapper.text()).toContain('\\.tmp$|@eaDir')
    // Desktop PC (HasBattery = false) hides Pause on Battery Power
    expect(wrapper.find('#battery-pause-switch').exists()).toBe(false)

    wrapper.unmount()
  })
})
