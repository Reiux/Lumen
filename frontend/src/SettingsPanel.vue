<script setup lang="ts">
import { computed, nextTick, onMounted, onUnmounted, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { toast } from 'vue-sonner'
import { ConfigManager } from '../bindings/app/core'
import { setLocale } from './i18n'
import M3Switch from './components/ui/M3Switch.vue'
import M3Slider from './components/ui/M3Slider.vue'
import M3Button from './components/ui/M3Button.vue'
import M3ButtonGroup from './components/ui/M3ButtonGroup.vue'
import M3Icon from './components/ui/M3Icon.vue'
import M3TextField from './components/ui/M3TextField.vue'
import {
  THEME_PRESETS,
  PALETTE_VARIANTS,
  currentThemeSeed,
  currentPaletteVariant,
  setThemeSeed,
  setPaletteVariant,
  getPresetPreviewColors,
  isMatugenSupported,
  isMatugenActive,
  matugenColors,
  setMatugenEnabled,
} from './utils/theme'
import {
  preferencesState,
  setUseQuota,
  setSaver,
  setNotifyOnComplete,
  setAutoCheckUpdates,
} from './utils/configStore'
import { useDialogFocusTrap } from './utils/useDialogFocusTrap'

const { t, locale } = useI18n()

const _emit = defineEmits<{
  (e: 'uploadFolder', path: string): void
  (e: 'navigateTab', tab: 'folders'): void
}>()

interface Settings {
  proxy: string
  useQuota: boolean
  saver: boolean
  recursive: boolean
  forceUpload: boolean
  pairLivePhotos: boolean
  skipIncompleteLivePhotos: boolean
  updateExistingPhotosToLive: boolean
  deleteFromHost: boolean
  disableUnsupportedFilesFilter: boolean
  setDateFromFilename: boolean
  excludePattern: string
  uploadThreads: number
  minimizeToTray: boolean
  launchOnStartup: boolean
  notifyOnComplete: boolean
  useLocalHashCache: boolean
  uploadSpeedLimitKBps: number
  watchFoldersEnabled: boolean
  watchedFolders: string[]
  scheduleEnabled: boolean
  scheduleStartHour: number
  scheduleEndHour: number
  pauseOnBattery: boolean
  dryRun: boolean
}

const settings = ref<Settings>({
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
  uploadThreads: 1,
  minimizeToTray: false,
  launchOnStartup: false,
  notifyOnComplete: true,
  useLocalHashCache: false,
  uploadSpeedLimitKBps: 0,
  watchFoldersEnabled: false,
  watchedFolders: [],
  scheduleEnabled: false,
  scheduleStartHour: 1,
  scheduleEndHour: 7,
  pauseOnBattery: false,
  dryRun: false,
})

const isHydrating = ref(true)
const showDeleteConfirm = ref(false)
const deleteDialogCardRef = ref<HTMLElement | null>(null)
const languageDialogCardRef = ref<HTMLElement | null>(null)
const hashCacheCount = ref(0)
const isClearingCache = ref(false)
const isTrayAvailable = ref(true)
const hasBattery = ref(false)
const activeAccountEmail = ref('')
const proxyError = ref('')
const excludeError = ref('')
let proxySaveTimer: ReturnType<typeof setTimeout> | null = null
let excludeSaveTimer: ReturnType<typeof setTimeout> | null = null
let threadsSaveTimer: ReturnType<typeof setTimeout> | null = null
let speedLimitSaveTimer: ReturnType<typeof setTimeout> | null = null
let lastCommittedThreads = 4
let lastCommittedSpeedLimit = 0

// Sticky Navigation Sections matching Mockup A
const sections = computed(() => [
  { id: 'section-appearance', label: t('settings.appearance'), icon: 'palette' },
  { id: 'section-uploads', label: t('settings.backup'), icon: 'cloud_upload' },
  { id: 'section-performance', label: t('settings.performance'), icon: 'speed' },
  { id: 'section-background', label: t('settings.background'), icon: 'notifications' },
  { id: 'section-danger', label: t('settings.dangerZone'), icon: 'local_fire_department' },
])

const activeSectionId = ref<string>('section-appearance')
let isProgrammaticScroll = false
let scrollResetTimer: ReturnType<typeof setTimeout> | null = null
let scrollContainerEl: HTMLElement | null = null

function scrollToSection(id: string) {
  activeSectionId.value = id
  isProgrammaticScroll = true
  if (scrollResetTimer) clearTimeout(scrollResetTimer)

  const container = (scrollContainerEl || document.querySelector('[data-tab="settings"]')) as HTMLElement | null
  const el = document.getElementById(id)
  if (container && el) {
    const containerTop = container.getBoundingClientRect().top
    const elTop = el.getBoundingClientRect().top
    const offset = Math.max(0, elTop - containerTop + container.scrollTop - 16)
    container.scrollTo({ top: offset, behavior: 'smooth' })
  } else if (el) {
    el.scrollIntoView({ behavior: 'smooth', block: 'start' })
  }

  scrollResetTimer = setTimeout(() => {
    isProgrammaticScroll = false
  }, 850)
}

function handleScroll() {
  if (isProgrammaticScroll || !scrollContainerEl) return

  // If scrolled near the bottom, select the last section
  if (scrollContainerEl.scrollTop + scrollContainerEl.clientHeight >= scrollContainerEl.scrollHeight - 30) {
    activeSectionId.value = sections.value[sections.value.length - 1].id
    return
  }

  const containerTop = scrollContainerEl.getBoundingClientRect().top
  let currentId = sections.value[0].id
  for (const s of sections.value) {
    const el = document.getElementById(s.id)
    if (el) {
      const top = el.getBoundingClientRect().top - containerTop
      if (top <= 90) {
        currentId = s.id
      }
    }
  }
  activeSectionId.value = currentId
}

async function refreshActiveAccount() {
  try {
    const acc = await ConfigManager.GetAccounts()
    activeAccountEmail.value = acc.selected || ''
  } catch (e) {
    console.warn('Failed to load accounts in settings:', e)
  }
}

const SPEED_STEPS = [0, 512, 1024, 2048, 5120, 10240, 20480, 51200]

const speedStepIndex = computed({
  get: () => {
    const current = settings.value.uploadSpeedLimitKBps || 0
    let bestIdx = 0
    let minDiff = Infinity
    for (let i = 0; i < SPEED_STEPS.length; i++) {
      const diff = Math.abs(SPEED_STEPS[i] - current)
      if (diff < minDiff) {
        minDiff = diff
        bestIdx = i
      }
    }
    return bestIdx
  },
  set: (idx: number) => {
    const clamped = Math.max(0, Math.min(SPEED_STEPS.length - 1, Math.round(idx)))
    settings.value.uploadSpeedLimitKBps = SPEED_STEPS[clamped]
  },
})

const speedLimitDisplay = computed(() => {
  const kbps = settings.value.uploadSpeedLimitKBps
  if (!kbps || kbps <= 0) return t('settings.unlimited')
  if (kbps >= 1024) {
    const mb = (kbps / 1024).toFixed(1).replace(/\.0$/, '')
    return `${mb} MB/s`
  }
  return `${kbps} KB/s`
})

function onToggleDelete(targetValue: boolean) {
  if (targetValue) {
    showDeleteConfirm.value = true
  } else {
    settings.value.deleteFromHost = false
  }
}

function confirmDeleteFromHost() {
  settings.value.deleteFromHost = true
  showDeleteConfirm.value = false
}

function cancelDeleteFromHost() {
  settings.value.deleteFromHost = false
  showDeleteConfirm.value = false
}

useDialogFocusTrap(() => showDeleteConfirm.value, deleteDialogCardRef, {
  onEscape: cancelDeleteFromHost,
})

async function clearHashCache() {
  try {
    isClearingCache.value = true
    await ConfigManager.ClearHashCache()
    hashCacheCount.value = 0
  } catch (err) {
    console.error('Failed to clear hash cache:', err)
    toast.error(t('settings.saveFailed'), {
      description: err instanceof Error ? err.message : String(err),
    })
  } finally {
    isClearingCache.value = false
  }
}

let isUnmounted = false

onMounted(async () => {
  try {
    const [config, hashCount, trayAvail, batteryPresent] = await Promise.all([
      ConfigManager.GetSettings(),
      ConfigManager.GetHashCacheCount(),
      ConfigManager.IsTrayAvailable(),
      ConfigManager.HasBattery().catch(() => false),
      refreshActiveAccount(),
    ])
    if (isUnmounted) return
    hasBattery.value = !!batteryPresent
    settings.value = {
      proxy: config.proxy || '',
      useQuota: config.useQuota || false,
      saver: config.saver || false,
      recursive: config.recursive || false,
      forceUpload: config.forceUpload || false,
      pairLivePhotos: config.pairLivePhotos || false,
      skipIncompleteLivePhotos: config.skipIncompleteLivePhotos ?? true,
      updateExistingPhotosToLive: config.updateExistingPhotosToLive || false,
      deleteFromHost: config.deleteFromHost || false,
      disableUnsupportedFilesFilter: config.disableUnsupportedFilesFilter || false,
      setDateFromFilename: config.setDateFromFilename || false,
      excludePattern: config.excludePattern || '',
      uploadThreads: config.uploadThreads || 4,
      minimizeToTray: config.minimizeToTray || false,
      launchOnStartup: config.launchOnStartup || false,
      notifyOnComplete: config.notifyOnComplete ?? true,
      useLocalHashCache: config.useLocalHashCache ?? false,
      uploadSpeedLimitKBps: config.uploadSpeedLimitKBps || 0,
      watchFoldersEnabled: config.watchFoldersEnabled || false,
      watchedFolders: config.watchedFolders || [],
      scheduleEnabled: config.scheduleEnabled ?? false,
      scheduleStartHour: config.scheduleStartHour ?? 1,
      scheduleEndHour: config.scheduleEndHour ?? 7,
      pauseOnBattery: config.pauseOnBattery ?? false,
      dryRun: config.dryRun ?? false,
    }
    hashCacheCount.value = hashCount
    isTrayAvailable.value = trayAvail
    if (!isTrayAvailable.value) {
      settings.value.minimizeToTray = false
    }
    lastCommittedThreads = settings.value.uploadThreads
    lastCommittedSpeedLimit = settings.value.uploadSpeedLimitKBps
    preferencesState.useQuota = settings.value.useQuota
    preferencesState.saver = settings.value.saver
  } catch (err) {
    console.error('Failed to load settings:', err)
  } finally {
    await nextTick()
    if (!isUnmounted) {
      isHydrating.value = false
    }
  }

  if (isUnmounted) return

  // Setup scroll-spy inside the tab pane container
  scrollContainerEl = document.querySelector('[data-tab="settings"]') as HTMLElement | null
  if (scrollContainerEl) {
    scrollContainerEl.addEventListener('scroll', handleScroll, { passive: true })
  }
})

onUnmounted(() => {
  isUnmounted = true
  if (scrollContainerEl) {
    scrollContainerEl.removeEventListener('scroll', handleScroll)
    scrollContainerEl = null
  }
  if (scrollResetTimer) clearTimeout(scrollResetTimer)
  if (proxySaveTimer) {
    clearTimeout(proxySaveTimer)
    proxySaveTimer = null
    ConfigManager.SetProxy(settings.value.proxy).catch((err) => {
      toast.error(t('settings.saveFailed'), {
        description: err instanceof Error ? err.message : String(err),
      })
    })
  }
  if (excludeSaveTimer) {
    clearTimeout(excludeSaveTimer)
    excludeSaveTimer = null
    ConfigManager.SetExcludePattern(settings.value.excludePattern).catch((err) => {
      toast.error(t('settings.saveFailed'), {
        description: err instanceof Error ? err.message : String(err),
      })
    })
  }
  if (threadsSaveTimer) {
    clearTimeout(threadsSaveTimer)
    threadsSaveTimer = null
    ConfigManager.SetUploadThreads(settings.value.uploadThreads).catch((err) => {
      toast.error(t('settings.saveFailed'), {
        description: err instanceof Error ? err.message : String(err),
      })
    })
  }
  if (speedLimitSaveTimer) {
    clearTimeout(speedLimitSaveTimer)
    speedLimitSaveTimer = null
    ConfigManager.SetUploadSpeedLimitKBps(settings.value.uploadSpeedLimitKBps).catch((err) => {
      toast.error(t('settings.saveFailed'), {
        description: err instanceof Error ? err.message : String(err),
      })
    })
  }
})

const suppressedKeyDepth: Record<string, number> = {}
const saveRequestSeq: Record<string, number> = {}

function isKeySuppressed(key: string): boolean {
  return isHydrating.value || (suppressedKeyDepth[key] ?? 0) > 0
}

function runSuppressedFor(keys: string | string[], fn: () => void) {
  const keyList = Array.isArray(keys) ? keys : [keys]
  for (const k of keyList) {
    suppressedKeyDepth[k] = (suppressedKeyDepth[k] ?? 0) + 1
  }
  try {
    fn()
  } finally {
    nextTick(() => {
      for (const k of keyList) {
        const next = (suppressedKeyDepth[k] ?? 1) - 1
        if (next <= 0) {
          delete suppressedKeyDepth[k]
        } else {
          suppressedKeyDepth[k] = next
        }
      }
    })
  }
}

async function saveSetting(
  key: string,
  fn: () => Promise<unknown>,
  revert?: () => void,
  suppressKeys?: string | string[],
) {
  const seq = (saveRequestSeq[key] ?? 0) + 1
  saveRequestSeq[key] = seq
  try {
    await fn()
  } catch (err) {
    if (saveRequestSeq[key] !== seq) {
      return
    }
    if (revert) {
      runSuppressedFor(suppressKeys ?? key, revert)
    }
    console.error('Failed to save setting:', err)
    toast.error(t('settings.saveFailed'), {
      description: err instanceof Error ? err.message : String(err),
    })
  }
}

watch(() => settings.value.proxy, (newValue) => {
  if (isKeySuppressed('proxy')) return
  if (proxySaveTimer) clearTimeout(proxySaveTimer)
  const seq = (saveRequestSeq.proxy ?? 0) + 1
  saveRequestSeq.proxy = seq
  proxySaveTimer = setTimeout(async () => {
    proxySaveTimer = null
    try {
      await ConfigManager.SetProxy(newValue)
      if (saveRequestSeq.proxy === seq) {
        proxyError.value = ''
      }
    } catch (err) {
      if (saveRequestSeq.proxy === seq) {
        proxyError.value = err instanceof Error ? err.message : String(err)
      }
    }
  }, 400)
})

watch(() => settings.value.excludePattern, (newValue) => {
  if (isKeySuppressed('excludePattern')) return
  if (excludeSaveTimer) clearTimeout(excludeSaveTimer)
  const seq = (saveRequestSeq.excludePattern ?? 0) + 1
  saveRequestSeq.excludePattern = seq
  excludeSaveTimer = setTimeout(async () => {
    excludeSaveTimer = null
    try {
      await ConfigManager.SetExcludePattern(newValue)
      if (saveRequestSeq.excludePattern === seq) {
        excludeError.value = ''
      }
    } catch (err) {
      if (saveRequestSeq.excludePattern === seq) {
        excludeError.value = err instanceof Error ? err.message : String(err)
      }
    }
  }, 400)
})

watch(() => settings.value.useQuota, (newValue, oldValue) => {
  if (isKeySuppressed('useQuota')) return
  const prevSaver = settings.value.saver
  if (newValue && settings.value.saver) {
    runSuppressedFor('saver', () => {
      settings.value.saver = false
    })
  }
  saveSetting(
    'storageMode',
    async () => {
      if (newValue && prevSaver) {
        await setSaver(false)
      }
      await setUseQuota(newValue)
    },
    () => {
      settings.value.useQuota = oldValue
      settings.value.saver = prevSaver
    },
    ['useQuota', 'saver'],
  )
})

watch(() => settings.value.saver, (newValue, oldValue) => {
  if (isKeySuppressed('saver')) return
  const prevQuota = settings.value.useQuota
  if (newValue && settings.value.useQuota) {
    runSuppressedFor('useQuota', () => {
      settings.value.useQuota = false
    })
  }
  saveSetting(
    'storageMode',
    async () => {
      if (newValue && prevQuota) {
        await setUseQuota(false)
      }
      await setSaver(newValue)
    },
    () => {
      settings.value.saver = oldValue
      settings.value.useQuota = prevQuota
    },
    ['useQuota', 'saver'],
  )
})

watch(() => settings.value.recursive, (newValue, oldValue) => {
  if (isKeySuppressed('recursive')) return
  saveSetting(
    'recursive',
    () => ConfigManager.SetRecursive(newValue),
    () => { settings.value.recursive = oldValue },
  )
})

watch(() => settings.value.forceUpload, (newValue, oldValue) => {
  if (isKeySuppressed('forceUpload')) return
  saveSetting(
    'forceUpload',
    () => ConfigManager.SetForceUpload(newValue),
    () => { settings.value.forceUpload = oldValue },
  )
})

watch(() => settings.value.pairLivePhotos, (newValue, oldValue) => {
  if (isKeySuppressed('pairLivePhotos')) return
  const prevSkip = settings.value.skipIncompleteLivePhotos
  const autoEnableSkip = newValue && !prevSkip
  if (autoEnableSkip) {
    runSuppressedFor('skipIncompleteLivePhotos', () => {
      settings.value.skipIncompleteLivePhotos = true
    })
  }
  saveSetting(
    'livePhotos',
    async () => {
      await ConfigManager.SetPairLivePhotos(newValue)
      if (autoEnableSkip) {
        await ConfigManager.SetSkipIncompleteLivePhotos(true)
      }
    },
    () => {
      settings.value.pairLivePhotos = oldValue
      settings.value.skipIncompleteLivePhotos = prevSkip
    },
    ['pairLivePhotos', 'skipIncompleteLivePhotos'],
  )
})

watch(() => settings.value.skipIncompleteLivePhotos, (newValue, oldValue) => {
  if (isKeySuppressed('skipIncompleteLivePhotos')) return
  saveSetting(
    'livePhotos',
    () => ConfigManager.SetSkipIncompleteLivePhotos(newValue),
    () => { settings.value.skipIncompleteLivePhotos = oldValue },
    'skipIncompleteLivePhotos',
  )
})

watch(() => settings.value.updateExistingPhotosToLive, (newValue, oldValue) => {
  if (isKeySuppressed('updateExistingPhotosToLive')) return
  saveSetting(
    'updateExistingPhotosToLive',
    () => ConfigManager.SetUpdateExistingPhotosToLive(newValue),
    () => { settings.value.updateExistingPhotosToLive = oldValue },
  )
})

watch(() => settings.value.deleteFromHost, (newValue, oldValue) => {
  if (isKeySuppressed('deleteFromHost')) return
  saveSetting(
    'deleteFromHost',
    () => ConfigManager.SetDeleteFromHost(newValue),
    () => { settings.value.deleteFromHost = oldValue },
  )
})

watch(() => settings.value.disableUnsupportedFilesFilter, (newValue, oldValue) => {
  if (isKeySuppressed('disableUnsupportedFilesFilter')) return
  saveSetting(
    'disableUnsupportedFilesFilter',
    () => ConfigManager.SetDisableUnsupportedFilesFilter(newValue),
    () => { settings.value.disableUnsupportedFilesFilter = oldValue },
  )
})

watch(() => settings.value.setDateFromFilename, (newValue, oldValue) => {
  if (isKeySuppressed('setDateFromFilename')) return
  saveSetting(
    'setDateFromFilename',
    () => ConfigManager.SetSetDateFromFilename(newValue),
    () => { settings.value.setDateFromFilename = oldValue },
  )
})

watch(() => settings.value.uploadThreads, (newValue) => {
  if (isKeySuppressed('uploadThreads')) return
  if (newValue < 1) {
    settings.value.uploadThreads = 1
    return
  }
  if (threadsSaveTimer) clearTimeout(threadsSaveTimer)
  const rollbackValue = lastCommittedThreads
  threadsSaveTimer = setTimeout(() => {
    threadsSaveTimer = null
    void saveSetting(
      'uploadThreads',
      async () => {
        await ConfigManager.SetUploadThreads(newValue)
        lastCommittedThreads = newValue
      },
      () => { settings.value.uploadThreads = rollbackValue },
    )
  }, 200)
})

watch(() => settings.value.minimizeToTray, (newValue, oldValue) => {
  if (isKeySuppressed('minimizeToTray')) return
  saveSetting(
    'minimizeToTray',
    () => ConfigManager.SetMinimizeToTray(newValue),
    () => { settings.value.minimizeToTray = oldValue },
  )
})

watch(() => settings.value.launchOnStartup, (newValue, oldValue) => {
  if (isKeySuppressed('launchOnStartup')) return
  saveSetting(
    'launchOnStartup',
    () => ConfigManager.SetLaunchOnStartup(newValue),
    () => { settings.value.launchOnStartup = oldValue },
  )
})

watch(() => settings.value.notifyOnComplete, (newValue, oldValue) => {
  if (isKeySuppressed('notifyOnComplete')) return
  saveSetting(
    'notifyOnComplete',
    () => setNotifyOnComplete(newValue),
    () => { settings.value.notifyOnComplete = oldValue },
  )
})

watch(() => settings.value.useLocalHashCache, (newValue, oldValue) => {
  if (isKeySuppressed('useLocalHashCache')) return
  saveSetting(
    'useLocalHashCache',
    () => ConfigManager.SetUseLocalHashCache(newValue),
    () => { settings.value.useLocalHashCache = oldValue },
  )
})

watch(() => settings.value.uploadSpeedLimitKBps, (newValue) => {
  if (isKeySuppressed('uploadSpeedLimitKBps')) return
  if (speedLimitSaveTimer) clearTimeout(speedLimitSaveTimer)
  const rollbackValue = lastCommittedSpeedLimit
  speedLimitSaveTimer = setTimeout(() => {
    speedLimitSaveTimer = null
    void saveSetting(
      'uploadSpeedLimitKBps',
      async () => {
        await ConfigManager.SetUploadSpeedLimitKBps(newValue)
        lastCommittedSpeedLimit = newValue
      },
      () => { settings.value.uploadSpeedLimitKBps = rollbackValue },
    )
  }, 200)
})

watch(() => settings.value.watchFoldersEnabled, (newValue, oldValue) => {
  if (isKeySuppressed('watchFoldersEnabled')) return
  saveSetting(
    'watchFoldersEnabled',
    () => ConfigManager.SetWatchFoldersEnabled(newValue),
    () => { settings.value.watchFoldersEnabled = oldValue },
  )
})

const currentLanguage = computed(() => String(locale.value || 'en'))
const showLanguageDialog = ref(false)

useDialogFocusTrap(() => showLanguageDialog.value, languageDialogCardRef, {
  onEscape: () => {
    showLanguageDialog.value = false
  },
})

const availableLanguages = [
  { code: 'en', nativeName: 'English', englishName: 'English' },
  { code: 'es', nativeName: 'Español', englishName: 'Spanish' },
  { code: 'id', nativeName: 'Bahasa Indonesia', englishName: 'Indonesian' },
  { code: 'ja', nativeName: '日本語', englishName: 'Japanese' },
]

const currentLanguageLabel = computed(() => {
  const match = availableLanguages.find(l => l.code === currentLanguage.value)
  return match ? match.nativeName : 'English'
})

function selectLanguage(lang: string) {
  try {
    setLocale(lang)
  } catch (err) {
    console.error('Failed to change language:', err)
  }
  showLanguageDialog.value = false
}

const autoCheckUpdatesEnabled = computed(() => preferencesState.autoCheckUpdates)

async function handleAutoCheckUpdates(enabled: boolean) {
  try {
    await setAutoCheckUpdates(enabled)
  } catch {
    toast.error(t('settings.saveFailed'))
  }
}

watch(() => settings.value.scheduleEnabled, (newValue, oldValue) => {
  if (isKeySuppressed('scheduleEnabled')) return
  saveSetting(
    'scheduleEnabled',
    () => ConfigManager.SetScheduleEnabled(newValue),
    () => { settings.value.scheduleEnabled = oldValue },
  )
})

watch([() => settings.value.scheduleStartHour, () => settings.value.scheduleEndHour], ([start, end], [oldStart, oldEnd]) => {
  if (isKeySuppressed('scheduleStartHour') || isKeySuppressed('scheduleEndHour')) return
  saveSetting(
    'scheduleHours',
    () => ConfigManager.SetScheduleHours(Number(start), Number(end)),
    () => {
      settings.value.scheduleStartHour = oldStart
      settings.value.scheduleEndHour = oldEnd
    },
    ['scheduleStartHour', 'scheduleEndHour'],
  )
})

watch(() => settings.value.pauseOnBattery, (newValue, oldValue) => {
  if (isKeySuppressed('pauseOnBattery')) return
  saveSetting(
    'pauseOnBattery',
    () => ConfigManager.SetPauseOnBattery(newValue),
    () => { settings.value.pauseOnBattery = oldValue },
  )
})

watch(() => settings.value.dryRun, (newValue, oldValue) => {
  if (isKeySuppressed('dryRun')) return
  saveSetting(
    'dryRun',
    () => ConfigManager.SetDryRun(newValue),
    () => { settings.value.dryRun = oldValue },
  )
})
</script>

<template>
  <div class="flex flex-col gap-6 p-2 w-full select-none">
    <!-- Header with friendly, complete sentence -->
    <div class="flex flex-col gap-1">
      <h1 class="m3-headline-large text-[var(--md-sys-color-on-surface)]">
        {{ t('settings.title') }}
      </h1>
      <p class="m3-body-medium">
        {{ t('settings.subtitle') }}
      </p>
    </div>

    <!-- Main Container: Sticky Section List (Left) + Flat One-Column Container (Right, max-w-[880px]) -->
    <div class="flex items-start gap-8 w-full">
      <!-- Sticky section list (Mockup A) -->
      <nav
        :aria-label="t('settings.sectionNavAria')"
        class="hidden min-[1100px]:flex flex-col w-52 shrink-0 sticky top-2 gap-1 self-start"
      >
        <span class="m3-label-small text-[var(--md-sys-color-on-surface-variant)] px-3 mb-1">
          {{ t('settings.sections') }}
        </span>
        <M3Button
          v-for="sec in sections"
          :key="sec.id"
          size="s"
          :toggle="true"
          :selected="activeSectionId === sec.id"
          :variant="activeSectionId === sec.id ? 'tonal' : 'text'"
          :icon="sec.icon"
          class="!justify-start w-full"
          @click="scrollToSection(sec.id)"
        >
          <span class="truncate">{{ sec.label }}</span>
        </M3Button>
      </nav>

      <!-- Right Column: max-w-[880px], flat grouped sections with thin dividers -->
      <div class="flex flex-col gap-8 w-full max-w-[880px] min-w-0">
        <!-- 1. Section: Appearance -->
        <div id="section-appearance" class="m3-settings-section flex flex-col gap-2.5 scroll-mt-6">
          <div class="flex items-center gap-2 px-1 text-[var(--md-sys-color-primary)]">
            <M3Icon name="palette" :size="20" />
            <h2 class="m3-headline-small text-[var(--md-sys-color-on-surface)]">
              {{ t('settings.appearance') }}
            </h2>
          </div>

          <div class="rounded-[24px] bg-[var(--md-sys-color-surface-container)] border border-[var(--md-sys-color-outline-variant)]/40 divide-y divide-[var(--md-sys-color-outline-variant)]/20 overflow-hidden shadow-xs">
            <!-- Row: Palette Style (Connected Button Group) -->
            <div class="flex flex-col sm:flex-row sm:items-center justify-between gap-4 p-5">
              <div class="flex flex-col pr-2">
                <span class="m3-title-medium text-[var(--md-sys-color-on-surface)]">
                  {{ t('settings.paletteStyle') }}
                </span>
                <span class="m3-body-medium mt-0.5">
                  {{ t('settings.paletteStyleSubtitle') }}
                </span>
              </div>
              <M3ButtonGroup :connected="true" class="shrink-0 self-start sm:self-auto">
                <M3Button
                  v-for="variant in PALETTE_VARIANTS"
                  :key="variant.id"
                  size="xs"
                  :toggle="true"
                  :selected="currentPaletteVariant === variant.id"
                  :variant="currentPaletteVariant === variant.id ? 'tonal' : 'text'"
                  :icon="variant.icon"
                  @click="setPaletteVariant(variant.id)"
                >
                  <span>{{ variant.label }}</span>
                </M3Button>
              </M3ButtonGroup>
            </div>

            <!-- Row: Compact single row of 8 color swatches -->
            <div class="flex flex-col sm:flex-row sm:items-center justify-between gap-4 p-5">
              <div class="flex flex-col pr-2">
                <span class="m3-title-medium text-[var(--md-sys-color-on-surface)]">
                  {{ t('settings.themeColor') }}
                </span>
                <span class="m3-body-medium mt-0.5">
                  {{ t('settings.themeColorSubtitle') }}
                </span>
              </div>
              <div class="flex items-center gap-2 shrink-0 flex-wrap">
                <!-- Matugen Dynamic Swatch (Linux exclusive) -->
                <button
                  v-if="isMatugenSupported"
                  type="button"
                  role="radio"
                  :aria-checked="isMatugenActive"
                  :aria-label="t('settings.matugenThemeAria')"
                  :title="t('settings.matugenThemeTitle')"
                  class="no-state-layer relative flex size-9 items-center justify-center rounded-full p-0.5 focus:outline-none cursor-pointer transition-transform duration-150 hover:scale-105"
                  @click="setMatugenEnabled(true)"
                >
                  <div
                    class="relative flex size-8 items-center justify-center rounded-full overflow-hidden transition-all duration-200"
                    :class="isMatugenActive
                      ? 'ring-2 ring-offset-2 ring-offset-[var(--md-sys-color-surface-container)] ring-[var(--md-sys-color-primary)] shadow-sm'
                      : 'border border-[var(--md-sys-color-outline-variant)]/40 opacity-85 hover:opacity-100'"
                  >
                    <div class="absolute inset-0 grid grid-cols-2 grid-rows-2 -rotate-45 scale-125 pointer-events-none">
                      <div :style="{ backgroundColor: matugenColors?.primary || 'var(--md-sys-color-primary)' }" />
                      <div :style="{ backgroundColor: matugenColors?.primary_container || 'var(--md-sys-color-primary-container)' }" />
                      <div :style="{ backgroundColor: matugenColors?.tertiary || 'var(--md-sys-color-tertiary)' }" />
                      <div :style="{ backgroundColor: matugenColors?.surface || 'var(--md-sys-color-surface)' }" />
                    </div>
                    <div
                      v-if="isMatugenActive"
                      class="relative z-10 flex size-4 items-center justify-center rounded-full bg-[var(--md-sys-color-surface)] text-[var(--md-sys-color-primary)] shadow-xs"
                    >
                      <M3Icon name="auto_awesome" :size="12" />
                    </div>
                  </div>
                </button>

                <button
                  v-for="preset in THEME_PRESETS"
                  :key="preset.id"
                  type="button"
                  role="radio"
                  :aria-checked="!isMatugenActive && currentThemeSeed.toLowerCase() === preset.seed.toLowerCase()"
                  :aria-label="t('settings.presetThemeColor', { name: preset.name })"
                  :title="preset.name"
                  class="no-state-layer relative flex size-9 items-center justify-center rounded-full p-0.5 focus:outline-none cursor-pointer transition-transform duration-150 hover:scale-105"
                  @click="setThemeSeed(preset.seed)"
                >
                  <div
                    class="relative flex size-8 items-center justify-center rounded-full overflow-hidden transition-all duration-200"
                    :class="!isMatugenActive && currentThemeSeed.toLowerCase() === preset.seed.toLowerCase()
                      ? 'ring-2 ring-offset-2 ring-offset-[var(--md-sys-color-surface-container)] ring-[var(--md-sys-color-primary)] shadow-sm'
                      : 'border border-[var(--md-sys-color-outline-variant)]/40 opacity-85 hover:opacity-100'"
                  >
                    <div class="absolute inset-0 grid grid-cols-2 grid-rows-2 -rotate-45 scale-125 pointer-events-none">
                      <div :style="{ backgroundColor: getPresetPreviewColors(preset, currentPaletteVariant).primary }" />
                      <div :style="{ backgroundColor: getPresetPreviewColors(preset, currentPaletteVariant).container }" />
                      <div :style="{ backgroundColor: getPresetPreviewColors(preset, currentPaletteVariant).tertiary }" />
                      <div :style="{ backgroundColor: getPresetPreviewColors(preset, currentPaletteVariant).primary }" />
                    </div>
                    <div
                      v-if="!isMatugenActive && currentThemeSeed.toLowerCase() === preset.seed.toLowerCase()"
                      class="relative z-10 flex size-4 items-center justify-center rounded-full bg-[var(--md-sys-color-surface)] text-[var(--md-sys-color-primary)] shadow-xs"
                    >
                      <M3Icon name="check" :size="12" />
                    </div>
                  </div>
                </button>
              </div>
            </div>

            <!-- Row: Language Selector (M3 Selection Dialog Trigger) -->
            <div
              class="flex flex-col sm:flex-row sm:items-center justify-between gap-4 p-5 hover:bg-[var(--md-sys-color-surface-container-high)]/40 transition-colors cursor-pointer group"
              role="button"
              tabindex="0"
              :aria-label="t('settings.selectLanguage')"
              @click="showLanguageDialog = true"
              @keydown.enter="showLanguageDialog = true"
              @keydown.space.prevent="showLanguageDialog = true"
            >
              <div class="flex flex-col pr-2">
                <span class="m3-title-medium text-[var(--md-sys-color-on-surface)] group-hover:text-[var(--md-sys-color-primary)] transition-colors">
                  {{ t('settings.language') }}
                </span>
                <span class="m3-body-medium mt-0.5">
                  {{ t('settings.languageSubtitle') }}
                </span>
              </div>
              <div class="flex items-center gap-2 shrink-0 self-start sm:self-auto">
                <div class="inline-flex items-center gap-2.5 rounded-full bg-[var(--md-sys-color-surface-container-high)] border border-[var(--md-sys-color-outline-variant)]/40 px-3.5 py-1.5 transition-all group-hover:border-[var(--md-sys-color-primary)]/60 group-hover:bg-[var(--md-sys-color-surface-container-highest)] shadow-xs">
                  <M3Icon name="language" :size="18" class="text-[var(--md-sys-color-primary)]" />
                  <span class="m3-label-large text-[var(--md-sys-color-on-surface)] font-medium">
                    {{ currentLanguageLabel }}
                  </span>
                  <M3Icon name="chevron_right" :size="18" class="text-[var(--md-sys-color-on-surface-variant)] opacity-70 group-hover:opacity-100 group-hover:translate-x-0.5 transition-all" />
                </div>
              </div>
            </div>
          </div>
        </div>

        <!-- 2. Section: Backup -->
        <div id="section-uploads" class="m3-settings-section flex flex-col gap-2.5 scroll-mt-6">
          <div class="flex items-center gap-2 px-1 text-[var(--md-sys-color-primary)]">
            <M3Icon name="cloud_upload" :size="20" />
            <h2 class="m3-headline-small text-[var(--md-sys-color-on-surface)]">
              {{ t('settings.backup') }}
            </h2>
          </div>

          <div class="rounded-[24px] bg-[var(--md-sys-color-surface-container)] border border-[var(--md-sys-color-outline-variant)]/40 divide-y divide-[var(--md-sys-color-outline-variant)]/20 overflow-hidden shadow-xs">
            <!-- Use Quota -->
            <div class="flex items-center justify-between gap-4 p-5">
              <div class="flex flex-col pr-2">
                <label
                  for="use-quota-switch"
                  class="m3-title-medium text-[var(--md-sys-color-on-surface)] cursor-pointer"
                >
                  {{ t('settings.useQuota') }}
                </label>
                <span class="m3-body-medium mt-0.5">
                  {{ t('settings.useQuotaSubtitle') }}
                </span>
              </div>
              <M3Switch
                id="use-quota-switch"
                v-model="settings.useQuota"
                :aria-label="t('settings.useQuota')"
              />
            </div>

            <!-- Storage Saver Quality -->
            <div class="flex items-center justify-between gap-4 p-5">
              <div class="flex flex-col pr-2">
                <label
                  for="saver-mode-switch"
                  class="m3-title-medium text-[var(--md-sys-color-on-surface)] cursor-pointer"
                >
                  {{ t('settings.saver') }}
                </label>
                <span class="m3-body-medium mt-0.5">
                  {{ t('settings.saverSubtitle') }}
                </span>
              </div>
              <M3Switch
                id="saver-mode-switch"
                v-model="settings.saver"
                :aria-label="t('settings.saver')"
              />
            </div>

            <!-- Local Deduplication Cache -->
            <div class="flex flex-col gap-2.5 p-5">
              <div class="flex items-center justify-between gap-4">
                <div class="flex flex-col pr-2">
                  <label
                    for="local-hash-cache-switch"
                    class="m3-title-medium text-[var(--md-sys-color-on-surface)] cursor-pointer"
                  >
                    {{ t('settings.localHashCache') }}
                  </label>
                  <span class="m3-body-medium mt-0.5">
                    {{ t('settings.localHashCacheSubtitle') }}
                  </span>
                </div>
                <M3Switch
                  id="local-hash-cache-switch"
                  v-model="settings.useLocalHashCache"
                  :aria-label="t('settings.localHashCache')"
                />
              </div>

              <!-- Cache count & Clear button -->
              <div
                v-if="settings.useLocalHashCache"
                class="flex items-center justify-between rounded-xl bg-[var(--md-sys-color-surface-container-high)]/70 px-3.5 py-2 border border-[var(--md-sys-color-outline-variant)]/30 mt-1"
              >
                <span class="m3-label-large text-[var(--md-sys-color-on-surface)]">
                  {{ t('settings.filesCached', { count: hashCacheCount.toLocaleString() }) }}
                </span>
                <M3Button
                  variant="outlined"
                  destructive
                  size="xs"
                  :disabled="isClearingCache || hashCacheCount === 0"
                  @click="clearHashCache"
                >
                  <span>{{ isClearingCache ? t('settings.clearing') : t('settings.clearCache') }}</span>
                </M3Button>
              </div>
            </div>

            <!-- Review before upload (formerly Dry Run Mode) -->
            <div class="flex items-center justify-between gap-4 p-5">
              <div class="flex flex-col pr-2">
                <label
                  for="dry-run-switch"
                  class="m3-title-medium text-[var(--md-sys-color-on-surface)] cursor-pointer"
                >
                  {{ t('settings.reviewBeforeUpload') }}
                </label>
                <span class="m3-body-medium mt-0.5">
                  {{ t('settings.reviewBeforeUploadSubtitle') }}
                </span>
              </div>
              <M3Switch
                id="dry-run-switch"
                v-model="settings.dryRun"
                :aria-label="t('settings.reviewBeforeUpload')"
              />
            </div>

            <!-- Force Upload -->
            <div class="flex items-center justify-between gap-4 p-5">
              <div class="flex flex-col pr-2">
                <label
                  for="force-upload-switch"
                  class="m3-title-medium text-[var(--md-sys-color-on-surface)] cursor-pointer"
                >
                  {{ t('settings.forceBackup') }}
                </label>
                <span class="m3-body-medium mt-0.5">
                  {{ t('settings.forceBackupSubtitle') }}
                </span>
              </div>
              <M3Switch
                id="force-upload-switch"
                v-model="settings.forceUpload"
                :aria-label="t('settings.forceBackup')"
              />
            </div>

            <!-- Try Unsupported Formats -->
            <div class="flex items-center justify-between gap-4 p-5">
              <div class="flex flex-col pr-2">
                <label
                  for="filter-unsupported-switch"
                  class="m3-title-medium text-[var(--md-sys-color-on-surface)] cursor-pointer"
                >
                  {{ t('settings.tryUnsupported') }}
                </label>
                <span class="m3-body-medium mt-0.5">
                  {{ t('settings.tryUnsupportedSubtitle') }}
                </span>
              </div>
              <M3Switch
                id="filter-unsupported-switch"
                v-model="settings.disableUnsupportedFilesFilter"
                :aria-label="t('settings.tryUnsupported')"
              />
            </div>

            <!-- Pair Apple Live Photos -->
            <div class="flex flex-col gap-2.5 p-5">
              <div class="flex items-center justify-between gap-4">
                <div class="flex flex-col pr-2">
                  <label
                    for="pair-live-photos-switch"
                    class="m3-title-medium text-[var(--md-sys-color-on-surface)] cursor-pointer"
                  >
                    {{ t('settings.pairLivePhotos') }}
                  </label>
                  <span class="m3-body-medium mt-0.5">
                    {{ t('settings.pairLivePhotosSubtitle') }}
                  </span>
                </div>
                <M3Switch
                  id="pair-live-photos-switch"
                  v-model="settings.pairLivePhotos"
                  :aria-label="t('settings.pairLivePhotos')"
                />
              </div>

              <!-- Sub-options when enabled -->
              <div
                v-if="settings.pairLivePhotos"
                class="flex flex-col gap-2 rounded-xl bg-[var(--md-sys-color-surface-container-high)]/70 p-3 border border-[var(--md-sys-color-outline-variant)]/30 mt-1"
              >
                <div class="flex items-center justify-between gap-3">
                  <span class="m3-body-large text-[var(--md-sys-color-on-surface)]">
                    {{ t('settings.skipIncomplete') }}
                  </span>
                  <M3Switch
                    v-model="settings.skipIncompleteLivePhotos"
                    :aria-label="t('settings.skipIncomplete')"
                  />
                </div>
                <div class="flex items-center justify-between gap-3">
                  <span class="m3-body-large text-[var(--md-sys-color-on-surface)]">
                    {{ t('settings.updateExistingPhotos') }}
                  </span>
                  <M3Switch
                    v-model="settings.updateExistingPhotosToLive"
                    :aria-label="t('settings.updateExistingPhotos')"
                  />
                </div>
              </div>
            </div>

            <!-- Date from Filename -->
            <div class="flex items-center justify-between gap-4 p-5">
              <div class="flex flex-col pr-2">
                <label
                  for="set-date-switch"
                  class="m3-title-medium text-[var(--md-sys-color-on-surface)] cursor-pointer"
                >
                  {{ t('settings.dateFromFilename') }}
                </label>
                <span class="m3-body-medium mt-0.5">
                  {{ t('settings.dateFromFilenameSubtitle') }}
                </span>
              </div>
              <M3Switch
                id="set-date-switch"
                v-model="settings.setDateFromFilename"
                :aria-label="t('settings.dateFromFilename')"
              />
            </div>

            <!-- Exclude Pattern -->
            <div class="flex flex-col sm:flex-row sm:items-center justify-between gap-4 p-5">
              <div class="flex flex-col pr-2">
                <label for="exclude-pattern-input" class="m3-title-medium text-[var(--md-sys-color-on-surface)]">
                  {{ t('settings.excludePattern') }}
                </label>
                <span class="m3-body-medium mt-0.5">
                  {{ t('settings.excludePatternSubtitle') }}
                </span>
              </div>
              <div class="w-64 shrink-0 self-start sm:self-auto">
                <M3TextField
                  id="exclude-pattern-input"
                  v-model="settings.excludePattern"
                  placeholder="\.tmp$|@eaDir"
                  variant="outlined"
                  leading-icon="filter_alt_off"
                  :error="!!excludeError"
                  :error-text="excludeError"
                />
              </div>
            </div>
          </div>
        </div>

        <!-- 3. Section: Performance -->
        <div id="section-performance" class="m3-settings-section flex flex-col gap-2.5 scroll-mt-6">
          <div class="flex items-center gap-2 px-1 text-[var(--md-sys-color-primary)]">
            <M3Icon name="speed" :size="20" />
            <h2 class="m3-headline-small text-[var(--md-sys-color-on-surface)]">
              {{ t('settings.performance') }}
            </h2>
          </div>

          <div class="rounded-[24px] bg-[var(--md-sys-color-surface-container)] border border-[var(--md-sys-color-outline-variant)]/40 divide-y divide-[var(--md-sys-color-outline-variant)]/20 overflow-hidden shadow-xs">
            <!-- Upload Threads (Compact slider, value label on right, no dot rows) -->
            <div class="flex flex-col sm:flex-row sm:items-center justify-between gap-4 p-5">
              <div class="flex flex-col pr-2">
                <span class="m3-title-medium text-[var(--md-sys-color-on-surface)]">
                  {{ t('settings.backupThreads') }}
                </span>
                <span class="m3-body-medium mt-0.5">
                  {{ t('settings.backupThreadsSubtitle') }}
                </span>
              </div>
              <div class="flex items-center gap-3 w-64 shrink-0 self-start sm:self-auto">
                <M3Slider
                  v-model="settings.uploadThreads"
                  :min="1"
                  :max="20"
                  :stops="false"
                  :labeled="false"
                  :aria-label="t('settings.backupThreads')"
                  class="flex-1"
                />
                <span class="shrink-0 m3-label-large text-[var(--md-sys-color-primary)] w-24 text-right">
                  {{ settings.uploadThreads }} {{ settings.uploadThreads === 1 ? t('settings.worker') : t('settings.workers') }}
                </span>
              </div>
            </div>

            <!-- Bandwidth Limit (Compact slider, value label on right, no dot rows) -->
            <div class="flex flex-col sm:flex-row sm:items-center justify-between gap-4 p-5">
              <div class="flex flex-col pr-2">
                <span class="m3-title-medium text-[var(--md-sys-color-on-surface)]">
                  {{ t('settings.speedLimit') }}
                </span>
                <span class="m3-body-medium mt-0.5">
                  {{ t('settings.speedLimitSubtitle') }}
                </span>
              </div>
              <div class="flex items-center gap-3 w-64 shrink-0 self-start sm:self-auto">
                <M3Slider
                  v-model="speedStepIndex"
                  :min="0"
                  :max="SPEED_STEPS.length - 1"
                  :step="1"
                  :stops="false"
                  :labeled="false"
                  :aria-label="t('settings.speedLimit')"
                  class="flex-1"
                />
                <span class="shrink-0 m3-label-large text-[var(--md-sys-color-primary)] w-24 text-right">
                  {{ speedLimitDisplay }}
                </span>
              </div>
            </div>

            <!-- Proxy URL -->
            <div class="flex flex-col sm:flex-row sm:items-center justify-between gap-4 p-5">
              <div class="flex flex-col pr-2">
                <label for="proxy-input" class="m3-title-medium text-[var(--md-sys-color-on-surface)]">
                  {{ t('settings.proxyUrl') }}
                </label>
                <span class="m3-body-medium mt-0.5">
                  {{ t('settings.proxyUrlSubtitle') }}
                </span>
              </div>
              <div class="w-64 shrink-0 self-start sm:self-auto">
                <M3TextField
                  id="proxy-input"
                  v-model="settings.proxy"
                  placeholder="http://127.0.0.1:7890"
                  variant="outlined"
                  leading-icon="vpn_lock"
                  :error="!!proxyError"
                  :error-text="proxyError"
                />
              </div>
            </div>
          </div>
        </div>

        <!-- 4. Section: Background -->
        <div id="section-background" class="m3-settings-section flex flex-col gap-2.5 scroll-mt-6">
          <div class="flex items-center gap-2 px-1 text-[var(--md-sys-color-primary)]">
            <M3Icon name="notifications" :size="20" />
            <h2 class="m3-headline-small text-[var(--md-sys-color-on-surface)]">
              {{ t('settings.background') }}
            </h2>
          </div>

          <div class="rounded-[24px] bg-[var(--md-sys-color-surface-container)] border border-[var(--md-sys-color-outline-variant)]/40 divide-y divide-[var(--md-sys-color-outline-variant)]/20 overflow-hidden shadow-xs">
            <!-- Minimize to Tray -->
            <div class="flex items-center justify-between gap-4 p-5">
              <div class="flex flex-col pr-2">
                <label
                  for="tray-minimize-switch"
                  class="m3-title-medium text-[var(--md-sys-color-on-surface)]"
                  :class="{ 'opacity-50 cursor-not-allowed': !isTrayAvailable, 'cursor-pointer': isTrayAvailable }"
                >
                  {{ t('settings.minimizeToTray') }}
                </label>
                <span class="m3-body-medium mt-0.5">
                  {{ isTrayAvailable ? t('settings.minimizeToTraySubtitle') : t('settings.trayNotSupported') }}
                </span>
              </div>
              <M3Switch
                id="tray-minimize-switch"
                v-model="settings.minimizeToTray"
                :disabled="!isTrayAvailable"
                :aria-label="t('settings.minimizeToTray')"
              />
            </div>

            <!-- Launch on Startup -->
            <div class="flex items-center justify-between gap-4 p-5">
              <div class="flex flex-col pr-2">
                <label
                  for="startup-switch"
                  class="m3-title-medium text-[var(--md-sys-color-on-surface)] cursor-pointer"
                >
                  {{ t('settings.launchOnStartup') }}
                </label>
                <span class="m3-body-medium mt-0.5">
                  {{ t('settings.launchOnStartupSubtitle') }}
                </span>
              </div>
              <M3Switch
                id="startup-switch"
                v-model="settings.launchOnStartup"
                :aria-label="t('settings.launchOnStartup')"
              />
            </div>

            <!-- Desktop Notifications -->
            <div class="flex items-center justify-between gap-4 p-5">
              <div class="flex flex-col pr-2">
                <label
                  for="desktop-notifications-switch"
                  class="m3-title-medium text-[var(--md-sys-color-on-surface)] cursor-pointer"
                >
                  {{ t('settings.desktopNotifications') }}
                </label>
                <span class="m3-body-medium mt-0.5">
                  {{ t('settings.desktopNotificationsSubtitle') }}
                </span>
              </div>
              <M3Switch
                id="desktop-notifications-switch"
                v-model="settings.notifyOnComplete"
                :aria-label="t('settings.desktopNotifications')"
              />
            </div>

            <!-- Automatic Update Checks -->
            <div class="flex items-center justify-between gap-4 p-5">
              <div class="flex flex-col pr-2">
                <label
                  for="settings-auto-updates-switch"
                  class="m3-title-medium text-[var(--md-sys-color-on-surface)] cursor-pointer"
                >
                  {{ t('settings.autoCheckUpdates') }}
                </label>
                <span class="m3-body-medium mt-0.5">
                  {{ t('settings.autoCheckUpdatesSubtitle') }}
                </span>
              </div>
              <M3Switch
                id="settings-auto-updates-switch"
                :model-value="autoCheckUpdatesEnabled"
                :aria-label="t('settings.autoCheckUpdates')"
                @update:model-value="handleAutoCheckUpdates"
              />
            </div>

            <!-- Upload schedule / Quiet hours -->
            <div class="flex flex-col gap-2.5 p-5">
              <div class="flex items-center justify-between gap-4">
                <div class="flex flex-col pr-2">
                  <label
                    for="schedule-switch"
                    class="m3-title-medium text-[var(--md-sys-color-on-surface)] cursor-pointer"
                  >
                    {{ t('settings.quietHoursSchedule') }}
                  </label>
                  <span class="m3-body-medium mt-0.5">
                    {{ t('settings.quietHoursScheduleSubtitle') }}
                  </span>
                </div>
                <M3Switch
                  id="schedule-switch"
                  v-model="settings.scheduleEnabled"
                  :aria-label="t('settings.quietHoursSchedule')"
                />
              </div>

              <!-- Time window range when enabled -->
              <div
                v-if="settings.scheduleEnabled"
                class="flex flex-col sm:flex-row items-center gap-3 rounded-xl bg-[var(--md-sys-color-surface-container-high)]/70 p-3 border border-[var(--md-sys-color-outline-variant)]/30 mt-1"
              >
                <div class="flex items-center gap-2">
                  <span class="m3-body-medium text-[var(--md-sys-color-on-surface)]">{{ t('settings.from') }}</span>
                  <div class="relative inline-flex items-center shrink-0">
                    <select
                      v-model.number="settings.scheduleStartHour"
                      class="appearance-none rounded-full bg-[var(--md-sys-color-surface-container-high)] pl-3.5 pr-8 py-1.5 m3-label-large text-[var(--md-sys-color-on-surface)] border border-[var(--md-sys-color-outline-variant)]/40 hover:border-[var(--md-sys-color-on-surface)]/60 focus:outline-none focus:border-[var(--md-sys-color-primary)] focus:ring-1 focus:ring-[var(--md-sys-color-primary)] cursor-pointer transition-colors shadow-xs"
                    >
                      <option
                        v-for="h in 24"
                        :key="h - 1"
                        :value="h - 1"
                        class="bg-[var(--md-sys-color-surface-container-high)] text-[var(--md-sys-color-on-surface)]"
                      >
                        {{ String(h - 1).padStart(2, '0') }}:00
                      </option>
                    </select>
                    <M3Icon
                      name="arrow_drop_down"
                      :size="18"
                      class="pointer-events-none absolute right-2 text-[var(--md-sys-color-on-surface-variant)]"
                    />
                  </div>
                </div>
                <div class="flex items-center gap-2">
                  <span class="m3-body-medium text-[var(--md-sys-color-on-surface)]">{{ t('settings.to') }}</span>
                  <div class="relative inline-flex items-center shrink-0">
                    <select
                      v-model.number="settings.scheduleEndHour"
                      class="appearance-none rounded-full bg-[var(--md-sys-color-surface-container-high)] pl-3.5 pr-8 py-1.5 m3-label-large text-[var(--md-sys-color-on-surface)] border border-[var(--md-sys-color-outline-variant)]/40 hover:border-[var(--md-sys-color-on-surface)]/60 focus:outline-none focus:border-[var(--md-sys-color-primary)] focus:ring-1 focus:ring-[var(--md-sys-color-primary)] cursor-pointer transition-colors shadow-xs"
                    >
                      <option
                        v-for="h in 24"
                        :key="h - 1"
                        :value="h - 1"
                        class="bg-[var(--md-sys-color-surface-container-high)] text-[var(--md-sys-color-on-surface)]"
                      >
                        {{ String(h - 1).padStart(2, '0') }}:00
                      </option>
                    </select>
                    <M3Icon
                      name="arrow_drop_down"
                      :size="18"
                      class="pointer-events-none absolute right-2 text-[var(--md-sys-color-on-surface-variant)]"
                    />
                  </div>
                </div>
                <span class="m3-body-small text-[var(--md-sys-color-outline)] sm:ml-auto">
                  {{ t('settings.pauseOutside') }}
                </span>
              </div>
            </div>

            <!-- Pause on Battery Power (shown only on laptops / battery-equipped devices) -->
            <div
              v-if="hasBattery"
              class="flex items-center justify-between gap-4 p-5"
            >
              <div class="flex flex-col pr-2">
                <label
                  for="battery-pause-switch"
                  class="m3-title-medium text-[var(--md-sys-color-on-surface)] cursor-pointer"
                >
                  {{ t('settings.pauseOnBattery') }}
                </label>
                <span class="m3-body-medium mt-0.5">
                  {{ t('settings.pauseOnBatterySubtitle') }}
                </span>
              </div>
              <M3Switch
                id="battery-pause-switch"
                v-model="settings.pauseOnBattery"
                :aria-label="t('settings.pauseOnBattery')"
              />
            </div>
          </div>
        </div>

        <!-- 5. Section: Danger zone -->
        <div id="section-danger" class="m3-settings-section flex flex-col gap-2.5 scroll-mt-6">
          <div class="flex items-center gap-2 px-1 text-[var(--md-sys-color-error)]">
            <M3Icon name="local_fire_department" :size="20" />
            <h2 class="m3-headline-small text-[var(--md-sys-color-error)]">
              {{ t('settings.dangerZone') }}
            </h2>
          </div>

          <div class="rounded-[24px] bg-[var(--md-sys-color-surface-container)] border border-[var(--md-sys-color-error)]/30 divide-y divide-[var(--md-sys-color-outline-variant)]/20 overflow-hidden shadow-xs">
            <div class="flex items-center justify-between gap-4 p-5">
              <div class="flex flex-col pr-2">
                <label
                  for="delete-host-switch"
                  class="m3-title-medium text-[var(--md-sys-color-on-surface)] cursor-pointer"
                >
                  {{ t('settings.deleteFromHost') }}
                </label>
                <span class="m3-body-medium mt-0.5">
                  {{ t('settings.deleteFromHostSubtitle') }}
                </span>
              </div>
              <M3Switch
                id="delete-host-switch"
                :model-value="settings.deleteFromHost"
                variant="danger"
                :aria-label="t('settings.deleteFromHost')"
                @update:model-value="onToggleDelete"
              />
            </div>
          </div>
        </div>
      </div>
    </div>

    <!-- Delete From Host Warning Modal Dialog -->
    <Teleport to="body">
      <Transition name="m3-dialog-modal">
        <div
          v-if="showDeleteConfirm"
          class="m3-dialog-wrapper fixed inset-0 z-50 grid place-items-center p-4 select-none overflow-y-auto"
          style="--wails-draggable: none"
          @click.self="cancelDeleteFromHost"
        >
          <div
            ref="deleteDialogCardRef"
            tabindex="-1"
            class="m3-dialog-card relative flex w-full max-w-[440px] flex-col gap-5 rounded-[28px] bg-[var(--md-sys-color-surface-container-high)] p-6 shadow-2xl outline-none focus-visible:outline-[3px] focus-visible:outline-[var(--md-sys-color-primary)]"
            role="alertdialog"
            aria-modal="true"
            aria-labelledby="delete-confirm-title"
            aria-describedby="delete-confirm-desc"
          >
            <!-- M3E Centered Hero Icon + Headline -->
            <div class="flex flex-col items-center text-center gap-3">
              <div class="flex size-14 items-center justify-center rounded-[20px] bg-[var(--md-sys-color-error-container)] text-[var(--md-sys-color-on-error-container)]">
                <M3Icon
                  name="delete_forever"
                  :size="28"
                  :filled="true"
                />
              </div>
              <div class="flex flex-col gap-1.5">
                <h3
                  id="delete-confirm-title"
                  class="m3-headline-small text-[var(--md-sys-color-on-surface)]"
                >
                  {{ t('settings.deleteConfirmTitle') }}
                </h3>
                <p
                  id="delete-confirm-desc"
                  class="m3-body-medium leading-relaxed text-[var(--md-sys-color-on-surface-variant)]"
                >
                  {{ t('settings.deleteConfirmP1') }}
                </p>
              </div>
            </div>

            <!-- Tonal Error Warning Callout -->
            <div class="flex items-start gap-3 rounded-[16px] bg-[var(--md-sys-color-error-container)] px-4 py-3.5 text-[var(--md-sys-color-on-error-container)]">
              <M3Icon
                name="warning"
                :filled="true"
                :size="20"
                class="shrink-0 mt-0.5"
              />
              <p class="m3-body-medium leading-snug !text-[var(--md-sys-color-on-error-container)]">
                {{ t('settings.deleteConfirmP2') }}
              </p>
            </div>

            <!-- M3E Action Row -->
            <div class="flex items-center justify-end gap-2 pt-1">
              <M3Button
                variant="text"
                size="s"
                @click="cancelDeleteFromHost"
              >
                <span>{{ t('settings.cancel') }}</span>
              </M3Button>
              <M3Button
                variant="filled"
                destructive
                size="m"
                icon="delete_forever"
                @click="confirmDeleteFromHost"
              >
                <span>{{ t('settings.enableDeletion') }}</span>
              </M3Button>
            </div>
          </div>
        </div>
      </Transition>
    </Teleport>

    <!-- Language Selection Modal Dialog (M3E Selection Dialog with Grouped Segmented List) -->
    <Teleport to="body">
      <Transition name="m3-dialog-modal">
        <div
          v-if="showLanguageDialog"
          class="m3-dialog-wrapper fixed inset-0 z-50 grid place-items-center p-4 select-none overflow-y-auto"
          style="--wails-draggable: none"
          @click.self="showLanguageDialog = false"
        >
          <div
            ref="languageDialogCardRef"
            tabindex="-1"
            class="m3-dialog-card relative flex w-full max-w-[400px] flex-col gap-5 rounded-[28px] bg-[var(--md-sys-color-surface-container-high)] p-6 shadow-2xl outline-none focus-visible:outline-[3px] focus-visible:outline-[var(--md-sys-color-primary)]"
            role="dialog"
            aria-modal="true"
            aria-labelledby="language-dialog-title"
            aria-describedby="language-dialog-desc"
          >
            <!-- M3E Centered Hero Icon + Headline -->
            <div class="flex flex-col items-center text-center gap-3">
              <div class="flex size-14 items-center justify-center rounded-[20px] bg-[var(--md-sys-color-primary-container)] text-[var(--md-sys-color-on-primary-container)]">
                <M3Icon
                  name="language"
                  :filled="true"
                  :size="28"
                />
              </div>
              <div class="flex flex-col gap-1">
                <h3
                  id="language-dialog-title"
                  class="m3-headline-small text-[var(--md-sys-color-on-surface)]"
                >
                  {{ t('settings.selectLanguage') }}
                </h3>
                <span
                  id="language-dialog-desc"
                  class="m3-body-medium text-[var(--md-sys-color-on-surface-variant)] leading-snug"
                >
                  {{ t('settings.selectLanguageSubtitle') }}
                </span>
              </div>
            </div>

            <!-- M3E Grouped Segmented Language List (4px gaps, 20px outer / 4px inner corners) -->
            <div
              role="radiogroup"
              :aria-label="t('settings.selectLanguage')"
              class="flex flex-col gap-1 max-h-72 overflow-y-auto pr-0.5"
            >
              <button
                v-for="lang in availableLanguages"
                :key="lang.code"
                type="button"
                role="radio"
                :aria-checked="currentLanguage === lang.code"
                class="flex items-center justify-between gap-3 rounded-[4px] first:rounded-t-[20px] last:rounded-b-[20px] px-4 py-3.5 text-left transition-colors cursor-pointer outline-none focus-visible:outline-[3px] focus-visible:outline-[var(--md-sys-color-primary)] focus-visible:outline-offset-[-3px]"
                :class="currentLanguage === lang.code
                  ? 'bg-[var(--md-sys-color-primary-container)] text-[var(--md-sys-color-on-primary-container)]'
                  : 'bg-[var(--md-sys-color-surface-container)] text-[var(--md-sys-color-on-surface)] hover:bg-[var(--md-sys-color-surface-container-highest)]'"
                @click="selectLanguage(lang.code)"
              >
                <div class="flex flex-col min-w-0">
                  <span
                    class="m3-title-medium leading-tight"
                    :class="currentLanguage === lang.code ? 'text-[var(--md-sys-color-on-primary-container)]' : 'text-[var(--md-sys-color-on-surface)]'"
                  >
                    {{ lang.nativeName }}
                  </span>
                  <span
                    v-if="lang.englishName !== lang.nativeName"
                    class="m3-body-small mt-0.5"
                    :class="currentLanguage === lang.code ? '!text-[var(--md-sys-color-on-primary-container)]/80' : 'text-[var(--md-sys-color-on-surface-variant)]'"
                  >
                    {{ lang.englishName }}
                  </span>
                </div>

                <!-- Selection Indicator -->
                <div
                  class="flex size-6 shrink-0 items-center justify-center rounded-full transition-colors"
                  :class="currentLanguage === lang.code
                    ? 'bg-[var(--md-sys-color-primary)] text-[var(--md-sys-color-on-primary)]'
                    : 'border-2 border-[var(--md-sys-color-outline)] bg-transparent'"
                >
                  <M3Icon
                    v-if="currentLanguage === lang.code"
                    name="check"
                    :size="15"
                  />
                </div>
              </button>
            </div>

            <!-- Dialog Actions -->
            <div class="flex items-center justify-end gap-2 pt-1">
              <M3Button
                variant="text"
                size="s"
                @click="showLanguageDialog = false"
              >
                <span>{{ t('settings.cancel') }}</span>
              </M3Button>
            </div>
          </div>
        </div>
      </Transition>
    </Teleport>
  </div>
</template>
