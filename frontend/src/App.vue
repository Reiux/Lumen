<script setup lang="ts">
import { computed, nextTick, onMounted, onUnmounted, ref, watch } from 'vue'
import { useColorMode, useWindowSize } from '@vueuse/core'
import { toast } from 'vue-sonner'
import { Events } from '@wailsio/runtime'
import { ConfigManager } from '../bindings/app/core'
import M3Button from './components/ui/M3Button.vue'
import M3NavigationRail from './components/ui/M3NavigationRail.vue'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { TooltipProvider } from '@/components/ui/tooltip'
import GoogleAccountSelect from './components/GoogleAccountSelect.vue'
import GoogleAuthSetup from './components/GoogleAuthSetup.vue'
import SettingsPanel from './SettingsPanel.vue'
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
import {
  getCustomAvatar,
  getEmailInitial,
  processImageFile,
  saveCustomAvatar,
} from './utils/avatar'
import './index.css'

useColorMode().value = 'dark'

const { state: uploadState } = uploadManager
const copyButtonText = ref('Copy as JSON')

// Navigation state
const activeTab = ref<'upload' | 'settings'>('upload')
const { width: windowWidth } = useWindowSize()
const isRailExpanded = computed(() => windowWidth.value >= 900)

const navRailItems = [
  { id: 'upload', icon: 'cloud_upload', label: 'Upload', ariaLabel: 'Upload view' },
  { id: 'settings', icon: 'settings', label: 'Settings', ariaLabel: 'Settings view' },
]

// Drag state for dual drop zones
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
const removingAccount = ref('')
const isSettling = ref(false)
const settleStagger = ref(40)
const isSplashActive = ref(typeof window !== 'undefined' && !(window as unknown as { __LUMEN_LAUNCHED__?: boolean }).__LUMEN_LAUNCHED__)

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

watch(activeTab, async () => {
  await nextTick()
  const scrollPane = document.querySelector('.m3-main-scroll-pane')
  if (scrollPane) {
    scrollPane.scrollTop = 0
  }
  const tabPanes = document.querySelectorAll('.m3-tab-pane')
  tabPanes.forEach((el) => {
    el.scrollTop = 0
  })
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

async function removeCredentials(email: string) {
  if (import.meta.env.DEV && mockState.enabled) {
    mockState.accounts = mockState.accounts.filter((a) => a !== email)
    if (mockState.account === email) {
      mockState.account = mockState.accounts[0] || ''
    }
    toast.success('Credentials removed.')
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

async function promptPickForAlbum() {
  if (isPickingMedia.value) return
  isPickingMedia.value = true
  try {
    if (import.meta.env.DEV && mockState.enabled) {
      pendingFiles.value = ['demo_photo_1.jpg', 'demo_photo_2.jpg']
      pendingFileCount.value = 2
      showAlbumInput.value = true
      return
    }
    const files = await ConfigManager.PickUploadFiles()
    if (files && files.length > 0) {
      pendingFiles.value = files
      pendingFileCount.value = files.length
      showAlbumInput.value = true
    }
  } catch (err) {
    console.error('Failed to pick files for album:', err)
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
    if (e.key === 'Escape' && showAlbumInput.value) {
      e.stopPropagation()
      cancelAlbumUpload()
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
  >
    <!-- Navigation Rail (Option A: expands on wider windows >=900px) -->
    <M3NavigationRail
      v-model="activeTab"
      :items="navRailItems"
      :expanded="isRailExpanded"
    >
      <template #header>
        <div class="flex items-center gap-3 px-3 py-2 cursor-default select-none">
          <div
            id="sidebar-app-logo"
            class="flex size-10 shrink-0 items-center justify-center rounded-2xl bg-[var(--md-sys-color-primary)] text-[var(--md-sys-color-on-primary)] shadow-sm transition-opacity duration-150"
            :class="isSplashActive ? 'opacity-0' : 'opacity-100'"
          >
            <M3Icon
              name="light_mode"
              :size="22"
            />
          </div>
          <div
            v-if="isRailExpanded"
            class="flex flex-col min-w-0 transition-opacity duration-200"
          >
            <span class="text-base font-bold tracking-tight text-[var(--md-sys-color-on-surface)]">Lumen</span>
            <span class="text-[10px] font-semibold uppercase tracking-wider text-[var(--md-sys-color-primary)]">Google Photos</span>
          </div>
        </div>
      </template>

      <!-- Bottom Slot: Quick Account Avatar / Version -->
      <div class="flex flex-col items-center gap-2 w-full pt-2">
        <input
          ref="avatarFileInput"
          type="file"
          accept="image/*"
          class="hidden"
          aria-label="Upload custom profile photo"
          @change="onAvatarFileSelected"
        >

        <button
          v-if="!isRailExpanded && displaySelectedOption"
          type="button"
          class="group relative flex size-10 cursor-pointer items-center justify-center overflow-hidden rounded-full border border-[var(--md-sys-color-outline-variant)]/40 bg-[var(--md-sys-color-surface-container-high)] text-xs font-bold text-[var(--md-sys-color-on-surface)] shadow-xs transition-transform hover:scale-105"
          :title="displaySelectedOption ? `${displaySelectedOption} (click to change photo)` : 'No account'"
          aria-label="Change profile photo"
          @click="triggerAvatarUpload"
        >
          <img
            v-if="customAvatar"
            :src="customAvatar"
            alt="Profile avatar"
            class="size-full object-cover"
          >
          <span v-else>{{ getEmailInitial(displaySelectedOption) }}</span>
        </button>

        <div
          v-if="isRailExpanded"
          class="w-full px-3 py-1 text-center text-[10px] font-medium text-[var(--md-sys-color-on-surface-variant)]/70 select-none"
        >
          Lumen &bull; Google Photos
        </div>
      </div>
    </M3NavigationRail>

    <!-- Main Content Area -->
    <div class="relative flex flex-1 h-full min-w-0 flex-col overflow-hidden bg-[var(--md-sys-color-surface)]">
      <!-- Top App Bar & Window Drag Header -->
      <header
        class="flex h-16 w-full shrink-0 items-center justify-between border-b border-[var(--md-sys-color-outline-variant)]/20 px-6 select-none bg-[var(--md-sys-color-surface)]"
        style="--wails-draggable: drag"
      >
        <!-- Title & Status -->
        <div
          class="flex items-center gap-3 min-w-0"
          style="--wails-draggable: none"
        >
          <h1 class="text-lg md:text-xl font-bold tracking-tight text-[var(--md-sys-color-on-surface)]">
            {{ activeTab === 'upload' ? 'Upload Hub' : 'Settings & Preferences' }}
          </h1>
          <span class="hidden sm:inline-flex rounded-full bg-[var(--md-sys-color-surface-container-high)] px-3 py-0.5 text-xs font-medium text-[var(--md-sys-color-on-surface-variant)]">
            {{ activeTab === 'upload' ? (uploadState.isUploading ? 'Uploading...' : 'Ready') : 'Preferences' }}
          </span>
        </div>

        <!-- Account Controls & Quick Actions -->
        <div
          class="flex items-center gap-3"
          style="--wails-draggable: none"
        >
          <GoogleAccountSelect
            v-if="displayOptions.length > 0"
            v-model="displaySelectedOption"
            :options="displayOptions"
            :avatar-url="customAvatar"
            :removing-account="removingAccount"
            @item-removed="removeCredentials"
            @add="openAccountSetup"
          />
          <M3Button
            v-else
            variant="filled"
            size="s"
            icon="person_add"
            aria-label="Connect Google account"
            @click="openAccountSetup"
          >
            <span>Connect Account</span>
          </M3Button>
        </div>
      </header>

      <!-- Scrollable Canvas Pane -->
      <section
        class="m3-main-scroll-pane relative flex flex-1 h-full min-w-0 flex-col overflow-y-auto p-6 md:p-8"
        :class="isSettling && 'm3-launch-settle'"
        :style="isSettling ? { '--settle-index': 2, '--launch-stagger': settleStagger + 'ms' } : undefined"
      >
        <Transition
          name="m3-fade-through"
          mode="out-in"
          type="animation"
        >
          <!-- Upload Screen -->
          <div
            v-if="activeTab === 'upload'"
            key="upload"
            class="m3-tab-pane flex flex-col flex-1 w-full gap-6 min-h-0"
            style="--wails-draggable: none"
          >
            <!-- Uploading active progress view -->
            <div
              v-if="uploadState.isUploading"
              class="h-full w-full"
            >
              <Upload />
            </div>

            <!-- Empty State: No Account Connected -->
            <div
              v-else-if="displayOptions.length === 0"
              class="flex h-full w-full flex-1 flex-col items-center justify-center text-center p-8 select-none my-auto"
            >
              <div class="relative mb-6 flex size-28 items-center justify-center m3-shape-cookie bg-[var(--md-sys-color-primary-container)]/40 text-[var(--md-sys-color-primary)] shadow-sm transition-transform duration-300 hover:scale-105">
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
                Sign in with your Google account to enable unlimited multithreaded uploads, album synchronization, and Apple Live Photos pairing.
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

            <!-- Normal Upload Hub with 3-Card Drop Targets -->
            <div
              v-else
              class="flex flex-col flex-1 w-full gap-6"
            >
              <!-- Token binding alert banner if needed -->
              <div
                v-if="tokenBindingEmail"
                class="w-full shrink-0 rounded-[24px] border border-amber-500/30 bg-amber-500/10 p-4 flex items-center justify-between gap-4"
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

              <!-- Hub Header & Capability Tags -->
              <div class="flex flex-col md:flex-row md:items-center justify-between gap-3">
                <div>
                  <h2 class="text-2xl font-bold tracking-tight text-[var(--md-sys-color-on-surface)]">
                    Upload Photos & Videos
                  </h2>
                  <p class="mt-1 text-xs text-[var(--md-sys-color-on-surface-variant)]">
                    Drag files or directories directly onto any destination card below, or use the action buttons.
                  </p>
                </div>
                <div class="flex flex-wrap items-center gap-2">
                  <span class="rounded-full bg-[var(--md-sys-color-surface-container-high)] px-3 py-1 text-[11px] font-medium text-[var(--md-sys-color-on-surface-variant)]">
                    Live Photos
                  </span>
                  <span class="rounded-full bg-[var(--md-sys-color-surface-container-high)] px-3 py-1 text-[11px] font-medium text-[var(--md-sys-color-on-surface-variant)]">
                    Hash Deduplication
                  </span>
                  <span class="rounded-full bg-[var(--md-sys-color-surface-container-high)] px-3 py-1 text-[11px] font-medium text-[var(--md-sys-color-on-surface-variant)]">
                    Original / Saver
                  </span>
                </div>
              </div>

              <!-- Three Expressive Drop & Action Cards -->
              <div class="grid grid-cols-1 md:grid-cols-3 gap-5 flex-1 min-h-[300px]">
                <!-- Card 1: Direct Library Upload -->
                <div
                  data-file-drop-target
                  data-drop-zone="regular"
                  class="drop-zone m3-drop-card group relative flex flex-col items-center justify-between rounded-[32px] border border-[var(--md-sys-color-outline-variant)]/40 bg-[var(--md-sys-color-surface-container-high)] p-6 md:p-8 text-center cursor-pointer shadow-xs transition-transform duration-200 hover:-translate-y-1"
                  :class="isDraggingFiles && 'border-[var(--md-sys-color-primary)] ring-2 ring-[var(--md-sys-color-primary)]/40 bg-[var(--md-sys-color-primary-container)]/20'"
                  :style="{ '--card-index': 0 }"
                  @click="handlePickFiles"
                >
                  <div class="flex flex-col items-center">
                    <div class="mb-4 flex size-16 items-center justify-center m3-shape-cookie bg-[var(--md-sys-color-primary-container)] text-[var(--md-sys-color-on-primary-container)] shadow-sm">
                      <M3Icon
                        name="cloud_upload"
                        :filled="isDraggingFiles"
                        :size="32"
                      />
                    </div>
                    <h3 class="text-base font-bold text-[var(--md-sys-color-on-surface)] select-none">
                      Direct Upload
                    </h3>
                    <p class="mt-1.5 text-xs text-[var(--md-sys-color-on-surface-variant)] select-none leading-relaxed max-w-[220px]">
                      Upload directly to your Google Photos timeline. Live Photos pair automatically.
                    </p>
                  </div>

                  <div class="mt-6 flex flex-col items-center gap-2">
                    <M3Button
                      variant="filled"
                      size="s"
                      icon="add_photo_alternate"
                      @click.stop="handlePickFiles"
                    >
                      Choose Files
                    </M3Button>
                    <span class="text-[10px] text-[var(--md-sys-color-on-surface-variant)]/70">
                      or drop files here
                    </span>
                  </div>
                </div>

                <!-- Card 2: Upload to Album -->
                <div
                  data-file-drop-target
                  data-drop-zone="album"
                  class="drop-zone m3-drop-card group relative flex flex-col items-center justify-between rounded-[32px] border border-[var(--md-sys-color-outline-variant)]/40 bg-[var(--md-sys-color-surface-container-high)] p-6 md:p-8 text-center cursor-pointer shadow-xs transition-transform duration-200 hover:-translate-y-1"
                  :class="isDraggingFiles && 'border-[var(--md-sys-color-secondary)] ring-2 ring-[var(--md-sys-color-secondary)]/40 bg-[var(--md-sys-color-secondary-container)]/20'"
                  :style="{ '--card-index': 1 }"
                  @click="promptPickForAlbum"
                >
                  <div class="flex flex-col items-center">
                    <div class="mb-4 flex size-16 items-center justify-center m3-shape-scallop bg-[var(--md-sys-color-secondary-container)] text-[var(--md-sys-color-on-secondary-container)] shadow-sm">
                      <M3Icon
                        name="create_new_folder"
                        :filled="isDraggingFiles"
                        :size="32"
                      />
                    </div>
                    <h3 class="text-base font-bold text-[var(--md-sys-color-on-surface)] select-none">
                      Upload to Album
                    </h3>
                    <p class="mt-1.5 text-xs text-[var(--md-sys-color-on-surface-variant)] select-none leading-relaxed max-w-[220px]">
                      Add files into a specific Google Photos album of your choice, or create a new one.
                    </p>
                  </div>

                  <div class="mt-6 flex flex-col items-center gap-2">
                    <M3Button
                      variant="tonal"
                      size="s"
                      icon="folder_special"
                      @click.stop="promptPickForAlbum"
                    >
                      Select Album
                    </M3Button>
                    <span class="text-[10px] text-[var(--md-sys-color-on-surface-variant)]/70">
                      or drop files here
                    </span>
                  </div>
                </div>

                <!-- Card 3: Smart Folder Sync -->
                <div
                  data-file-drop-target
                  data-drop-zone="auto-album"
                  class="drop-zone m3-drop-card group relative flex flex-col items-center justify-between rounded-[32px] border border-[var(--md-sys-color-outline-variant)]/40 bg-[var(--md-sys-color-surface-container-high)] p-6 md:p-8 text-center cursor-pointer shadow-xs transition-transform duration-200 hover:-translate-y-1"
                  :class="isDraggingFiles && 'border-[var(--md-sys-color-tertiary)] ring-2 ring-[var(--md-sys-color-tertiary)]/40 bg-[var(--md-sys-color-tertiary-container)]/20'"
                  :style="{ '--card-index': 2 }"
                  @click="handlePickFolder"
                >
                  <div class="flex flex-col items-center">
                    <div class="mb-4 flex size-16 items-center justify-center m3-shape-squircle bg-[var(--md-sys-color-tertiary-container)] text-[var(--md-sys-color-on-tertiary-container)] shadow-sm">
                      <M3Icon
                        name="auto_awesome"
                        :filled="isDraggingFiles"
                        :size="32"
                      />
                    </div>
                    <h3 class="text-base font-bold text-[var(--md-sys-color-on-surface)] select-none">
                      Auto Album by Folder
                    </h3>
                    <p class="mt-1.5 text-xs text-[var(--md-sys-color-on-surface-variant)] select-none leading-relaxed max-w-[220px]">
                      Upload entire directories. Albums will be generated automatically based on folder names.
                    </p>
                  </div>

                  <div class="mt-6 flex flex-col items-center gap-2">
                    <M3Button
                      variant="outlined"
                      size="s"
                      icon="folder_copy"
                      @click.stop="handlePickFolder"
                    >
                      Choose Folder
                    </M3Button>
                    <span class="text-[10px] text-[var(--md-sys-color-on-surface-variant)]/70">
                      or drop folders here
                    </span>
                  </div>
                </div>
              </div>

              <!-- Floating Extended FAB -->
              <div
                class="fixed bottom-6 right-8 z-30"
                style="--wails-draggable: none"
              >
                <M3ExtendedFab
                  :disabled="isPickingMedia"
                  @pick-files="handlePickFiles"
                  @pick-folder="handlePickFolder"
                />
              </div>

              <!-- Upload Results Summary if present -->
              <UploadResultsCard
                v-if="uploadState.uploadedFiles > 0 || uploadState.results.fail.length > 0"
                :results="uploadState.results"
                class="shrink-0 mt-2"
                style="--wails-draggable: none"
                @copy="handleCopyClick"
                @clear="() => uploadManager.resetUploadResults()"
                @retry-all="handleRetryAll"
                @retry-item="handleRetryItem"
              />
            </div>
          </div>

          <!-- Settings Screen -->
          <div
            v-else-if="activeTab === 'settings'"
            key="settings"
            class="m3-tab-pane flex flex-col flex-1 w-full min-h-0"
            style="--wails-draggable: none"
          >
            <TooltipProvider disable-hoverable-content>
              <SettingsPanel @upload-folder="onUploadFolderFromSettings" />
            </TooltipProvider>
          </div>
        </Transition>
      </section>
    </div>

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
            class="m3-dialog-card relative flex w-full max-w-md flex-col items-center justify-center gap-5 rounded-[32px] border border-[var(--md-sys-color-outline-variant)]/50 bg-[var(--md-sys-color-surface-container)] p-8 text-center shadow-2xl"
            role="dialog"
            aria-modal="true"
            aria-labelledby="album-dialog-title"
          >
            <div class="flex size-16 items-center justify-center rounded-full bg-[var(--md-sys-color-primary-container)] text-[var(--md-sys-color-on-primary-container)]">
              <M3Icon
                name="create_new_folder"
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

            <div class="flex w-full flex-col gap-2 text-left">
              <Label
                for="album-input"
                class="text-xs font-medium text-[var(--md-sys-color-on-surface-variant)]"
              >
                Album name or key
              </Label>
              <Input
                id="album-input"
                v-model="albumNameOrKey"
                placeholder="e.g. Vacation 2026 or AF1Qip... key"
                autofocus
                class="h-11 rounded-2xl bg-[var(--md-sys-color-surface-container-high)] px-4 text-sm border-[var(--md-sys-color-outline-variant)]"
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
