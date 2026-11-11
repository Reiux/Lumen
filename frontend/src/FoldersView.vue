<script setup lang="ts">
import { onMounted, ref } from 'vue'
import { toast } from 'vue-sonner'
import { ConfigManager } from '../bindings/app/core'
import { WatchedFolderConfig } from '../bindings/app/core/models'
import BacklogConfirmModal from './components/BacklogConfirmModal.vue'
import M3Button from './components/ui/M3Button.vue'
import M3Icon from './components/ui/M3Icon.vue'
import M3Switch from './components/ui/M3Switch.vue'
import M3TextField from './components/ui/M3TextField.vue'

const emit = defineEmits<{
  (e: 'uploadFolder', path: string): void
}>()

const isLoading = ref(true)
const watchFoldersEnabled = ref(false)
const watchedConfigs = ref<WatchedFolderConfig[]>([])
const accountsList = ref<string[]>([])
const selectedAccount = ref('')
const inotifyError = ref('')
const isAddingFolder = ref(false)
const recentAlbums = ref<string[]>([])
const albumNicknames = ref<Record<string, string | undefined>>({})

// Pending backlog dialog state
const pendingBacklog = ref<{ folder: string; count: number; account: string } | null>(null)

// Broad folder confirmation state
const pendingBroadFolder = ref<WatchedFolderConfig | null>(null)
const showBroadWarning = ref(false)

// Expand/collapse per folder card
const expandedFolders = ref<Record<string, boolean>>({})

function toggleExpand(path: string) {
  expandedFolders.value[path] = !expandedFolders.value[path]
}

function folderBasename(path: string): string {
  if (!path) return ''
  const trimmed = path.replace(/[/\\]+$/, '')
  const parts = trimmed.split(/[/\\]/)
  return parts[parts.length - 1] || path
}

function isBroadDirectory(path: string): boolean {
  const p = path.replace(/[/\\]+$/, '')
  return (
    p === '' ||
    p === '/' ||
    p === '/home' ||
    p === '/root' ||
    p === '/Users' ||
    p === 'C:' ||
    p === 'C:\\'
  )
}

function getAlbumLabel(keyOrName: string): string {
  if (!keyOrName) return ''
  if (albumNicknames.value[keyOrName]) {
    return albumNicknames.value[keyOrName]
  }
  return keyOrName
}

async function loadData() {
  try {
    isLoading.value = true
    const settings = await ConfigManager.GetSettings()
    watchFoldersEnabled.value = !!settings?.watchFoldersEnabled

    const accounts = await ConfigManager.GetAccounts()
    selectedAccount.value = accounts?.selected || ''
    const accList = (accounts?.accounts || []).map((a) => a.email)
    if (selectedAccount.value && !accList.includes(selectedAccount.value)) {
      accList.push(selectedAccount.value)
    }
    accountsList.value = accList

    watchedConfigs.value = (await ConfigManager.GetWatchedFolderConfigs()) || []
    inotifyError.value = (await ConfigManager.GetInotifyError()) || ''

    if (selectedAccount.value) {
      albumNicknames.value = (await ConfigManager.GetAlbumNicknames(selectedAccount.value)) || {}
    }
    recentAlbums.value = (await ConfigManager.GetRecentAlbums()) || []

    // Default expand all enabled folders
    for (const fc of watchedConfigs.value) {
      if (expandedFolders.value[fc.path] === undefined) {
        expandedFolders.value[fc.path] = fc.enabled
      }
    }
  } catch (err) {
    console.error('Failed to load folders config:', err)
  } finally {
    isLoading.value = false
  }
}

async function onToggleMasterSwitch(enabled: boolean) {
  watchFoldersEnabled.value = enabled
  try {
    await ConfigManager.SetWatchFoldersEnabled(enabled)
    toast.success(enabled ? 'Folder watcher active' : 'Folder watcher disabled')
  } catch (err) {
    console.error('Failed to toggle folder watcher:', err)
    toast.error('Failed to update folder watcher state')
  }
}

async function addFolder() {
  try {
    isAddingFolder.value = true
    const selected = await ConfigManager.PickWatchedFolder()
    if (!selected || !selected.trim()) return
    const folder = selected.trim()

    // Check if already added
    const alreadyExists = watchedConfigs.value.some((c) => c.path === folder)
    if (alreadyExists) {
      toast.info('Folder is already in the list')
      return
    }

    const activeAccount = selectedAccount.value || ''
    const backlog = await ConfigManager.CountFolderBacklog(folder)
    if (backlog > 0) {
      pendingBacklog.value = {
        folder,
        count: backlog,
        account: activeAccount,
      }
    } else {
      await ConfigManager.AddWatchedFolderWithAccount(folder, activeAccount)
      await loadData()
      toast.success(`Added folder: ${folderBasename(folder)}`)
    }
  } catch (err) {
    console.error('Failed to add watched folder:', err)
    toast.error('Failed to add folder')
  } finally {
    isAddingFolder.value = false
  }
}

async function confirmBacklog(uploadBacklog: boolean) {
  if (!pendingBacklog.value) return
  const { folder, account } = pendingBacklog.value
  try {
    await ConfigManager.AddWatchedFolderWithAccount(folder, account)
    await loadData()
    if (uploadBacklog) {
      emit('uploadFolder', folder)
      toast.success(`Starting upload for ${folderBasename(folder)}...`)
    } else {
      toast.success(`Watching folder: ${folderBasename(folder)}`)
    }
  } catch (err) {
    console.error('Failed to add watched folder with backlog:', err)
    toast.error('Failed to add watched folder')
  } finally {
    pendingBacklog.value = null
  }
}

function cancelBacklog() {
  pendingBacklog.value = null
}

async function toggleFolderEnabled(cfg: WatchedFolderConfig) {
  cfg.enabled = !cfg.enabled
  if (cfg.enabled) {
    expandedFolders.value[cfg.path] = true
  }
  await saveFolderConfig(cfg)
}

async function toggleIncludeSubfolders(cfg: WatchedFolderConfig) {
  if (!cfg.includeSubfolders && isBroadDirectory(cfg.path)) {
    pendingBroadFolder.value = cfg
    showBroadWarning.value = true
    return
  }
  cfg.includeSubfolders = !cfg.includeSubfolders
  await saveFolderConfig(cfg)
}

function confirmBroadFolder() {
  if (pendingBroadFolder.value) {
    pendingBroadFolder.value.includeSubfolders = true
    saveFolderConfig(pendingBroadFolder.value)
    pendingBroadFolder.value = null
  }
  showBroadWarning.value = false
}

function cancelBroadFolder() {
  pendingBroadFolder.value = null
  showBroadWarning.value = false
}

async function saveFolderConfig(cfg: WatchedFolderConfig) {
  try {
    await ConfigManager.UpdateWatchedFolderConfig(cfg)
    inotifyError.value = (await ConfigManager.GetInotifyError()) || ''
  } catch (err) {
    console.error('Failed to update folder configuration:', err)
    toast.error('Failed to update folder settings')
  }
}

async function removeFolder(cfg: WatchedFolderConfig) {
  try {
    await ConfigManager.RemoveWatchedFolder(cfg.path)
    watchedConfigs.value = watchedConfigs.value.filter((c) => c.path !== cfg.path)
    delete expandedFolders.value[cfg.path]
    inotifyError.value = (await ConfigManager.GetInotifyError()) || ''
    toast.success(`Removed ${folderBasename(cfg.path)}`)
  } catch (err) {
    console.error('Failed to remove watched folder:', err)
    toast.error('Failed to remove folder')
  }
}

function onUploadNow(cfg: WatchedFolderConfig) {
  emit('uploadFolder', cfg.path)
  toast.success(`Queued upload for ${folderBasename(cfg.path)}`)
}

async function saveAlbumNickname(key: string, nickname: string) {
  if (!selectedAccount.value || !key.trim()) return
  try {
    await ConfigManager.SetAlbumNickname(selectedAccount.value, key.trim(), nickname.trim())
    albumNicknames.value = (await ConfigManager.GetAlbumNicknames(selectedAccount.value)) || {}
    toast.success('Album nickname saved')
  } catch (err) {
    console.error('Failed to save album nickname:', err)
  }
}

onMounted(() => {
  loadData()
})
</script>

<template>
  <div class="flex flex-col gap-6 max-w-4xl mx-auto px-4 py-6 w-full">
    <!-- Header banner -->
    <div class="flex flex-col sm:flex-row items-start sm:items-center justify-between gap-4">
      <div>
        <h1 class="m3-headline-large text-[var(--md-sys-color-on-surface)] leading-tight">
          Backup Folders
        </h1>
        <p class="m3-body-medium mt-0.5">
          Photos and videos in enabled folders will be backed up automatically
        </p>
      </div>

      <M3Button
        variant="filled"
        size="s"
        icon="add"
        :disabled="isAddingFolder"
        @click="addFolder"
      >
        <span>{{ isAddingFolder ? 'Selecting...' : 'Add folder' }}</span>
      </M3Button>
    </div>

    <!-- Master Watcher Switch Card -->
    <div class="rounded-[24px] bg-[var(--md-sys-color-surface-container)] border border-[var(--md-sys-color-outline-variant)]/20 p-5 shadow-xs transition-colors">
      <div class="flex items-center justify-between gap-4">
        <div class="flex flex-col pr-2">
          <label
            for="master-watch-folders-switch"
            class="m3-title-medium text-[var(--md-sys-color-on-surface)] cursor-pointer"
          >
            Auto-watch folders
          </label>
          <span class="m3-body-medium mt-0.5 leading-relaxed">
            Continuously monitor local folders and back up new photos and videos when they arrive
          </span>
        </div>
        <M3Switch
          id="master-watch-folders-switch"
          :model-value="watchFoldersEnabled"
          aria-label="Auto-watch folders master toggle"
          @update:model-value="onToggleMasterSwitch"
        />
      </div>

      <!-- Inotify Limit Warning (Linux) -->
      <div
        v-if="inotifyError"
        class="mt-4 flex items-start gap-2.5 rounded-2xl bg-[var(--md-sys-color-error-container)] p-3.5 m3-body-medium text-[var(--md-sys-color-on-error-container)] border border-[var(--md-sys-color-error)]/30"
      >
        <M3Icon name="error" :size="18" class="shrink-0 mt-0.5 text-[var(--md-sys-color-error)]" />
        <div class="flex flex-col gap-1 leading-relaxed">
          <span class="m3-title-medium text-[var(--md-sys-color-on-surface)]">Filesystem Watcher Warning</span>
          <span>{{ inotifyError }}</span>
        </div>
      </div>
    </div>

    <!-- Folders List Section -->
    <div class="flex flex-col gap-3">
      <div class="flex items-center justify-between px-1">
        <span class="m3-label-small text-[var(--md-sys-color-on-surface-variant)]">
          Watched Directories ({{ watchedConfigs.length }})
        </span>
      </div>

      <!-- Empty State -->
      <div
        v-if="watchedConfigs.length === 0"
        class="flex flex-col items-center justify-center gap-3 rounded-[24px] bg-[var(--md-sys-color-surface-container-low)] border border-dashed border-[var(--md-sys-color-outline-variant)]/40 p-12 text-center"
      >
        <div class="flex size-14 items-center justify-center rounded-full bg-[var(--md-sys-color-surface-container-high)] text-[var(--md-sys-color-on-surface-variant)]">
          <M3Icon name="folder_off" :size="28" />
        </div>
        <div class="flex flex-col gap-1 max-w-sm">
          <h3 class="m3-title-medium text-[var(--md-sys-color-on-surface)]">
            No folders added yet
          </h3>
          <p class="m3-body-medium leading-relaxed">
            Choose camera, download, or screenshot folders on your computer to back them up automatically.
          </p>
        </div>
        <M3Button
          variant="tonal"
          size="s"
          icon="add"
          class="mt-2"
          :disabled="isAddingFolder"
          @click="addFolder"
        >
          Choose a folder
        </M3Button>
      </div>

      <!-- Folders Cards -->
      <div v-else class="flex flex-col gap-3">
        <div
          v-for="cfg in watchedConfigs"
          :key="cfg.path"
          class="rounded-[24px] bg-[var(--md-sys-color-surface-container)] border border-[var(--md-sys-color-outline-variant)]/30 overflow-hidden shadow-xs transition-all duration-200"
          :class="cfg.enabled ? '' : 'opacity-70 bg-[var(--md-sys-color-surface-container-low)]'"
        >
          <!-- Main Folder Header Row -->
          <div class="flex items-center justify-between gap-3 p-4 sm:p-5">
            <div
              class="flex items-center gap-3.5 flex-1 min-w-0 cursor-pointer"
              @click="toggleExpand(cfg.path)"
            >
              <div
                class="flex size-10 shrink-0 items-center justify-center rounded-xl transition-colors"
                :class="
                  cfg.enabled
                    ? 'bg-[var(--md-sys-color-primary-container)] text-[var(--md-sys-color-primary)]'
                    : 'bg-[var(--md-sys-color-surface-container-high)] text-[var(--md-sys-color-outline)]'
                "
              >
                <M3Icon name="folder" :filled="cfg.enabled" :size="22" />
              </div>

              <div class="flex flex-col min-w-0 flex-1">
                <div class="flex items-center gap-2 flex-wrap">
                  <span class="m3-title-medium text-[var(--md-sys-color-on-surface)] truncate">
                    {{ folderBasename(cfg.path) }}
                  </span>

                  <!-- Subfolders Pill Badge -->
                  <span
                    v-if="cfg.includeSubfolders && cfg.enabled"
                    class="rounded-full bg-[var(--md-sys-color-primary-container)] px-2 py-0.5 m3-label-small normal-case text-[var(--md-sys-color-on-primary-container)]"
                  >
                    subfolders
                  </span>

                  <!-- Album Mode Badge -->
                  <span
                    v-if="cfg.albumMode === 'auto' && cfg.enabled"
                    class="rounded-full bg-[var(--md-sys-color-secondary-container)] px-2 py-0.5 m3-label-small normal-case text-[var(--md-sys-color-on-secondary-container)]"
                  >
                    Auto album
                  </span>
                  <span
                    v-else-if="cfg.albumMode === 'custom' && cfg.albumNameOrKey && cfg.enabled"
                    class="rounded-full bg-[var(--md-sys-color-secondary-container)] px-2 py-0.5 m3-label-small normal-case text-[var(--md-sys-color-on-secondary-container)] truncate max-w-[140px]"
                    :title="cfg.albumNameOrKey"
                  >
                    {{ getAlbumLabel(cfg.albumNameOrKey) }}
                  </span>
                </div>

                <span
                  class="m3-mono m3-body-medium truncate mt-0.5"
                  :title="cfg.path"
                >
                  {{ cfg.path }}
                </span>
              </div>
            </div>

            <!-- Header Controls -->
            <div class="flex items-center gap-2 shrink-0">
              <!-- Upload Now Button -->
              <button
                type="button"
                class="flex size-8 cursor-pointer items-center justify-center rounded-full text-[var(--md-sys-color-on-surface-variant)] hover:bg-[var(--md-sys-color-surface-container-high)] hover:text-[var(--md-sys-color-primary)] transition-colors"
                title="Upload this folder now"
                aria-label="Upload folder now"
                @click="onUploadNow(cfg)"
              >
                <M3Icon name="upload" :size="18" />
              </button>

              <!-- Enable/Disable Switch -->
              <M3Switch
                :model-value="cfg.enabled"
                :aria-label="`Toggle watching ${folderBasename(cfg.path)}`"
                @update:model-value="toggleFolderEnabled(cfg)"
              />

              <!-- Remove Folder Button -->
              <button
                type="button"
                class="flex size-8 cursor-pointer items-center justify-center rounded-full text-[var(--md-sys-color-on-surface-variant)] hover:bg-[var(--md-sys-color-error-container)] hover:text-[var(--md-sys-color-error)] transition-colors"
                title="Remove folder"
                aria-label="Remove watched folder"
                @click="removeFolder(cfg)"
              >
                <M3Icon name="delete" :size="18" />
              </button>

              <!-- Expand/Collapse Chevron -->
              <button
                type="button"
                class="flex size-8 cursor-pointer items-center justify-center rounded-full text-[var(--md-sys-color-on-surface-variant)] hover:bg-[var(--md-sys-color-surface-container-high)] transition-transform duration-200"
                :class="expandedFolders[cfg.path] ? 'rotate-180' : ''"
                aria-label="Toggle folder options"
                @click="toggleExpand(cfg.path)"
              >
                <M3Icon name="expand_more" :size="20" />
              </button>
            </div>
          </div>

          <!-- Expanded Folder Settings Section with M3E spring transition -->
          <div
            class="grid transition-all duration-300 ease-[cubic-bezier(0.05,0.7,0.1,1.0)]"
            :class="expandedFolders[cfg.path] ? 'grid-rows-[1fr] opacity-100' : 'grid-rows-[0fr] opacity-0 pointer-events-none'"
          >
            <div class="overflow-hidden">
              <div class="flex flex-col gap-4 border-t border-[var(--md-sys-color-outline-variant)]/20 bg-[var(--md-sys-color-surface-container-high)]/40 p-4 sm:p-5">
                <!-- 1. Include Subfolders Option -->
                <div class="flex items-center justify-between gap-4 rounded-2xl bg-[var(--md-sys-color-surface-container-lowest)] p-3.5 border border-[var(--md-sys-color-outline-variant)]/20">
                  <div class="flex flex-col pr-2">
                    <span class="m3-title-medium text-[var(--md-sys-color-on-surface)]">
                      Include subfolders
                    </span>
                    <span class="m3-body-medium mt-0.5">
                      Also back up photos and videos in folders inside this one
                    </span>
                  </div>
                  <M3Switch
                    :model-value="cfg.includeSubfolders"
                    :aria-label="`Include subfolders for ${folderBasename(cfg.path)}`"
                    @update:model-value="toggleIncludeSubfolders(cfg)"
                  />
                </div>

                <!-- 2. Target Album Option -->
                <div class="flex flex-col gap-3 rounded-2xl bg-[var(--md-sys-color-surface-container-lowest)] p-3.5 border border-[var(--md-sys-color-outline-variant)]/20">
                  <div class="flex flex-col">
                    <span class="m3-title-medium text-[var(--md-sys-color-on-surface)]">
                      Destination Album
                    </span>
                    <span class="m3-body-medium mt-0.5">
                      Organize uploads from this folder into a Google Photos album
                    </span>
                  </div>

                  <!-- Mode Buttons -->
                  <div class="flex flex-wrap items-center gap-2">
                    <M3Button
                      size="xs"
                      :toggle="true"
                      :selected="cfg.albumMode === 'none' || !cfg.albumMode"
                      :variant="cfg.albumMode === 'none' || !cfg.albumMode ? 'filled' : 'outlined'"
                      @click="cfg.albumMode = 'none'; saveFolderConfig(cfg)"
                    >
                      None (Library only)
                    </M3Button>
                    <M3Button
                      size="xs"
                      :toggle="true"
                      :selected="cfg.albumMode === 'auto'"
                      :variant="cfg.albumMode === 'auto' ? 'filled' : 'outlined'"
                      @click="cfg.albumMode = 'auto'; saveFolderConfig(cfg)"
                    >
                      Auto by folder name
                    </M3Button>
                    <M3Button
                      size="xs"
                      :toggle="true"
                      :selected="cfg.albumMode === 'custom'"
                      :variant="cfg.albumMode === 'custom' ? 'filled' : 'outlined'"
                      @click="cfg.albumMode = 'custom'; saveFolderConfig(cfg)"
                    >
                      Specific album
                    </M3Button>
                  </div>

                  <!-- Album Input when Specific Album is Selected -->
                  <div
                    v-if="cfg.albumMode === 'custom'"
                    class="flex flex-col gap-3 pt-2"
                  >
                    <M3TextField
                      :id="`album-input-${cfg.path}`"
                      v-model="cfg.albumNameOrKey"
                      label="Album name or album key (AF1Qip...)"
                      placeholder="e.g. Vacation or AF1Qip..."
                      variant="outlined"
                      leading-icon="folder"
                      @change="saveFolderConfig(cfg)"
                    />

                    <!-- Editable Nickname if using an AF1Qip album key -->
                    <div
                      v-if="cfg.albumNameOrKey && cfg.albumNameOrKey.trim().startsWith('AF1Qip')"
                      class="flex flex-col gap-2 rounded-2xl bg-[var(--md-sys-color-surface-container)] p-4 sm:p-5 border border-[var(--md-sys-color-outline-variant)]/30"
                    >
                      <div class="flex flex-col gap-0.5">
                        <span class="m3-title-medium text-[var(--md-sys-color-on-surface)]">
                          Album Nickname (optional)
                        </span>
                        <span class="m3-body-medium leading-relaxed">
                          Set a friendly display name for this album key so the 50-character code doesn't clutter your app.
                        </span>
                      </div>
                      <div class="pt-2">
                        <M3TextField
                          :id="`album-nickname-${cfg.path}`"
                          :model-value="albumNicknames[cfg.albumNameOrKey.trim()] || ''"
                          label="Friendly nickname"
                          placeholder="e.g. Vacation 2026"
                          variant="outlined"
                          leading-icon="label"
                          @change="(val: string) => saveAlbumNickname(cfg.albumNameOrKey, val)"
                        />
                      </div>
                    </div>

                    <!-- Recent Albums Chips -->
                    <div
                      v-if="recentAlbums.length > 0"
                      class="flex flex-col gap-1.5 pt-1"
                    >
                      <span class="m3-label-small text-[var(--md-sys-color-on-surface-variant)]">
                        Recent Albums
                      </span>
                      <div class="flex flex-wrap gap-1.5 max-h-24 overflow-y-auto">
                        <M3Button
                          v-for="recent in recentAlbums"
                          :key="recent"
                          size="xs"
                          :toggle="true"
                          :selected="cfg.albumNameOrKey === recent"
                          :variant="cfg.albumNameOrKey === recent ? 'filled' : 'outlined'"
                          icon="folder"
                          @click="cfg.albumNameOrKey = recent; saveFolderConfig(cfg)"
                        >
                          <span class="max-w-[130px] truncate">{{ getAlbumLabel(recent) }}</span>
                        </M3Button>
                      </div>
                    </div>
                  </div>
                </div>

                <!-- 3. Target Account Selector (if multiple accounts exist) -->
                <div
                  v-if="accountsList.length > 1"
                  class="flex items-center justify-between gap-4 rounded-2xl bg-[var(--md-sys-color-surface-container-lowest)] p-3.5 border border-[var(--md-sys-color-outline-variant)]/20"
                >
                  <div class="flex flex-col pr-2">
                    <span class="m3-title-medium text-[var(--md-sys-color-on-surface)]">
                      Account
                    </span>
                    <span class="m3-body-medium mt-0.5">
                      Route uploads from this folder to a specific Google account
                    </span>
                  </div>
                  <div class="relative inline-flex items-center shrink-0">
                    <select
                      v-model="cfg.accountEmail"
                      class="appearance-none rounded-full bg-[var(--md-sys-color-surface-container-high)] pl-3.5 pr-8 py-1.5 m3-label-large text-[var(--md-sys-color-on-surface)] border border-[var(--md-sys-color-outline-variant)]/40 hover:border-[var(--md-sys-color-on-surface)]/60 focus:outline-none focus:border-[var(--md-sys-color-primary)] focus:ring-1 focus:ring-[var(--md-sys-color-primary)] cursor-pointer transition-colors shadow-xs"
                      @change="saveFolderConfig(cfg)"
                    >
                      <option
                        value=""
                        class="bg-[var(--md-sys-color-surface-container-high)] text-[var(--md-sys-color-on-surface)]"
                      >
                        Default ({{ selectedAccount || 'Active' }})
                      </option>
                      <option
                        v-for="acc in accountsList"
                        :key="acc"
                        :value="acc"
                        class="bg-[var(--md-sys-color-surface-container-high)] text-[var(--md-sys-color-on-surface)]"
                      >
                        {{ acc }}
                      </option>
                    </select>
                    <div class="pointer-events-none absolute right-2.5 flex items-center text-[var(--md-sys-color-on-surface-variant)]">
                      <M3Icon name="arrow_drop_down" :size="18" />
                    </div>
                  </div>
                </div>
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>

    <!-- Backlog Confirmation Modal -->
    <BacklogConfirmModal
      :pending-backlog="pendingBacklog"
      @cancel="cancelBacklog"
      @confirm="confirmBacklog"
    />

    <!-- Broad Folder Confirmation Modal -->
    <Teleport to="body">
      <Transition name="m3-dialog-modal">
        <div
          v-if="showBroadWarning"
          class="m3-dialog-wrapper fixed inset-0 z-50 grid place-items-center bg-black/70 p-4 select-none overflow-y-auto"
          @click.self="cancelBroadFolder"
        >
          <div
            class="m3-dialog-card relative flex flex-col gap-4 w-full max-w-md rounded-[28px] bg-[var(--md-sys-color-surface-container-high)] p-6 shadow-2xl border border-[var(--md-sys-color-outline-variant)]/40"
            role="dialog"
            aria-modal="true"
          >
            <div class="flex items-center gap-3 text-[var(--md-sys-color-error)]">
              <div class="flex size-10 items-center justify-center rounded-[16px] bg-[var(--md-sys-color-error-container)]">
                <M3Icon name="warning" :filled="true" :size="22" />
              </div>
              <h3 class="m3-title-medium text-[var(--md-sys-color-on-surface)]">
                Broad Directory Warning
              </h3>
            </div>
            <p class="m3-body-medium leading-relaxed">
              Enabling recursive subfolders on a root directory such as <code class="rounded bg-black/20 px-1 py-0.5 m3-mono text-[var(--md-sys-color-on-surface)]">{{ pendingBroadFolder?.path }}</code> can watch thousands of subdirectories and may exhaust system inotify limits.
            </p>
            <p class="m3-body-medium leading-relaxed">
              Are you sure you want to watch all subdirectories in this path?
            </p>
            <div class="flex flex-col sm:flex-row items-center justify-end gap-2 pt-2">
              <M3Button variant="text" size="s" @click="cancelBroadFolder">
                Cancel
              </M3Button>
              <M3Button variant="filled" size="s" @click="confirmBroadFolder">
                Enable Subfolders
              </M3Button>
            </div>
          </div>
        </div>
      </Transition>
    </Teleport>
  </div>
</template>
