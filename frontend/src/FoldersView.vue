<script setup lang="ts">
import { onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { toast } from 'vue-sonner'

const { t } = useI18n()
import { ConfigManager } from '../bindings/app/core'
import { WatchedFolderConfig } from '../bindings/app/core/models'
import BacklogConfirmModal from './components/BacklogConfirmModal.vue'
import M3Button from './components/ui/M3Button.vue'
import M3Icon from './components/ui/M3Icon.vue'
import M3Switch from './components/ui/M3Switch.vue'
import M3TextField from './components/ui/M3TextField.vue'
import { useDialogFocusTrap } from './utils/useDialogFocusTrap'

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
const albumNicknamesByAccount = ref<Record<string, Record<string, string | undefined>>>({})
const nicknameDrafts = ref<Record<string, string>>({})

// Pending backlog dialog state
const pendingBacklog = ref<{ folder: string; count: number; account: string } | null>(null)
const isConfirmingBacklog = ref(false)

// Broad folder confirmation state
const pendingBroadFolder = ref<WatchedFolderConfig | null>(null)
const showBroadWarning = ref(false)
const broadWarningDialogCardRef = ref<HTMLElement | null>(null)

useDialogFocusTrap(() => showBroadWarning.value, broadWarningDialogCardRef, {
  onEscape: cancelBroadFolder,
})

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
    /^[a-zA-Z]:$/.test(p)
  )
}

function resolveFolderAccount(cfg?: WatchedFolderConfig): string {
  return cfg?.accountEmail || selectedAccount.value || ''
}

function getAlbumNickname(keyOrName: string, accountEmail?: string): string {
  const cleanKey = keyOrName.trim()
  if (!cleanKey) return ''
  const acc = accountEmail || selectedAccount.value || ''
  return albumNicknamesByAccount.value[acc]?.[cleanKey] || ''
}

function getAlbumLabel(keyOrName: string, accountEmail?: string): string {
  if (!keyOrName) return ''
  return getAlbumNickname(keyOrName, accountEmail) || keyOrName
}

function syncNicknameDraft(cfg: WatchedFolderConfig) {
  nicknameDrafts.value[cfg.path] = getAlbumNickname(cfg.albumNameOrKey || '', resolveFolderAccount(cfg))
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

    const accountsToFetch = new Set<string>()
    if (selectedAccount.value) accountsToFetch.add(selectedAccount.value)
    for (const fc of watchedConfigs.value) {
      if (fc.accountEmail) accountsToFetch.add(fc.accountEmail)
    }
    const nextNicknames: Record<string, Record<string, string | undefined>> = {}
    await Promise.all(
      Array.from(accountsToFetch).map(async (acc) => {
        nextNicknames[acc] = (await ConfigManager.GetAlbumNicknames(acc)) || {}
      }),
    )
    albumNicknamesByAccount.value = nextNicknames
    recentAlbums.value = (await ConfigManager.GetRecentAlbums()) || []

    // Default expand all enabled folders and sync nickname drafts
    for (const fc of watchedConfigs.value) {
      if (expandedFolders.value[fc.path] === undefined) {
        expandedFolders.value[fc.path] = fc.enabled
      }
      syncNicknameDraft(fc)
    }
  } catch (err) {
    console.error('Failed to load folders config:', err)
  } finally {
    isLoading.value = false
  }
}

async function onToggleMasterSwitch(enabled: boolean) {
  const prev = watchFoldersEnabled.value
  watchFoldersEnabled.value = enabled
  try {
    await ConfigManager.SetWatchFoldersEnabled(enabled)
    toast.success(enabled ? t('folders.toastWatcherActive') : t('folders.toastWatcherDisabled'))
  } catch (err) {
    watchFoldersEnabled.value = prev
    console.error('Failed to toggle folder watcher:', err)
    toast.error(t('folders.toastWatcherError'))
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
      toast.info(t('folders.toastFolderExists'))
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
      toast.success(t('folders.toastFolderAdded', { name: folderBasename(folder) }))
    }
  } catch (err) {
    console.error('Failed to add watched folder:', err)
    toast.error(t('folders.toastFolderAddError'))
  } finally {
    isAddingFolder.value = false
  }
}

async function confirmBacklog(uploadBacklog: boolean) {
  if (!pendingBacklog.value || isConfirmingBacklog.value) return
  const { folder, account } = pendingBacklog.value
  isConfirmingBacklog.value = true
  try {
    await ConfigManager.AddWatchedFolderWithAccount(folder, account)
    await loadData()
    if (uploadBacklog) {
      emit('uploadFolder', folder)
      toast.success(t('folders.toastFolderBackupStarted', { name: folderBasename(folder) }))
    } else {
      toast.success(t('folders.toastFolderWatched', { name: folderBasename(folder) }))
    }
  } catch (err) {
    console.error('Failed to add watched folder with backlog:', err)
    toast.error(t('folders.toastFolderWatchError'))
  } finally {
    isConfirmingBacklog.value = false
    pendingBacklog.value = null
  }
}

function cancelBacklog() {
  if (isConfirmingBacklog.value) return
  pendingBacklog.value = null
}

async function toggleFolderEnabled(cfg: WatchedFolderConfig) {
  const prevSnapshot = { ...cfg }
  cfg.enabled = !cfg.enabled
  if (cfg.enabled) {
    expandedFolders.value[cfg.path] = true
  }
  await saveFolderConfig(cfg, prevSnapshot)
}

async function toggleIncludeSubfolders(cfg: WatchedFolderConfig) {
  if (!cfg.includeSubfolders && isBroadDirectory(cfg.path)) {
    pendingBroadFolder.value = cfg
    showBroadWarning.value = true
    return
  }
  const prevSnapshot = { ...cfg }
  cfg.includeSubfolders = !cfg.includeSubfolders
  await saveFolderConfig(cfg, prevSnapshot)
}

async function confirmBroadFolder() {
  const folder = pendingBroadFolder.value
  pendingBroadFolder.value = null
  showBroadWarning.value = false
  if (folder) {
    const prevSnapshot = { ...folder }
    folder.includeSubfolders = true
    await saveFolderConfig(folder, prevSnapshot)
  }
}

function cancelBroadFolder() {
  pendingBroadFolder.value = null
  showBroadWarning.value = false
}

async function updateFolderField(cfg: WatchedFolderConfig, patch: Partial<WatchedFolderConfig>) {
  const prevSnapshot = { ...cfg }
  Object.assign(cfg, patch)
  syncNicknameDraft(cfg)
  await saveFolderConfig(cfg, prevSnapshot)
}

async function saveFolderConfig(cfg: WatchedFolderConfig, prevSnapshot?: WatchedFolderConfig) {
  try {
    await ConfigManager.UpdateWatchedFolderConfig(cfg)
    syncNicknameDraft(cfg)
    inotifyError.value = (await ConfigManager.GetInotifyError()) || ''
  } catch (err) {
    if (prevSnapshot) {
      Object.assign(cfg, prevSnapshot)
      syncNicknameDraft(cfg)
    } else {
      await loadData()
    }
    console.error('Failed to update folder configuration:', err)
    toast.error(t('folders.toastSettingsError'))
  }
}

async function removeFolder(cfg: WatchedFolderConfig) {
  try {
    await ConfigManager.RemoveWatchedFolder(cfg.path)
    watchedConfigs.value = watchedConfigs.value.filter((c) => c.path !== cfg.path)
    delete expandedFolders.value[cfg.path]
    delete nicknameDrafts.value[cfg.path]
    inotifyError.value = (await ConfigManager.GetInotifyError()) || ''
    toast.success(t('folders.toastFolderRemoved', { name: folderBasename(cfg.path) }))
  } catch (err) {
    console.error('Failed to remove watched folder:', err)
    toast.error(t('folders.toastFolderRemoveError'))
  }
}

async function onUploadNow(cfg: WatchedFolderConfig) {
  try {
    await ConfigManager.StartWatchedFolderUpload(cfg.path)
    toast.success(t('folders.toastBackupQueued', { name: folderBasename(cfg.path) }))
  } catch (err) {
    console.error('Failed to start watched folder backup:', err)
    toast.error(t('backup.toastStartFailed'), {
      description: err instanceof Error ? err.message : String(err),
    })
  }
}

async function saveAlbumNickname(accountEmail: string, key: string, nickname: string, folderPath?: string) {
  const targetAccount = accountEmail || selectedAccount.value
  if (!targetAccount || !key.trim()) return
  try {
    await ConfigManager.SetAlbumNickname(targetAccount, key.trim(), nickname.trim())
    albumNicknamesByAccount.value = {
      ...albumNicknamesByAccount.value,
      [targetAccount]: (await ConfigManager.GetAlbumNicknames(targetAccount)) || {},
    }
    if (folderPath) {
      nicknameDrafts.value[folderPath] = getAlbumNickname(key, targetAccount)
    }
    toast.success(t('folders.toastNicknameSaved'))
  } catch (err) {
    console.error('Failed to save album nickname:', err)
    toast.error(t('folders.toastSettingsError'))
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
          {{ t('folders.title') }}
        </h1>
        <p class="m3-body-medium mt-0.5">
          {{ t('folders.subtitle') }}
        </p>
      </div>

      <M3Button
        variant="filled"
        size="s"
        icon="add"
        :disabled="isAddingFolder"
        @click="addFolder"
      >
        <span>{{ isAddingFolder ? t('folders.selecting') : t('folders.addFolder') }}</span>
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
            {{ t('folders.autoWatch') }}
          </label>
          <span class="m3-body-medium mt-0.5 leading-relaxed">
            {{ t('folders.autoWatchSubtitle') }}
          </span>
        </div>
        <M3Switch
          id="master-watch-folders-switch"
          :model-value="watchFoldersEnabled"
          :aria-label="t('folders.autoWatchToggleAria')"
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
          <span class="m3-title-medium text-[var(--md-sys-color-on-surface)]">{{ t('folders.watcherWarning') }}</span>
          <span>{{ inotifyError }}</span>
        </div>
      </div>
    </div>

    <!-- Folders List Section -->
    <div class="flex flex-col gap-3">
      <div class="flex items-center justify-between px-1">
        <span class="m3-label-small text-[var(--md-sys-color-on-surface-variant)]">
          {{ t('folders.watchedDirs', { count: watchedConfigs.length }) }}
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
            {{ t('folders.noFolders') }}
          </h3>
          <p class="m3-body-medium leading-relaxed">
            {{ t('folders.noFoldersSubtitle') }}
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
          {{ t('folders.chooseFolder') }}
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
                    {{ t('folders.subfolders') }}
                  </span>

                  <!-- Album Mode Badge -->
                  <span
                    v-if="cfg.albumMode === 'auto' && cfg.enabled"
                    class="rounded-full bg-[var(--md-sys-color-secondary-container)] px-2 py-0.5 m3-label-small normal-case text-[var(--md-sys-color-on-secondary-container)]"
                  >
                    {{ t('folders.autoAlbum') }}
                  </span>
                  <span
                    v-else-if="cfg.albumMode === 'custom' && cfg.albumNameOrKey && cfg.enabled"
                    class="rounded-full bg-[var(--md-sys-color-secondary-container)] px-2 py-0.5 m3-label-small normal-case text-[var(--md-sys-color-on-secondary-container)] truncate max-w-[140px]"
                    :title="cfg.albumNameOrKey"
                  >
                    {{ getAlbumLabel(cfg.albumNameOrKey, resolveFolderAccount(cfg)) }}
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
              <!-- Back Up Now Button -->
              <button
                type="button"
                class="flex size-8 cursor-pointer items-center justify-center rounded-full text-[var(--md-sys-color-on-surface-variant)] hover:bg-[var(--md-sys-color-surface-container-high)] hover:text-[var(--md-sys-color-primary)] transition-colors"
                :title="t('folders.backupFolderAria')"
                :aria-label="t('folders.backupFolderAria')"
                @click="onUploadNow(cfg)"
              >
                <M3Icon name="backup" :size="18" />
              </button>

              <!-- Enable/Disable Switch -->
              <M3Switch
                :model-value="cfg.enabled"
                :aria-label="t('folders.toggleWatchAria', { name: folderBasename(cfg.path) })"
                @update:model-value="toggleFolderEnabled(cfg)"
              />

              <!-- Remove Folder Button -->
              <button
                type="button"
                class="flex size-8 cursor-pointer items-center justify-center rounded-full text-[var(--md-sys-color-on-surface-variant)] hover:bg-[var(--md-sys-color-error-container)] hover:text-[var(--md-sys-color-error)] transition-colors"
                :title="t('folders.removeFolderAria')"
                :aria-label="t('folders.removeFolderAria')"
                @click="removeFolder(cfg)"
              >
                <M3Icon name="delete" :size="18" />
              </button>

              <!-- Expand/Collapse Chevron -->
              <button
                type="button"
                class="flex size-8 cursor-pointer items-center justify-center rounded-full text-[var(--md-sys-color-on-surface-variant)] hover:bg-[var(--md-sys-color-surface-container-high)] transition-transform duration-200"
                :class="expandedFolders[cfg.path] ? 'rotate-180' : ''"
                :aria-label="t('folders.toggleOptionsAria')"
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
                      {{ t('folders.includeSubfolders') }}
                    </span>
                    <span class="m3-body-medium mt-0.5">
                      {{ t('folders.subfoldersDesc') }}
                    </span>
                  </div>
                  <M3Switch
                    :model-value="cfg.includeSubfolders"
                    :aria-label="t('folders.includeSubfoldersAria', { name: folderBasename(cfg.path) })"
                    @update:model-value="toggleIncludeSubfolders(cfg)"
                  />
                </div>

                <!-- 2. Target Album Option -->
                <div class="flex flex-col gap-3 rounded-2xl bg-[var(--md-sys-color-surface-container-lowest)] p-3.5 border border-[var(--md-sys-color-outline-variant)]/20">
                  <div class="flex flex-col">
                    <span class="m3-title-medium text-[var(--md-sys-color-on-surface)]">
                      {{ t('folders.destAlbum') }}
                    </span>
                    <span class="m3-body-medium mt-0.5">
                      {{ t('folders.destAlbumDesc') }}
                    </span>
                  </div>

                  <!-- Mode Buttons -->
                  <div class="flex flex-wrap items-center gap-2">
                    <M3Button
                      size="xs"
                      :toggle="true"
                      :selected="cfg.albumMode === 'none' || !cfg.albumMode"
                      :variant="cfg.albumMode === 'none' || !cfg.albumMode ? 'filled' : 'outlined'"
                      @click="updateFolderField(cfg, { albumMode: 'none' })"
                    >
                      {{ t('folders.modeNone') }}
                    </M3Button>
                    <M3Button
                      size="xs"
                      :toggle="true"
                      :selected="cfg.albumMode === 'auto'"
                      :variant="cfg.albumMode === 'auto' ? 'filled' : 'outlined'"
                      @click="updateFolderField(cfg, { albumMode: 'auto' })"
                    >
                      {{ t('folders.modeAuto') }}
                    </M3Button>
                    <M3Button
                      size="xs"
                      :toggle="true"
                      :selected="cfg.albumMode === 'custom'"
                      :variant="cfg.albumMode === 'custom' ? 'filled' : 'outlined'"
                      @click="updateFolderField(cfg, { albumMode: 'custom' })"
                    >
                      {{ t('folders.modeCustom') }}
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
                      :label="t('folders.albumInputLabel')"
                      :placeholder="t('folders.albumInputPlaceholder')"
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
                          {{ t('dialogs.album.nickname') }}
                        </span>
                        <span class="m3-body-medium leading-relaxed">
                          {{ t('folders.albumNicknameDesc') }}
                        </span>
                      </div>
                      <div class="pt-2">
                        <M3TextField
                          :id="`album-nickname-${cfg.path}`"
                          v-model="nicknameDrafts[cfg.path]"
                          :label="t('dialogs.album.nicknameLabel')"
                          :placeholder="t('dialogs.album.nicknamePlaceholder')"
                          variant="outlined"
                          leading-icon="label"
                          @change="(val: string) => saveAlbumNickname(resolveFolderAccount(cfg), cfg.albumNameOrKey, val, cfg.path)"
                        />
                      </div>
                    </div>

                    <!-- Recent Albums Chips -->
                    <div
                      v-if="recentAlbums.length > 0"
                      class="flex flex-col gap-1.5 pt-1"
                    >
                      <span class="m3-label-small text-[var(--md-sys-color-on-surface-variant)]">
                        {{ t('dialogs.album.recentAlbums') }}
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
                          @click="updateFolderField(cfg, { albumNameOrKey: recent })"
                        >
                          <span class="max-w-[130px] truncate">{{ getAlbumLabel(recent, resolveFolderAccount(cfg)) }}</span>
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
                      {{ t('folders.account') }}
                    </span>
                    <span class="m3-body-medium mt-0.5">
                      {{ t('folders.accountDesc') }}
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
                        {{ t('folders.defaultAccount', { account: selectedAccount || t('about.active') }) }}
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
      :loading="isConfirmingBacklog"
      @cancel="cancelBacklog"
      @confirm="confirmBacklog"
    />

    <!-- Broad Folder Confirmation Modal (M3E Alert Dialog) -->
    <Teleport to="body">
      <Transition name="m3-dialog-modal">
        <div
          v-if="showBroadWarning"
          class="m3-dialog-wrapper fixed inset-0 z-50 grid place-items-center p-4 select-none overflow-y-auto"
          @click.self="cancelBroadFolder"
        >
          <div
            ref="broadWarningDialogCardRef"
            tabindex="-1"
            class="m3-dialog-card relative flex flex-col gap-5 w-full max-w-[440px] rounded-[28px] bg-[var(--md-sys-color-surface-container-high)] p-6 shadow-2xl outline-none focus-visible:outline-[3px] focus-visible:outline-[var(--md-sys-color-primary)]"
            role="alertdialog"
            aria-modal="true"
            aria-labelledby="broad-warning-title"
            aria-describedby="broad-warning-desc"
          >
            <!-- M3E Centered Hero Icon + Headline -->
            <div class="flex flex-col items-center text-center gap-3">
              <div class="flex size-14 items-center justify-center rounded-[20px] bg-[var(--md-sys-color-error-container)] text-[var(--md-sys-color-on-error-container)]">
                <M3Icon
                  name="warning"
                  :filled="true"
                  :size="28"
                />
              </div>
              <div class="flex flex-col gap-1.5">
                <h3
                  id="broad-warning-title"
                  class="m3-headline-small text-[var(--md-sys-color-on-surface)]"
                >
                  {{ t('folders.broadWarningTitle') }}
                </h3>
                <p
                  id="broad-warning-desc"
                  class="m3-body-medium leading-relaxed text-[var(--md-sys-color-on-surface-variant)]"
                >
                  {{ t('folders.broadWarningP1', { path: pendingBroadFolder?.path }) }}
                </p>
              </div>
            </div>

            <!-- Tonal Warning Callout -->
            <div class="flex items-start gap-3 rounded-[16px] bg-[var(--md-sys-color-surface-container)] px-4 py-3.5 text-[var(--md-sys-color-on-surface)]">
              <M3Icon
                name="folder_special"
                :filled="true"
                :size="20"
                class="shrink-0 mt-0.5 text-[var(--md-sys-color-error)]"
              />
              <p class="m3-body-medium leading-snug text-[var(--md-sys-color-on-surface-variant)]">
                {{ t('folders.broadWarningP2') }}
              </p>
            </div>

            <!-- M3E Action Row -->
            <div class="flex flex-col sm:flex-row items-center justify-end gap-2 pt-1">
              <M3Button
                variant="text"
                size="s"
                class="w-full sm:w-auto"
                @click="cancelBroadFolder"
              >
                {{ t('settings.cancel') }}
              </M3Button>
              <M3Button
                variant="filled"
                size="m"
                class="w-full sm:w-auto"
                @click="confirmBroadFolder"
              >
                {{ t('folders.enableSubfolders') }}
              </M3Button>
            </div>
          </div>
        </div>
      </Transition>
    </Teleport>
  </div>
</template>
