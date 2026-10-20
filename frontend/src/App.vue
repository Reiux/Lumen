<script setup lang="ts">
import { computed, nextTick, onMounted, onUnmounted, ref, watch } from 'vue'
import { useColorMode, useWindowSize } from '@vueuse/core'
import { toast } from 'vue-sonner'
import { Events } from '@wailsio/runtime'
import { ConfigManager } from '../bindings/app/core'
import M3Button from './components/ui/M3Button.vue'
import M3TextField from './components/ui/M3TextField.vue'
import { TooltipProvider } from '@/components/ui/tooltip'
import GoogleAccountSelect from './components/GoogleAccountSelect.vue'
import GoogleAuthSetup from './components/GoogleAuthSetup.vue'
import SettingsPanel from './SettingsPanel.vue'
import AboutView from './AboutView.vue'
import Upload from './Upload.vue'
import UploadResultsCard from './components/UploadResultsCard.vue'
import DevMockBar from './components/DevMockBar.vue'
import Toaster from './components/ui/sonner/Sonner.vue'
import AppLaunchSplash from './components/AppLaunchSplash.vue'
import M3Icon from './components/ui/M3Icon.vue'
import M3ExtendedFab from './components/ui/M3ExtendedFab.vue'
import { launchReplayTrigger } from './utils/launchState'
import { uploadManager } from './utils/UploadManager'
import { mockState } from './utils/mockMode'
import { cycleNextTheme, themePulseCounter } from './utils/theme'
import {
  displayNameFromEmail,
  getAvatarColor,
  getCustomAvatar,
  getCustomDisplayName,
  getEmailInitial,
  processImageFile,
  saveCustomAvatar,
  saveCustomDisplayName,
} from './utils/avatar'
import './index.css'

useColorMode().value = 'dark'

const { state: uploadState } = uploadManager
const copyButtonText = ref('Copy as JSON')

// Navigation state: upload, settings, or about
const activeTab = ref<'upload' | 'settings' | 'about'>('upload')
const tabOrder = ['upload', 'settings', 'about'] as const
const activeTabIndex = computed(() => {
  const idx = tabOrder.indexOf(activeTab.value)
  return idx >= 0 ? idx : 0
})

// Tablet collapsible navigation state
const { width: windowWidth } = useWindowSize()
const isSidebarCollapsedByUser = ref<boolean | null>(null)
const isSidebarExpanded = computed({
  get() {
    if (isSidebarCollapsedByUser.value !== null) {
      return !isSidebarCollapsedByUser.value
    }
    return windowWidth.value >= 860
  },
  set(val: boolean) {
    isSidebarCollapsedByUser.value = !val
  },
})

function toggleSidebar() {
  isSidebarExpanded.value = !isSidebarExpanded.value
}

// Drag state for dual/triple drop zones
const isDraggingFiles = ref(false)

// Album upload flow state
const showAlbumInput = ref(false)
const pendingFiles = ref<string[]>([])
const pendingFileCount = ref(0)

const selectedOption = ref('')
const options = ref<string[]>([])
const accountNeedsTokenBinding = ref<Record<string, boolean>>({})
const albumNameOrKey = ref('')
const recentAlbums = ref<string[]>([])
const tokenBindingEmail = ref('')
const isExtractingTokenBinding = ref(false)
const isAccountSetupOpen = ref(false)
const showEditNameDialog = ref(false)
const editingDisplayName = ref('')

function openEditNameDialog() {
  if (!displaySelectedOption.value) return
  editingDisplayName.value = getCustomDisplayName(displaySelectedOption.value) || displayNameFromEmail(displaySelectedOption.value)
  showEditNameDialog.value = true
}

function closeEditNameDialog() {
  showEditNameDialog.value = false
}

function saveNewDisplayName() {
  if (!displaySelectedOption.value) return
  saveCustomDisplayName(displaySelectedOption.value, editingDisplayName.value.trim())
  showEditNameDialog.value = false
  toast.success('Display name updated')
}

function resetDisplayName() {
  if (!displaySelectedOption.value) return
  saveCustomDisplayName(displaySelectedOption.value, '')
  showEditNameDialog.value = false
  toast.success('Display name reset to default')
}
const removingAccount = ref('')
const isSettling = ref(false)
const settleStagger = ref(40)
const isSplashActive = ref(typeof window !== 'undefined' && !(window as unknown as { __LUMEN_LAUNCHED__?: boolean }).__LUMEN_LAUNCHED__)

const isLogoPulsing = ref(false)
let logoPulseTimeout: ReturnType<typeof setTimeout> | null = null

watch(themePulseCounter, () => {
  isLogoPulsing.value = true
  if (logoPulseTimeout) clearTimeout(logoPulseTimeout)
  logoPulseTimeout = setTimeout(() => {
    isLogoPulsing.value = false
  }, 500)
})

function onLaunchStart() {
  isSplashActive.value = true
}

function onLaunchSettle(stagger?: number) {
  if (typeof stagger === 'number') {
    settleStagger.value = stagger
  }
  isSettling.value = true
  const maxDelay = (settleStagger.value * 3) + 600
  setTimeout(() => {
    isSettling.value = false
  }, maxDelay)
}

function onLaunchFinish() {
  isSplashActive.value = false
}

watch(launchReplayTrigger, () => {
  isSplashActive.value = true
  isSettling.value = false
})

const displayOptions = computed(() => {
  if (import.meta.env.DEV && mockState.enabled) {
    return mockState.currentView === 'empty' ? [] : mockState.accounts
  }
  return options.value
})

const displaySelectedOption = computed({
  get() {
    if (import.meta.env.DEV && mockState.enabled) {
      return mockState.currentView === 'empty' ? '' : mockState.account
    }
    return selectedOption.value
  },
  set(val: string) {
    if (import.meta.env.DEV && mockState.enabled) {
      mockState.account = val
    } else {
      selectedOption.value = val
    }
  },
})

// Avatar state
const customAvatar = ref<string | null>(null)
const avatarFileInput = ref<HTMLInputElement | null>(null)

function updateCustomAvatar(email: string) {
  customAvatar.value = getCustomAvatar(email)
}

watch(selectedOption, async (newValue) => {
  if (newValue) {
    updateCustomAvatar(newValue)
    try {
      await ConfigManager.SetSelected(newValue)
      await updateTokenBindingPrompt(newValue)
      console.log('Successfully updated selected value:', newValue)
    } catch (error) {
      console.error('Failed to update selected value:', error)
      toast.error('Failed to update selected account.')
    }
  } else {
    tokenBindingEmail.value = ''
    customAvatar.value = null
  }
})

watch(displaySelectedOption, (newValue) => {
  if (newValue) {
    updateCustomAvatar(newValue)
  } else {
    customAvatar.value = null
  }
})

watch(activeTab, async (newTab) => {
  await nextTick()
  const pane = document.querySelector<HTMLElement>(`.m3-tab-pane[data-tab="${newTab}"]`)
  if (pane) {
    pane.scrollTop = 0
  }
})

watch(
  () => mockState.currentView,
  (view) => {
    if (!import.meta.env.DEV || !mockState.enabled) return
    if (view === 'album') {
      showAlbumInput.value = true
      pendingFileCount.value = 3
    } else {
      showAlbumInput.value = false
    }
  }
)

async function updateTokenBindingPrompt(email: string) {
  tokenBindingEmail.value = accountNeedsTokenBinding.value[email] ? email : ''
}

async function refreshCredentials() {
  try {
    const state = await ConfigManager.GetAccounts()
    const nextNeedsTokenBinding: Record<string, boolean> = {}
    const nextOptions = state.accounts.map((account) => {
      nextNeedsTokenBinding[account.email] = account.needsTokenBinding
      return account.email
    })

    accountNeedsTokenBinding.value = nextNeedsTokenBinding
    options.value = nextOptions
    selectedOption.value = state.selected || ''
    if (state.selected) {
      await updateTokenBindingPrompt(state.selected)
      updateCustomAvatar(state.selected)
    } else {
      tokenBindingEmail.value = ''
      customAvatar.value = null
    }
  } catch (error) {
    console.error('Failed to refresh Google accounts:', error)
    toast.error('Could not refresh Google accounts', {
      description: error instanceof Error ? error.message : String(error),
    })
  }
}

async function addTokenBindingAliasFromADB() {
  if (!tokenBindingEmail.value) return
  if (import.meta.env.DEV && mockState.enabled) {
    toast.success('Token binding key added.')
    tokenBindingEmail.value = ''
    return
  }

  isExtractingTokenBinding.value = true
  try {
    await ConfigManager.AddTokenBindingAliasFromADB(tokenBindingEmail.value)
    await refreshCredentials()
    tokenBindingEmail.value = ''
    toast.success('Token binding key added.')
  } catch (error) {
    console.error('Failed to add token binding key:', error)
    toast.error('Failed to add token binding key', {
      description: error instanceof Error ? error.message : String(error),
    })
  } finally {
    isExtractingTokenBinding.value = false
  }
}

const removeCredentials = async (email: string) => {
  if (removingAccount.value) {
    return false
  }

  if (import.meta.env.DEV && mockState.enabled) {
    mockState.accounts = mockState.accounts.filter((account) => account !== email)
    if (mockState.account === email) {
      mockState.account = mockState.accounts[0] || ''
    }
    toast.success('Credentials removed (Mock Mode).')
    return true
  }

  removingAccount.value = email
  try {
    await ConfigManager.RemoveCredentials(email)

    const removedSelectedAccount = selectedOption.value === email
    await refreshCredentials()
    if (removedSelectedAccount && options.value.length > 0 && !selectedOption.value) {
      selectedOption.value = options.value[0]
    }
    toast.success('Credentials removed.')
    return true
  } catch (error) {
    console.error('Failed to remove credentials:', error)
    toast.error('Failed to remove credentials.')
    return false
  } finally {
    removingAccount.value = ''
  }
}

function openAccountSetup() {
  isAccountSetupOpen.value = true
}

const handleCopyClick = () => {
  uploadManager.copyResultsAsJson()
  copyButtonText.value = 'Copied!'
  setTimeout(() => (copyButtonText.value = 'Copy as JSON'), 1000)
}

// Global drag event handlers to detect file dragging
let dragLeaveTimeout: ReturnType<typeof setTimeout> | null = null

const onDragEnter = (e: DragEvent) => {
  if (!e.dataTransfer?.types.includes('Files')) return

  if (dragLeaveTimeout) {
    clearTimeout(dragLeaveTimeout)
    dragLeaveTimeout = null
  }

  isDraggingFiles.value = true
}

const onDragOver = (e: DragEvent) => {
  if (!e.dataTransfer?.types.includes('Files')) return
  e.preventDefault()

  if (dragLeaveTimeout) {
    clearTimeout(dragLeaveTimeout)
    dragLeaveTimeout = null
  }
}

const onDragLeave = (e: DragEvent) => {
  if (!e.dataTransfer?.types.includes('Files')) return

  if (dragLeaveTimeout) {
    clearTimeout(dragLeaveTimeout)
  }
  dragLeaveTimeout = setTimeout(() => {
    isDraggingFiles.value = false
    dragLeaveTimeout = null
  }, 50)
}

const onDrop = () => {
  if (dragLeaveTimeout) {
    clearTimeout(dragLeaveTimeout)
    dragLeaveTimeout = null
  }

  setTimeout(() => {
    isDraggingFiles.value = false
  }, 100)
}

// Album upload confirmation
async function loadRecentAlbums() {
  try {
    recentAlbums.value = (await ConfigManager.GetRecentAlbums()) || []
  } catch (err) {
    console.error('Failed to load recent albums', err)
  }
}

async function removeRecentAlbum(name: string, e: Event) {
  e.stopPropagation()
  await ConfigManager.RemoveRecentAlbum(name)
  await loadRecentAlbums()
}

const confirmAlbumUpload = async () => {
  const chosenAlbum = albumNameOrKey.value.trim()
  if (chosenAlbum) {
    await ConfigManager.AddRecentAlbum(chosenAlbum)
  }

  if (import.meta.env.DEV && mockState.enabled) {
    showAlbumInput.value = false
    pendingFiles.value = []
    pendingFileCount.value = 0
    albumNameOrKey.value = ''
    mockState.currentView = 'uploading'
    uploadManager.state.isUploading = true
    return
  }

  await ConfigManager.SetAlbumName(albumNameOrKey.value)
  await ConfigManager.SetAlbumAutoMode(false)
  Events.Emit('startUpload', { files: pendingFiles.value })
  showAlbumInput.value = false
  pendingFiles.value = []
  pendingFileCount.value = 0
  albumNameOrKey.value = ''
}

const cancelAlbumUpload = () => {
  showAlbumInput.value = false
  pendingFiles.value = []
  pendingFileCount.value = 0
  albumNameOrKey.value = ''
}

async function onUploadFolderFromSettings(folderPath: string) {
  activeTab.value = 'upload'
  uploadManager.resetUploadResults()
  await Events.Emit('startUpload', { files: [folderPath] })
}

const isPickingMedia = ref(false)

async function handlePickFiles() {
  if (isPickingMedia.value) return
  isPickingMedia.value = true
  try {
    if (import.meta.env.DEV && mockState.enabled) {
      mockState.currentView = 'uploading'
      uploadManager.state.isUploading = true
      return
    }
    const files = await ConfigManager.PickUploadFiles()
    if (files && files.length > 0) {
      uploadManager.resetUploadResults()
      await ConfigManager.SetAlbumName('')
      await ConfigManager.SetAlbumAutoMode(false)
      await Events.Emit('startUpload', { files })
    }
  } catch (err) {
    console.error('Failed to pick files:', err)
  } finally {
    isPickingMedia.value = false
  }
}

async function handlePickFolder() {
  if (isPickingMedia.value) return
  isPickingMedia.value = true
  try {
    if (import.meta.env.DEV && mockState.enabled) {
      mockState.currentView = 'uploading'
      uploadManager.state.isUploading = true
      return
    }
    const folder = await ConfigManager.PickUploadFolder()
    if (folder) {
      uploadManager.resetUploadResults()
      await ConfigManager.SetAlbumName('')
      await ConfigManager.SetAlbumAutoMode(false)
      await Events.Emit('startUpload', { files: [folder] })
    }
  } catch (err) {
    console.error('Failed to pick folder:', err)
  } finally {
    isPickingMedia.value = false
  }
}

async function handleRetryAll() {
  if (import.meta.env.DEV && mockState.enabled) {
    mockState.currentView = 'uploading'
    uploadManager.state.isUploading = true
    return
  }
  await uploadManager.retryFailed()
}

async function handleRetryItem(filePath: string) {
  if (import.meta.env.DEV && mockState.enabled) {
    mockState.currentView = 'uploading'
    uploadManager.state.isUploading = true
    return
  }
  await uploadManager.retryFailed([filePath])
}

const albumErrorHandler = (e: Event) => {
  const event = e as CustomEvent<{ AlbumName: string; Error: string }>
  const { AlbumName, Error } = event.detail
  if (Error.includes('404')) {
    toast.error('Album not found', {
      description: `The album key "${AlbumName}" does not exist or is invalid.`,
    })
  } else {
    toast.error('Failed to create album', {
      description: `Album "${AlbumName}": ${Error}`,
    })
  }
}

const uploadErrorHandler = (e: Event) => {
  const event = e as CustomEvent<{ FileName: string; Message: string }>
  const { FileName, Message } = event.detail
  const errorMessage = Message.replace(/^Error:\s*/, '')
  toast.error(FileName ? `Upload failed: ${FileName}` : 'Upload failed', {
    description: errorMessage,
    duration: 10000,
    important: true,
  })
}

watch(showAlbumInput, async (isOpen) => {
  if (isOpen) {
    await loadRecentAlbums()
  }
})

async function onAvatarFileSelected(event: Event) {
  const target = event.target as HTMLInputElement
  const file = target.files?.[0]
  if (!file || !displaySelectedOption.value) return

  try {
    const resizedDataUrl = await processImageFile(file)
    saveCustomAvatar(displaySelectedOption.value, resizedDataUrl)
    customAvatar.value = resizedDataUrl
    toast.success('Profile photo updated.')
  } catch (err) {
    console.error('Failed to update avatar:', err)
    toast.error('Could not update profile photo.')
  } finally {
    target.value = ''
  }
}

function triggerAvatarUpload() {
  if (displaySelectedOption.value && avatarFileInput.value) {
    avatarFileInput.value.click()
  }
}

let onGlobalKeydown: ((e: KeyboardEvent) => void) | null = null

onMounted(async () => {
  await refreshCredentials()

  document.addEventListener('dragenter', onDragEnter)
  document.addEventListener('dragleave', onDragLeave)
  document.addEventListener('dragover', onDragOver)
  document.addEventListener('drop', onDrop)
  window.addEventListener('albumError', albumErrorHandler)
  window.addEventListener('uploadError', uploadErrorHandler)

  onGlobalKeydown = (e: KeyboardEvent) => {
    if (e.key === 'Escape') {
      if (showEditNameDialog.value) {
        e.stopPropagation()
        closeEditNameDialog()
        return
      }
      if (showAlbumInput.value) {
        e.stopPropagation()
        cancelAlbumUpload()
      }
    }
  }
  window.addEventListener('keydown', onGlobalKeydown)

  Events.On('files-dropped', async (event: { data: { files: string[]; dropZone: string } }) => {
    const { files, dropZone } = event.data

    if (import.meta.env.DEV && mockState.enabled) {
      if (dropZone === 'album') {
        pendingFiles.value = files
        pendingFileCount.value = files.length
        showAlbumInput.value = true
      } else {
        mockState.currentView = 'uploading'
      }
      return
    }

    if (dropZone === 'album') {
      pendingFiles.value = files
      pendingFileCount.value = files.length
      showAlbumInput.value = true
    } else {
      try {
        await ConfigManager.SetAlbumName('')
        await ConfigManager.SetAlbumAutoMode(dropZone === 'auto-album')
        await Events.Emit('startUpload', { files })
      } catch (error) {
        console.error('Failed to start upload:', error)
        toast.error('Failed to start upload', {
          description: error instanceof Error ? error.message : String(error),
        })
      }
    }
  })
})

onUnmounted(() => {
  document.removeEventListener('dragenter', onDragEnter)
  document.removeEventListener('dragleave', onDragLeave)
  document.removeEventListener('dragover', onDragOver)
  document.removeEventListener('drop', onDrop)
  window.removeEventListener('albumError', albumErrorHandler)
  window.removeEventListener('uploadError', uploadErrorHandler)
  if (onGlobalKeydown) {
    window.removeEventListener('keydown', onGlobalKeydown)
  }
  if (dragLeaveTimeout) {
    clearTimeout(dragLeaveTimeout)
  }
})
</script>

<template>
  <main
    class="relative flex h-screen w-screen overflow-hidden select-none bg-[var(--md-sys-color-surface)] text-[var(--md-sys-color-on-surface)]"
    style="--wails-draggable: drag"
  >
    <!-- Navigation Sidebar (Collapsible for tablet & desktop) -->
    <aside
      class="flex h-full shrink-0 flex-col justify-between border-r border-[var(--md-sys-color-outline-variant)]/30 bg-[var(--md-sys-color-surface-container-low)] select-none transition-[width,padding] duration-200 ease-[var(--md-sys-motion-easing-spatial)]"
      :class="isSidebarExpanded ? 'w-[260px] p-4' : 'w-[76px] p-2'"
      style="contain: layout style; --wails-draggable: drag"
    >
      <div
        class="flex flex-col gap-3.5"
        style="--wails-draggable: none"
      >
        <!-- App Header / Branding with collapse toggle button -->
        <div
          class="flex items-center justify-between px-1 pt-1 pb-1"
          :class="!isSidebarExpanded && 'flex-col gap-2'"
        >
          <div class="flex items-center gap-2.5">
            <div
              id="sidebar-app-logo"
              class="flex size-9 shrink-0 items-center justify-center rounded-[14px] bg-[var(--md-sys-color-primary)] text-[var(--md-sys-color-on-primary)] shadow-sm transition-all duration-300 hover:scale-105"
              :class="[
                isSplashActive ? 'opacity-0' : 'opacity-100',
                isLogoPulsing ? 'scale-125 rotate-12 !rounded-[22px] shadow-lg shadow-[var(--md-sys-color-primary)]/50 ring-2 ring-[var(--md-sys-color-on-primary)]' : '',
              ]"
            >
              <M3Icon
                name="light_mode"
                :size="20"
              />
            </div>
            <div
              v-if="isSidebarExpanded"
              class="flex flex-col min-w-0"
            >
              <span class="text-base font-bold tracking-tight text-[var(--md-sys-color-on-surface)]">Lumen</span>
              <span class="text-[10px] font-medium uppercase tracking-wider text-[var(--md-sys-color-primary)]">Google Photos</span>
            </div>
          </div>

          <div
            class="flex items-center gap-1"
            :class="!isSidebarExpanded && 'flex-col'"
          >
            <!-- Quick Theme Palette Cycle Button -->
            <button
              type="button"
              class="inline-flex size-8 shrink-0 cursor-pointer items-center justify-center rounded-full text-[var(--md-sys-color-primary)] hover:bg-[var(--md-sys-color-primary-container)]/50 transition-colors"
              title="Cycle theme color palette"
              aria-label="Cycle theme color palette"
              @click="cycleNextTheme"
            >
              <M3Icon
                name="palette"
                :size="18"
              />
            </button>

            <!-- Collapse/Expand Toggle Button -->
            <button
              type="button"
              class="inline-flex size-8 shrink-0 cursor-pointer items-center justify-center rounded-full text-[var(--md-sys-color-on-surface-variant)] hover:bg-[var(--md-sys-color-surface-container-high)] hover:text-[var(--md-sys-color-on-surface)] transition-colors"
              :title="isSidebarExpanded ? 'Collapse navigation' : 'Expand navigation'"
              :aria-label="isSidebarExpanded ? 'Collapse navigation' : 'Expand navigation'"
              @click="toggleSidebar"
            >
              <M3Icon
                :name="isSidebarExpanded ? 'menu_open' : 'menu'"
                :size="18"
              />
            </button>
          </div>
        </div>

        <!-- Profile Card (Expanded View) -->
        <div
          v-if="isSidebarExpanded"
          class="relative flex flex-col items-center rounded-[28px] border border-[var(--md-sys-color-outline-variant)]/40 bg-[var(--md-sys-color-surface-container)] p-4 text-center shadow-xs"
          :class="isSettling && 'm3-launch-settle'"
          :style="isSettling ? { '--settle-index': 0, '--launch-stagger': settleStagger + 'ms' } : undefined"
        >
          <input
            ref="avatarFileInput"
            type="file"
            accept="image/*"
            class="hidden"
            aria-label="Upload custom profile photo"
            @change="onAvatarFileSelected"
          >

          <div
            class="group relative mb-2.5 flex size-16 cursor-pointer items-center justify-center overflow-hidden rounded-[22px] shadow-inner transition-all duration-200 hover:scale-105 hover:rounded-[16px]"
            :style="{
              backgroundColor: customAvatar ? 'transparent' : getAvatarColor(displaySelectedOption).bg,
              color: getAvatarColor(displaySelectedOption).text,
            }"
            :title="displaySelectedOption ? 'Click to change profile photo' : 'No account'"
            role="button"
            tabindex="0"
            aria-label="Change profile photo"
            @click="triggerAvatarUpload"
            @keydown.enter="triggerAvatarUpload"
          >
            <img
              v-if="customAvatar"
              :src="customAvatar"
              alt="Profile avatar"
              class="size-full object-cover"
            >
            <span
              v-else
              class="text-xl font-bold tracking-tight"
            >
              {{ getEmailInitial(displaySelectedOption) }}
            </span>

            <div
              v-if="displaySelectedOption"
              class="absolute inset-0 flex items-center justify-center bg-black/60 opacity-0 transition-opacity group-hover:opacity-100"
            >
              <M3Icon
                name="photo_camera"
                :filled="true"
                :size="20"
                class="text-white"
              />
            </div>
          </div>

          <div class="flex items-center justify-center gap-1.5 max-w-[210px] group/name">
            <h3
              class="truncate text-sm font-semibold text-[var(--md-sys-color-on-surface)] transition-colors"
              :class="displaySelectedOption && 'cursor-pointer hover:text-[var(--md-sys-color-primary)]'"
              :title="displaySelectedOption ? 'Click to change display name' : undefined"
              @click="displaySelectedOption && openEditNameDialog()"
            >
              {{ displaySelectedOption ? displayNameFromEmail(displaySelectedOption) : 'No Account' }}
            </h3>
            <button
              v-if="displaySelectedOption"
              type="button"
              class="flex size-6 shrink-0 items-center justify-center rounded-full text-[var(--md-sys-color-on-surface-variant)] transition-all hover:bg-[var(--md-sys-color-primary-container)]/50 hover:text-[var(--md-sys-color-primary)] hover:scale-110 active:scale-95 cursor-pointer"
              title="Change display name"
              aria-label="Change display name"
              @click="openEditNameDialog"
            >
              <M3Icon
                name="edit"
                :size="13"
              />
            </button>
          </div>
          <p class="max-w-[210px] truncate text-xs text-[var(--md-sys-color-on-surface-variant)]">
            {{ displaySelectedOption || 'Connect an account to upload' }}
          </p>

          <!-- Account switcher chip -->
          <div class="mt-3 w-full flex justify-center">
            <GoogleAccountSelect
              v-if="displayOptions.length > 0"
              v-model="displaySelectedOption"
              :options="displayOptions"
              :avatar-url="customAvatar"
              :removing-account="removingAccount"
              @item-removed="removeCredentials"
              @add="openAccountSetup"
            />
            <button
              v-else
              type="button"
              class="inline-flex h-8 items-center gap-1.5 rounded-full bg-[var(--md-sys-color-primary)] px-3.5 text-xs font-medium text-[var(--md-sys-color-on-primary)] shadow-xs hover:bg-[var(--md-sys-color-primary)]/90 cursor-pointer"
              @click="openAccountSetup"
            >
              <M3Icon
                name="person_add"
                :size="14"
              />
              Connect Account
            </button>
          </div>
        </div>

        <!-- Compact Avatar (Collapsed Tablet View) -->
        <div
          v-else
          class="flex flex-col items-center py-1"
        >
          <div
            class="group relative flex size-10 cursor-pointer items-center justify-center overflow-hidden rounded-[14px] shadow-inner transition-all hover:scale-105 hover:rounded-[10px] border border-[var(--md-sys-color-outline-variant)]/40"
            :style="{
              backgroundColor: customAvatar ? 'transparent' : getAvatarColor(displaySelectedOption).bg,
              color: getAvatarColor(displaySelectedOption).text,
            }"
            :title="displaySelectedOption ? `${displaySelectedOption} (click to change photo)` : 'Connect account'"
            role="button"
            tabindex="0"
            aria-label="Account photo"
            @click="displaySelectedOption ? triggerAvatarUpload() : openAccountSetup()"
          >
            <img
              v-if="customAvatar"
              :src="customAvatar"
              alt="Profile avatar"
              class="size-full object-cover"
            >
            <span
              v-else
              class="text-sm font-bold"
            >
              {{ getEmailInitial(displaySelectedOption) }}
            </span>
          </div>
        </div>

        <!-- Navigation Items (Expanded Sidebar View) -->
        <nav
          v-if="isSidebarExpanded"
          class="flex flex-col gap-1.5 pt-1"
          :class="isSettling && 'm3-launch-settle'"
          :style="isSettling ? { '--settle-index': 1, '--launch-stagger': settleStagger + 'ms' } : undefined"
          aria-label="Sidebar Navigation"
        >
          <button
            type="button"
            class="m3-nav-item group relative flex h-11 w-full cursor-pointer items-center gap-3.5 px-4 text-sm font-medium transition-colors"
            :class="activeTab === 'upload' ? 'is-selected font-semibold' : 'hover:bg-[var(--md-sys-color-surface-container-high)]/60'"
            aria-label="Upload view"
            @click="activeTab = 'upload'"
          >
            <span
              class="m3-nav-indicator absolute inset-0 rounded-full bg-[var(--md-sys-color-secondary-container)] shadow-xs pointer-events-none"
              :class="activeTab === 'upload' ? 'scale-x-100 opacity-100' : 'scale-x-0 opacity-0'"
            />
            <M3Icon
              name="cloud_upload"
              :filled="activeTab === 'upload'"
              :size="20"
              class="relative z-10 size-5 shrink-0 transition-colors duration-200"
              :class="activeTab === 'upload'
                ? 'text-[var(--md-sys-color-on-secondary-container)]'
                : 'text-[var(--md-sys-color-on-surface-variant)] group-hover:text-[var(--md-sys-color-on-surface)]'"
            />
            <span
              class="relative z-10 transition-colors duration-200"
              :class="activeTab === 'upload'
                ? 'text-[var(--md-sys-color-on-secondary-container)]'
                : 'text-[var(--md-sys-color-on-surface-variant)] group-hover:text-[var(--md-sys-color-on-surface)]'"
            >
              Upload
            </span>
          </button>

          <button
            type="button"
            class="m3-nav-item group relative flex h-11 w-full cursor-pointer items-center gap-3.5 px-4 text-sm font-medium transition-colors"
            :class="activeTab === 'settings' ? 'is-selected font-semibold' : 'hover:bg-[var(--md-sys-color-surface-container-high)]/60'"
            aria-label="Settings view"
            @click="activeTab = 'settings'"
          >
            <span
              class="m3-nav-indicator absolute inset-0 rounded-full bg-[var(--md-sys-color-secondary-container)] shadow-xs pointer-events-none"
              :class="activeTab === 'settings' ? 'scale-x-100 opacity-100' : 'scale-x-0 opacity-0'"
            />
            <M3Icon
              name="settings"
              :filled="activeTab === 'settings'"
              :size="20"
              class="relative z-10 size-5 shrink-0 transition-colors duration-200"
              :class="activeTab === 'settings'
                ? 'text-[var(--md-sys-color-on-secondary-container)]'
                : 'text-[var(--md-sys-color-on-surface-variant)] group-hover:text-[var(--md-sys-color-on-surface)]'"
            />
            <span
              class="relative z-10 transition-colors duration-200"
              :class="activeTab === 'settings'
                ? 'text-[var(--md-sys-color-on-secondary-container)]'
                : 'text-[var(--md-sys-color-on-surface-variant)] group-hover:text-[var(--md-sys-color-on-surface)]'"
            >
              Settings
            </span>
          </button>

          <button
            type="button"
            class="m3-nav-item group relative flex h-11 w-full cursor-pointer items-center gap-3.5 px-4 text-sm font-medium transition-colors"
            :class="activeTab === 'about' ? 'is-selected font-semibold' : 'hover:bg-[var(--md-sys-color-surface-container-high)]/60'"
            aria-label="About Lumen"
            @click="activeTab = 'about'"
          >
            <span
              class="m3-nav-indicator absolute inset-0 rounded-full bg-[var(--md-sys-color-secondary-container)] shadow-xs pointer-events-none"
              :class="activeTab === 'about' ? 'scale-x-100 opacity-100' : 'scale-x-0 opacity-0'"
            />
            <M3Icon
              name="info"
              :filled="activeTab === 'about'"
              :size="20"
              class="relative z-10 size-5 shrink-0 transition-colors duration-200"
              :class="activeTab === 'about'
                ? 'text-[var(--md-sys-color-on-secondary-container)]'
                : 'text-[var(--md-sys-color-on-surface-variant)] group-hover:text-[var(--md-sys-color-on-surface)]'"
            />
            <span
              class="relative z-10 transition-colors duration-200"
              :class="activeTab === 'about'
                ? 'text-[var(--md-sys-color-on-secondary-container)]'
                : 'text-[var(--md-sys-color-on-surface-variant)] group-hover:text-[var(--md-sys-color-on-surface)]'"
            >
              About
            </span>
          </button>

          <button
            type="button"
            class="m3-nav-item group relative flex h-11 w-full cursor-pointer items-center gap-3.5 px-4 text-sm font-medium text-[var(--md-sys-color-on-surface-variant)] transition-colors hover:bg-[var(--md-sys-color-surface-container-high)] hover:text-[var(--md-sys-color-on-surface)]"
            aria-label="Manage accounts"
            @click="openAccountSetup"
          >
            <M3Icon
              name="person_add"
              :size="20"
              class="relative z-10 size-5 shrink-0 transition-transform duration-200"
            />
            <span class="relative z-10">Accounts</span>
          </button>
        </nav>

        <!-- Navigation Items (Collapsed Compact Tablet Rail) -->
        <nav
          v-else
          class="flex flex-col items-center gap-2 pt-1"
          aria-label="Sidebar Compact Navigation"
        >
          <button
            type="button"
            class="group relative flex flex-col items-center justify-center size-12 rounded-2xl transition-colors cursor-pointer"
            :class="activeTab === 'upload' ? 'bg-[var(--md-sys-color-secondary-container)] text-[var(--md-sys-color-on-secondary-container)]' : 'text-[var(--md-sys-color-on-surface-variant)] hover:bg-[var(--md-sys-color-surface-container-high)]'"
            title="Upload"
            aria-label="Upload view"
            @click="activeTab = 'upload'"
          >
            <M3Icon
              name="cloud_upload"
              :filled="activeTab === 'upload'"
              :size="22"
            />
            <span class="text-[9px] font-semibold tracking-tighter leading-none mt-0.5">Upload</span>
          </button>

          <button
            type="button"
            class="group relative flex flex-col items-center justify-center size-12 rounded-2xl transition-colors cursor-pointer"
            :class="activeTab === 'settings' ? 'bg-[var(--md-sys-color-secondary-container)] text-[var(--md-sys-color-on-secondary-container)]' : 'text-[var(--md-sys-color-on-surface-variant)] hover:bg-[var(--md-sys-color-surface-container-high)]'"
            title="Settings"
            aria-label="Settings view"
            @click="activeTab = 'settings'"
          >
            <M3Icon
              name="settings"
              :filled="activeTab === 'settings'"
              :size="22"
            />
            <span class="text-[9px] font-semibold tracking-tighter leading-none mt-0.5">Settings</span>
          </button>

          <button
            type="button"
            class="group relative flex flex-col items-center justify-center size-12 rounded-2xl transition-colors cursor-pointer"
            :class="activeTab === 'about' ? 'bg-[var(--md-sys-color-secondary-container)] text-[var(--md-sys-color-on-secondary-container)]' : 'text-[var(--md-sys-color-on-surface-variant)] hover:bg-[var(--md-sys-color-surface-container-high)]'"
            title="About Lumen"
            aria-label="About Lumen"
            @click="activeTab = 'about'"
          >
            <M3Icon
              name="info"
              :filled="activeTab === 'about'"
              :size="22"
            />
            <span class="text-[9px] font-semibold tracking-tighter leading-none mt-0.5">About</span>
          </button>

          <button
            type="button"
            class="group relative flex flex-col items-center justify-center size-12 rounded-2xl transition-colors cursor-pointer text-[var(--md-sys-color-on-surface-variant)] hover:bg-[var(--md-sys-color-surface-container-high)]"
            title="Manage Accounts"
            aria-label="Manage accounts"
            @click="openAccountSetup"
          >
            <M3Icon
              name="person_add"
              :size="22"
            />
            <span class="text-[9px] font-semibold tracking-tighter leading-none mt-0.5">Accounts</span>
          </button>
        </nav>
      </div>

      <!-- Bottom info -->
      <div
        v-if="isSidebarExpanded"
        class="px-2 text-[11px] text-[var(--md-sys-color-on-surface-variant)]/70 select-none"
      >
        Lumen &bull; Google Photos Desktop
      </div>
      <div
        v-else
        class="text-center text-[10px] text-[var(--md-sys-color-on-surface-variant)]/70 select-none py-1"
      >
        &bull;
      </div>
    </aside>

    <!-- Main Content Area: Continuous Viewport -->
    <section
      class="m3-main-viewport relative flex flex-1 h-full min-w-0 flex-col overflow-hidden bg-[var(--md-sys-color-surface)]"
      :class="isSettling && 'm3-launch-settle'"
      :style="isSettling ? { '--settle-index': 2, '--launch-stagger': settleStagger + 'ms', '--wails-draggable': 'drag', contain: 'layout style' } : { '--wails-draggable': 'drag', contain: 'layout style' }"
    >
      <div
        class="m3-pan-track flex h-full w-[300%]"
        :style="{
          transform: `translate3d(-${activeTabIndex * (100 / 3)}%, 0, 0)`,
        }"
      >
        <!-- View: Upload Tab -->
        <div
          data-tab="upload"
          class="m3-tab-pane flex h-full w-[33.333333%] shrink-0 min-w-0 flex-col overflow-y-auto overflow-x-hidden p-6"
          :inert="activeTab !== 'upload' ? true : undefined"
          :aria-hidden="activeTab !== 'upload'"
          style="contain: layout style; --wails-draggable: none"
        >
          <!-- If actively uploading, show Upload.vue directly in main area -->
          <div
            v-if="uploadState.isUploading"
            class="h-full w-full"
            style="--wails-draggable: none"
          >
            <Upload />
          </div>

          <!-- Dragging files: M3 Expressive responsive drop zones -->
          <div
            v-else-if="isDraggingFiles && displayOptions.length > 0"
            class="grid h-full w-full flex-1 grid-cols-1 md:grid-cols-3 gap-4"
            style="--wails-draggable: none"
          >
            <div
              data-file-drop-target
              data-drop-zone="regular"
              class="drop-zone m3-drop-card flex flex-col items-center justify-center rounded-[32px] border border-[var(--md-sys-color-outline-variant)]/40 bg-[var(--md-sys-color-surface-container-high)] p-6 text-center cursor-pointer shadow-xs"
              :style="{ '--card-index': 0 }"
            >
              <div class="mb-3 flex size-14 items-center justify-center m3-shape-scallop bg-[var(--md-sys-color-primary-container)] text-[var(--md-sys-color-on-primary-container)] shadow-sm">
                <M3Icon
                  name="cloud_upload"
                  :size="28"
                />
              </div>
              <h2 class="text-base font-semibold text-[var(--md-sys-color-on-surface)] select-none">
                Upload Only
              </h2>
              <p class="mt-1 text-xs text-[var(--md-sys-color-on-surface-variant)] select-none max-w-[200px]">
                Upload files directly without adding to any album
              </p>
            </div>

            <div
              data-file-drop-target
              data-drop-zone="album"
              class="drop-zone m3-drop-card flex flex-col items-center justify-center rounded-[32px] border border-[var(--md-sys-color-outline-variant)]/40 bg-[var(--md-sys-color-surface-container-high)] p-6 text-center cursor-pointer shadow-xs"
              :style="{ '--card-index': 1 }"
            >
              <div class="mb-3 flex size-14 items-center justify-center m3-shape-scallop bg-[var(--md-sys-color-secondary-container)] text-[var(--md-sys-color-on-secondary-container)] shadow-sm">
                <M3Icon
                  name="folder_special"
                  :size="28"
                />
              </div>
              <h2 class="text-base font-semibold text-[var(--md-sys-color-on-surface)] select-none">
                Upload to Album
              </h2>
              <p class="mt-1 text-xs text-[var(--md-sys-color-on-surface-variant)] select-none max-w-[200px]">
                Upload and add to a specific album of your choice
              </p>
            </div>

            <div
              data-file-drop-target
              data-drop-zone="auto-album"
              class="drop-zone m3-drop-card flex flex-col items-center justify-center rounded-[32px] border border-[var(--md-sys-color-outline-variant)]/40 bg-[var(--md-sys-color-surface-container-high)] p-6 text-center cursor-pointer shadow-xs"
              :style="{ '--card-index': 2 }"
            >
              <div class="mb-3 flex size-14 items-center justify-center m3-shape-scallop bg-[var(--md-sys-color-tertiary-container)] text-[var(--md-sys-color-on-tertiary-container)] shadow-sm">
                <M3Icon
                  name="auto_awesome"
                  :size="28"
                />
              </div>
              <h2 class="text-base font-semibold text-[var(--md-sys-color-on-surface)] select-none">
                Auto Album
              </h2>
              <p class="mt-1 text-xs text-[var(--md-sys-color-on-surface-variant)] select-none max-w-[200px]">
                Create albums automatically based on folder names
              </p>
            </div>
          </div>

          <!-- No account connected empty state -->
          <div
            v-else-if="displayOptions.length === 0"
            class="flex h-full w-full flex-1 flex-col items-center justify-center text-center p-8 select-none"
            style="--wails-draggable: none"
          >
            <div class="relative mb-6 flex size-28 items-center justify-center rounded-[36px] border border-[var(--md-sys-color-primary-container)]/60 bg-[var(--md-sys-color-primary-container)]/35 text-[var(--md-sys-color-primary)] shadow-sm transition-transform duration-300 hover:scale-105">
              <M3Icon
                name="person_add"
                :filled="true"
                :size="56"
              />
            </div>
            <h1 class="text-3xl sm:text-4xl font-extrabold tracking-tight text-[var(--md-sys-color-on-surface)] select-none">
              Connect Google Photos
            </h1>
            <p class="mt-3 text-sm leading-relaxed text-[var(--md-sys-color-on-surface-variant)] select-none max-w-md">
              Sign in with your Google account to enable unlimited uploads, album synchronization, and Apple Live Photos pairing.
            </p>
            <div class="mt-6 flex flex-wrap items-center justify-center gap-2">
              <span class="rounded-full bg-[var(--md-sys-color-surface-container-high)] px-3.5 py-1 text-xs font-medium text-[var(--md-sys-color-on-surface-variant)] select-none">
                Fast multithreaded uploads
              </span>
              <span class="rounded-full bg-[var(--md-sys-color-surface-container-high)] px-3.5 py-1 text-xs font-medium text-[var(--md-sys-color-on-surface-variant)] select-none">
                Auto album organization
              </span>
              <span class="rounded-full bg-[var(--md-sys-color-surface-container-high)] px-3.5 py-1 text-xs font-medium text-[var(--md-sys-color-on-surface-variant)] select-none">
                Original & Saver quality
              </span>
            </div>
            <M3Button
              variant="filled"
              size="l"
              icon="person_add"
              class="mt-8"
              aria-label="Add Google account"
              @click="openAccountSetup"
            >
              <span>Add Google Account</span>
            </M3Button>
            <p class="mt-4 text-[11px] text-[var(--md-sys-color-on-surface-variant)]/60 select-none">
              Credentials are stored locally in your system configuration.
            </p>
          </div>

          <!-- Normal Drop View (with account connected) -->
          <div
            v-else
            class="relative flex h-full w-full flex-1 flex-col gap-4 min-h-0"
          >
            <!-- Token binding alert banner if needed -->
            <div
              v-if="tokenBindingEmail"
              class="w-full shrink-0 rounded-[24px] border border-amber-500/30 bg-amber-500/10 p-4 flex items-center justify-between gap-4"
              style="--wails-draggable: none"
            >
              <p class="text-xs text-amber-200">
                This credential needs a token binding key from the rooted Android device it was captured from.
              </p>
              <button
                type="button"
                class="inline-flex h-9 shrink-0 items-center justify-center rounded-full bg-amber-400 px-4 text-xs font-semibold text-amber-950 transition-colors hover:bg-amber-300 disabled:opacity-50 cursor-pointer"
                :disabled="isExtractingTokenBinding"
                @click="addTokenBindingAliasFromADB"
              >
                {{ isExtractingTokenBinding ? 'Reading ADB...' : 'Read from ADB' }}
              </button>
            </div>

            <!-- Large Tonal Drop Zone Card -->
            <div
              data-file-drop-target
              data-drop-zone="regular"
              class="drop-zone group relative flex flex-1 w-full min-h-[260px] flex-col items-center justify-center rounded-[32px] border border-[var(--md-sys-color-outline-variant)]/40 bg-[var(--md-sys-color-surface-container-high)] p-8 md:p-10 text-center select-none shadow-xs"
              style="--wails-draggable: none"
            >
              <div class="relative mb-5 flex size-28 items-center justify-center m3-shape-cookie bg-[var(--md-sys-color-primary-container)]/40 text-[var(--md-sys-color-primary)] shadow-xs">
                <M3Icon
                  name="cloud_upload"
                  :filled="isDraggingFiles"
                  :size="56"
                />
              </div>
              <h2 class="text-2xl font-bold text-[var(--md-sys-color-on-surface)] select-none m3-headline-medium">
                Drop files to upload
              </h2>
              <p class="mt-2 text-sm text-[var(--md-sys-color-on-surface-variant)] select-none max-w-md m3-body-medium">
                Drag photos, videos, or complete directories anywhere on this card.
              </p>
              <div class="mt-6 flex flex-wrap items-center justify-center gap-2">
                <span class="rounded-full bg-[var(--md-sys-color-surface-container-high)] px-3.5 py-1 text-xs font-medium text-[var(--md-sys-color-on-surface-variant)] select-none">
                  Apple Live Photos
                </span>
                <span class="rounded-full bg-[var(--md-sys-color-surface-container-high)] px-3.5 py-1 text-xs font-medium text-[var(--md-sys-color-on-surface-variant)] select-none">
                  Original & Saver Quality
                </span>
                <span class="rounded-full bg-[var(--md-sys-color-surface-container-high)] px-3.5 py-1 text-xs font-medium text-[var(--md-sys-color-on-surface-variant)] select-none">
                  Auto Album by Folder
                </span>
              </div>

              <!-- Floating Extended FAB for File & Folder Selection inside the card -->
              <div
                class="absolute bottom-4 right-4 md:bottom-6 md:right-6 z-20"
                style="--wails-draggable: none"
              >
                <M3ExtendedFab
                  :disabled="isPickingMedia"
                  @pick-files="handlePickFiles"
                  @pick-folder="handlePickFolder"
                />
              </div>
            </div>

            <!-- Upload Results Summary -->
            <UploadResultsCard
              v-if="uploadState.uploadedFiles > 0 || uploadState.results.fail.length > 0"
              :results="uploadState.results"
              class="shrink-0"
              style="--wails-draggable: none"
              @copy="handleCopyClick"
              @clear="() => uploadManager.resetUploadResults()"
              @retry-all="handleRetryAll"
              @retry-item="handleRetryItem"
            />
          </div>
        </div>

        <!-- View: Settings Tab -->
        <div
          data-tab="settings"
          class="m3-tab-pane flex h-full w-[33.333333%] shrink-0 min-w-0 flex-col overflow-y-auto overflow-x-hidden p-6"
          :inert="activeTab !== 'settings' ? true : undefined"
          :aria-hidden="activeTab !== 'settings'"
          style="contain: layout style; --wails-draggable: none"
        >
          <TooltipProvider disable-hoverable-content>
            <SettingsPanel @upload-folder="onUploadFolderFromSettings" />
          </TooltipProvider>
        </div>

        <!-- View: About Tab -->
        <div
          data-tab="about"
          class="m3-tab-pane flex h-full w-[33.333333%] shrink-0 min-w-0 flex-col overflow-y-auto overflow-x-hidden p-6"
          :inert="activeTab !== 'about' ? true : undefined"
          :aria-hidden="activeTab !== 'about'"
          style="contain: layout style; --wails-draggable: none"
        >
          <AboutView />
        </div>
      </div>
    </section>

    <!-- Add Account Dialog -->
    <GoogleAuthSetup
      v-model:open="isAccountSetupOpen"
      @account-added="refreshCredentials"
    />

    <!-- Album Name Input State (Modal Dialog with Scrim & Spatial Spring) -->
    <Teleport to="body">
      <Transition name="m3-dialog-modal">
        <div
          v-if="showAlbumInput"
          class="m3-dialog-wrapper fixed inset-0 z-50 grid place-items-center bg-black/70 p-4 select-none overflow-y-auto"
          style="--wails-draggable: none"
          @click.self="cancelAlbumUpload"
        >
          <div
            class="m3-dialog-card relative flex w-full max-w-md flex-col items-center justify-center gap-5 rounded-[28px] border border-[var(--md-sys-color-outline-variant)]/50 bg-[var(--md-sys-color-surface-container-high)] p-8 text-center shadow-2xl"
            role="dialog"
            aria-modal="true"
            aria-labelledby="album-dialog-title"
          >
            <div class="flex size-16 items-center justify-center rounded-full bg-[var(--md-sys-color-primary-container)] text-[var(--md-sys-color-on-primary-container)]">
              <M3Icon
                name="folder_special"
                :filled="true"
                :size="32"
              />
            </div>
            <div class="flex flex-col gap-1">
              <h2
                id="album-dialog-title"
                class="text-xl font-bold text-[var(--md-sys-color-on-surface)] select-none"
              >
                Upload to Album
              </h2>
              <p class="text-xs text-[var(--md-sys-color-on-surface-variant)] select-none">
                {{ pendingFileCount }} file(s) selected for upload
              </p>
            </div>

            <div class="w-full text-left">
              <M3TextField
                id="album-input"
                v-model="albumNameOrKey"
                label="Album name or key"
                placeholder="e.g. Vacation 2026 or AF1Qip... key"
                variant="outlined"
                leading-icon="folder"
                autofocus
                @keydown.enter="confirmAlbumUpload"
              />
            </div>

            <!-- Recent Albums Quick-select Chips -->
            <div
              v-if="recentAlbums.length > 0"
              class="flex w-full flex-col gap-1.5 text-left"
            >
              <span class="text-[11px] font-medium text-[var(--md-sys-color-on-surface-variant)]">
                Recent Albums
              </span>
              <div class="flex flex-wrap gap-1.5 max-h-24 overflow-y-auto">
                <button
                  v-for="recent in recentAlbums"
                  :key="recent"
                  type="button"
                  class="group inline-flex items-center gap-1.5 rounded-full border border-[var(--md-sys-color-outline-variant)]/60 bg-[var(--md-sys-color-surface-container-high)] px-3 py-1 text-xs text-[var(--md-sys-color-on-surface)] transition-all hover:bg-[var(--md-sys-color-primary-container)] hover:text-[var(--md-sys-color-on-primary-container)] hover:border-[var(--md-sys-color-primary)] cursor-pointer"
                  :class="albumNameOrKey === recent ? 'border-[var(--md-sys-color-primary)] bg-[var(--md-sys-color-primary-container)] text-[var(--md-sys-color-on-primary-container)]' : ''"
                  @click="albumNameOrKey = recent"
                >
                  <M3Icon
                    name="folder"
                    :filled="albumNameOrKey === recent"
                    :size="14"
                    class="shrink-0 text-[var(--md-sys-color-primary)]"
                  />
                  <span class="max-w-[130px] truncate">{{ recent }}</span>
                  <span
                    class="ml-0.5 rounded-full p-0.5 opacity-60 hover:opacity-100 hover:bg-black/10 dark:hover:bg-white/10"
                    title="Remove from recents"
                    @click="removeRecentAlbum(recent, $event)"
                  >
                    <M3Icon
                      name="close"
                      :size="12"
                    />
                  </span>
                </button>
              </div>
            </div>

            <div class="flex gap-3 mt-1">
              <M3Button
                variant="outlined"
                destructive
                size="s"
                @click="cancelAlbumUpload"
              >
                Cancel
              </M3Button>
              <M3Button
                variant="filled"
                size="s"
                :disabled="!albumNameOrKey.trim()"
                @click="confirmAlbumUpload"
              >
                Start Upload
              </M3Button>
            </div>
          </div>
        </div>
      </Transition>
    </Teleport>

    <!-- Edit Display Name Dialog (Modal Dialog with Scrim & Spatial Spring) -->
    <Teleport to="body">
      <Transition name="m3-dialog-modal">
        <div
          v-if="showEditNameDialog"
          class="m3-dialog-wrapper fixed inset-0 z-50 grid place-items-center bg-black/70 p-4 select-none overflow-y-auto"
          style="--wails-draggable: none"
          @click.self="closeEditNameDialog"
        >
          <div
            class="m3-dialog-card relative flex w-full max-w-md flex-col items-center justify-center gap-5 rounded-[28px] border border-[var(--md-sys-color-outline-variant)]/50 bg-[var(--md-sys-color-surface-container-high)] p-8 text-center shadow-2xl"
            role="dialog"
            aria-modal="true"
            aria-labelledby="edit-name-dialog-title"
          >
            <div class="flex size-16 items-center justify-center rounded-full bg-[var(--md-sys-color-primary-container)] text-[var(--md-sys-color-on-primary-container)]">
              <M3Icon
                name="badge"
                :filled="true"
                :size="32"
              />
            </div>
            <div class="flex flex-col gap-1">
              <h2
                id="edit-name-dialog-title"
                class="text-xl font-bold text-[var(--md-sys-color-on-surface)] select-none"
              >
                Change Display Name
              </h2>
              <p class="text-xs text-[var(--md-sys-color-on-surface-variant)] select-none">
                For account <strong class="text-[var(--md-sys-color-on-surface)]">{{ displaySelectedOption }}</strong>
              </p>
            </div>

            <div class="w-full text-left flex flex-col gap-1.5">
              <M3TextField
                id="edit-name-input"
                v-model="editingDisplayName"
                label="Display Name"
                placeholder="e.g. Alex"
                variant="outlined"
                leading-icon="person"
                autofocus
                @keydown.enter="saveNewDisplayName"
              />
              <span class="text-[11px] text-[var(--md-sys-color-on-surface-variant)] px-1">
                Saved locally on this device since login credentials only include an OAuth token.
              </span>
            </div>

            <div class="flex w-full items-center justify-between gap-3 mt-1">
              <button
                type="button"
                class="m3-interactive text-xs font-medium text-[var(--md-sys-color-on-surface-variant)] hover:text-[var(--md-sys-color-error)] transition-colors px-2.5 py-1.5 rounded-full cursor-pointer hover:bg-[var(--md-sys-color-surface-container-highest)]"
                @click="resetDisplayName"
              >
                Reset to default
              </button>
              <div class="flex items-center gap-2">
                <M3Button
                  variant="outlined"
                  destructive
                  size="s"
                  @click="closeEditNameDialog"
                >
                  Cancel
                </M3Button>
                <M3Button
                  variant="filled"
                  size="s"
                  @click="saveNewDisplayName"
                >
                  Save
                </M3Button>
              </div>
            </div>
          </div>
        </div>
      </Transition>
    </Teleport>

    <Teleport to="body">
      <Toaster
        position="bottom-center"
        rich-colors
        expand
        :visible-toasts="4"
      />
    </Teleport>

    <AppLaunchSplash
      @start="onLaunchStart"
      @settle="onLaunchSettle"
      @finish="onLaunchFinish"
    />

    <DevMockBar />
  </main>
</template>
