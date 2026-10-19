<script setup lang="ts">
import { nextTick, onMounted, ref, watch } from 'vue'
import {
  AlertTriangle,
  Flame,
  Globe,
  HardDrive,
  Info,
  Sliders,
} from '@lucide/vue'
import { ConfigManager } from '../bindings/app/core'
import M3Switch from './components/ui/M3Switch.vue'
import M3Slider from './components/ui/M3Slider.vue'
import Button from './components/ui/button/Button.vue'
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from '@/components/ui/tooltip'

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
})

const isHydrating = ref(true)
const showDeleteConfirm = ref(false)

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
</script>

<template>
  <div class="flex flex-col gap-6 p-2 max-w-5xl mx-auto w-full select-none">
    <!-- Header -->
    <div class="flex flex-col gap-1">
      <h1 class="text-2xl font-bold tracking-tight text-[var(--md-sys-color-on-surface)]">
        Settings
      </h1>
      <p class="text-xs text-[var(--md-sys-color-on-surface-variant)]">
        Configure upload quality, concurrency, behavior, and network options.
      </p>
    </div>

    <!-- 2-Column Responsive Grid -->
    <div class="grid grid-cols-1 lg:grid-cols-2 gap-5">
      <!-- COLUMN 1 -->
      <div class="flex flex-col gap-5">
        <!-- Card: Performance & Network -->
        <div class="flex flex-col gap-5 rounded-[28px] border border-[var(--md-sys-color-outline-variant)]/40 bg-[var(--md-sys-color-surface-container)] p-6 shadow-xs">
          <div class="flex items-center gap-2.5 text-[var(--md-sys-color-primary)]">
            <Sliders class="size-5" />
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

          <!-- Proxy URL with Floating Label -->
          <div class="relative pt-2">
            <div class="m3-outlined-field relative rounded-2xl border border-[var(--md-sys-color-outline)] bg-[var(--md-sys-color-surface-container-low)] transition-all focus-within:border-2 focus-within:border-[var(--md-sys-color-primary)]">
              <input
                id="proxy-input"
                v-model="settings.proxy"
                type="text"
                placeholder=" "
                class="peer w-full rounded-2xl bg-transparent px-4 pt-5 pb-2 text-sm text-[var(--md-sys-color-on-surface)] outline-none"
              >
              <label
                for="proxy-input"
                class="pointer-events-none absolute left-4 top-2 text-[11px] font-medium text-[var(--md-sys-color-primary)] transition-all peer-placeholder-shown:top-3.5 peer-placeholder-shown:text-xs peer-placeholder-shown:text-[var(--md-sys-color-on-surface-variant)] peer-focus:top-2 peer-focus:text-[11px] peer-focus:text-[var(--md-sys-color-primary)]"
              >
                Proxy URL (optional)
              </label>
            </div>
            <p class="mt-1.5 px-1 text-[11px] text-[var(--md-sys-color-on-surface-variant)]">
              Optional HTTP, HTTPS, or SOCKS5 proxy (e.g. http://127.0.0.1:7890)
            </p>
          </div>
        </div>

        <!-- Card: Quality & Quota -->
        <div class="flex flex-col gap-4 rounded-[28px] border border-[var(--md-sys-color-outline-variant)]/40 bg-[var(--md-sys-color-surface-container)] p-6 shadow-xs">
          <div class="flex items-center gap-2.5 text-[var(--md-sys-color-primary)]">
            <HardDrive class="size-5" />
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
        </div>
      </div>

      <!-- COLUMN 2 -->
      <div class="flex flex-col gap-5">
        <!-- Card: Behavior -->
        <div class="flex flex-col gap-4 rounded-[28px] border border-[var(--md-sys-color-outline-variant)]/40 bg-[var(--md-sys-color-surface-container)] p-6 shadow-xs">
          <div class="flex items-center gap-2.5 text-[var(--md-sys-color-primary)]">
            <Globe class="size-5" />
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
                      <Info class="size-3" />
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
            <Flame class="size-5" />
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
              <AlertTriangle class="size-5 shrink-0 mt-0.5" />
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
</template>
