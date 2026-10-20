<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, watch } from 'vue'
import {
  Camera,
  CloudUpload,
  FolderPlus,
  Settings2,
  Sparkles,
  SunMedium,
  Upload as UploadIcon,
  UserPlus,
} from '@lucide/vue'
import { useColorMode } from '@vueuse/core'
import { toast } from 'vue-sonner'
import { Events } from '@wailsio/runtime'
import { ConfigManager } from '../bindings/app/core'
import Button from './components/ui/button/Button.vue'
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
import { launchReplayTrigger } from './utils/launchState'
import { uploadManager } from './utils/UploadManager'
import { mockState } from './utils/mockMode'
import {
  displayNameFromEmail,
  getAvatarColor,
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
const confirmAlbumUpload = async () => {
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
    style="--wails-draggable: drag"
  >
    <!-- Navigation Rail / Side Panel (~280px) -->
    <aside
      class="flex h-full w-[280px] shrink-0 flex-col justify-between border-r border-[var(--md-sys-color-outline-variant)]/30 bg-[var(--md-sys-color-surface-container-low)] p-4 select-none"
      style="--wails-draggable: drag; contain: layout paint"
    >
      <div
        class="flex flex-col gap-4"
        style="--wails-draggable: none"
      >
        <!-- App Header / Branding -->
        <div class="flex items-center gap-2.5 px-2 pt-1 pb-2">
          <div
            id="sidebar-app-logo"
            class="flex size-9 items-center justify-center rounded-xl bg-[var(--md-sys-color-primary)] text-[var(--md-sys-color-on-primary)] shadow-sm transition-opacity duration-150"
            :class="isSplashActive ? 'opacity-0' : 'opacity-100'"
          >
            <SunMedium class="size-5" />
          </div>
          <div class="flex flex-col">
            <span class="text-base font-bold tracking-tight text-[var(--md-sys-color-on-surface)]">Lumen</span>
            <span class="text-[10px] font-medium uppercase tracking-wider text-[var(--md-sys-color-primary)]">Google Photos</span>
          </div>
        </div>

        <!-- Profile Card -->
        <div
          class="relative flex flex-col items-center rounded-[24px] border border-[var(--md-sys-color-outline-variant)]/40 bg-[var(--md-sys-color-surface-container)] p-4 text-center shadow-xs"
          :class="isSettling && 'm3-launch-settle'"
          :style="isSettling ? { '--settle-index': 0, '--launch-stagger': settleStagger + 'ms', contain: 'layout paint' } : { contain: 'layout paint' }"
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
            class="group relative mb-2.5 flex size-16 cursor-pointer items-center justify-center overflow-hidden rounded-full shadow-inner transition-transform hover:scale-105"
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
              <Camera class="size-5 text-white" />
            </div>
          </div>

          <h3 class="max-w-[210px] truncate text-sm font-semibold text-[var(--md-sys-color-on-surface)]">
            {{ displaySelectedOption ? displayNameFromEmail(displaySelectedOption) : 'No Account' }}
          </h3>
          <p class="max-w-[210px] truncate text-xs text-[var(--md-sys-color-on-surface-variant)]">
            {{ displaySelectedOption || 'Connect an account to upload' }}
          </p>

          <!-- Account switcher chip -->
          <div class="mt-3 w-full flex justify-center">
            <GoogleAccountSelect
              v-if="displayOptions.length > 0"
              v-model="displaySelectedOption"
              :options="displayOptions"
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
              <UserPlus class="size-3.5" />
              Connect Account
            </button>
          </div>
        </div>

        <!-- Navigation Items -->
        <nav
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
            <CloudUpload
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
            <Settings2
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
            class="m3-nav-item group relative flex h-11 w-full cursor-pointer items-center gap-3.5 px-4 text-sm font-medium text-[var(--md-sys-color-on-surface-variant)] transition-colors hover:bg-[var(--md-sys-color-surface-container-high)] hover:text-[var(--md-sys-color-on-surface)]"
            aria-label="Manage accounts"
            @click="openAccountSetup"
          >
            <UserPlus class="relative z-10 size-5 shrink-0 transition-transform duration-200" />
            <span class="relative z-10">Accounts</span>
          </button>
        </nav>
      </div>

      <!-- Bottom info -->
      <div class="px-2 text-[11px] text-[var(--md-sys-color-on-surface-variant)]/70 select-none">
        Lumen &bull; Google Photos Desktop
      </div>
    </aside>

    <!-- Main Content Area -->
    <section
      class="relative flex flex-1 h-full min-w-0 flex-col overflow-y-auto bg-[var(--md-sys-color-surface)] p-6"
      :class="isSettling && 'm3-launch-settle'"
      :style="isSettling ? { '--settle-index': 2, '--launch-stagger': settleStagger + 'ms', '--wails-draggable': 'drag', contain: 'layout paint' } : { '--wails-draggable': 'drag', contain: 'layout paint' }"
    >
      <Transition
        name="m3-fade-through"
        mode="out-in"
      >
        <!-- View: Upload Tab -->
        <div
          v-if="activeTab === 'upload'"
          key="upload"
          class="flex h-full w-full flex-1 flex-col min-h-0"
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
              class="drop-zone m3-drop-card flex flex-col items-center justify-center rounded-[32px] border-2 border-dashed border-[var(--md-sys-color-outline-variant)]/60 bg-[var(--md-sys-color-surface-container)] p-6 text-center transition-[background-color,border-color,transform,box-shadow] duration-200 cursor-pointer"
              :style="{ '--card-index': 0 }"
            >
              <div class="mb-3 flex size-14 items-center justify-center rounded-full bg-[var(--md-sys-color-primary-container)] text-[var(--md-sys-color-on-primary-container)] shadow-sm">
                <UploadIcon class="size-7" />
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
              class="drop-zone m3-drop-card flex flex-col items-center justify-center rounded-[32px] border-2 border-dashed border-[var(--md-sys-color-outline-variant)]/60 bg-[var(--md-sys-color-surface-container)] p-6 text-center transition-[background-color,border-color,transform,box-shadow] duration-200 cursor-pointer"
              :style="{ '--card-index': 1 }"
            >
              <div class="mb-3 flex size-14 items-center justify-center rounded-full bg-[var(--md-sys-color-secondary-container)] text-[var(--md-sys-color-on-secondary-container)] shadow-sm">
                <FolderPlus class="size-7" />
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
              class="drop-zone m3-drop-card flex flex-col items-center justify-center rounded-[32px] border-2 border-dashed border-[var(--md-sys-color-outline-variant)]/60 bg-[var(--md-sys-color-surface-container)] p-6 text-center transition-[background-color,border-color,transform,box-shadow] duration-200 cursor-pointer"
              :style="{ '--card-index': 2 }"
            >
              <div class="mb-3 flex size-14 items-center justify-center rounded-full bg-[var(--md-sys-color-tertiary-container)] text-[var(--md-sys-color-on-tertiary-container)] shadow-sm">
                <Sparkles class="size-7" />
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
              <UserPlus class="size-14" />
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
            <Button
              class="mt-8 h-12 rounded-full px-8 text-sm font-semibold cursor-pointer select-none bg-[var(--md-sys-color-primary)] text-[var(--md-sys-color-on-primary)] hover:bg-[var(--md-sys-color-primary)]/90 shadow-md transition-[background-color,transform] duration-200 hover:scale-[1.02] active:scale-[0.98]"
              aria-label="Add Google account"
              @click="openAccountSetup"
            >
              <UserPlus class="size-4 mr-2.5 stroke-[2.5]" />
              <span>Add Google Account</span>
            </Button>
            <p class="mt-4 text-[11px] text-[var(--md-sys-color-on-surface-variant)]/60 select-none">
              Credentials are stored locally in your system configuration.
            </p>
          </div>

          <!-- Normal Drop View (with account connected) -->
          <div
            v-else
            class="flex h-full w-full flex-1 flex-col gap-4 min-h-0"
          >
            <!-- Album Name Input State (Modal Dialog with Scrim & Spatial Spring) -->
            <Teleport to="body">
              <Transition name="m3-dialog-modal">
                <div
                  v-if="showAlbumInput"
                  class="fixed inset-0 z-50 grid place-items-center bg-black/70 p-4 select-none overflow-y-auto"
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
                      <FolderPlus class="size-8" />
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

                    <div class="flex gap-3 mt-1">
                      <Button
                        variant="outline"
                        class="rounded-full px-6 text-xs h-10 cursor-pointer select-none"
                        @click="cancelAlbumUpload"
                      >
                        Cancel
                      </Button>
                      <Button
                        class="rounded-full px-7 text-xs h-10 cursor-pointer select-none bg-[var(--md-sys-color-primary)] text-[var(--md-sys-color-on-primary)]"
                        :disabled="!albumNameOrKey.trim()"
                        @click="confirmAlbumUpload"
                      >
                        Start Upload
                      </Button>
                    </div>
                  </div>
                </div>
              </Transition>
            </Teleport>

            <div
              class="flex h-full w-full flex-1 flex-col gap-4 min-h-0"
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
                class="drop-zone group relative flex flex-1 w-full flex-col items-center justify-center rounded-[36px] border-2 border-dashed border-[var(--md-sys-color-outline-variant)]/50 bg-[var(--md-sys-color-surface-container-low)] p-10 text-center select-none shadow-xs"
                style="--wails-draggable: drag"
              >
                <div class="relative mb-5 flex size-28 items-center justify-center rounded-full bg-[var(--md-sys-color-primary-container)]/40 text-[var(--md-sys-color-primary)] shadow-xs transition-transform duration-300 group-hover:scale-105">
                  <CloudUpload class="size-14" />
                </div>
                <h2 class="text-2xl font-bold text-[var(--md-sys-color-on-surface)] select-none">
                  Drop files to upload
                </h2>
                <p class="mt-2 text-sm text-[var(--md-sys-color-on-surface-variant)] select-none max-w-md">
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
              </div>

              <!-- Upload Results Summary -->
              <UploadResultsCard
                v-if="uploadState.uploadedFiles > 0 || uploadState.results.fail.length > 0"
                :results="uploadState.results"
                class="shrink-0"
                style="--wails-draggable: none"
                @copy="handleCopyClick"
                @clear="() => uploadManager.resetUploadResults()"
              />
            </div>
          </div>
        </div>

        <!-- View: Settings Tab -->
        <div
          v-else-if="activeTab === 'settings'"
          key="settings"
          class="flex h-full w-full flex-1 flex-col min-h-0"
          style="--wails-draggable: none"
        >
          <TooltipProvider disable-hoverable-content>
            <SettingsPanel />
          </TooltipProvider>
        </div>
      </Transition>
    </section>

    <!-- Add Account Dialog -->
    <GoogleAuthSetup
      v-model:open="isAccountSetupOpen"
      @account-added="refreshCredentials"
    />

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
