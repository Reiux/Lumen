<script setup lang="ts">
import { computed, nextTick, onMounted, ref, watch } from 'vue'
import { ConfigManager } from '../bindings/app/core'
import type { WatchedFolderConfig } from '../bindings/app/core/models'
import M3Switch from './components/ui/M3Switch.vue'
import M3Slider from './components/ui/M3Slider.vue'
import Button from './components/ui/button/Button.vue'
import M3Icon from './components/ui/M3Icon.vue'
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from '@/components/ui/tooltip'

const emit = defineEmits<{
  (e: 'uploadFolder', path: string): void
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
const isAddingFolder = ref(false)
const isTrayAvailable = ref(true)
const watchedConfigs = ref<WatchedFolderConfig[]>([])
const inotifyError = ref('')
const pendingBacklog = ref<{ folder: string; count: number; account: string } | null>(null)

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

async function addWatchedFolder() {
  try {
    isAddingFolder.value = true
    const selected = await ConfigManager.PickWatchedFolder()
    if (!selected || !selected.trim()) return
    const folder = selected.trim()
    const accounts = await ConfigManager.GetAccounts()
    const activeAccount = accounts.selected || ''
    const backlog = await ConfigManager.CountFolderBacklog(folder)
    if (backlog > 0) {
      pendingBacklog.value = {
        folder,
        count: backlog,
        account: activeAccount,
      }
    } else {
      await ConfigManager.AddWatchedFolderWithAccount(folder, activeAccount)
      watchedConfigs.value = (await ConfigManager.GetWatchedFolderConfigs()) || []
      settings.value.watchedFolders = (await ConfigManager.GetWatchedFolders()) || []
      inotifyError.value = (await ConfigManager.GetInotifyError()) || ''
    }
  } catch (err: unknown) {
    console.error('Failed to add watched folder:', err)
  } finally {
    isAddingFolder.value = false
  }
}

async function confirmBacklog(uploadBacklog: boolean) {
  if (!pendingBacklog.value) return
  const { folder, account } = pendingBacklog.value
  try {
    await ConfigManager.AddWatchedFolderWithAccount(folder, account)
    watchedConfigs.value = (await ConfigManager.GetWatchedFolderConfigs()) || []
    settings.value.watchedFolders = (await ConfigManager.GetWatchedFolders()) || []
    inotifyError.value = (await ConfigManager.GetInotifyError()) || ''
    if (uploadBacklog) {
      emit('uploadFolder', folder)
    }
  } finally {
    pendingBacklog.value = null
  }
}

function cancelBacklog() {
  pendingBacklog.value = null
}

async function removeWatchedFolder(folderPath: string) {
  await ConfigManager.RemoveWatchedFolder(folderPath)
  watchedConfigs.value = (await ConfigManager.GetWatchedFolderConfigs()) || []
  settings.value.watchedFolders = (await ConfigManager.GetWatchedFolders()) || []
  inotifyError.value = (await ConfigManager.GetInotifyError()) || ''
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
    const config = await ConfigManager.GetSettings()
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
    hashCacheCount.value = await ConfigManager.GetHashCacheCount()
    watchedConfigs.value = (await ConfigManager.GetWatchedFolderConfigs()) || []
    inotifyError.value = (await ConfigManager.GetInotifyError()) || ''
    isTrayAvailable.value = await ConfigManager.IsTrayAvailable()
    if (!isTrayAvailable.value) {
      settings.value.minimizeToTray = false
    }
  } finally {
    await nextTick()
    isHydrating.value = false
  }
})

watch(() => settings.value.proxy, async (newValue) => {
  if (isHydrating.value) return
  await ConfigManager.SetProxy(newValue)
})

watch(() => settings.value.useQuota, async (newValue) => {
  if (isHydrating.value) return
  await ConfigManager.SetUseQuota(newValue)
})

watch(() => settings.value.saver, async (newValue) => {
  if (isHydrating.value) return
  await ConfigManager.SetSaver(newValue)
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
  <div class="flex flex-col gap-6 p-2 max-w-5xl mx-auto w-full select-none">
    <!-- Header -->
    <div class="flex flex-col gap-1">
      <h1 class="text-2xl font-bold tracking-tight text-[var(--md-sys-color-on-surface)]">
        Settings
      </h1>
      <p class="text-xs text-[var(--md-sys-color-on-surface-variant)]">
        Configure upload quality, concurrency, background automation, and system integration.
      </p>
    </div>

    <!-- 2-Column Responsive Grid -->
    <div class="grid grid-cols-1 lg:grid-cols-2 gap-5">
      <!-- COLUMN 1 -->
      <div class="flex flex-col gap-5">
        <!-- Card: System & Notifications -->
        <div class="flex flex-col gap-5 rounded-[28px] border border-[var(--md-sys-color-outline-variant)]/40 bg-[var(--md-sys-color-surface-container)] p-6 shadow-xs">
          <div class="flex items-center gap-2.5 text-[var(--md-sys-color-primary)]">
            <M3Icon
              name="notifications"
              :size="20"
            />
            <h2 class="text-base font-semibold text-[var(--md-sys-color-on-surface)]">
              System & Notifications
            </h2>
          </div>

          <!-- Minimize to Tray on Close -->
          <div class="flex items-start justify-between gap-4 pt-1">
            <div class="flex flex-col">
              <label
                for="tray-minimize-switch"
                class="text-sm font-medium text-[var(--md-sys-color-on-surface)]"
                :class="{ 'opacity-50 cursor-not-allowed': !isTrayAvailable, 'cursor-pointer': isTrayAvailable }"
              >
                Minimize to Tray on Close
              </label>
              <span class="text-xs text-[var(--md-sys-color-on-surface-variant)] mt-0.5">
                {{ isTrayAvailable ? 'Keep Lumen running in the system tray when closing the window.' : 'System tray host not detected on this desktop.' }}
              </span>
            </div>
            <M3Switch
              id="tray-minimize-switch"
              v-model="settings.minimizeToTray"
              :disabled="!isTrayAvailable"
              aria-label="Minimize to tray toggle"
            />
          </div>

          <div class="h-px w-full bg-[var(--md-sys-color-outline-variant)]/30" />

          <!-- Desktop Notifications -->
          <div class="flex items-start justify-between gap-4">
            <div class="flex flex-col">
              <label
                for="desktop-notifications-switch"
                class="text-sm font-medium text-[var(--md-sys-color-on-surface)] cursor-pointer"
              >
                Desktop Notifications
              </label>
              <span class="text-xs text-[var(--md-sys-color-on-surface-variant)] mt-0.5">
                Display system desktop notification when upload batches finish.
              </span>
            </div>
            <M3Switch
              id="desktop-notifications-switch"
              v-model="settings.notifyOnComplete"
              aria-label="Desktop notifications toggle"
            />
          </div>
        </div>

        <!-- Card: Performance & Network -->
        <div class="flex flex-col gap-5 rounded-[28px] border border-[var(--md-sys-color-outline-variant)]/40 bg-[var(--md-sys-color-surface-container)] p-6 shadow-xs">
          <div class="flex items-center gap-2.5 text-[var(--md-sys-color-primary)]">
            <M3Icon
              name="tune"
              :size="20"
            />
            <h2 class="text-base font-semibold text-[var(--md-sys-color-on-surface)]">
              Performance & Network
            </h2>
          </div>

          <!-- Upload Threads Slider -->
          <div class="flex flex-col gap-2 pt-1">
            <div class="flex items-center justify-between">
              <div class="flex flex-col">
                <span class="text-sm font-medium text-[var(--md-sys-color-on-surface)]">Upload Threads</span>
                <span class="text-xs text-[var(--md-sys-color-on-surface-variant)]">Concurrent upload workers</span>
              </div>
              <span class="inline-flex items-center justify-center rounded-full bg-[var(--md-sys-color-primary-container)] px-3 py-0.5 text-xs font-bold text-[var(--md-sys-color-on-primary-container)] shadow-xs">
                {{ settings.uploadThreads }} {{ settings.uploadThreads === 1 ? 'thread' : 'threads' }}
              </span>
            </div>
            <div class="flex items-center gap-3">
              <span class="text-xs font-medium text-[var(--md-sys-color-on-surface-variant)]">1</span>
              <M3Slider
                v-model="settings.uploadThreads"
                :min="1"
                :max="20"
                aria-label="Upload Threads slider"
              />
              <span class="text-xs font-medium text-[var(--md-sys-color-on-surface-variant)]">20</span>
            </div>
          </div>

          <div class="h-px w-full bg-[var(--md-sys-color-outline-variant)]/30" />

          <!-- Upload Speed Limiter -->
          <div class="flex flex-col gap-2">
            <div class="flex items-center justify-between">
              <div class="flex flex-col">
                <div class="flex items-center gap-1.5">
                  <M3Icon
                    name="speed"
                    :size="16"
                    class="text-[var(--md-sys-color-primary)]"
                  />
                  <span class="text-sm font-medium text-[var(--md-sys-color-on-surface)]">Bandwidth Limit</span>
                </div>
                <span class="text-xs text-[var(--md-sys-color-on-surface-variant)]">Pace upload speed to prevent network saturation</span>
              </div>
              <span class="inline-flex items-center justify-center rounded-full bg-[var(--md-sys-color-secondary-container)] px-3 py-0.5 text-xs font-bold text-[var(--md-sys-color-on-secondary-container)] shadow-xs shrink-0 whitespace-nowrap">
                {{ speedLimitDisplay }}
              </span>
            </div>
            <div class="flex items-center gap-3">
              <span class="text-xs font-medium text-[var(--md-sys-color-on-surface-variant)] shrink-0 select-none">Unlimited</span>
              <M3Slider
                v-model="speedStepIndex"
                :min="0"
                :max="SPEED_STEPS.length - 1"
                :step="1"
                aria-label="Upload Speed Limit slider"
              />
              <span class="text-xs font-medium text-[var(--md-sys-color-on-surface-variant)] shrink-0 whitespace-nowrap select-none">50 MB/s</span>
            </div>
          </div>

          <div class="h-px w-full bg-[var(--md-sys-color-outline-variant)]/30" />

          <!-- Proxy URL with Floating Label -->
          <div class="relative pt-1">
            <div class="m3-outlined-field relative rounded-2xl border border-[var(--md-sys-color-outline)] bg-[var(--md-sys-color-surface-container-low)] transition-colors duration-150 focus-within:border-2 focus-within:border-[var(--md-sys-color-primary)]">
              <input
                id="proxy-input"
                v-model="settings.proxy"
                type="text"
                placeholder=" "
                class="peer w-full rounded-2xl bg-transparent px-4 pt-5 pb-2 text-sm text-[var(--md-sys-color-on-surface)] outline-none"
              >
              <label
                for="proxy-input"
                class="pointer-events-none absolute left-4 top-2 text-[11px] font-medium text-[var(--md-sys-color-primary)] transition-[top,font-size,color] duration-150 peer-placeholder-shown:top-3.5 peer-placeholder-shown:text-xs peer-placeholder-shown:text-[var(--md-sys-color-on-surface-variant)] peer-focus:top-2 peer-focus:text-[11px] peer-focus:text-[var(--md-sys-color-primary)]"
              >
                Proxy URL (optional)
              </label>
            </div>
            <p class="mt-1.5 px-1 text-[11px] text-[var(--md-sys-color-on-surface-variant)]">
              Optional HTTP, HTTPS, or SOCKS5 proxy (e.g. http://127.0.0.1:7890)
            </p>
          </div>
        </div>

        <!-- Card: Auto-Backup (Folder Watcher) -->
        <div class="flex flex-col gap-4 rounded-[28px] border border-[var(--md-sys-color-outline-variant)]/40 bg-[var(--md-sys-color-surface-container)] p-6 shadow-xs">
          <div class="flex items-center gap-2.5 text-[var(--md-sys-color-primary)]">
            <M3Icon
              name="sync_saved_locally"
              :size="20"
            />
            <h2 class="text-base font-semibold text-[var(--md-sys-color-on-surface)]">
              Auto-Backup (Folder Watcher)
            </h2>
          </div>

          <div class="flex items-start justify-between gap-4 pt-1">
            <div class="flex flex-col">
              <label
                for="watch-folders-switch"
                class="text-sm font-medium text-[var(--md-sys-color-on-surface)] cursor-pointer"
              >
                Auto-Watch Folders
              </label>
              <span class="text-xs text-[var(--md-sys-color-on-surface-variant)] mt-0.5">
                Automatically monitor local directories and queue new photos and videos for upload.
              </span>
            </div>
            <M3Switch
              id="watch-folders-switch"
              v-model="settings.watchFoldersEnabled"
              aria-label="Auto-Watch Folders toggle"
            />
          </div>

          <!-- Watched Folders List & Picker -->
          <div
            v-if="settings.watchFoldersEnabled"
            class="flex flex-col gap-3 rounded-2xl bg-[var(--md-sys-color-surface-container-high)]/60 p-3.5 border border-[var(--md-sys-color-outline-variant)]/30 mt-1"
          >
            <!-- Inotify Limit Alert -->
            <div
              v-if="inotifyError"
              class="flex items-start gap-2.5 rounded-2xl bg-[var(--md-sys-color-error-container)] p-3 text-xs text-[var(--md-sys-color-on-error-container)] border border-[var(--md-sys-color-error)]/30"
            >
              <M3Icon
                name="error"
                :filled="true"
                :size="16"
                class="shrink-0 mt-0.5 text-[var(--md-sys-color-error)]"
              />
              <div class="flex flex-col gap-0.5">
                <span class="font-semibold">System inotify watch limit reached</span>
                <span class="leading-relaxed">{{ inotifyError }}. Some subdirectories cannot be watched. Increase fs.inotify.max_user_watches or remove folders.</span>
              </div>
            </div>

            <!-- Delete From Host Safety Notice -->
            <div
              v-if="settings.deleteFromHost"
              class="flex items-center gap-2 rounded-xl bg-[var(--md-sys-color-surface-container-highest)]/80 px-3 py-2 text-[11px] text-[var(--md-sys-color-on-surface-variant)] border border-[var(--md-sys-color-outline-variant)]/30"
            >
              <M3Icon
                name="info"
                :filled="true"
                :size="14"
                class="shrink-0 text-[var(--md-sys-color-primary)]"
              />
              <span>Watched folders always keep local files. "Delete from host" is disabled for background uploads.</span>
            </div>

            <div class="flex items-center justify-between">
              <span class="text-xs font-semibold text-[var(--md-sys-color-on-surface)]">
                Monitored Directories ({{ watchedConfigs.length || settings.watchedFolders.length }})
              </span>
              <Button
                variant="outline"
                size="sm"
                class="h-7 text-xs rounded-full px-3 gap-1 cursor-pointer select-none"
                :disabled="isAddingFolder"
                @click="addWatchedFolder"
              >
                <M3Icon
                  name="add"
                  :size="14"
                />
                <span>{{ isAddingFolder ? 'Selecting...' : 'Add Folder' }}</span>
              </Button>
            </div>

            <div
              v-if="(watchedConfigs.length || settings.watchedFolders.length) === 0"
              class="flex flex-col items-center justify-center py-4 text-center text-xs text-[var(--md-sys-color-on-surface-variant)]"
            >
              <M3Icon
                name="folder"
                :size="24"
                class="text-[var(--md-sys-color-on-surface-variant)]/40 mb-1"
              />
              <span>No folders added yet. Click Add Folder to begin monitoring.</span>
            </div>

            <div
              v-else
              class="flex flex-col gap-1.5 max-h-48 overflow-y-auto"
            >
              <div
                v-for="item in (watchedConfigs.length ? watchedConfigs : settings.watchedFolders.map(f => ({ path: f, accountEmail: '' })))"
                :key="item.path"
                class="flex items-center justify-between gap-2 rounded-xl bg-[var(--md-sys-color-surface-container-highest)]/60 px-3 py-2 text-xs border border-[var(--md-sys-color-outline-variant)]/20"
              >
                <div class="flex flex-col truncate flex-1 min-w-0">
                  <div class="flex items-center gap-2 truncate">
                    <M3Icon
                      name="folder"
                      :filled="true"
                      :size="14"
                      class="shrink-0 text-[var(--md-sys-color-primary)]"
                    />
                    <span
                      class="truncate font-mono text-[11px] text-[var(--md-sys-color-on-surface)]"
                      :title="item.path"
                    >
                      {{ item.path }}
                    </span>
                  </div>
                  <div class="flex items-center gap-1.5 mt-0.5 pl-5.5 text-[10px] text-[var(--md-sys-color-on-surface-variant)]">
                    <span class="rounded-full bg-[var(--md-sys-color-surface-container-high)] px-2 py-0.5 border border-[var(--md-sys-color-outline-variant)]/40 font-medium">
                      {{ item.accountEmail ? `Account: ${item.accountEmail}` : 'Active Account' }}
                    </span>
                  </div>
                </div>
                <button
                  type="button"
                  class="rounded-full p-1.5 text-[var(--md-sys-color-on-surface-variant)] hover:text-[var(--md-sys-color-error)] transition-colors cursor-pointer shrink-0"
                  title="Remove folder"
                  @click="removeWatchedFolder(item.path)"
                >
                  <M3Icon
                    name="delete"
                    :size="14"
                  />
                </button>
              </div>
            </div>
          </div>
        </div>
      </div>

      <!-- COLUMN 2 -->
      <div class="flex flex-col gap-5">
        <!-- Card: Quality & Quota -->
        <div class="flex flex-col gap-4 rounded-[28px] border border-[var(--md-sys-color-outline-variant)]/40 bg-[var(--md-sys-color-surface-container)] p-6 shadow-xs">
          <div class="flex items-center gap-2.5 text-[var(--md-sys-color-primary)]">
            <M3Icon
              name="storage"
              :size="20"
            />
            <h2 class="text-base font-semibold text-[var(--md-sys-color-on-surface)]">
              Quality & Storage
            </h2>
          </div>

          <!-- Use Quota -->
          <div class="flex items-start justify-between gap-4 pt-1">
            <div class="flex flex-col">
              <label
                for="use-quota-switch"
                class="text-sm font-medium text-[var(--md-sys-color-on-surface)] cursor-pointer"
              >
                Use Quota
              </label>
              <span class="text-xs text-[var(--md-sys-color-on-surface-variant)] mt-0.5">
                Uploads in original quality that count against your Google Drive storage limit.
              </span>
            </div>
            <M3Switch
              id="use-quota-switch"
              v-model="settings.useQuota"
              aria-label="Use Quota toggle"
            />
          </div>

          <div class="h-px w-full bg-[var(--md-sys-color-outline-variant)]/30" />

          <!-- Storage Saver Quality -->
          <div class="flex items-start justify-between gap-4">
            <div class="flex flex-col">
              <label
                for="saver-mode-switch"
                class="text-sm font-medium text-[var(--md-sys-color-on-surface)] cursor-pointer"
              >
                Storage Saver Quality
              </label>
              <span class="text-xs text-[var(--md-sys-color-on-surface-variant)] mt-0.5">
                Compress photos and videos slightly to save storage space without noticeable loss.
              </span>
            </div>
            <M3Switch
              id="saver-mode-switch"
              v-model="settings.saver"
              aria-label="Storage Saver Quality toggle"
            />
          </div>

          <div class="h-px w-full bg-[var(--md-sys-color-outline-variant)]/30" />

          <!-- Local Hash Cache -->
          <div class="flex items-start justify-between gap-4">
            <div class="flex flex-col">
              <div class="flex items-center gap-1.5">
                <M3Icon
                  name="database"
                  :filled="true"
                  :size="16"
                  class="text-[var(--md-sys-color-primary)]"
                />
                <label
                  for="local-hash-cache-switch"
                  class="text-sm font-medium text-[var(--md-sys-color-on-surface)] cursor-pointer"
                >
                  Local Hash Cache
                </label>
              </div>
              <span class="text-xs text-[var(--md-sys-color-on-surface-variant)] mt-0.5">
                Deduplicate media locally by file hash to avoid querying Google Photos. Files deleted from Google Photos won't re-upload while cached.
              </span>
            </div>
            <M3Switch
              id="local-hash-cache-switch"
              v-model="settings.useLocalHashCache"
              aria-label="Local Hash Cache toggle"
            />
          </div>

          <!-- Hash Cache Status & Clear Button -->
          <div
            v-if="settings.useLocalHashCache"
            class="flex items-center justify-between rounded-2xl bg-[var(--md-sys-color-surface-container-high)]/60 px-4 py-2.5 border border-[var(--md-sys-color-outline-variant)]/30"
          >
            <div class="flex items-center gap-2 text-xs text-[var(--md-sys-color-on-surface)]">
              <span class="font-medium">Active account:</span>
              <span class="font-bold text-[var(--md-sys-color-primary)]">
                {{ hashCacheCount.toLocaleString() }} {{ hashCacheCount === 1 ? 'file' : 'files' }} cached
              </span>
            </div>
            <Button
              variant="outline"
              size="sm"
              class="h-7 text-xs rounded-full px-3 cursor-pointer select-none"
              :disabled="isClearingCache || hashCacheCount === 0"
              @click="clearHashCache"
            >
              {{ isClearingCache ? 'Clearing...' : 'Clear Cache' }}
            </Button>
          </div>
        </div>

        <!-- Card: Behavior -->
        <div class="flex flex-col gap-4 rounded-[28px] border border-[var(--md-sys-color-outline-variant)]/40 bg-[var(--md-sys-color-surface-container)] p-6 shadow-xs">
          <div class="flex items-center gap-2.5 text-[var(--md-sys-color-primary)]">
            <M3Icon
              name="language"
              :size="20"
            />
            <h2 class="text-base font-semibold text-[var(--md-sys-color-on-surface)]">
              Upload Behavior
            </h2>
          </div>

          <!-- Recursive Directory Upload -->
          <div class="flex items-start justify-between gap-4 pt-1">
            <div class="flex flex-col">
              <label
                for="recursive-switch"
                class="text-sm font-medium text-[var(--md-sys-color-on-surface)] cursor-pointer"
              >
                Recursive Directory Upload
              </label>
              <span class="text-xs text-[var(--md-sys-color-on-surface-variant)] mt-0.5">
                Recursively discover and upload all media inside subfolders.
              </span>
            </div>
            <M3Switch
              id="recursive-switch"
              v-model="settings.recursive"
              aria-label="Recursive Directory Upload toggle"
            />
          </div>

          <div class="h-px w-full bg-[var(--md-sys-color-outline-variant)]/30" />

          <!-- Force Upload -->
          <div class="flex items-start justify-between gap-4">
            <div class="flex flex-col">
              <div class="flex items-center gap-1.5">
                <label
                  for="force-upload-switch"
                  class="text-sm font-medium text-[var(--md-sys-color-on-surface)] cursor-pointer"
                >
                  Force Upload
                </label>
                <Tooltip>
                  <TooltipTrigger as-child>
                    <button
                      type="button"
                      class="inline-flex size-4 items-center justify-center rounded-full text-[var(--md-sys-color-on-surface-variant)] hover:text-[var(--md-sys-color-on-surface)]"
                      aria-label="More information about Force Upload"
                    >
                      <M3Icon
                        name="info"
                        :size="14"
                      />
                    </button>
                  </TooltipTrigger>
                  <TooltipContent class="max-w-72 rounded-xl bg-[var(--md-sys-color-surface-container-highest)] p-2.5 text-xs text-[var(--md-sys-color-on-surface)]">
                    Applies to single files. Live Photo pairs always check both components for duplicates.
                  </TooltipContent>
                </Tooltip>
              </div>
              <span class="text-xs text-[var(--md-sys-color-on-surface-variant)] mt-0.5">
                Bypasses cloud duplicate checks and uploads every file again.
              </span>
            </div>
            <M3Switch
              id="force-upload-switch"
              v-model="settings.forceUpload"
              aria-label="Force Upload toggle"
            />
          </div>

          <div class="h-px w-full bg-[var(--md-sys-color-outline-variant)]/30" />

          <!-- Disable Unsupported Files Filter -->
          <div class="flex items-start justify-between gap-4">
            <div class="flex flex-col">
              <label
                for="filter-unsupported-switch"
                class="text-sm font-medium text-[var(--md-sys-color-on-surface)] cursor-pointer"
              >
                Disable Unsupported Files Filter
              </label>
              <span class="text-xs text-[var(--md-sys-color-on-surface-variant)] mt-0.5">
                Attempts upload on all selected files even if the format isn't recognized.
              </span>
            </div>
            <M3Switch
              id="filter-unsupported-switch"
              v-model="settings.disableUnsupportedFilesFilter"
              aria-label="Disable Unsupported Files Filter toggle"
            />
          </div>

          <div class="h-px w-full bg-[var(--md-sys-color-outline-variant)]/30" />

          <!-- Pair Apple Live Photos -->
          <div class="flex items-start justify-between gap-4">
            <div class="flex flex-col">
              <label
                for="pair-live-photos-switch"
                class="text-sm font-medium text-[var(--md-sys-color-on-surface)] cursor-pointer"
              >
                Pair Apple Live Photos
              </label>
              <span class="text-xs text-[var(--md-sys-color-on-surface-variant)] mt-0.5">
                Identify Apple Live Photo image and MOV pairs via metadata and upload them as one item.
              </span>
            </div>
            <M3Switch
              id="pair-live-photos-switch"
              v-model="settings.pairLivePhotos"
              aria-label="Pair Apple Live Photos toggle"
            />
          </div>

          <!-- Live Photo Sub-options (indented card when enabled) -->
          <div
            v-if="settings.pairLivePhotos"
            class="flex flex-col gap-3 rounded-2xl bg-[var(--md-sys-color-surface-container-high)]/60 p-3.5 border border-[var(--md-sys-color-outline-variant)]/30"
          >
            <div class="flex items-center justify-between gap-3">
              <span class="text-xs text-[var(--md-sys-color-on-surface)] font-medium">Skip Incomplete Live Photos</span>
              <M3Switch
                v-model="settings.skipIncompleteLivePhotos"
                aria-label="Skip Incomplete Live Photos toggle"
              />
            </div>
            <div class="flex items-center justify-between gap-3">
              <span class="text-xs text-[var(--md-sys-color-on-surface)] font-medium">Update Existing to Live</span>
              <M3Switch
                v-model="settings.updateExistingPhotosToLive"
                aria-label="Update Existing Photos to Live toggle"
              />
            </div>
          </div>

          <div class="h-px w-full bg-[var(--md-sys-color-outline-variant)]/30" />

          <!-- Set Upload Date from Filename -->
          <div class="flex items-start justify-between gap-4">
            <div class="flex flex-col">
              <label
                for="set-date-switch"
                class="text-sm font-medium text-[var(--md-sys-color-on-surface)] cursor-pointer"
              >
                Set Upload Date from Filename
              </label>
              <span class="text-xs text-[var(--md-sys-color-on-surface-variant)] mt-0.5">
                Parses timestamp directly from the filename to set the photo creation date.
              </span>
            </div>
            <M3Switch
              id="set-date-switch"
              v-model="settings.setDateFromFilename"
              aria-label="Set Upload Date from Filename toggle"
            />
          </div>
        </div>

        <!-- Card: Danger Zone -->
        <div class="flex flex-col gap-4 rounded-[28px] border border-[var(--md-sys-color-error)]/40 bg-[var(--md-sys-color-error-container)]/15 p-6 shadow-xs">
          <div class="flex items-center gap-2.5 text-[var(--md-sys-color-error)]">
            <M3Icon
              name="local_fire_department"
              :filled="true"
              :size="20"
            />
            <h2 class="text-base font-semibold text-[var(--md-sys-color-error)]">
              Danger Zone
            </h2>
          </div>

          <div class="flex items-start justify-between gap-4 pt-1">
            <div class="flex flex-col">
              <label
                for="delete-host-switch"
                class="text-sm font-medium text-[var(--md-sys-color-on-surface)] cursor-pointer"
              >
                Delete From Host After Upload
              </label>
              <span class="text-xs text-[var(--md-sys-color-on-surface-variant)] mt-0.5">
                Permanently delete source files from your local storage immediately after successful upload.
              </span>
            </div>
            <M3Switch
              id="delete-host-switch"
              :model-value="settings.deleteFromHost"
              variant="danger"
              aria-label="Delete From Host After Upload toggle"
              @update:model-value="onToggleDelete"
            />
          </div>

          <!-- Confirm Dialog / Banner -->
          <div
            v-if="showDeleteConfirm"
            class="mt-2 flex flex-col gap-3 rounded-2xl border border-[var(--md-sys-color-error)]/60 bg-[var(--md-sys-color-error-container)]/40 p-4"
          >
            <div class="flex items-start gap-2.5 text-[var(--md-sys-color-error)]">
              <M3Icon
                name="warning"
                :filled="true"
                :size="20"
                class="shrink-0 mt-0.5"
              />
              <div class="flex flex-col gap-1">
                <span class="text-xs font-bold text-[var(--md-sys-color-on-error-container)]">Confirm Local File Deletion</span>
                <p class="text-[11px] leading-relaxed text-[var(--md-sys-color-on-error-container)]/90">
                  Are you absolutely sure? All files successfully uploaded to Google Photos will be permanently deleted from your local hard drive. This cannot be undone.
                </p>
              </div>
            </div>
            <div class="flex justify-end gap-2.5 pt-1">
              <Button
                variant="outline"
                class="h-8 rounded-full px-4 text-xs cursor-pointer select-none"
                @click="cancelDeleteFromHost"
              >
                Cancel
              </Button>
              <Button
                class="h-8 rounded-full px-4 text-xs font-semibold cursor-pointer select-none bg-[var(--md-sys-color-error)] text-[var(--md-sys-color-on-error)] hover:bg-[var(--md-sys-color-error)]/90"
                @click="confirmDeleteFromHost"
              >
                Yes, Enable Deletion
              </Button>
            </div>
          </div>
        </div>
      </div>
    </div>
  </div>

  <!-- Backlog Confirmation Modal -->
  <Teleport to="body">
    <Transition name="m3-dialog-modal">
      <div
        v-if="pendingBacklog"
        class="m3-dialog-wrapper fixed inset-0 z-50 grid place-items-center bg-black/70 p-4 select-none overflow-y-auto"
        @click.self="cancelBacklog"
      >
        <div class="flex flex-col gap-4 w-full max-w-md rounded-[28px] bg-[var(--md-sys-color-surface-container-high)] p-6 shadow-2xl border border-[var(--md-sys-color-outline-variant)]/40">
          <div class="flex items-center gap-3 text-[var(--md-sys-color-primary)]">
            <M3Icon
              name="folder"
              :filled="true"
              :size="24"
            />
            <h3 class="text-lg font-bold text-[var(--md-sys-color-on-surface)]">
              Upload Existing Backlog?
            </h3>
          </div>
          <p class="text-xs text-[var(--md-sys-color-on-surface-variant)] leading-relaxed">
            Found <strong class="text-[var(--md-sys-color-on-surface)] font-semibold">{{ pendingBacklog.count }}</strong> existing photo and video files in:
          </p>
          <div class="rounded-xl bg-[var(--md-sys-color-surface-container)] p-3 text-[11px] font-mono break-all border border-[var(--md-sys-color-outline-variant)]/30 text-[var(--md-sys-color-on-surface)]">
            {{ pendingBacklog.folder }}
          </div>
          <p class="text-xs text-[var(--md-sys-color-on-surface-variant)] leading-relaxed">
            Would you like to upload existing files now, or only monitor this folder for newly added files in the future?
          </p>
          <div class="flex flex-col sm:flex-row items-center justify-end gap-2 pt-2">
            <Button
              variant="outline"
              class="rounded-full px-4 text-xs h-9 cursor-pointer w-full sm:w-auto"
              @click="cancelBacklog"
            >
              Cancel
            </Button>
            <Button
              variant="outline"
              class="rounded-full px-4 text-xs h-9 cursor-pointer w-full sm:w-auto"
              @click="confirmBacklog(false)"
            >
              Watch Only
            </Button>
            <Button
              class="rounded-full px-4 text-xs h-9 cursor-pointer bg-[var(--md-sys-color-primary)] text-[var(--md-sys-color-on-primary)] w-full sm:w-auto"
              @click="confirmBacklog(true)"
            >
              Upload Backlog & Watch
            </Button>
          </div>
        </div>
      </div>
    </Transition>
  </Teleport>
</template>
