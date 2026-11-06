<script setup lang="ts">
import { computed, nextTick, onMounted, onUnmounted, ref, watch } from 'vue'
import { ConfigManager } from '../bindings/app/core'
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
} from './utils/theme'
import { preferencesState, setUseQuota, setSaver } from './utils/configStore'

const emit = defineEmits<{
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
  uploadThreads: number
  minimizeToTray: boolean
  notifyOnComplete: boolean
  useLocalHashCache: boolean
  uploadSpeedLimitKBps: number
  watchFoldersEnabled: boolean
  watchedFolders: string[]
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
  uploadThreads: 1,
  minimizeToTray: false,
  notifyOnComplete: true,
  useLocalHashCache: false,
  uploadSpeedLimitKBps: 0,
  watchFoldersEnabled: false,
  watchedFolders: [],
})

const isHydrating = ref(true)
const showDeleteConfirm = ref(false)
const hashCacheCount = ref(0)
const isClearingCache = ref(false)
const isTrayAvailable = ref(true)
const activeAccountEmail = ref('')

// Sticky Navigation Sections matching Mockup A
const sections = [
  { id: 'section-appearance', label: 'Appearance', icon: 'palette' },
  { id: 'section-uploads', label: 'Uploads', icon: 'cloud_upload' },
  { id: 'section-performance', label: 'Performance', icon: 'speed' },
  { id: 'section-background', label: 'Background', icon: 'notifications' },
  { id: 'section-danger', label: 'Danger zone', icon: 'local_fire_department' },
] as const

const activeSectionId = ref<string>('section-appearance')

function scrollToSection(id: string) {
  activeSectionId.value = id
  const el = document.getElementById(id)
  if (el) {
    el.scrollIntoView({ behavior: 'smooth', block: 'start' })
  }
}

let observer: IntersectionObserver | null = null

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
  if (!kbps || kbps <= 0) return 'Unlimited'
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

async function clearHashCache() {
  try {
    isClearingCache.value = true
    await ConfigManager.ClearHashCache()
    hashCacheCount.value = 0
  } finally {
    isClearingCache.value = false
  }
}

onMounted(async () => {
  try {
    const [config, hashCount, trayAvail] = await Promise.all([
      ConfigManager.GetSettings(),
      ConfigManager.GetHashCacheCount(),
      ConfigManager.IsTrayAvailable(),
      refreshActiveAccount(),
    ])
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
      uploadThreads: config.uploadThreads || 4,
      minimizeToTray: config.minimizeToTray || false,
      notifyOnComplete: config.notifyOnComplete ?? true,
      useLocalHashCache: config.useLocalHashCache ?? false,
      uploadSpeedLimitKBps: config.uploadSpeedLimitKBps || 0,
      watchFoldersEnabled: config.watchFoldersEnabled || false,
      watchedFolders: config.watchedFolders || [],
    }
    hashCacheCount.value = hashCount
    isTrayAvailable.value = trayAvail
    if (!isTrayAvailable.value) {
      settings.value.minimizeToTray = false
    }
    preferencesState.useQuota = settings.value.useQuota
    preferencesState.saver = settings.value.saver
  } finally {
    await nextTick()
    isHydrating.value = false
  }

  // Setup scroll-spy inside the tab pane container with rAF throttle
  const scrollRoot = document.querySelector('[data-tab="settings"]') || null
  let scrollTicking = false
  observer = new IntersectionObserver(
    (entries) => {
      for (const entry of entries) {
        if (entry.isIntersecting && activeSectionId.value !== entry.target.id) {
          if (!scrollTicking) {
            scrollTicking = true
            requestAnimationFrame(() => {
              activeSectionId.value = entry.target.id
              scrollTicking = false
            })
          }
          break
        }
      }
    },
    { root: scrollRoot, threshold: 0.2 }
  )

  for (const s of sections) {
    const el = document.getElementById(s.id)
    if (el) observer.observe(el)
  }
})

onUnmounted(() => {
  if (observer) {
    observer.disconnect()
    observer = null
  }
})

watch(() => settings.value.proxy, async (newValue) => {
  if (isHydrating.value) return
  await ConfigManager.SetProxy(newValue)
})

watch(() => settings.value.useQuota, async (newValue) => {
  if (isHydrating.value) return
  if (newValue && settings.value.saver) {
    settings.value.saver = false
    await setSaver(false)
  }
  await setUseQuota(newValue)
})

watch(() => settings.value.saver, async (newValue) => {
  if (isHydrating.value) return
  if (newValue && settings.value.useQuota) {
    settings.value.useQuota = false
    await setUseQuota(false)
  }
  await setSaver(newValue)
})

watch(() => settings.value.recursive, async (newValue) => {
  if (isHydrating.value) return
  await ConfigManager.SetRecursive(newValue)
})

watch(() => settings.value.forceUpload, async (newValue) => {
  if (isHydrating.value) return
  await ConfigManager.SetForceUpload(newValue)
})

watch(() => settings.value.pairLivePhotos, async (newValue) => {
  if (isHydrating.value) return
  await ConfigManager.SetPairLivePhotos(newValue)
  if (newValue && !settings.value.skipIncompleteLivePhotos) {
    settings.value.skipIncompleteLivePhotos = true
    await ConfigManager.SetSkipIncompleteLivePhotos(true)
  }
})

watch(() => settings.value.skipIncompleteLivePhotos, async (newValue) => {
  if (isHydrating.value) return
  await ConfigManager.SetSkipIncompleteLivePhotos(newValue)
})

watch(() => settings.value.updateExistingPhotosToLive, async (newValue) => {
  if (isHydrating.value) return
  await ConfigManager.SetUpdateExistingPhotosToLive(newValue)
})

watch(() => settings.value.deleteFromHost, async (newValue) => {
  if (isHydrating.value) return
  await ConfigManager.SetDeleteFromHost(newValue)
})

watch(() => settings.value.disableUnsupportedFilesFilter, async (newValue) => {
  if (isHydrating.value) return
  await ConfigManager.SetDisableUnsupportedFilesFilter(newValue)
})

watch(() => settings.value.setDateFromFilename, async (newValue) => {
  if (isHydrating.value) return
  await ConfigManager.SetSetDateFromFilename(newValue)
})

watch(() => settings.value.uploadThreads, async (newValue) => {
  if (isHydrating.value) return
  if (newValue < 1) {
    settings.value.uploadThreads = 1
  } else {
    await ConfigManager.SetUploadThreads(newValue)
  }
})

watch(() => settings.value.minimizeToTray, async (newValue) => {
  if (isHydrating.value) return
  await ConfigManager.SetMinimizeToTray(newValue)
})

watch(() => settings.value.notifyOnComplete, async (newValue) => {
  if (isHydrating.value) return
  await ConfigManager.SetNotifyOnComplete(newValue)
})

watch(() => settings.value.useLocalHashCache, async (newValue) => {
  if (isHydrating.value) return
  await ConfigManager.SetUseLocalHashCache(newValue)
})

watch(() => settings.value.uploadSpeedLimitKBps, async (newValue) => {
  if (isHydrating.value) return
  await ConfigManager.SetUploadSpeedLimitKBps(newValue)
})

watch(() => settings.value.watchFoldersEnabled, async (newValue) => {
  if (isHydrating.value) return
  await ConfigManager.SetWatchFoldersEnabled(newValue)
})
</script>

<template>
  <div class="flex flex-col gap-6 p-2 w-full select-none">
    <!-- Header with friendly, complete sentence -->
    <div class="flex flex-col gap-1">
      <h1 class="m3-headline-large text-[var(--md-sys-color-on-surface)]">
        Settings
      </h1>
      <p class="m3-body-medium">
        Change how Lumen uploads, how fast it runs, and how it looks.
      </p>
    </div>

    <!-- Main Container: Sticky Section List (Left) + Flat One-Column Container (Right, max-w-[880px]) -->
    <div class="flex items-start gap-8 w-full">
      <!-- Sticky section list (Mockup A) -->
      <nav
        aria-label="Settings section navigation"
        class="hidden min-[1100px]:flex flex-col w-52 shrink-0 sticky top-2 gap-1 self-start"
      >
        <span class="m3-label-small text-[var(--md-sys-color-on-surface-variant)] px-3 mb-1">
          Sections
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
              Appearance
            </h2>
          </div>

          <div class="rounded-[24px] bg-[var(--md-sys-color-surface-container)] border border-[var(--md-sys-color-outline-variant)]/40 divide-y divide-[var(--md-sys-color-outline-variant)]/20 overflow-hidden shadow-xs">
            <!-- Row: Palette Style (Connected Button Group) -->
            <div class="flex flex-col sm:flex-row sm:items-center justify-between gap-4 p-5">
              <div class="flex flex-col pr-2">
                <span class="m3-title-medium text-[var(--md-sys-color-on-surface)]">
                  Palette style
                </span>
                <span class="m3-body-medium mt-0.5">
                  Select how colorful or vibrant your theme appears.
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
                  Theme color
                </span>
                <span class="m3-body-medium mt-0.5">
                  Choose a primary accent color for the interface.
                </span>
              </div>
              <div class="flex items-center gap-2 shrink-0 flex-wrap">
                <button
                  v-for="preset in THEME_PRESETS"
                  :key="preset.id"
                  type="button"
                  role="radio"
                  :aria-checked="currentThemeSeed.toLowerCase() === preset.seed.toLowerCase()"
                  :aria-label="`${preset.name} theme color`"
                  :title="preset.name"
                  class="no-state-layer relative flex size-9 items-center justify-center rounded-full p-0.5 focus:outline-none cursor-pointer transition-transform duration-150 hover:scale-105"
                  @click="setThemeSeed(preset.seed)"
                >
                  <div
                    class="relative flex size-8 items-center justify-center rounded-full overflow-hidden transition-all duration-200"
                    :class="currentThemeSeed.toLowerCase() === preset.seed.toLowerCase()
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
                      v-if="currentThemeSeed.toLowerCase() === preset.seed.toLowerCase()"
                      class="relative z-10 flex size-4 items-center justify-center rounded-full bg-[var(--md-sys-color-surface)] text-[var(--md-sys-color-primary)] shadow-xs"
                    >
                      <M3Icon name="check" :size="12" />
                    </div>
                  </div>
                </button>
              </div>
            </div>
          </div>
        </div>

        <!-- 2. Section: Uploads -->
        <div id="section-uploads" class="m3-settings-section flex flex-col gap-2.5 scroll-mt-6">
          <div class="flex items-center gap-2 px-1 text-[var(--md-sys-color-primary)]">
            <M3Icon name="cloud_upload" :size="20" />
            <h2 class="m3-headline-small text-[var(--md-sys-color-on-surface)]">
              Uploads
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
                  Use quota
                </label>
                <span class="m3-body-medium mt-0.5">
                  Uploads in original quality count against your Google storage limit.
                </span>
              </div>
              <M3Switch
                id="use-quota-switch"
                v-model="settings.useQuota"
                aria-label="Use quota toggle"
              />
            </div>

            <!-- Storage Saver Quality -->
            <div class="flex items-center justify-between gap-4 p-5">
              <div class="flex flex-col pr-2">
                <label
                  for="saver-mode-switch"
                  class="m3-title-medium text-[var(--md-sys-color-on-surface)] cursor-pointer"
                >
                  Storage saver quality
                </label>
                <span class="m3-body-medium mt-0.5">
                  Compress photos and videos slightly so they take up less storage.
                </span>
              </div>
              <M3Switch
                id="saver-mode-switch"
                v-model="settings.saver"
                aria-label="Storage saver quality toggle"
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
                    Local deduplication cache
                  </label>
                  <span class="m3-body-medium mt-0.5">
                    Remember what you already uploaded so Lumen does not ask Google Photos about those files again.
                  </span>
                </div>
                <M3Switch
                  id="local-hash-cache-switch"
                  v-model="settings.useLocalHashCache"
                  aria-label="Local deduplication cache toggle"
                />
              </div>

              <!-- Cache count & Clear button -->
              <div
                v-if="settings.useLocalHashCache"
                class="flex items-center justify-between rounded-xl bg-[var(--md-sys-color-surface-container-high)]/70 px-3.5 py-2 border border-[var(--md-sys-color-outline-variant)]/30 mt-1"
              >
                <span class="m3-label-large text-[var(--md-sys-color-on-surface)]">
                  {{ hashCacheCount.toLocaleString() }} files cached
                </span>
                <M3Button
                  variant="outlined"
                  destructive
                  size="xs"
                  :disabled="isClearingCache || hashCacheCount === 0"
                  @click="clearHashCache"
                >
                  <span>{{ isClearingCache ? 'Clearing...' : 'Clear cache' }}</span>
                </M3Button>
              </div>
            </div>

            <!-- Force Upload -->
            <div class="flex items-center justify-between gap-4 p-5">
              <div class="flex flex-col pr-2">
                <label
                  for="force-upload-switch"
                  class="m3-title-medium text-[var(--md-sys-color-on-surface)] cursor-pointer"
                >
                  Force upload
                </label>
                <span class="m3-body-medium mt-0.5">
                  Upload every file even if Lumen thinks it was uploaded before.
                </span>
              </div>
              <M3Switch
                id="force-upload-switch"
                v-model="settings.forceUpload"
                aria-label="Force upload toggle"
              />
            </div>

            <!-- Try Unsupported Formats -->
            <div class="flex items-center justify-between gap-4 p-5">
              <div class="flex flex-col pr-2">
                <label
                  for="filter-unsupported-switch"
                  class="m3-title-medium text-[var(--md-sys-color-on-surface)] cursor-pointer"
                >
                  Try unsupported formats
                </label>
                <span class="m3-body-medium mt-0.5">
                  Attempt to upload files that have uncommon or unrecognized extensions.
                </span>
              </div>
              <M3Switch
                id="filter-unsupported-switch"
                v-model="settings.disableUnsupportedFilesFilter"
                aria-label="Try unsupported formats toggle"
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
                    Pair Apple Live Photos
                  </label>
                  <span class="m3-body-medium mt-0.5">
                    Group matching still photos and short videos together as Live Photos.
                  </span>
                </div>
                <M3Switch
                  id="pair-live-photos-switch"
                  v-model="settings.pairLivePhotos"
                  aria-label="Pair Apple Live Photos toggle"
                />
              </div>

              <!-- Sub-options when enabled -->
              <div
                v-if="settings.pairLivePhotos"
                class="flex flex-col gap-2 rounded-xl bg-[var(--md-sys-color-surface-container-high)]/70 p-3 border border-[var(--md-sys-color-outline-variant)]/30 mt-1"
              >
                <div class="flex items-center justify-between gap-3">
                  <span class="m3-body-large text-[var(--md-sys-color-on-surface)]">
                    Skip incomplete pairs
                  </span>
                  <M3Switch
                    v-model="settings.skipIncompleteLivePhotos"
                    aria-label="Skip incomplete pairs toggle"
                  />
                </div>
                <div class="flex items-center justify-between gap-3">
                  <span class="m3-body-large text-[var(--md-sys-color-on-surface)]">
                    Update existing photos
                  </span>
                  <M3Switch
                    v-model="settings.updateExistingPhotosToLive"
                    aria-label="Update existing photos toggle"
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
                  Date from filename
                </label>
                <span class="m3-body-medium mt-0.5">
                  Use dates written in file names when camera metadata is missing.
                </span>
              </div>
              <M3Switch
                id="set-date-switch"
                v-model="settings.setDateFromFilename"
                aria-label="Date from filename toggle"
              />
            </div>
          </div>
        </div>

        <!-- 3. Section: Performance -->
        <div id="section-performance" class="m3-settings-section flex flex-col gap-2.5 scroll-mt-6">
          <div class="flex items-center gap-2 px-1 text-[var(--md-sys-color-primary)]">
            <M3Icon name="speed" :size="20" />
            <h2 class="m3-headline-small text-[var(--md-sys-color-on-surface)]">
              Performance
            </h2>
          </div>

          <div class="rounded-[24px] bg-[var(--md-sys-color-surface-container)] border border-[var(--md-sys-color-outline-variant)]/40 divide-y divide-[var(--md-sys-color-outline-variant)]/20 overflow-hidden shadow-xs">
            <!-- Upload Threads (Compact slider, value label on right, no dot rows) -->
            <div class="flex flex-col sm:flex-row sm:items-center justify-between gap-4 p-5">
              <div class="flex flex-col pr-2">
                <span class="m3-title-medium text-[var(--md-sys-color-on-surface)]">
                  Upload threads
                </span>
                <span class="m3-body-medium mt-0.5">
                  Set how many files upload at the same time.
                </span>
              </div>
              <div class="flex items-center gap-3 w-64 shrink-0 self-start sm:self-auto">
                <M3Slider
                  v-model="settings.uploadThreads"
                  :min="1"
                  :max="20"
                  :stops="false"
                  aria-label="Upload threads slider"
                  class="flex-1"
                />
                <span class="shrink-0 m3-label-large text-[var(--md-sys-color-primary)] w-24 text-right">
                  {{ settings.uploadThreads }} {{ settings.uploadThreads === 1 ? 'worker' : 'workers' }}
                </span>
              </div>
            </div>

            <!-- Bandwidth Limit (Compact slider, value label on right, no dot rows) -->
            <div class="flex flex-col sm:flex-row sm:items-center justify-between gap-4 p-5">
              <div class="flex flex-col pr-2">
                <span class="m3-title-medium text-[var(--md-sys-color-on-surface)]">
                  Bandwidth limit
                </span>
                <span class="m3-body-medium mt-0.5">
                  Limit upload speed so other devices on your network stay fast.
                </span>
              </div>
              <div class="flex items-center gap-3 w-64 shrink-0 self-start sm:self-auto">
                <M3Slider
                  v-model="speedStepIndex"
                  :min="0"
                  :max="SPEED_STEPS.length - 1"
                  :step="1"
                  :stops="false"
                  aria-label="Bandwidth limit slider"
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
                  Proxy URL
                </label>
                <span class="m3-body-medium mt-0.5">
                  Route your uploads through an HTTP or SOCKS5 proxy server.
                </span>
              </div>
              <div class="w-64 shrink-0 self-start sm:self-auto">
                <M3TextField
                  id="proxy-input"
                  v-model="settings.proxy"
                  placeholder="http://127.0.0.1:7890"
                  variant="outlined"
                  leading-icon="vpn_lock"
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
              Background
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
                  Minimize to tray on close
                </label>
                <span class="m3-body-medium mt-0.5">
                  {{ isTrayAvailable ? 'Keep Lumen running in the system tray when you close the window.' : 'System tray is not supported on this desktop.' }}
                </span>
              </div>
              <M3Switch
                id="tray-minimize-switch"
                v-model="settings.minimizeToTray"
                :disabled="!isTrayAvailable"
                aria-label="Minimize to tray toggle"
              />
            </div>

            <!-- Desktop Notifications -->
            <div class="flex items-center justify-between gap-4 p-5">
              <div class="flex flex-col pr-2">
                <label
                  for="desktop-notifications-switch"
                  class="m3-title-medium text-[var(--md-sys-color-on-surface)] cursor-pointer"
                >
                  Desktop notifications
                </label>
                <span class="m3-body-medium mt-0.5">
                  Get a notification on your desktop when all uploads finish.
                </span>
              </div>
              <M3Switch
                id="desktop-notifications-switch"
                v-model="settings.notifyOnComplete"
                aria-label="Desktop notifications toggle"
              />
            </div>
          </div>
        </div>

        <!-- 5. Section: Danger zone -->
        <div id="section-danger" class="m3-settings-section flex flex-col gap-2.5 scroll-mt-6">
          <div class="flex items-center gap-2 px-1 text-[var(--md-sys-color-error)]">
            <M3Icon name="local_fire_department" :size="20" />
            <h2 class="m3-headline-small text-[var(--md-sys-color-error)]">
              Danger zone
            </h2>
          </div>

          <div class="rounded-[24px] bg-[var(--md-sys-color-surface-container)] border border-[var(--md-sys-color-error)]/30 divide-y divide-[var(--md-sys-color-outline-variant)]/20 overflow-hidden shadow-xs">
            <div class="flex items-center justify-between gap-4 p-5">
              <div class="flex flex-col pr-2">
                <label
                  for="delete-host-switch"
                  class="m3-title-medium text-[var(--md-sys-color-on-surface)] cursor-pointer"
                >
                  Delete files after upload
                </label>
                <span class="m3-body-medium mt-0.5">
                  Delete local photos from your computer once they're uploaded.
                </span>
              </div>
              <M3Switch
                id="delete-host-switch"
                :model-value="settings.deleteFromHost"
                variant="danger"
                aria-label="Delete files after upload toggle"
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
          class="m3-dialog-wrapper fixed inset-0 z-50 grid place-items-center bg-black/70 p-4 select-none"
          style="--wails-draggable: none"
          @click.self="cancelDeleteFromHost"
        >
          <div
            class="m3-dialog-card relative flex w-full max-w-md flex-col gap-5 rounded-[28px] border border-[var(--md-sys-color-error)]/40 bg-[var(--md-sys-color-surface-container-high)] p-6 shadow-2xl"
            role="alertdialog"
            aria-modal="true"
            aria-labelledby="delete-confirm-title"
          >
            <div class="flex items-center gap-3">
              <div class="flex size-11 shrink-0 items-center justify-center rounded-2xl bg-[var(--md-sys-color-error-container)] text-[var(--md-sys-color-error)]">
                <M3Icon
                  name="warning"
                  :size="24"
                  :filled="true"
                />
              </div>
              <h3
                id="delete-confirm-title"
                class="m3-headline-small text-[var(--md-sys-color-on-surface)]"
              >
                Delete local files after upload?
              </h3>
            </div>

            <div class="flex flex-col gap-3 text-[var(--md-sys-color-on-surface-variant)] m3-body-medium leading-relaxed">
              <p>
                Once a photo or video finishes uploading, Lumen will delete the copy on this computer to save space.
              </p>
              <p>
                This won't touch anything you uploaded previously, and your photos in Google Photos stay safe. You won't be able to recover the deleted files from your computer.
              </p>
            </div>

            <div class="flex items-center justify-end gap-2 pt-2">
              <M3Button
                variant="text"
                size="s"
                @click="cancelDeleteFromHost"
              >
                <span>Cancel</span>
              </M3Button>
              <M3Button
                variant="filled"
                destructive
                size="s"
                icon="delete_forever"
                @click="confirmDeleteFromHost"
              >
                <span>Enable deletion</span>
              </M3Button>
            </div>
          </div>
        </div>
      </Transition>
    </Teleport>
  </div>

</template>
