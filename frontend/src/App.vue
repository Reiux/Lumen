<script setup lang="ts">
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { TooltipProvider } from '@/components/ui/tooltip'
import {
  Sheet,
  SheetContent,
  SheetTrigger,
} from '@/components/ui/sheet'
import { useColorMode } from '@vueuse/core'
import { onMounted, onUnmounted, ref, watch } from 'vue'
import { CloudUpload, FolderPlus, Settings2, Sparkles, Upload as UploadIcon, UserPlus } from '@lucide/vue'
import { ConfigManager } from '../bindings/app/core'
import { Events } from '@wailsio/runtime'
import Button from "./components/ui/button/Button.vue"
import GoogleAccountSelect from './components/GoogleAccountSelect.vue'
import GoogleAuthSetup from "./components/GoogleAuthSetup.vue"
import './index.css'
import SettingsPanel from "./SettingsPanel.vue"
import Upload from './Upload.vue'
import { uploadManager } from './utils/UploadManager'
import Toaster from './components/ui/sonner/Sonner.vue'

import { toast } from "vue-sonner"

useColorMode().value = "dark"

const { state: uploadState } = uploadManager
const copyButtonText = ref('Copy as JSON');

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

watch(selectedOption, async (newValue) => {
  if (newValue) {
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
  }
})

async function updateTokenBindingPrompt(email: string) {
  tokenBindingEmail.value = accountNeedsTokenBinding.value[email] ? email : ''
}

async function refreshCredentials() {
  try {
    const state = await ConfigManager.GetAccounts()
    const nextNeedsTokenBinding: Record<string, boolean> = {}
    const nextOptions = state.accounts.map(account => {
      nextNeedsTokenBinding[account.email] = account.needsTokenBinding
      return account.email
    })

    accountNeedsTokenBinding.value = nextNeedsTokenBinding
    options.value = nextOptions
    selectedOption.value = state.selected || ''
    if (state.selected) {
      await updateTokenBindingPrompt(state.selected)
    } else {
      tokenBindingEmail.value = ''
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

onMounted(async () => {
  await refreshCredentials()

})

const handleCopyClick = () => {
  uploadManager.copyResultsAsJson();
  copyButtonText.value = 'Copied!';
  setTimeout(() => copyButtonText.value = 'Copy as JSON', 1000);
};



// Global drag event handlers to detect file dragging
let dragLeaveTimeout: ReturnType<typeof setTimeout> | null = null

const onDragEnter = (e: DragEvent) => {
  if (!e.dataTransfer?.types.includes('Files')) return
  
  // Clear any pending drag leave timeout
  if (dragLeaveTimeout) {
    clearTimeout(dragLeaveTimeout)
    dragLeaveTimeout = null
  }
  
  isDraggingFiles.value = true
}

const onDragOver = (e: DragEvent) => {
  if (!e.dataTransfer?.types.includes('Files')) return
  e.preventDefault()
  
  // Clear any pending drag leave timeout - we're still dragging
  if (dragLeaveTimeout) {
    clearTimeout(dragLeaveTimeout)
    dragLeaveTimeout = null
  }
}

const onDragLeave = (e: DragEvent) => {
  if (!e.dataTransfer?.types.includes('Files')) return
  
  // Use timeout to detect if we've truly left the window
  // dragover will cancel this if we're still in the window
  if (dragLeaveTimeout) {
    clearTimeout(dragLeaveTimeout)
  }
  dragLeaveTimeout = setTimeout(() => {
    isDraggingFiles.value = false
    dragLeaveTimeout = null
  }, 50)
}

const onDrop = () => {
  // Clear any pending timeout
  if (dragLeaveTimeout) {
    clearTimeout(dragLeaveTimeout)
    dragLeaveTimeout = null
  }
  
  // Delay resetting isDraggingFiles to allow Wails to process the drop target
  // before Vue re-renders and hides the drop zones
  setTimeout(() => {
    isDraggingFiles.value = false
  }, 100)
}

// Album upload confirmation
const confirmAlbumUpload = async () => {
  // Set album name in backend (not persisted to disk)
  await ConfigManager.SetAlbumName(albumNameOrKey.value)
  await ConfigManager.SetAlbumAutoMode(false)
  // Start upload with pending files
  Events.Emit('startUpload', { files: pendingFiles.value })
  showAlbumInput.value = false
  pendingFiles.value = []
  pendingFileCount.value = 0
  albumNameOrKey.value = '' // Reset for next upload
}

const cancelAlbumUpload = () => {
  showAlbumInput.value = false
  pendingFiles.value = []
  pendingFileCount.value = 0
  albumNameOrKey.value = '' // Reset on cancel too
}

// Handle album error event
const albumErrorHandler = (e: Event) => {
  const event = e as CustomEvent<{ AlbumName: string; Error: string }>
  const { AlbumName, Error } = event.detail
  // Check if it's a 404 error (album key not found)
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

onMounted(() => {
  document.addEventListener('dragenter', onDragEnter)
  document.addEventListener('dragleave', onDragLeave)
  document.addEventListener('dragover', onDragOver)
  document.addEventListener('drop', onDrop)
  window.addEventListener('albumError', albumErrorHandler)
  window.addEventListener('uploadError', uploadErrorHandler)

  // Listen for files-dropped event from backend
  Events.On('files-dropped', async (event: { data: { files: string[]; dropZone: string } }) => {
    const { files, dropZone } = event.data

    if (dropZone === 'album') {
      // Show album input screen
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
  if (dragLeaveTimeout) {
    clearTimeout(dragLeaveTimeout)
  }
})
</script>

<template>
  <main
    class="relative flex h-full w-full flex-col overflow-hidden p-3.5 select-none bg-[var(--md-sys-color-surface)] text-[var(--md-sys-color-on-surface)]"
    style="--wails-draggable: drag"
  >
    <!-- Header with Account Select and Settings -->
    <header
      v-if="!uploadState.isUploading && options.length > 0 && !isDraggingFiles"
      class="flex w-full shrink-0 items-center justify-between pb-3"
      style="--wails-draggable: drag"
    >
      <div style="--wails-draggable: none">
        <GoogleAccountSelect
          v-model="selectedOption"
          :options="options"
          :removing-account="removingAccount"
          @item-removed="removeCredentials"
          @add="openAccountSetup"
        />
      </div>

      <Sheet>
        <SheetTrigger as-child>
          <button
            type="button"
            class="flex size-9 cursor-pointer items-center justify-center rounded-full bg-[var(--md-sys-color-surface-container-high)] text-[var(--md-sys-color-on-surface-variant)] shadow-xs outline-none transition-colors hover:bg-[var(--md-sys-color-surface-container-highest)] hover:text-[var(--md-sys-color-on-surface)] focus-visible:ring-2 focus-visible:ring-[var(--md-sys-color-primary)]"
            aria-label="Settings"
            title="Settings"
            style="--wails-draggable: none"
          >
            <Settings2 class="size-4" />
          </button>
        </SheetTrigger>
        <SheetContent
          side="bottom"
          class="max-h-[85vh] overflow-y-auto rounded-t-[28px] border-t border-[var(--md-sys-color-outline-variant)]/50 bg-[var(--md-sys-color-surface-container-low)] p-4 shadow-2xl"
          style="--wails-draggable: none"
        >
          <TooltipProvider disable-hoverable-content>
            <SettingsPanel />
          </TooltipProvider>
        </SheetContent>
      </Sheet>
    </header>

    <!-- Drop zones shown when dragging files -->
    <div
      v-if="!uploadState.isUploading && isDraggingFiles && options.length > 0"
      class="flex h-full w-full flex-1 flex-col gap-2.5 py-1"
      style="--wails-draggable: none"
    >
      <div
        data-file-drop-target
        data-drop-zone="regular"
        class="drop-zone flex flex-1 flex-col items-center justify-center rounded-[28px] border-2 border-dashed border-[var(--md-sys-color-outline-variant)]/60 bg-[var(--md-sys-color-surface-container)] p-3 text-center transition-all"
      >
        <div class="mb-1.5 flex size-10 items-center justify-center rounded-full bg-[var(--md-sys-color-primary-container)] text-[var(--md-sys-color-on-primary-container)]">
          <UploadIcon class="size-5" />
        </div>
        <h2 class="text-sm font-semibold text-[var(--md-sys-color-on-surface)] select-none">
          Upload Only
        </h2>
        <p class="text-[11px] text-[var(--md-sys-color-on-surface-variant)] select-none">
          Upload files without adding to any album
        </p>
      </div>

      <div
        data-file-drop-target
        data-drop-zone="album"
        class="drop-zone flex flex-1 flex-col items-center justify-center rounded-[28px] border-2 border-dashed border-[var(--md-sys-color-outline-variant)]/60 bg-[var(--md-sys-color-surface-container)] p-3 text-center transition-all"
      >
        <div class="mb-1.5 flex size-10 items-center justify-center rounded-full bg-[var(--md-sys-color-secondary-container)] text-[var(--md-sys-color-on-secondary-container)]">
          <FolderPlus class="size-5" />
        </div>
        <h2 class="text-sm font-semibold text-[var(--md-sys-color-on-surface)] select-none">
          Upload to Album
        </h2>
        <p class="text-[11px] text-[var(--md-sys-color-on-surface-variant)] select-none">
          Upload and add to a specific album
        </p>
      </div>

      <div
        data-file-drop-target
        data-drop-zone="auto-album"
        class="drop-zone flex flex-1 flex-col items-center justify-center rounded-[28px] border-2 border-dashed border-[var(--md-sys-color-outline-variant)]/60 bg-[var(--md-sys-color-surface-container)] p-3 text-center transition-all"
      >
        <div class="mb-1.5 flex size-10 items-center justify-center rounded-full bg-[var(--md-sys-color-tertiary-container)] text-[var(--md-sys-color-on-tertiary-container)]">
          <Sparkles class="size-5" />
        </div>
        <h2 class="text-sm font-semibold text-[var(--md-sys-color-on-surface)] select-none">
          Auto Album
        </h2>
        <p class="text-[11px] text-[var(--md-sys-color-on-surface-variant)] select-none">
          Create albums automatically based on folder names
        </p>
      </div>
    </div>

    <!-- Normal UI (not dragging) -->
    <div
      v-else-if="!uploadState.isUploading"
      class="flex h-full w-full flex-1 flex-col items-center justify-between min-h-0"
      data-file-drop-target
      data-drop-zone="regular"
      style="--wails-draggable: drag"
    >
      <template v-if="options.length === 0">
        <div
          class="m-auto flex max-w-xs flex-col items-center gap-4 text-center"
          style="--wails-draggable: none"
        >
          <div class="flex size-14 items-center justify-center rounded-full bg-[var(--md-sys-color-surface-container-high)] text-[var(--md-sys-color-on-surface-variant)]">
            <UserPlus class="size-6" />
          </div>
          <div class="flex flex-col gap-1">
            <h1 class="text-lg font-semibold text-[var(--md-sys-color-on-surface)] select-none">
              Connect Google Photos
            </h1>
            <p class="text-xs text-[var(--md-sys-color-on-surface-variant)] select-none">
              Add an account before uploading photos and videos.
            </p>
          </div>
          <Button
            class="rounded-full px-6 cursor-pointer select-none bg-[var(--md-sys-color-primary)] text-[var(--md-sys-color-on-primary)] hover:bg-[var(--md-sys-color-primary)]/90"
            @click="openAccountSetup"
          >
            Add Google account
          </Button>
        </div>
      </template>

      <template v-else>
        <!-- Show album input screen when files dropped on album zone -->
        <template v-if="showAlbumInput">
          <div
            class="m-auto flex w-full max-w-xs flex-col items-center justify-center gap-4 rounded-[28px] border border-[var(--md-sys-color-outline-variant)]/40 bg-[var(--md-sys-color-surface-container)] p-6 text-center shadow-lg"
            style="--wails-draggable: none"
          >
            <div class="flex size-12 items-center justify-center rounded-full bg-[var(--md-sys-color-primary-container)] text-[var(--md-sys-color-on-primary-container)]">
              <FolderPlus class="size-6" />
            </div>
            <div class="flex flex-col gap-1">
              <h1 class="text-base font-semibold text-[var(--md-sys-color-on-surface)] select-none">
                Upload to Album
              </h1>
              <p class="text-xs text-[var(--md-sys-color-on-surface-variant)] select-none">
                {{ pendingFileCount }} file(s) ready to upload
              </p>
            </div>
            
            <div class="flex w-full flex-col gap-1.5 text-left">
              <Label
                for="album-input"
                class="text-xs text-[var(--md-sys-color-on-surface-variant)]"
              >Album name or key</Label>
              <Input
                id="album-input"
                v-model="albumNameOrKey"
                placeholder="Album name or AF1Qip... key"
                autofocus
                class="rounded-full bg-[var(--md-sys-color-surface-container-high)] text-xs border-[var(--md-sys-color-outline-variant)]"
              />
            </div>

            <div class="flex gap-2.5 mt-2">
              <Button
                variant="outline"
                class="rounded-full px-5 text-xs cursor-pointer select-none"
                @click="cancelAlbumUpload"
              >
                Cancel
              </Button>
              <Button
                class="rounded-full px-6 text-xs cursor-pointer select-none bg-[var(--md-sys-color-primary)] text-[var(--md-sys-color-on-primary)]"
                :disabled="!albumNameOrKey.trim()"
                @click="confirmAlbumUpload"
              >
                Upload
              </Button>
            </div>
          </div>
        </template>

        <!-- Normal UI when not dragging -->
        <template v-else>
          <div
            v-if="tokenBindingEmail"
            class="w-full shrink-0 rounded-[20px] border border-amber-500/30 bg-amber-500/10 p-3 flex flex-col gap-2 mb-2"
            style="--wails-draggable: none"
          >
            <p class="text-xs text-amber-200">
              This credential needs a token binding key from the rooted Android device it was captured from.
            </p>
            <button
              type="button"
              class="inline-flex h-8 items-center justify-center rounded-full bg-amber-400 px-3 text-xs font-medium text-amber-950 transition-colors hover:bg-amber-300 disabled:opacity-50 cursor-pointer"
              :disabled="isExtractingTokenBinding"
              @click="addTokenBindingAliasFromADB"
            >
              {{ isExtractingTokenBinding ? 'Reading ADB...' : 'Read from ADB' }}
            </button>
          </div>

          <div
            class="drop-zone group relative flex flex-1 w-full flex-col items-center justify-center rounded-[32px] border-2 border-dashed border-[var(--md-sys-color-outline-variant)]/40 bg-[var(--md-sys-color-surface-container-low)] p-6 text-center select-none"
            style="--wails-draggable: drag"
          >
            <div class="relative mb-3 flex size-20 items-center justify-center rounded-full bg-[var(--md-sys-color-primary-container)]/40 text-[var(--md-sys-color-primary)] shadow-xs transition-transform duration-300 group-hover:scale-105">
              <CloudUpload class="size-10" />
            </div>
            <h1 class="text-base font-semibold text-[var(--md-sys-color-on-surface)] select-none">
              Drop files to upload
            </h1>
            <p class="mt-1 text-xs text-[var(--md-sys-color-on-surface-variant)] select-none max-w-[220px]">
              Photos, videos, or entire folders
            </p>
            <div class="mt-4 flex items-center gap-1.5">
              <span class="rounded-full bg-[var(--md-sys-color-surface-container-high)] px-3 py-1 text-[11px] font-medium text-[var(--md-sys-color-on-surface-variant)] select-none">
                Drop folders for auto-albums
              </span>
            </div>
          </div>

          <div
            v-if="uploadState.uploadedFiles > 0 || uploadState.results.fail.length > 0"
            class="flex w-full shrink-0 flex-col items-center gap-2 rounded-[24px] border border-[var(--md-sys-color-outline-variant)]/40 bg-[var(--md-sys-color-surface-container)] p-4 mt-2.5"
            style="--wails-draggable: none"
          >
            <h2 class="text-sm font-semibold select-none text-[var(--md-sys-color-on-surface)]">
              Upload Results
            </h2>
            <div class="grid grid-cols-2 gap-x-4 gap-y-1 text-xs text-[var(--md-sys-color-on-surface-variant)]">
              <span>Successful: {{ uploadState.results.success.length }}</span>
              <span>Failed: {{ uploadState.results.fail.length }}</span>
              <span>Skipped: {{ uploadState.results.skipped.length }}</span>
              <span>Warnings: {{ uploadState.results.warnings.length }}</span>
            </div>
            <Button
              variant="outline"
              class="rounded-full text-xs h-8 px-4 cursor-pointer select-none mt-1"
              @click="handleCopyClick"
            >
              {{ copyButtonText }}
            </Button>
          </div>
        </template>
      </template>
    </div>

    <div
      v-if="uploadState.isUploading"
      class="w-full h-full"
    >
      <Upload />
    </div>

    <GoogleAuthSetup
      v-model:open="isAccountSetupOpen"
      @account-added="refreshCredentials"
    />
    <Toaster
      position="bottom-center"
      rich-colors
      expand
      :visible-toasts="4"
    />
  </main>
</template>
