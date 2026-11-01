<script setup lang="ts">
import { computed, nextTick, onMounted, onUnmounted, ref, watch } from 'vue'
import { onClickOutside, useColorMode, useWindowSize } from '@vueuse/core'
import { toast } from 'vue-sonner'
import { Events } from '@wailsio/runtime'
import { ConfigManager } from '../bindings/app/core'
import type { LogEntry } from '../bindings/app/core/models'
import M3Button from './components/ui/M3Button.vue'
import M3ButtonGroup from './components/ui/M3ButtonGroup.vue'
import M3TextField from './components/ui/M3TextField.vue'
import M3LoadingIndicator from './components/ui/M3LoadingIndicator.vue'
import M3WavyProgress from './components/ui/M3WavyProgress.vue'
import { TooltipProvider } from '@/components/ui/tooltip'
import GoogleAccountSelect from './components/GoogleAccountSelect.vue'
import GoogleAuthSetup from './components/GoogleAuthSetup.vue'
import SettingsPanel from './SettingsPanel.vue'
import FoldersView from './FoldersView.vue'
import AboutView from './AboutView.vue'
import Upload from './Upload.vue'
import UploadResultsCard from './components/UploadResultsCard.vue'
import CircuitBreakerAlert from './components/CircuitBreakerAlert.vue'
import DeveloperView from './DeveloperView.vue'
import EditDisplayNameModal from './components/EditDisplayNameModal.vue'
import RemoveAccountModal from './components/RemoveAccountModal.vue'
import Toaster from './components/ui/sonner/Sonner.vue'
import AppLaunchSplash from './components/AppLaunchSplash.vue'
import LumenLogo from './components/LumenLogo.vue'
import AccountPickerMenu from './components/AccountPickerMenu.vue'
import M3Icon from './components/ui/M3Icon.vue'
import { launchReplayTrigger } from './utils/launchState'
import { uploadManager } from './utils/UploadManager'
import { devOptionsState, mockBatches, mockLogEntries, mockProblems, mockState, setMockView } from './utils/mockMode'
import { preferencesState, refreshPreferences } from './utils/configStore'
import { cycleNextTheme, themePulseCounter } from './utils/theme'
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
const isAppReady = ref(false)
const isSaverMode = ref(false)

function handleResetToEmpty() {
  uploadManager.resetUploadResults()
  uploadManager.state.uploadedFiles = 0
  uploadManager.state.totalFiles = 0
  if (mockState.enabled) {
    setMockView('drop')
  }
}

// Navigation state: upload, folders, activity, accounts, settings, or about
export type LumenTab = 'upload' | 'folders' | 'activity' | 'accounts' | 'settings' | 'about' | 'developer'
const activeTab = ref<LumenTab>('upload')
const tabOrder = computed<readonly LumenTab[]>(() =>
  devOptionsState.unlocked
    ? ['upload', 'folders', 'activity', 'accounts', 'settings', 'about', 'developer']
    : ['upload', 'folders', 'activity', 'accounts', 'settings', 'about']
)
const activeTabIndex = computed(() => {
  const idx = tabOrder.value.indexOf(activeTab.value)
  return idx >= 0 ? idx : 0
})

watch(() => devOptionsState.unlocked, (unlocked) => {
  if (!unlocked && activeTab.value === 'developer') {
    activeTab.value = 'upload'
  }
})

// Account chip popover menu state
const isAccountMenuOpen = ref(false)
const accountTriggerRef = ref<HTMLButtonElement | null>(null)
const accountChipContainerRef = ref<HTMLElement | null>(null)

function switchAccount(email: string) {
  displaySelectedOption.value = email
  isAccountMenuOpen.value = false
}

// Tablet & desktop window state
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
const albumNickname = ref('')
const albumNicknames = ref<Record<string, string | undefined>>({})
const recentAlbums = ref<string[]>([])
const tokenBindingEmail = ref('')
const isExtractingTokenBinding = ref(false)
const isAccountSetupOpen = ref(false)
const showEditNameDialog = ref(false)
const logProblemCount = ref(0)

function openEditNameDialog() {
  if (!displaySelectedOption.value) return
  showEditNameDialog.value = true
}

// Album target selection state (Task 3)
type AlbumTargetMode = 'none' | 'auto' | 'custom'
const albumTargetMode = ref<AlbumTargetMode>('none')
const customAlbumName = ref('')
const showAlbumKeyHelp = ref(false)

const isInputAnAlbumKey = computed(() => {
  const trimmed = albumNameOrKey.value.trim()
  if (trimmed.startsWith('AF1Qip')) return true
  if (trimmed.startsWith('http://') || trimmed.startsWith('https://')) {
    return trimmed.includes('AF1Qip')
  }
  return false
})

watch(albumNameOrKey, (val) => {
  const trimmed = val.trim()
  if (trimmed.startsWith('AF1Qip') && albumNicknames.value[trimmed]) {
    albumNickname.value = albumNicknames.value[trimmed]
  }
})

const albumDisplayLabel = computed(() => {
  if (albumTargetMode.value === 'auto') {
    return 'Auto by folder'
  }
  if (albumTargetMode.value === 'custom' && customAlbumName.value) {
    if (albumNicknames.value[customAlbumName.value]) {
      return albumNicknames.value[customAlbumName.value]
    }
    return customAlbumName.value
  }
  return 'None'
})

function openAlbumPrompt() {
  albumNameOrKey.value = albumTargetMode.value === 'custom' ? customAlbumName.value : ''
  if (albumNameOrKey.value && albumNicknames.value[albumNameOrKey.value]) {
    albumNickname.value = albumNicknames.value[albumNameOrKey.value] || ''
  } else {
    albumNickname.value = ''
  }
  showAlbumInput.value = true
}

// Quality indicator state (Task 1)
const qualityModeLabel = computed(() => {
  if (preferencesState.useQuota) {
    return 'Counts to quota'
  }
  if (preferencesState.saver) {
    return 'Storage saver'
  }
  return 'Unlimited'
})

// Activity page state (Task 4)
interface BatchSummaryItem {
  runId: string
  name: string
  timestamp: string
  totalCount: number
  uploadedCount: number
  skippedCount: number
  failedCount: number
  duration: string
  sizeFormatted: string
}

const activityLogs = ref<LogEntry[]>([])
const activityProblems = ref<LogEntry[]>([])
const activityBatches = ref<BatchSummaryItem[]>([])
const activityFilterLevel = ref<'ALL' | 'INFO' | 'WARN' | 'ERROR'>('ALL')
const sessionProcessedCount = ref(0)
const sessionSkippedCount = ref(0)

const displayProcessedCount = computed(() => {
  if (mockState.enabled && sessionProcessedCount.value === 0) {
    return 170
  }
  return sessionProcessedCount.value + uploadState.uploadedFiles
})

const displaySkippedCount = computed(() => {
  if (mockState.enabled && sessionSkippedCount.value === 0) {
    return 15
  }
  return sessionSkippedCount.value + uploadState.results.skipped.length
})

const displayBatches = computed(() => {
  if (mockState.enabled && activityBatches.value.length === 0) {
    return mockBatches
  }
  return activityBatches.value
})

const selectedRunId = ref<string | null>(null)

function toggleRunExpand(runId: string) {
  selectedRunId.value = selectedRunId.value === runId ? null : runId
}

function getRunFiles(runId: string): LogEntry[] {
  const logs = (mockState.enabled && activityLogs.value.length === 0)
    ? mockLogEntries
    : activityLogs.value
  return logs.filter((l) => l.runId === runId && (l.fileName || l.filePath))
}

function formatFileSize(bytes?: number): string {
  if (!bytes || bytes <= 0) return ''
  if (bytes < 1024) return `${bytes} B`
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`
}

const displayProblems = computed(() => {
  if (mockState.enabled && activityProblems.value.length === 0) {
    return mockProblems
  }
  return activityProblems.value
})

const displayLogs = computed(() => {
  let list = activityLogs.value
  if (mockState.enabled && list.length === 0) {
    list = mockLogEntries
  }
  if (activityFilterLevel.value === 'ALL') {
    return list
  }
  return list.filter(e => e.level?.toUpperCase() === activityFilterLevel.value)
})

async function loadActivityData() {
  try {
    const entries = await ConfigManager.GetLogEntries(200, 'ALL')
    if (entries && entries.length > 0) {
      activityLogs.value = entries
      activityProblems.value = entries.filter(e => e.level === 'WARN' || e.level === 'ERROR')
      logProblemCount.value = activityProblems.value.length

      const batchesMap = new Map<string, BatchSummaryItem>()
      for (const entry of entries) {
        if (!entry.runId) continue
        if (!batchesMap.has(entry.runId)) {
          batchesMap.set(entry.runId, {
            runId: entry.runId,
            name: entry.runId,
            timestamp: entry.timestamp ? new Date(entry.timestamp).toLocaleTimeString() : 'Recent',
            totalCount: 0,
            uploadedCount: 0,
            skippedCount: 0,
            failedCount: 0,
            duration: entry.durationMs ? `${Math.round(entry.durationMs / 1000)}s` : '--',
            sizeFormatted: entry.sizeBytes ? `${(entry.sizeBytes / 1024 / 1024).toFixed(1)} MB` : '--',
          })
        }
        const b = batchesMap.get(entry.runId)!
        b.totalCount++
        if (entry.outcome === 'UPLOADED') {
          b.uploadedCount++
          sessionProcessedCount.value++
        } else if (entry.outcome === 'FAILED') {
          b.failedCount++
        } else if (entry.outcome?.startsWith('SKIPPED')) {
          b.skippedCount++
          sessionSkippedCount.value++
        }
      }
      activityBatches.value = Array.from(batchesMap.values())
    }
  } catch (err) {
    console.debug('Failed to load activity data:', err)
  }
}
const removingAccount = ref('')
const showRemoveAccountConfirm = ref(false)
const accountToRemove = ref('')

function promptRemoveAccount(email: string) {
  accountToRemove.value = email
  showRemoveAccountConfirm.value = true
}

async function handleConfirmRemoveAccount(email: string) {
  const success = await removeCredentials(email)
  if (success) {
    showRemoveAccountConfirm.value = false
    accountToRemove.value = ''
  }
}
const isSettling = ref(false)
const settleStagger = ref(40)
const isSplashActive = ref(typeof window !== 'undefined' && !(window as unknown as { __LUMEN_LAUNCHED__?: boolean }).__LUMEN_LAUNCHED__)

const railLogoRotation = ref(0)

watch(themePulseCounter, () => {
  railLogoRotation.value += 360
})

function handleRailLogoClick() {
  railLogoRotation.value += 360
}

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

const displayOptions = computed(() => {
  let list: string[] = []
  if (import.meta.env.DEV && mockState.enabled) {
    list = mockState.currentView === 'empty' ? [] : [...mockState.accounts]
  } else {
    list = [...options.value]
  }
  const active = displaySelectedOption.value
  if (active && list.includes(active)) {
    return [active, ...list.filter((email) => email !== active)]
  }
  return list
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
  if (newTab === 'upload') {
    await refreshPreferences()
  }
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
  showAlbumKeyHelp.value = false
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

// Drop zone element & drag state
const dropZoneRef = ref<HTMLElement | null>(null)
let dropObserver: MutationObserver | null = null
let dragCounter = 0
let isNativeDragging = false

function hasDragFiles(e: DragEvent): boolean {
  if (!e.dataTransfer) return false
  const types = e.dataTransfer.types
  if (!types || types.length === 0) return true
  for (let i = 0; i < types.length; i++) {
    const t = types[i]
    if (t === 'Files' || t === 'text/uri-list' || t === 'application/x-moz-file' || t === 'File') {
      return true
    }
  }
  return false
}

const onDragEnter = (e: DragEvent) => {
  if (!hasDragFiles(e)) return
  e.preventDefault()
  dragCounter++
  isNativeDragging = true
  isDraggingFiles.value = true
}

const onDragOver = (e: DragEvent) => {
  if (!hasDragFiles(e)) return
  e.preventDefault()
  if (e.dataTransfer) {
    e.dataTransfer.dropEffect = 'copy'
  }
  isNativeDragging = true
  isDraggingFiles.value = true
}

const onDragLeave = (e: DragEvent) => {
  if (!hasDragFiles(e)) return
  e.preventDefault()
  dragCounter--
  if (dragCounter <= 0) {
    dragCounter = 0
    isNativeDragging = false
    if (!dropZoneRef.value?.classList.contains('file-drop-target-active')) {
      isDraggingFiles.value = false
    }
  }
}

const onDrop = (e?: DragEvent) => {
  if (e) e.preventDefault()
  dragCounter = 0
  isNativeDragging = false
  isDraggingFiles.value = false
}

const onDropZoneDragEnter = (e: DragEvent) => {
  if (!hasDragFiles(e)) return
  e.preventDefault()
  isNativeDragging = true
  isDraggingFiles.value = true
}

const onDropZoneDragOver = (e: DragEvent) => {
  if (!hasDragFiles(e)) return
  e.preventDefault()
  if (e.dataTransfer) {
    e.dataTransfer.dropEffect = 'copy'
  }
  isNativeDragging = true
  isDraggingFiles.value = true
}

const onDropZoneDragLeave = (e: DragEvent) => {
  e.preventDefault()
  if (!dropZoneRef.value?.classList.contains('file-drop-target-active')) {
    if (dragCounter <= 0) {
      isDraggingFiles.value = false
    }
  }
}

async function handleFilesDropped(files: string[], dropZone = 'regular') {
  if (!files || files.length === 0) return

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
      if (dropZone === 'auto-album' || albumTargetMode.value === 'auto') {
        await ConfigManager.SetAlbumName('')
        await ConfigManager.SetAlbumAutoMode(true)
      } else if (albumTargetMode.value === 'custom') {
        await ConfigManager.SetAlbumName(customAlbumName.value)
        await ConfigManager.SetAlbumAutoMode(false)
      } else {
        await ConfigManager.SetAlbumName('')
        await ConfigManager.SetAlbumAutoMode(false)
      }
      await Events.Emit('startUpload', { files })
    } catch (error) {
      console.error('Failed to start upload:', error)
      toast.error('Failed to start upload', {
        description: error instanceof Error ? error.message : String(error),
      })
    }
  }
}

const onDropZoneDrop = (e: DragEvent) => {
  e.preventDefault()
  dragCounter = 0
  isNativeDragging = false
  isDraggingFiles.value = false
  if (e.dataTransfer?.files && e.dataTransfer.files.length > 0) {
    const fileList = Array.from(e.dataTransfer.files)
    const paths = fileList.map((f: any) => f.path || f.name).filter(Boolean)
    if (paths.length > 0) {
      handleFilesDropped(paths, 'regular')
    }
  }
}

async function handleCancelIndexing() {
  if (mockState.enabled && mockState.currentView === 'indexing') {
    setMockView('drop')
    return
  }
  await uploadManager.cancelScan()
}

watch(dropZoneRef, (el) => {
  if (dropObserver) {
    dropObserver.disconnect()
    dropObserver = null
  }
  if (el) {
    dropObserver = new MutationObserver(() => {
      const isTargetActive = el.classList.contains('file-drop-target-active') || false
      if (isTargetActive) {
        isDraggingFiles.value = true
      } else if (!isNativeDragging) {
        isDraggingFiles.value = false
      }
    })
    dropObserver.observe(el, { attributes: true, attributeFilter: ['class'] })
  }
})

// Album upload confirmation
async function loadRecentAlbums() {
  try {
    recentAlbums.value = (await ConfigManager.GetRecentAlbums()) || []
    if (selectedOption.value) {
      albumNicknames.value = (await ConfigManager.GetAlbumNicknames(selectedOption.value)) || {}
    }
  } catch (err) {
    console.error('Failed to load recent albums', err)
  }
}

function onSelectRecentAlbum(recent: string) {
  albumNameOrKey.value = recent
  albumNickname.value = albumNicknames.value[recent] || ''
  albumTargetMode.value = 'custom'
}

async function removeRecentAlbum(name: string, e: Event) {
  e.stopPropagation()
  await ConfigManager.RemoveRecentAlbum(name)
  await loadRecentAlbums()
}

function parseAlbumInput(input: string): { keyOrName: string; error?: string } {
  const trimmed = input.trim()
  if (!trimmed) return { keyOrName: '' }

  // 1. Raw album key (starts with AF1Qip)
  if (trimmed.startsWith('AF1Qip')) {
    const key = trimmed.split(/[?#/& \t\n\r]/)[0]
    return { keyOrName: key }
  }

  // 2. HTTP or HTTPS link
  if (trimmed.startsWith('http://') || trimmed.startsWith('https://')) {
    const idx = trimmed.indexOf('AF1Qip')
    if (idx === -1) {
      return {
        keyOrName: '',
        error: 'Invalid Google Photos album link. Expected an album URL with a valid album key (starts with AF1Qip).'
      }
    }
    const key = trimmed.slice(idx).split(/[?#/& \t\n\r]/)[0]
    if (key.length <= 6) {
      return {
        keyOrName: '',
        error: 'Invalid Google Photos album link. Extracted key is malformed.'
      }
    }
    return { keyOrName: key }
  }

  // 3. Regular album name
  return { keyOrName: trimmed }
}

const confirmAlbumUpload = async () => {
  const rawInput = albumNameOrKey.value.trim()
  const parsed = parseAlbumInput(rawInput)
  if (parsed.error) {
    toast.error(parsed.error)
    return
  }

  const chosenAlbum = parsed.keyOrName
  if (chosenAlbum) {
    albumTargetMode.value = 'custom'
    customAlbumName.value = chosenAlbum
    if (chosenAlbum.startsWith('AF1Qip') && albumNickname.value.trim() && selectedOption.value) {
      await ConfigManager.SetAlbumNickname(selectedOption.value, chosenAlbum, albumNickname.value.trim())
      albumNicknames.value[chosenAlbum] = albumNickname.value.trim()
    }
    await ConfigManager.AddRecentAlbum(chosenAlbum)
    await ConfigManager.SetAlbumName(chosenAlbum)
    await ConfigManager.SetAlbumAutoMode(false)
  } else if (albumTargetMode.value === 'auto') {
    await ConfigManager.SetAlbumName('')
    await ConfigManager.SetAlbumAutoMode(true)
  } else {
    albumTargetMode.value = 'none'
    customAlbumName.value = ''
    await ConfigManager.SetAlbumName('')
    await ConfigManager.SetAlbumAutoMode(false)
  }

  showAlbumInput.value = false
  showAlbumKeyHelp.value = false
  albumNickname.value = ''

  if (import.meta.env.DEV && mockState.enabled) {
    if (pendingFiles.value.length > 0) {
      pendingFiles.value = []
      pendingFileCount.value = 0
      albumNameOrKey.value = ''
      albumNickname.value = ''
      mockState.currentView = 'uploading'
      uploadManager.state.isUploading = true
    } else {
      toast.success(`Target album set: ${albumDisplayLabel.value}`)
    }
    return
  }

  if (pendingFiles.value.length > 0) {
    Events.Emit('startUpload', { files: pendingFiles.value })
    pendingFiles.value = []
    pendingFileCount.value = 0
    albumNameOrKey.value = ''
    albumNickname.value = ''
  } else {
    toast.success(`Target album set: ${albumDisplayLabel.value}`)
  }
}

const cancelAlbumUpload = () => {
  showAlbumInput.value = false
  showAlbumKeyHelp.value = false
  pendingFiles.value = []
  pendingFileCount.value = 0
  albumNameOrKey.value = ''
  albumNickname.value = ''
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
      if (albumTargetMode.value === 'auto') {
        await ConfigManager.SetAlbumName('')
        await ConfigManager.SetAlbumAutoMode(true)
      } else if (albumTargetMode.value === 'custom') {
        await ConfigManager.SetAlbumName(customAlbumName.value)
        await ConfigManager.SetAlbumAutoMode(false)
      } else {
        await ConfigManager.SetAlbumName('')
        await ConfigManager.SetAlbumAutoMode(false)
      }
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
      if (albumTargetMode.value === 'auto') {
        await ConfigManager.SetAlbumName('')
        await ConfigManager.SetAlbumAutoMode(true)
      } else if (albumTargetMode.value === 'custom') {
        await ConfigManager.SetAlbumName(customAlbumName.value)
        await ConfigManager.SetAlbumAutoMode(false)
      } else {
        await ConfigManager.SetAlbumName('')
        await ConfigManager.SetAlbumAutoMode(false)
      }
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
  await refreshPreferences()
  await loadActivityData()
  try {
    const cfg = await ConfigManager.GetSettings()
    if (cfg) {
      isSaverMode.value = !!cfg.saver
    }
  } catch {
    // Ignore in test/mock environment
  }
  isAppReady.value = true

  document.addEventListener('dragenter', onDragEnter)
  document.addEventListener('dragleave', onDragLeave)
  document.addEventListener('dragover', onDragOver)
  document.addEventListener('drop', onDrop)
  window.addEventListener('albumError', albumErrorHandler)
  window.addEventListener('uploadError', uploadErrorHandler)

  Events.On('folder-watch:detected', (data: any) => {
    const count = data?.count || (Array.isArray(data?.files) ? data.files.length : 1)
    toast.info(`Auto-watch detected ${count} new file${count === 1 ? '' : 's'}. Uploading...`)
  })

  onGlobalKeydown = (e: KeyboardEvent) => {
    if (e.key === 'Escape') {
      if (isAccountMenuOpen.value) {
        e.stopPropagation()
        isAccountMenuOpen.value = false
        return
      }
      if (showEditNameDialog.value) {
        e.stopPropagation()
        showEditNameDialog.value = false
        return
      }
      if (showAlbumInput.value) {
        e.stopPropagation()
        cancelAlbumUpload()
      }
    }
  }
  window.addEventListener('keydown', onGlobalKeydown)

  ConfigManager.GetProblemLogs().then(problems => {
    logProblemCount.value = problems?.length || 0
  }).catch(err => {
    console.debug('Failed to get problem logs on startup', err)
  })

  Events.On('log:entry', (event: { data: LogEntry }) => {
    const entry = event.data
    if (!entry) return
    activityLogs.value.unshift(entry)
    if (entry.level === 'WARN' || entry.level === 'ERROR') {
      logProblemCount.value++
      activityProblems.value.unshift(entry)
    }
    if (entry.outcome === 'UPLOADED') {
      sessionProcessedCount.value++
    } else if (entry.outcome?.startsWith('SKIPPED')) {
      sessionSkippedCount.value++
    }
    if (entry.runId) {
      let batch = activityBatches.value.find(b => b.runId === entry.runId)
      if (!batch) {
        batch = {
          runId: entry.runId,
          name: entry.runId,
          timestamp: new Date().toLocaleTimeString(),
          totalCount: 0,
          uploadedCount: 0,
          skippedCount: 0,
          failedCount: 0,
          duration: '--',
          sizeFormatted: '--',
        }
        activityBatches.value.unshift(batch)
      }
      batch.totalCount++
      if (entry.outcome === 'UPLOADED') {
        batch.uploadedCount++
      } else if (entry.outcome === 'FAILED') {
        batch.failedCount++
      } else if (entry.outcome?.startsWith('SKIPPED')) {
        batch.skippedCount++
      }
    }
  })

  Events.On('files-dropped', async (event: { data: { files: string[]; dropZone: string } }) => {
    isDraggingFiles.value = false
    dragCounter = 0
    isNativeDragging = false
    await handleFilesDropped(event.data.files, event.data.dropZone)
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
  if (dropObserver) {
    dropObserver.disconnect()
    dropObserver = null
  }
})
</script>

<template>
  <main
    class="relative flex h-full w-full overflow-hidden select-none bg-[var(--md-sys-color-surface)] text-[var(--md-sys-color-on-surface)]"
    style="--wails-draggable: drag"
  >
    <!-- Navigation Rail (M3 Expressive Style, ~80px w-20) -->
    <nav
      class="flex h-full w-20 shrink-0 flex-col items-center justify-between border-r border-[var(--md-sys-color-outline-variant)] bg-[var(--md-sys-color-surface)] py-4 select-none z-20"
      style="contain: layout style; --wails-draggable: drag"
      aria-label="Application Navigation Rail"
    >
      <!-- Top App Mark & Rail Items -->
      <div
        class="flex w-full flex-col items-center gap-3"
        style="--wails-draggable: none"
      >
        <!-- Lumen Brand Mark (No background box or container) -->
        <div
          id="rail-app-logo"
          class="flex size-10 shrink-0 items-center justify-center cursor-pointer mb-1 bg-transparent select-none transition-transform duration-700 ease-out"
          :class="isSplashActive ? 'opacity-0' : 'opacity-100'"
          :style="{
            transform: `rotate(${railLogoRotation}deg)`,
            transformOrigin: 'center center',
          }"
          title="Lumen • Google Photos Desktop"
          aria-label="Lumen application"
          @click="handleRailLogoClick"
        >
          <LumenLogo :size="40" />
        </div>

        <!-- 1. Upload Item -->
        <button
          type="button"
          class="nav-rail-item group relative flex flex-col items-center justify-center w-16 py-1.5 rounded-2xl transition-colors cursor-pointer text-[var(--md-sys-color-on-surface-variant)] hover:bg-[var(--md-sys-color-surface-container-high)]/70 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--md-sys-color-primary)]"
          :class="activeTab === 'upload' ? 'text-[var(--md-sys-color-on-surface)]' : ''"
          title="Upload photos"
          aria-label="Upload view"
          @click="activeTab = 'upload'"
        >
          <div
            class="indicator w-12 h-7 rounded-2xl flex items-center justify-center transition-colors"
            :class="activeTab === 'upload' ? 'bg-[var(--md-sys-color-secondary-container)] text-[var(--md-sys-color-on-secondary-container)]' : 'text-[var(--md-sys-color-on-surface-variant)]'"
          >
            <M3Icon
              name="cloud_upload"
              :filled="activeTab === 'upload'"
              :size="20"
            />
          </div>
          <span class="m3-rail-label mt-1">Upload</span>
        </button>

        <!-- 2. Folders Item -->
        <button
          type="button"
          class="nav-rail-item group relative flex flex-col items-center justify-center w-16 py-1.5 rounded-2xl transition-colors cursor-pointer text-[var(--md-sys-color-on-surface-variant)] hover:bg-[var(--md-sys-color-surface-container-high)]/70 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--md-sys-color-primary)]"
          :class="activeTab === 'folders' ? 'text-[var(--md-sys-color-on-surface)]' : ''"
          title="Backup folders"
          aria-label="Backup folders view"
          @click="activeTab = 'folders'"
        >
          <div
            class="indicator w-12 h-7 rounded-2xl flex items-center justify-center transition-colors"
            :class="activeTab === 'folders' ? 'bg-[var(--md-sys-color-secondary-container)] text-[var(--md-sys-color-on-secondary-container)]' : 'text-[var(--md-sys-color-on-surface-variant)]'"
          >
            <M3Icon
              name="folder"
              :filled="activeTab === 'folders'"
              :size="20"
            />
          </div>
          <span class="m3-rail-label mt-1">Folders</span>
        </button>

        <!-- 2. Activity Item -->
        <button
          type="button"
          class="nav-rail-item group relative flex flex-col items-center justify-center w-16 py-1.5 rounded-2xl transition-colors cursor-pointer text-[var(--md-sys-color-on-surface-variant)] hover:bg-[var(--md-sys-color-surface-container-high)]/70 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--md-sys-color-primary)]"
          :class="activeTab === 'activity' ? 'text-[var(--md-sys-color-on-surface)]' : ''"
          title="Activity & problem logs"
          aria-label="Activity logs view"
          @click="activeTab = 'activity'"
        >
          <div
            class="indicator relative w-12 h-7 rounded-2xl flex items-center justify-center transition-colors"
            :class="activeTab === 'activity' ? 'bg-[var(--md-sys-color-secondary-container)] text-[var(--md-sys-color-on-secondary-container)]' : 'text-[var(--md-sys-color-on-surface-variant)]'"
          >
            <M3Icon
              name="list_alt"
              :filled="activeTab === 'activity'"
              :size="20"
            />
            <span
              v-if="logProblemCount > 0"
              class="absolute -top-0.5 -right-0.5 flex size-3.5 items-center justify-center rounded-full bg-[var(--md-sys-color-error)] m3-badge-label text-[var(--md-sys-color-on-error)] shadow-xs"
            >
              {{ logProblemCount > 9 ? '9+' : logProblemCount }}
            </span>
          </div>
          <span class="m3-rail-label mt-1">Activity</span>
        </button>

        <!-- 3. Accounts Item -->
        <button
          type="button"
          class="nav-rail-item group relative flex flex-col items-center justify-center w-16 py-1.5 rounded-2xl transition-colors cursor-pointer text-[var(--md-sys-color-on-surface-variant)] hover:bg-[var(--md-sys-color-surface-container-high)]/70 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--md-sys-color-primary)]"
          :class="activeTab === 'accounts' ? 'text-[var(--md-sys-color-on-surface)]' : ''"
          title="Google accounts"
          aria-label="Accounts view"
          @click="activeTab = 'accounts'"
        >
          <div
            class="indicator w-12 h-7 rounded-2xl flex items-center justify-center transition-colors"
            :class="activeTab === 'accounts' ? 'bg-[var(--md-sys-color-secondary-container)] text-[var(--md-sys-color-on-secondary-container)]' : 'text-[var(--md-sys-color-on-surface-variant)]'"
          >
            <M3Icon
              name="manage_accounts"
              :filled="activeTab === 'accounts'"
              :size="20"
            />
          </div>
          <span class="m3-rail-label mt-1">Accounts</span>
        </button>

        <!-- 4. Settings Item -->
        <button
          type="button"
          class="nav-rail-item group relative flex flex-col items-center justify-center w-16 py-1.5 rounded-2xl transition-colors cursor-pointer text-[var(--md-sys-color-on-surface-variant)] hover:bg-[var(--md-sys-color-surface-container-high)]/70 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--md-sys-color-primary)]"
          :class="activeTab === 'settings' ? 'text-[var(--md-sys-color-on-surface)]' : ''"
          title="Settings"
          aria-label="Settings view"
          @click="activeTab = 'settings'"
        >
          <div
            class="indicator w-12 h-7 rounded-2xl flex items-center justify-center transition-colors"
            :class="activeTab === 'settings' ? 'bg-[var(--md-sys-color-secondary-container)] text-[var(--md-sys-color-on-secondary-container)]' : 'text-[var(--md-sys-color-on-surface-variant)]'"
          >
            <M3Icon
              name="settings"
              :filled="activeTab === 'settings'"
              :size="20"
            />
          </div>
          <span class="m3-rail-label mt-1">Settings</span>
        </button>

        <!-- 5. About Item -->
        <button
          type="button"
          class="nav-rail-item group relative flex flex-col items-center justify-center w-16 py-1.5 rounded-2xl transition-colors cursor-pointer text-[var(--md-sys-color-on-surface-variant)] hover:bg-[var(--md-sys-color-surface-container-high)]/70 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--md-sys-color-primary)]"
          :class="activeTab === 'about' ? 'text-[var(--md-sys-color-on-surface)]' : ''"
          title="About Lumen"
          aria-label="About view"
          @click="activeTab = 'about'"
        >
          <div
            class="indicator w-12 h-7 rounded-2xl flex items-center justify-center transition-colors"
            :class="activeTab === 'about' ? 'bg-[var(--md-sys-color-secondary-container)] text-[var(--md-sys-color-on-secondary-container)]' : 'text-[var(--md-sys-color-on-surface-variant)]'"
          >
            <M3Icon
              name="info"
              :filled="activeTab === 'about'"
              :size="20"
            />
          </div>
          <span class="m3-rail-label mt-1">About</span>
        </button>

        <!-- 6. Developer Item (Visible only when unlocked) -->
        <button
          v-if="devOptionsState.unlocked"
          type="button"
          class="nav-rail-item group relative flex flex-col items-center justify-center w-16 py-1.5 rounded-2xl transition-colors cursor-pointer text-[var(--md-sys-color-on-surface-variant)] hover:bg-[var(--md-sys-color-surface-container-high)]/70 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--md-sys-color-primary)]"
          :class="activeTab === 'developer' ? 'text-[var(--md-sys-color-on-surface)]' : ''"
          title="Developer options"
          aria-label="Developer view"
          @click="activeTab = 'developer'"
        >
          <div
            class="indicator w-12 h-7 rounded-2xl flex items-center justify-center transition-colors"
            :class="activeTab === 'developer' ? 'bg-[var(--md-sys-color-secondary-container)] text-[var(--md-sys-color-on-secondary-container)]' : 'text-[var(--md-sys-color-on-surface-variant)]'"
          >
            <M3Icon
              name="terminal"
              :filled="activeTab === 'developer'"
              :size="20"
            />
          </div>
          <span class="m3-rail-label mt-1">Developer</span>
        </button>
      </div>

      <!-- Bottom: Small Account Chip & Popover Menu -->
      <div
        ref="accountChipContainerRef"
        class="relative flex w-full flex-col items-center justify-center px-2"
        style="--wails-draggable: none"
      >
        <!-- Hidden file input for custom avatar uploading -->
        <input
          ref="avatarFileInput"
          type="file"
          accept="image/*"
          class="hidden"
          aria-label="Upload custom profile photo"
          @change="onAvatarFileSelected"
        >

        <!-- Small Account Chip (Avatar only) -->
        <button
          ref="accountTriggerRef"
          type="button"
          class="group relative flex size-9 items-center justify-center rounded-full border bg-[var(--md-sys-color-surface-container)] p-0.5 hover:bg-[var(--md-sys-color-surface-container-high)] hover:scale-105 transition cursor-pointer focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--md-sys-color-primary)]"
          :class="isAccountMenuOpen
            ? 'ring-2 ring-[var(--md-sys-color-primary)] ring-offset-2 ring-offset-[var(--md-sys-color-surface)] border-[var(--md-sys-color-primary)]'
            : 'border-[var(--md-sys-color-outline-variant)]'"
          :title="displaySelectedOption ? `Signed in as ${displaySelectedOption}` : 'No account connected'"
          aria-label="Account options"
          aria-haspopup="menu"
          :aria-expanded="isAccountMenuOpen"
          @click="isAccountMenuOpen = !isAccountMenuOpen"
        >
          <div
            class="size-7 rounded-full flex items-center justify-center m3-label-large overflow-hidden"
            :style="{
              backgroundColor: customAvatar ? 'transparent' : getAvatarColor(displaySelectedOption).bg,
              color: getAvatarColor(displaySelectedOption).text,
            }"
          >
            <img
              v-if="customAvatar"
              :src="customAvatar"
              alt="Avatar"
              class="size-full object-cover"
            >
            <span v-else-if="displaySelectedOption">
              {{ getEmailInitial(displaySelectedOption) }}
            </span>
            <M3Icon
              v-else
              name="person"
              :size="16"
            />
          </div>
        </button>

        <!-- Account Popover Menu as Teleported Overlay -->
        <AccountPickerMenu
          :open="isAccountMenuOpen"
          :anchor-el="accountTriggerRef"
          :active-account="displaySelectedOption"
          :accounts="displayOptions"
          :custom-avatar="customAvatar"
          @close="isAccountMenuOpen = false"
          @switch-account="switchAccount"
          @manage-accounts="activeTab = 'accounts'; isAccountMenuOpen = false"
          @cycle-theme="cycleNextTheme"
          @upload-avatar="triggerAvatarUpload"
        />
      </div>
    </nav>

    <!-- Main Content Area -->
    <section
      class="m3-main-viewport relative flex flex-1 h-full min-w-0 flex-col overflow-hidden bg-[var(--md-sys-color-surface)]"
      :class="isSettling && 'm3-launch-settle'"
      :style="isSettling ? { '--settle-index': 2, '--launch-stagger': settleStagger + 'ms', '--wails-draggable': 'drag', contain: 'layout style' } : { '--wails-draggable': 'drag', contain: 'layout style' }"
    >
      <Transition name="fade" mode="out-in">
        <!-- View: Upload Tab -->
        <div
          v-if="activeTab === 'upload'"
          key="upload"
          data-tab="upload"
          class="m3-tab-pane flex h-full w-full flex-1 flex-col overflow-y-auto overflow-x-hidden p-6"
          style="contain: layout style; --wails-draggable: none"
        >
          <div
            class="w-full mx-auto flex flex-col h-full"
            :class="(uploadState.isUploading || (mockState.enabled && mockState.currentView === 'uploading')) ? 'max-w-[1600px]' : 'max-w-[768px]'"
          >
          <!-- 1. Indexing Loading Screen (When scanner is running before upload starts) -->
          <div
            v-if="uploadState.scanProgress.isScanning || (mockState.enabled && mockState.currentView === 'indexing')"
            class="h-full w-full flex flex-col items-center justify-center my-auto py-10 px-4"
            style="--wails-draggable: none"
          >
            <div class="w-full max-w-md rounded-[32px] border border-[var(--md-sys-color-outline-variant)]/30 bg-[var(--md-sys-color-surface-container)] p-8 sm:p-10 text-center shadow-xl flex flex-col items-center justify-center gap-4">
              <!-- M3E Shape-Morphing Loading Indicator -->
              <div class="flex items-center justify-center p-3 rounded-full bg-[var(--md-sys-color-primary-container)]/30">
                <M3LoadingIndicator
                  size="xl"
                  class="text-[var(--md-sys-color-primary)]"
                />
              </div>

              <div class="flex flex-col items-center gap-1.5 mt-1">
                <h2 class="m3-headline-small text-[var(--md-sys-color-on-surface)]">
                  Indexing Media Folders
                </h2>
                <div class="flex items-center gap-2 mt-1">
                  <span class="rounded-full bg-[var(--md-sys-color-primary-container)] px-3 py-0.5 m3-label-large text-[var(--md-sys-color-on-primary-container)] tabular-nums">
                    {{ uploadState.scanProgress.filesFound.toLocaleString() }} {{ uploadState.scanProgress.filesFound === 1 ? 'file' : 'files' }} found
                  </span>
                </div>
                <p
                  v-if="uploadState.scanProgress.currentDirectory"
                  class="m3-mono m3-body-medium mt-2 max-w-sm truncate"
                  :title="uploadState.scanProgress.currentDirectory"
                >
                  {{ uploadState.scanProgress.currentDirectory }}
                </p>
                <p
                  v-else
                  class="m3-body-medium mt-2"
                >
                  Scanning enabled folders for photos and videos...
                </p>
              </div>

              <!-- M3E Indeterminate Wavy Progress -->
              <div class="w-full max-w-xs mt-2">
                <M3WavyProgress :indeterminate="true" />
              </div>

              <div class="mt-4">
                <M3Button
                  variant="outlined"
                  size="s"
                  icon="close"
                  @click="handleCancelIndexing"
                >
                  Cancel Indexing
                </M3Button>
              </div>
            </div>
          </div>

          <!-- 2. If actively uploading, show Upload.vue directly in main area -->
          <div
            v-else-if="uploadState.isUploading || (mockState.enabled && mockState.currentView === 'uploading')"
            class="h-full w-full"
            style="--wails-draggable: none"
          >
            <Upload
              @open-sign-in="isAccountSetupOpen = true"
              @account-removed="refreshCredentials"
              @pick-more="handlePickFiles"
            />
          </div>

          <!-- 2. Results State (Upload summary matching screen-upload-results) -->
          <div
            v-else-if="(mockState.enabled && mockState.currentView === 'results') || (!mockState.enabled && (uploadState.uploadedFiles > 0 || uploadState.results.fail.length > 0))"
            class="h-full w-full flex flex-col justify-center my-auto py-6"
            style="--wails-draggable: none"
          >
            <!-- Safety Circuit Breaker Alert Banner -->
            <CircuitBreakerAlert
              @open-sign-in="isAccountSetupOpen = true"
              @account-removed="refreshCredentials"
            />

            <UploadResultsCard
              :results="uploadState.results"
              style="--wails-draggable: none"
              @copy="handleCopyClick"
              @clear="handleResetToEmpty"
              @retry-all="handleRetryAll"
              @retry-item="handleRetryItem"
            />
          </div>

          <!-- 3. No account connected empty state -->
          <div
            v-else-if="(mockState.enabled && mockState.currentView === 'empty') || displayOptions.length === 0"
            class="flex h-full w-full flex-1 flex-col items-center justify-center text-center p-8 select-none my-auto"
            style="--wails-draggable: none"
          >
            <div class="relative mb-6 flex size-28 items-center justify-center rounded-[36px] border border-[var(--md-sys-color-primary)]/40 bg-[var(--md-sys-color-primary-container)] text-[var(--md-sys-color-primary)] shadow-sm transition-transform duration-300 hover:scale-105">
              <M3Icon
                name="person_add"
                :filled="true"
                :size="56"
                class="text-[var(--md-sys-color-primary)]"
              />
            </div>
            <h1 class="m3-headline-large text-[var(--md-sys-color-on-surface)] select-none">
              Add Google account
            </h1>
            <p class="mt-3 m3-body-medium select-none max-w-md">
              Back up your photos and videos to Google Photos with albums and Live Photo support.
            </p>
            <div class="mt-6 flex flex-wrap items-center justify-center gap-2">
              <span class="rounded-full bg-[var(--md-sys-color-surface-container-high)] px-3.5 py-1 m3-label-large text-[var(--md-sys-color-on-surface-variant)] select-none">
                Fast uploads
              </span>
              <span class="rounded-full bg-[var(--md-sys-color-surface-container-high)] px-3.5 py-1 m3-label-large text-[var(--md-sys-color-on-surface-variant)] select-none">
                Album organization
              </span>
              <span class="rounded-full bg-[var(--md-sys-color-surface-container-high)] px-3.5 py-1 m3-label-large text-[var(--md-sys-color-on-surface-variant)] select-none">
                Original and saver quality
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
              <span>Add Google account</span>
            </M3Button>
            <p class="mt-4 m3-body-medium opacity-60 select-none">
              Credentials stay on your computer.
            </p>
          </div>

          <!-- 4. Empty State: Normal Drop View (with account connected, matching screen-upload-empty) -->
          <div
            v-else
            class="flex flex-col justify-center my-auto w-full select-none"
            style="--wails-draggable: none"
          >
            <!-- Page Header above the card -->
            <div class="mb-6 text-left w-full">
              <h1 class="m3-headline-large text-[var(--md-sys-color-on-surface)]">
                Upload
              </h1>
              <p class="m3-body-medium mt-1">
                Add photos and videos to back up to Google Photos.
              </p>
            </div>

            <!-- Safety Circuit Breaker Alert Banner -->
            <CircuitBreakerAlert
              @open-sign-in="isAccountSetupOpen = true"
              @account-removed="refreshCredentials"
            />

            <!-- Token binding alert banner if needed -->
            <div
              v-if="tokenBindingEmail"
              class="w-full shrink-0 rounded-[24px] border border-amber-500/30 bg-amber-500/10 p-4 mb-4 flex items-center justify-between gap-4"
              style="--wails-draggable: none"
            >
              <p class="m3-body-medium text-amber-200">
                This credential needs a token binding key from the rooted Android device it was captured from.
              </p>
              <M3Button
                variant="filled"
                size="xs"
                color="secondary"
                :disabled="isExtractingTokenBinding"
                @click="addTokenBindingAliasFromADB"
              >
                {{ isExtractingTokenBinding ? 'Reading ADB...' : 'Read from ADB' }}
              </M3Button>
            </div>

            <!-- Big M3 Expressive Drop Card -->
            <div
              ref="dropZoneRef"
              data-file-drop-target
              data-drop-zone="regular"
              class="drop-zone m3-drop-zone-glow group relative flex w-full min-h-[320px] flex-col items-center justify-center rounded-[28px] border-2 border-dashed p-10 text-center transition-all duration-300 shadow-lg cursor-default select-none"
              :class="isDraggingFiles ? 'is-dragging border-[var(--md-sys-color-primary)] bg-[var(--md-sys-color-primary-container)]/20 shadow-2xl ring-4 ring-[var(--md-sys-color-primary)]/20' : 'border-[var(--md-sys-color-outline)] bg-[var(--md-sys-color-surface-container)] hover:border-[var(--md-sys-color-outline)]/80'"
              style="--wails-draggable: none"
              @dragenter.prevent="onDropZoneDragEnter"
              @dragover.prevent="onDropZoneDragOver"
              @dragleave.prevent="onDropZoneDragLeave"
              @drop.prevent="onDropZoneDrop"
            >
              <Transition name="fade" mode="out-in">
                <!-- State A: Actively Dragging Files Over (Animated Drop State, buttons hidden) -->
                <div
                  v-if="isDraggingFiles"
                  key="dragging"
                  class="flex flex-col items-center justify-center py-4 pointer-events-none"
                >
                  <div class="relative flex size-24 items-center justify-center rounded-[32px] bg-[var(--md-sys-color-primary-container)] text-[var(--md-sys-color-primary)] shadow-lg animate-bounce">
                    <M3Icon
                      name="arrow_downward"
                      :size="48"
                    />
                  </div>
                  <h2 class="mt-6 m3-headline-small text-[var(--md-sys-color-primary)]">
                    Drop files to upload
                  </h2>
                  <p class="mt-2 m3-body-medium">
                    Release your photos and videos to start backing up
                  </p>
                </div>

                <!-- State B: Resting Idle Drop Target State -->
                <div
                  v-else
                  key="idle"
                  class="flex flex-col items-center justify-center w-full"
                >
                  <!-- 80x80 Icon Tile -->
                  <div class="w-20 h-20 rounded-[28px] bg-[var(--md-sys-color-surface-container-highest)] text-[var(--md-sys-color-primary)] flex items-center justify-center mb-5 group-hover:scale-105 transition duration-300">
                    <svg
                      class="w-10 h-10"
                      fill="none"
                      stroke="currentColor"
                      viewBox="0 0 24 24"
                    >
                      <path
                        stroke-linecap="round"
                        stroke-linejoin="round"
                        stroke-width="1.8"
                        d="M4 16l4.586-4.586a2 2 0 012.828 0L16 16m-2-2l1.586-1.586a2 2 0 012.828 0L20 14m-6-6h.01M6 20h12a2 2 0 002-2V6a2 2 0 00-2-2H6a2 2 0 00-2 2v12a2 2 0 002 2z"
                      />
                    </svg>
                  </div>

                  <!-- 24px Medium Title -->
                  <h2 class="m3-headline-small text-[var(--md-sys-color-on-surface)] select-none">
                    Drop photos and videos here
                  </h2>

                  <!-- 12-13px Description Line -->
                  <p class="m3-body-medium mt-1 max-w-[384px] select-none">
                    Drag images, videos, and folders here to back them up
                  </p>

                  <!-- Action buttons directly under text with 24px gap -->
                  <div
                    class="flex flex-col items-center gap-3 mt-6 z-10"
                    style="--wails-draggable: none"
                  >
                    <!-- Connected button group (segmented) for Choose files / Choose folder -->
                    <M3ButtonGroup :connected="true">
                      <M3Button
                        variant="filled"
                        size="m"
                        icon="add"
                        :disabled="isPickingMedia"
                        @click="handlePickFiles"
                      >
                        <span>Choose files</span>
                      </M3Button>

                      <M3Button
                        variant="tonal"
                        size="m"
                        icon="folder"
                        :disabled="isPickingMedia"
                        @click="handlePickFolder"
                      >
                        <span>Choose folder</span>
                      </M3Button>
                    </M3ButtonGroup>

                    <!-- Album Option Button -->
                    <M3Button
                      variant="tonal"
                      size="s"
                      icon="folder_special"
                      title="Albums made by Lumen are reused by name. To use an album you made in Google Photos, paste its link."
                      @click="openAlbumPrompt"
                    >
                      <span>Album: {{ albumDisplayLabel }}</span>
                    </M3Button>
                  </div>
                </div>
              </Transition>
            </div>

            <!-- Below the card: one slim row -->
            <div class="mt-8 flex items-center justify-between m3-body-medium px-4 select-none">
              <div class="flex items-center gap-2.5">
                <span class="flex items-center space-x-1.5">
                  <span class="size-2 rounded-full bg-[var(--md-sys-color-primary)]"></span>
                  <span class="m3-label-large text-[var(--md-sys-color-on-surface)]">{{ displaySelectedOption || 'No account selected' }}</span>
                </span>
                <span
                  class="rounded-full border px-2.5 py-0.5 m3-label-small transition-colors inline-flex items-center"
                  :class="
                    preferencesState.useQuota
                      ? 'bg-[var(--md-sys-color-error-container)] border-[var(--md-sys-color-error)]/40 text-[var(--md-sys-color-on-error-container)]'
                      : preferencesState.saver
                        ? 'bg-[var(--md-sys-color-tertiary-container)] border-[var(--md-sys-color-tertiary)]/40 text-[var(--md-sys-color-on-tertiary-container)]'
                        : 'bg-[var(--md-sys-color-primary-container)] border-[var(--md-sys-color-primary)]/40 text-[var(--md-sys-color-on-primary-container)]'
                  "
                >
                  {{ qualityModeLabel }}
                </span>
              </div>
              <span>Credentials stay on your computer</span>
            </div>
          </div>
        </div>
      </div>

        <!-- View: Folders Tab -->
        <div
          v-else-if="activeTab === 'folders'"
          key="folders"
          data-tab="folders"
          class="m3-tab-pane flex h-full w-full flex-1 flex-col overflow-y-auto overflow-x-hidden p-6"
          style="contain: layout style; --wails-draggable: none"
        >
          <FoldersView @upload-folder="onUploadFolderFromSettings" />
        </div>

        <!-- View: Activity Tab (Task 2: 2-column grid on >=1100px; Task 8: no live logs for normal users) -->
        <div
          v-else-if="activeTab === 'activity'"
          key="activity"
          data-tab="activity"
          class="m3-tab-pane flex h-full w-full flex-1 flex-col overflow-y-auto overflow-x-hidden p-6"
          style="contain: layout style; --wails-draggable: none"
        >
          <div class="w-full max-w-[1400px] mx-auto flex flex-col gap-6 pb-12">
            <!-- Header -->
            <div class="flex flex-col gap-1">
              <h1 class="m3-headline-large text-[var(--md-sys-color-on-surface)]">
                Activity
              </h1>
              <p class="m3-body-medium">
                Check your upload history, overall progress, and any problems.
              </p>
            </div>

            <!-- 2-column grid on >=1100px -->
            <div class="grid grid-cols-1 min-[1100px]:grid-cols-2 gap-6 items-start">
              <!-- Column 1: Health status, counters, problems -->
              <div class="flex flex-col gap-6">
                <!-- Health Status Banner (Task 11: plain language) -->
                <div
                  class="flex items-center justify-between gap-4 rounded-[28px] border p-5 shadow-xs"
                  :class="logProblemCount > 0
                    ? 'border-[var(--md-sys-color-error)]/40 bg-[var(--md-sys-color-error-container)]/30 text-[var(--md-sys-color-on-surface)]'
                    : 'border-[var(--md-sys-color-success)]/40 bg-[var(--md-sys-color-success-container)]/30 text-[var(--md-sys-color-on-surface)]'"
                >
                  <div class="flex items-center gap-3.5 min-w-0">
                    <div
                      class="flex size-11 shrink-0 items-center justify-center rounded-2xl shadow-xs"
                      :class="logProblemCount > 0
                        ? 'bg-[var(--md-sys-color-error)] text-[var(--md-sys-color-on-error)]'
                        : 'bg-[var(--md-sys-color-success)] text-[var(--md-sys-color-on-success)]'"
                    >
                      <M3Icon
                        :name="logProblemCount > 0 ? 'warning' : 'check_circle'"
                        :size="24"
                      />
                    </div>
                    <div class="flex flex-col min-w-0">
                      <span class="m3-title-medium">
                        {{ logProblemCount > 0 ? `${logProblemCount} problem${logProblemCount > 1 ? 's' : ''} found` : 'No problems found.' }}
                      </span>
                      <span class="m3-body-medium">
                        {{ logProblemCount > 0 ? 'Review the issues below to retry failed files.' : 'All services and upload workers are running.' }}
                      </span>
                    </div>
                  </div>
                </div>

                <!-- Session Counters -->
                <div class="grid grid-cols-1 sm:grid-cols-2 gap-4">
                  <div class="flex flex-col gap-2 rounded-[24px] border border-[var(--md-sys-color-outline-variant)]/40 bg-[var(--md-sys-color-surface-container)] p-5 shadow-xs">
                    <div class="flex items-center gap-2 text-[var(--md-sys-color-primary)]">
                      <M3Icon name="history" :size="20" />
                      <span class="m3-title-medium text-[var(--md-sys-color-on-surface)]">Upload session</span>
                    </div>
                    <p class="m3-body-medium leading-relaxed">
                      Files transferred or queued in this session.
                    </p>
                    <div class="mt-2 flex items-center justify-between m3-body-medium border-t border-[var(--md-sys-color-outline-variant)]/30 pt-3">
                      <span>Processed this session:</span>
                      <span class="m3-label-large tabular-nums text-[var(--md-sys-color-on-surface)]">{{ displayProcessedCount }} files</span>
                    </div>
                  </div>

                  <div class="flex flex-col gap-2 rounded-[24px] border border-[var(--md-sys-color-outline-variant)]/40 bg-[var(--md-sys-color-surface-container)] p-5 shadow-xs">
                    <div class="flex items-center gap-2 text-[var(--md-sys-color-primary)]">
                      <M3Icon name="fingerprint" :size="20" />
                      <span class="m3-title-medium text-[var(--md-sys-color-on-surface)]">Deduplication cache</span>
                    </div>
                    <p class="m3-body-medium leading-relaxed">
                      Local cache skips already backed up files.
                    </p>
                    <div class="mt-2 flex items-center justify-between m3-body-medium border-t border-[var(--md-sys-color-outline-variant)]/30 pt-3">
                      <span>Skipped duplicates:</span>
                      <span class="m3-label-large tabular-nums text-[var(--md-sys-color-on-surface)]">{{ displaySkippedCount }} files</span>
                    </div>
                  </div>
                </div>

                <!-- Problems (Task 4c: Classified errors with plain-language recovery action) -->
                <div class="flex flex-col gap-3 rounded-[28px] border border-[var(--md-sys-color-outline-variant)]/40 bg-[var(--md-sys-color-surface-container)] p-6 shadow-xs">
                  <div class="flex items-center justify-between">
                    <div class="flex items-center gap-2 text-[var(--md-sys-color-error)]">
                      <M3Icon name="report_problem" :size="20" />
                      <h2 class="m3-headline-small text-[var(--md-sys-color-on-surface)]">
                        Problems
                      </h2>
                    </div>
                    <span class="m3-body-medium">
                      {{ displayProblems.length }} item{{ displayProblems.length === 1 ? '' : 's' }}
                    </span>
                  </div>

                  <div class="flex flex-col gap-3 mt-1">
                    <div
                      v-for="prob in displayProblems"
                      :key="prob.id"
                      class="flex flex-col gap-2 rounded-[20px] border border-[var(--md-sys-color-error)]/30 bg-[var(--md-sys-color-surface-container-low)] p-4 shadow-xs"
                    >
                      <div class="flex items-center justify-between gap-3">
                        <div class="flex items-center gap-2 min-w-0">
                          <span class="rounded-md bg-[var(--md-sys-color-error-container)] px-2 py-0.5 m3-label-small text-[var(--md-sys-color-on-error-container)]">
                            {{ prob.reasonCode || prob.level }}
                          </span>
                          <span class="m3-title-medium text-[var(--md-sys-color-on-surface)] truncate">
                            {{ prob.fileName || prob.category || 'Notice' }}
                          </span>
                        </div>
                        <span class="m3-body-medium shrink-0">
                          {{ prob.timestamp ? new Date(prob.timestamp).toLocaleTimeString() : '' }}
                        </span>
                      </div>

                      <p class="m3-body-medium text-[var(--md-sys-color-on-surface)] leading-relaxed">
                        {{ prob.userMessage || 'An issue was recorded.' }}
                      </p>

                      <div
                        v-if="prob.suggestedAction"
                        class="flex items-start gap-2 rounded-xl bg-[var(--md-sys-color-surface-container-high)] p-3 m3-body-medium"
                      >
                        <M3Icon
                          name="lightbulb"
                          :size="16"
                          class="shrink-0 text-[var(--md-sys-color-primary)] mt-0.5"
                        />
                        <div class="leading-relaxed">
                          <strong class="text-[var(--md-sys-color-on-surface)]">How to fix: </strong>
                          {{ prob.suggestedAction }}
                        </div>
                      </div>
                    </div>

                    <div
                      v-if="displayProblems.length === 0"
                      class="flex flex-col items-center justify-center p-6 text-center m3-body-medium"
                    >
                      <M3Icon name="check_circle" :size="24" class="mb-1 text-[var(--md-sys-color-success)]" />
                      <span>No problems recorded.</span>
                    </div>
                  </div>
                </div>
              </div>

              <!-- Column 2: Upload history -->
              <div class="flex flex-col gap-6">
                <!-- Upload History -->
                <div class="flex flex-col gap-3 rounded-[28px] border border-[var(--md-sys-color-outline-variant)]/40 bg-[var(--md-sys-color-surface-container)] p-6 shadow-xs">
                  <div class="flex items-center justify-between">
                    <div class="flex items-center gap-2 text-[var(--md-sys-color-primary)]">
                      <M3Icon name="cloud_done" :size="20" />
                      <h2 class="m3-headline-small text-[var(--md-sys-color-on-surface)]">
                        Upload history
                      </h2>
                    </div>
                    <span class="m3-body-medium">
                      Recent batches
                    </span>
                  </div>

                  <div class="flex flex-col gap-2.5 mt-1">
                    <div
                      v-for="batch in displayBatches"
                      :key="batch.runId"
                      class="rounded-[20px] bg-[var(--md-sys-color-surface-container-low)] border border-[var(--md-sys-color-outline-variant)]/30 overflow-hidden shadow-xs transition-all duration-200"
                    >
                      <!-- Main Clickable Batch Row -->
                      <div
                        class="flex items-center justify-between gap-4 p-4 cursor-pointer hover:bg-[var(--md-sys-color-surface-container-high)]/60 transition-colors"
                        @click="toggleRunExpand(batch.runId)"
                      >
                        <div class="flex items-center gap-3.5 min-w-0">
                          <div class="flex size-10 shrink-0 items-center justify-center rounded-xl bg-[var(--md-sys-color-primary-container)] text-[var(--md-sys-color-on-primary-container)]">
                            <M3Icon name="backup" :size="20" />
                          </div>
                          <div class="flex flex-col min-w-0">
                            <div class="flex items-center gap-2">
                              <span class="m3-title-medium text-[var(--md-sys-color-on-surface)] truncate">
                                {{ batch.name }}
                              </span>
                              <span class="m3-body-medium">
                                {{ batch.timestamp }}
                              </span>
                            </div>
                            <span class="m3-body-medium">
                              {{ batch.totalCount }} items &bull; {{ batch.duration }} &bull; {{ batch.sizeFormatted }}
                            </span>
                          </div>
                        </div>

                        <div class="flex items-center gap-2 shrink-0">
                          <span
                            class="rounded-full px-3 py-1 m3-label-large"
                            :class="batch.failedCount > 0
                              ? 'bg-[var(--md-sys-color-error-container)] text-[var(--md-sys-color-on-error-container)]'
                              : 'bg-[var(--md-sys-color-success-container)] text-[var(--md-sys-color-on-success-container)]'"
                          >
                            {{ batch.failedCount > 0 ? `${batch.failedCount} failed` : 'All completed' }}
                          </span>
                          <M3Icon
                            name="expand_more"
                            :size="20"
                            class="text-[var(--md-sys-color-on-surface-variant)] transition-transform duration-200"
                            :class="selectedRunId === batch.runId ? 'rotate-180' : ''"
                          />
                        </div>
                      </div>

                      <!-- Expandable Files in Run with M3E spring transition -->
                      <div
                        class="grid transition-all duration-300 ease-[cubic-bezier(0.05,0.7,0.1,1.0)]"
                        :class="selectedRunId === batch.runId ? 'grid-rows-[1fr] opacity-100' : 'grid-rows-[0fr] opacity-0 pointer-events-none'"
                      >
                        <div class="overflow-hidden">
                          <div class="flex flex-col border-t border-[var(--md-sys-color-outline-variant)]/20 bg-[var(--md-sys-color-surface-container)]/60 p-3.5 sm:p-4 gap-2">
                            <div class="flex items-center justify-between px-1 m3-label-small text-[var(--md-sys-color-on-surface-variant)]">
                              <span>Files in this run ({{ getRunFiles(batch.runId).length }})</span>
                              <span v-if="batch.uploadedCount > 0">{{ batch.uploadedCount }} uploaded</span>
                            </div>

                            <div
                              v-if="getRunFiles(batch.runId).length === 0"
                              class="py-4 text-center m3-body-medium"
                            >
                              No individual file logs recorded for this batch.
                            </div>

                            <div v-else class="flex flex-col gap-1.5 max-h-56 overflow-y-auto pr-1">
                              <div
                                v-for="file in getRunFiles(batch.runId)"
                                :key="file.id"
                                class="flex items-center justify-between gap-3 rounded-xl bg-[var(--md-sys-color-surface-container-high)]/50 px-3 py-2 border border-[var(--md-sys-color-outline-variant)]/20"
                              >
                                <div class="flex items-center gap-2.5 min-w-0 flex-1">
                                  <M3Icon
                                    :name="file.outcome === 'FAILED' ? 'error' : file.outcome?.startsWith('SKIPPED') ? 'skip_next' : 'check_circle'"
                                    :size="16"
                                    :class="file.outcome === 'FAILED' ? 'text-[var(--md-sys-color-error)]' : file.outcome?.startsWith('SKIPPED') ? 'text-[var(--md-sys-color-on-surface-variant)]' : 'text-[var(--md-sys-color-primary)]'"
                                  />
                                  <span class="truncate m3-mono m3-body-medium text-[var(--md-sys-color-on-surface)]" :title="file.filePath || file.fileName">
                                    {{ file.fileName || file.filePath }}
                                  </span>
                                </div>

                                <div class="flex items-center gap-2 shrink-0">
                                  <span v-if="file.sizeBytes" class="m3-body-medium">
                                    {{ formatFileSize(file.sizeBytes) }}
                                  </span>
                                  <span
                                    class="rounded-full px-2 py-0.5 m3-label-small"
                                    :class="file.outcome === 'FAILED'
                                      ? 'bg-[var(--md-sys-color-error-container)] text-[var(--md-sys-color-on-error-container)]'
                                      : file.outcome?.startsWith('SKIPPED')
                                        ? 'bg-[var(--md-sys-color-surface-container-highest)] text-[var(--md-sys-color-on-surface-variant)]'
                                        : 'bg-[var(--md-sys-color-primary-container)] text-[var(--md-sys-color-on-primary-container)]'"
                                  >
                                    {{ file.outcome === 'UPLOADED' ? 'Uploaded' : file.outcome?.startsWith('SKIPPED') ? 'Skipped' : 'Failed' }}
                                  </span>
                                </div>
                              </div>
                            </div>
                          </div>
                        </div>
                      </div>
                    </div>

                    <div
                      v-if="displayBatches.length === 0"
                      class="p-6 text-center m3-body-medium"
                    >
                      No recent batches found.
                    </div>
                  </div>
                </div>
              </div>
            </div>
          </div>
        </div>

        <!-- View: Accounts Tab (Task 2: 2-column grid on >=1100px) -->
        <div
          v-else-if="activeTab === 'accounts'"
          key="accounts"
          data-tab="accounts"
          class="m3-tab-pane flex h-full w-full flex-1 flex-col overflow-y-auto overflow-x-hidden p-6"
          style="contain: layout style; --wails-draggable: none"
        >
          <div class="w-full max-w-[1400px] mx-auto flex flex-col gap-6">
            <!-- Header (Task 11: plain language) -->
            <div class="flex flex-col sm:flex-row sm:items-center justify-between gap-4">
              <div class="flex flex-col gap-1">
                <h1 class="m3-headline-large text-[var(--md-sys-color-on-surface)]">
                  Google accounts
                </h1>
                <p class="m3-body-medium">
                  Add, switch or remove the Google accounts you upload to.
                </p>
              </div>

              <M3Button
                variant="filled"
                size="m"
                icon="person_add"
                @click="openAccountSetup"
              >
                <span>Add account</span>
              </M3Button>
            </div>

            <!-- Safety Circuit Breaker Alert Banner -->
            <CircuitBreakerAlert
              @open-sign-in="isAccountSetupOpen = true"
              @account-removed="refreshCredentials"
            />

            <!-- 2-column grid on >=1100px -->
            <div class="grid grid-cols-1 min-[1100px]:grid-cols-2 gap-6 items-start">
              <!-- Left Column: Active Account Card -->
              <div
                v-if="displaySelectedOption"
                class="flex flex-col gap-4 rounded-[28px] border border-[var(--md-sys-color-outline-variant)]/40 bg-[var(--md-sys-color-surface-container)] p-6 shadow-xs"
              >
                <div class="flex items-center justify-between border-b border-[var(--md-sys-color-outline-variant)]/30 pb-3">
                  <span class="m3-label-small text-[var(--md-sys-color-primary)]">
                    Active account
                  </span>
                  <span class="rounded-full bg-[var(--md-sys-color-primary-container)] px-2.5 py-0.5 m3-label-small text-[var(--md-sys-color-on-primary-container)]">
                    Selected
                  </span>
                </div>

                <div class="flex flex-col sm:flex-row sm:items-center justify-between gap-4">
                  <div class="flex items-center gap-4 min-w-0">
                    <div
                      class="group/avatar relative size-16 shrink-0 cursor-pointer overflow-hidden rounded-[22px] border border-[var(--md-sys-color-outline-variant)] shadow-inner transition-transform hover:scale-105"
                      :style="{
                        backgroundColor: customAvatar ? 'transparent' : getAvatarColor(displaySelectedOption).bg,
                        color: getAvatarColor(displaySelectedOption).text,
                      }"
                      title="Click to change profile photo"
                      role="button"
                      tabindex="0"
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
                        class="flex size-full items-center justify-center m3-headline-small"
                      >
                        {{ getEmailInitial(displaySelectedOption) }}
                      </span>
                      <div class="absolute inset-0 flex items-center justify-center bg-black/60 opacity-0 group-hover/avatar:opacity-100 transition-opacity">
                        <M3Icon
                          name="photo_camera"
                          :size="18"
                          class="text-white"
                        />
                      </div>
                    </div>

                    <div class="flex flex-col min-w-0">
                      <div class="flex items-center gap-2">
                        <h2 class="truncate m3-headline-small text-[var(--md-sys-color-on-surface)]">
                          {{ displayNameFromEmail(displaySelectedOption) }}
                        </h2>
                      </div>
                      <span class="truncate m3-body-medium">
                        {{ displaySelectedOption }}
                      </span>
                    </div>
                  </div>

                  <div class="flex items-center gap-2">
                    <M3Button
                      variant="tonal"
                      size="s"
                      icon="photo_camera"
                      @click="triggerAvatarUpload"
                    >
                      <span>Change photo</span>
                    </M3Button>
                    <M3Button
                      variant="tonal"
                      size="s"
                      icon="edit"
                      @click="openEditNameDialog"
                    >
                      <span>Edit name</span>
                    </M3Button>
                  </div>
                </div>
              </div>

              <!-- Prompt if no account active -->
              <div
                v-else
                class="flex flex-col items-center justify-center p-8 text-center rounded-[28px] border border-[var(--md-sys-color-outline-variant)]/40 bg-[var(--md-sys-color-surface-container)]"
              >
                <M3Icon name="no_accounts" :size="36" class="mb-2 text-[var(--md-sys-color-outline)]" />
                <p class="m3-title-medium text-[var(--md-sys-color-on-surface)]">No account selected</p>
                <p class="m3-body-medium mt-1">Connect or select a Google account to start uploading.</p>
              </div>

              <!-- Right Column: Connected Accounts List -->
              <div class="flex flex-col gap-3 rounded-[28px] border border-[var(--md-sys-color-outline-variant)]/40 bg-[var(--md-sys-color-surface-container)] p-6 shadow-xs">
                <h2 class="m3-headline-small text-[var(--md-sys-color-on-surface)]">
                  Connected accounts ({{ displayOptions.length }})
                </h2>

                <div
                  v-if="displayOptions.length === 0"
                  class="flex flex-col items-center justify-center p-8 text-center m3-body-medium"
                >
                  <M3Icon name="no_accounts" :size="36" class="mb-2 text-[var(--md-sys-color-outline)]" />
                  <p>No Google accounts connected yet.</p>
                  <M3Button
                    variant="filled"
                    size="s"
                    icon="person_add"
                    class="mt-4"
                    @click="openAccountSetup"
                  >
                    <span>Add your first account</span>
                  </M3Button>
                </div>

                <div
                  v-else
                  class="flex flex-col divide-y divide-[var(--md-sys-color-outline-variant)]/20"
                >
                  <div
                    v-for="opt in displayOptions"
                    :key="opt"
                    class="flex items-center justify-between py-3.5 first:pt-1 last:pb-1"
                  >
                    <div class="flex items-center gap-3.5 min-w-0">
                      <div
                        class="size-10 shrink-0 rounded-2xl flex items-center justify-center m3-title-medium overflow-hidden"
                        :style="{
                          backgroundColor: getAvatarColor(opt).bg,
                          color: getAvatarColor(opt).text,
                        }"
                      >
                        <img
                          v-if="getCustomAvatar(opt)"
                          :src="getCustomAvatar(opt)!"
                          :alt="opt"
                          class="size-full object-cover"
                        />
                        <span v-else>
                          {{ getEmailInitial(opt) }}
                        </span>
                      </div>
                      <div class="flex flex-col min-w-0">
                        <div class="flex items-center gap-2">
                          <span class="truncate m3-title-medium text-[var(--md-sys-color-on-surface)]">
                            {{ displayNameFromEmail(opt) }}
                          </span>
                          <span
                            v-if="opt === displaySelectedOption"
                            class="rounded-full bg-[var(--md-sys-color-primary-container)] px-2 py-0.5 m3-label-small text-[var(--md-sys-color-on-primary-container)]"
                          >
                            Active
                          </span>
                        </div>
                        <span class="truncate m3-body-medium">
                          {{ opt }}
                        </span>
                      </div>
                    </div>

                    <div class="flex items-center gap-2">
                      <M3Button
                        v-if="opt !== displaySelectedOption"
                        variant="tonal"
                        size="xs"
                        icon="swap_horiz"
                        @click="displaySelectedOption = opt"
                      >
                        <span>Switch</span>
                      </M3Button>
                      <M3Button
                        variant="tonal"
                        destructive
                        size="xs"
                        icon="delete"
                        :loading="removingAccount === opt"
                        @click="promptRemoveAccount(opt)"
                      >
                        <span>Remove</span>
                      </M3Button>
                    </div>
                  </div>
                </div>
              </div>
            </div>
          </div>
        </div>

        <!-- View: Settings Tab (Task 2 & 10: wide screen max-w-[1700px]) -->
        <div
          v-else-if="activeTab === 'settings'"
          key="settings"
          data-tab="settings"
          class="m3-tab-pane flex h-full w-full flex-1 flex-col overflow-y-auto overflow-x-hidden p-6"
          style="contain: layout style; --wails-draggable: none"
        >
          <div class="w-full max-w-[1700px] mx-auto flex flex-col">
            <TooltipProvider disable-hoverable-content>
              <SettingsPanel
                @upload-folder="onUploadFolderFromSettings"
                @navigate-tab="(tab) => activeTab = tab"
              />
            </TooltipProvider>
          </div>
        </div>

        <!-- View: About Tab -->
        <div
          v-else-if="activeTab === 'about'"
          key="about"
          data-tab="about"
          class="m3-tab-pane flex h-full w-full flex-1 flex-col overflow-y-auto overflow-x-hidden p-6"
          style="contain: layout style; --wails-draggable: none"
        >
          <div class="w-full max-w-[768px] mx-auto flex flex-col">
            <AboutView />
          </div>
        </div>

        <!-- View: Developer Tab (Task 8: only visible when unlocked) -->
        <div
          v-else-if="activeTab === 'developer' && devOptionsState.unlocked"
          key="developer"
          data-tab="developer"
          class="m3-tab-pane flex h-full w-full flex-1 flex-col overflow-y-auto overflow-x-hidden p-6"
          style="contain: layout style; --wails-draggable: none"
        >
          <div class="w-full max-w-[1600px] mx-auto flex flex-col">
            <DeveloperView :live-logs="activityLogs" />
          </div>
        </div>
      </Transition>
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
            <div class="flex size-16 items-center justify-center rounded-full bg-[var(--md-sys-color-primary-container)] text-[var(--md-sys-color-primary)]">
              <M3Icon
                name="folder_special"
                :filled="true"
                :size="32"
              />
            </div>
            <div class="flex flex-col gap-1">
              <h2
                id="album-dialog-title"
                class="m3-headline-small text-[var(--md-sys-color-on-surface)] select-none"
              >
                {{ pendingFiles.length > 0 ? 'Upload to Album' : 'Album Options' }}
              </h2>
              <p class="m3-body-medium select-none">
                {{ pendingFiles.length > 0 ? `${pendingFileCount} file(s) selected for upload` : 'Choose how Google Photos organizes your uploads' }}
              </p>
            </div>

            <!-- Mode selector pills -->
            <div class="flex flex-wrap items-center justify-center gap-2">
              <M3Button
                size="xs"
                :toggle="true"
                :selected="albumTargetMode === 'none' && !albumNameOrKey"
                :variant="albumTargetMode === 'none' && !albumNameOrKey ? 'filled' : 'outlined'"
                @click="albumTargetMode = 'none'; albumNameOrKey = ''"
              >
                None
              </M3Button>
              <M3Button
                size="xs"
                :toggle="true"
                :selected="albumTargetMode === 'auto'"
                :variant="albumTargetMode === 'auto' ? 'filled' : 'outlined'"
                @click="albumTargetMode = 'auto'; albumNameOrKey = ''"
              >
                Auto by folder
              </M3Button>
              <M3Button
                size="xs"
                :toggle="true"
                :selected="albumTargetMode === 'custom' || !!albumNameOrKey"
                :variant="albumTargetMode === 'custom' || !!albumNameOrKey ? 'filled' : 'outlined'"
                @click="albumTargetMode = 'custom'"
              >
                Named album
              </M3Button>
            </div>

            <div
              v-if="albumTargetMode === 'custom' || albumNameOrKey"
              class="w-full text-left flex flex-col gap-2"
            >
              <M3TextField
                id="album-input"
                v-model="albumNameOrKey"
                label="Album name or album key"
                placeholder="Vacation 2026"
                variant="outlined"
                leading-icon="folder"
                autofocus
                @keydown.enter="confirmAlbumUpload"
              />
              <p class="m3-body-medium px-1 whitespace-normal break-words">
                Type a name to create an album. Lumen reuses it next time. To add to an album you already have, paste its album key, which starts with AF1Qip (for example AF1QipM9x4Kq...).
              </p>
              <div class="px-1 text-left">
                <button
                  type="button"
                  class="inline-flex items-center gap-1 m3-body-medium text-[var(--md-sys-color-primary)] hover:underline focus:outline-none cursor-pointer"
                  @click="showAlbumKeyHelp = !showAlbumKeyHelp"
                >
                  <M3Icon name="help_outline" :size="13" />
                  <span>Where do I find the key?</span>
                </button>
                <div
                  v-if="showAlbumKeyHelp"
                  class="mt-1.5 p-2.5 rounded-xl bg-[var(--md-sys-color-surface-container-highest)]/70 m3-body-medium whitespace-normal break-words"
                >
                  Open the album in Google Photos on the web. The key is the part of the address that starts with AF1Qip.
                </div>
              </div>

              <!-- Editable Nickname if using an AF1Qip album key -->
              <div
                v-if="isInputAnAlbumKey"
                class="flex flex-col gap-2 rounded-2xl bg-[var(--md-sys-color-surface-container)] p-4 sm:p-5 text-left border border-[var(--md-sys-color-outline-variant)]/30 mt-2"
              >
                <div class="flex flex-col gap-0.5">
                  <span class="m3-title-medium text-[var(--md-sys-color-on-surface)]">
                    Album Nickname (optional)
                  </span>
                  <span class="m3-body-medium">
                    Name this album so Lumen shows a clean name instead of the 50-character album key.
                  </span>
                </div>
                <div class="pt-2">
                  <M3TextField
                    id="album-nickname-input"
                    v-model="albumNickname"
                    label="Friendly nickname"
                    placeholder="e.g. Vacation 2026"
                    variant="outlined"
                    leading-icon="label"
                    @keydown.enter="confirmAlbumUpload"
                  />
                </div>
              </div>
            </div>

            <!-- Recent Albums Quick-select Chips -->
            <div
              v-if="(albumTargetMode === 'custom' || albumNameOrKey) && recentAlbums.length > 0"
              class="flex w-full flex-col gap-1.5 text-left"
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
                  :selected="albumNameOrKey === recent"
                  :variant="albumNameOrKey === recent ? 'filled' : 'outlined'"
                  icon="folder"
                  @click="onSelectRecentAlbum(recent)"
                >
                  <span class="max-w-[130px] truncate">{{ albumNicknames[recent] || recent }}</span>
                  <span
                    class="ml-0.5 rounded-full p-0.5 opacity-60 hover:opacity-100 hover:bg-black/10 dark:hover:bg-white/10"
                    title="Remove from recents"
                    @click.stop="removeRecentAlbum(recent, $event)"
                  >
                    <M3Icon
                      name="close"
                      :size="12"
                    />
                  </span>
                </M3Button>
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
                :disabled="albumTargetMode === 'custom' && !albumNameOrKey.trim()"
                @click="confirmAlbumUpload"
              >
                {{ pendingFiles.length > 0 ? 'Start Upload' : 'Confirm' }}
              </M3Button>
            </div>
          </div>
        </div>
      </Transition>
    </Teleport>

    <!-- Edit Display Name Modal Dialog -->
    <EditDisplayNameModal
      v-model="showEditNameDialog"
      :account-email="displaySelectedOption"
      @saved="toast.success('Display name updated')"
      @reset="toast.success('Display name reset to default')"
    />

    <!-- Remove Account Modal Dialog -->
    <RemoveAccountModal
      v-model="showRemoveAccountConfirm"
      :account-email="accountToRemove"
      :loading="!!removingAccount"
      @confirm="handleConfirmRemoveAccount"
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
      :ready="isAppReady"
      @start="onLaunchStart"
      @settle="onLaunchSettle"
      @finish="onLaunchFinish"
    />
  </main>
</template>
