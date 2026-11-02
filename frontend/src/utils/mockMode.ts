import { reactive, ref } from 'vue'
import { uploadManager } from './UploadManager'
import { LogEntry } from '../../bindings/app/core/models'
import { ConfigManager } from '../../bindings/app/core'

export interface MockState {
  enabled: boolean
  currentView: 'empty' | 'drop' | 'uploading' | 'results' | 'album' | 'indexing'
  account: string
  accounts: string[]
}

export const devOptionsState = reactive({
  unlocked: typeof localStorage !== 'undefined' ? localStorage.getItem('lumen_dev_options') === 'true' : false,
  showMockBar: typeof localStorage !== 'undefined' ? localStorage.getItem('lumen_show_mock_bar') !== 'false' : true,
})

export function setDevOptionsUnlocked(unlocked: boolean) {
  devOptionsState.unlocked = unlocked
  if (typeof localStorage !== 'undefined') {
    localStorage.setItem('lumen_dev_options', unlocked ? 'true' : 'false')
  }
  if (!unlocked) {
    mockState.enabled = false
    if (typeof localStorage !== 'undefined') {
      localStorage.setItem('lumen_mock_active', 'false')
    }
  }
}

export function setShowMockBar(show: boolean) {
  devOptionsState.showMockBar = show
  if (typeof localStorage !== 'undefined') {
    localStorage.setItem('lumen_show_mock_bar', show ? 'true' : 'false')
  }
}

export interface MockBatchSummary {
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

export const mockBatches: MockBatchSummary[] = [
  {
    runId: 'Run 2',
    name: 'Camera Roll 2026',
    timestamp: '10 mins ago',
    totalCount: 48,
    uploadedCount: 42,
    skippedCount: 3,
    failedCount: 3,
    duration: '18s',
    sizeFormatted: '142 MB',
  },
  {
    runId: 'Run 1',
    name: 'Family Vacation',
    timestamp: '2 hours ago',
    totalCount: 140,
    uploadedCount: 128,
    skippedCount: 12,
    failedCount: 0,
    duration: '45s',
    sizeFormatted: '512 MB',
  },
]

export const mockProblems: LogEntry[] = [
  new LogEntry({
    id: 'prob_1',
    timestamp: new Date(Date.now() - 1000 * 60 * 12).toISOString(),
    level: 'ERROR',
    category: 'Upload',
    runId: 'Run 2',
    fileName: 'Corrupt_Archive.raw',
    reasonCode: 'FILE_UNREADABLE',
    userMessage: 'Unrecognized codec header or invalid EXIF stream.',
    suggestedAction: 'Verify the RAW file is not corrupted and check that your camera format is supported by Google Photos.',
    rawError: 'os error 22: invalid argument during decode',
  }),
  new LogEntry({
    id: 'prob_2',
    timestamp: new Date(Date.now() - 1000 * 60 * 11).toISOString(),
    level: 'ERROR',
    category: 'Upload',
    runId: 'Run 2',
    fileName: 'Video_Exceeds_Quota.mp4',
    reasonCode: 'RATE_LIMITED',
    userMessage: 'Google Photos storage quota exceeded for non-saver mode.',
    suggestedAction: 'Switch to Storage Saver quality in Settings or free up Google Drive/Photos storage.',
    rawError: 'HTTP 429 quotaExceeded',
  }),
  new LogEntry({
    id: 'prob_3',
    timestamp: new Date(Date.now() - 1000 * 60 * 10).toISOString(),
    level: 'WARN',
    category: 'Upload',
    runId: 'Run 2',
    fileName: 'Locked_File.png',
    reasonCode: 'NETWORK_TIMEOUT',
    userMessage: 'Resource temporarily locked by another local process.',
    suggestedAction: 'Close other applications accessing this file and retry.',
    rawError: 'resource locked (os error 11)',
  }),
]

export const mockLogEntries: LogEntry[] = [
  ...mockProblems,
  new LogEntry({
    id: 'log_4',
    timestamp: new Date(Date.now() - 1000 * 60 * 9).toISOString(),
    level: 'INFO',
    category: 'Upload',
    runId: 'Run 2',
    fileName: 'DSC_1042.JPG',
    outcome: 'UPLOADED',
    sizeBytes: 4200000,
    durationMs: 320,
    userMessage: 'Uploaded successfully',
  }),
  new LogEntry({
    id: 'log_5',
    timestamp: new Date(Date.now() - 1000 * 60 * 9).toISOString(),
    level: 'INFO',
    category: 'Upload',
    runId: 'Run 2',
    fileName: 'DSC_1041.JPG',
    outcome: 'UPLOADED',
    sizeBytes: 3800000,
    durationMs: 290,
    userMessage: 'Uploaded successfully',
  }),
  new LogEntry({
    id: 'log_6',
    timestamp: new Date(Date.now() - 1000 * 60 * 10).toISOString(),
    level: 'INFO',
    category: 'Upload',
    runId: 'Run 2',
    fileName: 'IMG_9910.JPG',
    outcome: 'SKIPPED_CACHE',
    sizeBytes: 2500000,
    userMessage: 'File checksum matches existing item in Google Photos',
  }),
  new LogEntry({
    id: 'log_7',
    timestamp: new Date(Date.now() - 1000 * 60 * 15).toISOString(),
    level: 'INFO',
    category: 'System',
    userMessage: 'Upload worker pool initialized (threads: 4)',
  }),
  new LogEntry({
    id: 'log_8',
    timestamp: new Date(Date.now() - 1000 * 60 * 45).toISOString(),
    level: 'INFO',
    category: 'Upload',
    runId: 'Run 1',
    userMessage: 'Batch Run 1 completed: 128 uploaded, 12 skipped, 0 failed in 45s',
  }),
]

export const mockState = reactive<MockState>({
  enabled: false,
  currentView: 'drop',
  account: 'clara.oswald@gmail.com',
  accounts: ['clara.oswald@gmail.com', 'lumen.preview@example.com'],
})

// Timer for simulated upload
let mockProgressInterval: ReturnType<typeof setInterval> | null = null

export const isDebugBinary = ref(false)

export async function detectDebugBuild(): Promise<boolean> {
  try {
    const isDebug = await ConfigManager.IsDebugBuild()
    isDebugBinary.value = !!isDebug
    return !!isDebug
  } catch {
    return false
  }
}

export function isDevMode(): boolean {
  return import.meta.env.DEV || isDebugBinary.value
}

export function initMockMode(): boolean {
  if (!isDevMode()) return false

  const params = new URLSearchParams(window.location.search)
  const isMockParam = params.get('mock') === '1'
  const isStoredMock = localStorage.getItem('lumen_mock_active') === 'true'

  if (isMockParam || isStoredMock) {
    mockState.enabled = true
    localStorage.setItem('lumen_mock_active', 'true')
    return true
  }

  return false
}

export function setMockView(view: MockState['currentView']) {
  if (!isDevMode()) return
  mockState.currentView = view

  if (mockProgressInterval) {
    clearInterval(mockProgressInterval)
    mockProgressInterval = null
  }

  if (view === 'uploading') {
    uploadManager.state.scanProgress.isScanning = false
    simulateUploadProgress()
  } else if (view === 'results') {
    uploadManager.state.scanProgress.isScanning = false
    simulateResults()
  } else if (view === 'indexing') {
    uploadManager.state.isUploading = false
    uploadManager.state.scanProgress = {
      isScanning: true,
      currentDirectory: '/home/user/Pictures/Summer 2026',
      filesFound: 342,
    }
  } else {
    uploadManager.state.isUploading = false
    uploadManager.state.scanProgress.isScanning = false
    uploadManager.resetUploadResults()
  }
}

export function toggleMockMode(): boolean {
  if (!isDevMode()) return false
  mockState.enabled = !mockState.enabled
  localStorage.setItem('lumen_mock_active', mockState.enabled ? 'true' : 'false')

  if (!mockState.enabled) {
    if (mockProgressInterval) {
      clearInterval(mockProgressInterval)
      mockProgressInterval = null
    }
    uploadManager.state.isUploading = false
    uploadManager.resetUploadResults()
  } else {
    setMockView('drop')
  }

  return mockState.enabled
}

function simulateUploadProgress() {
  const state = uploadManager.state
  state.isUploading = true
  state.totalFiles = 48
  state.uploadedFiles = 12
  state.totalBytes = 256 * 1024 * 1024
  state.uploadedBytes = 64 * 1024 * 1024
  state.startTime = Date.now() - 15000
  state.uploadSpeed = 18.4 * 1024 * 1024

  // Sample threads
  state.threads.clear()
  state.threads.set(1, {
    WorkerID: 1,
    Status: 'uploading',
    FilePath: '/Photos/2026/DSC_0891.JPG',
    FileName: 'DSC_0891.JPG',
    Message: 'Uploading full resolution...',
    BytesUploaded: 14 * 1024 * 1024,
    BytesTotal: 22 * 1024 * 1024,
    Attempt: 1,
  })
  state.threads.set(2, {
    WorkerID: 2,
    Status: 'uploading',
    FilePath: '/Photos/2026/IMG_4412.MOV',
    FileName: 'IMG_4412.MOV',
    Message: 'Streaming video track...',
    BytesUploaded: 42 * 1024 * 1024,
    BytesTotal: 65 * 1024 * 1024,
    Attempt: 1,
  })
  state.threads.set(3, {
    WorkerID: 3,
    Status: 'checking',
    FilePath: '/Photos/2026/PANO_002.HEIC',
    FileName: 'PANO_002.HEIC',
    Message: 'Checking cloud duplicate fingerprint...',
    BytesUploaded: 0,
    BytesTotal: 8 * 1024 * 1024,
    Attempt: 1,
  })
  state.threads.set(4, {
    WorkerID: 4,
    Status: 'completed',
    FilePath: '/Photos/2026/IMG_4411.JPG',
    FileName: 'IMG_4411.JPG',
    Message: 'Uploaded successfully',
    BytesUploaded: 12 * 1024 * 1024,
    BytesTotal: 12 * 1024 * 1024,
    Attempt: 1,
  })

  mockProgressInterval = setInterval(() => {
    if (state.uploadedFiles < state.totalFiles) {
      state.uploadedFiles += 1
      const remainingFiles = state.totalFiles - state.uploadedFiles
      if (remainingFiles <= 0) {
        state.uploadedBytes = state.totalBytes
      } else {
        const remainingBytes = state.totalBytes - state.uploadedBytes
        const bytesToAdd = Math.max(1024 * 1024, Math.round(remainingBytes / (remainingFiles + 1)))
        state.uploadedBytes = Math.min(state.totalBytes, state.uploadedBytes + bytesToAdd)
      }
      state.uploadSpeed = (16 + Math.random() * 6) * 1024 * 1024
    } else {
      setMockView('results')
    }
  }, 1200)
}

function simulateResults() {
  const state = uploadManager.state
  state.isUploading = false
  state.totalFiles = 48
  state.uploadedFiles = 48

  state.results.success = Array.from({ length: 42 }, (_, i) => ({
    path: `/Photos/2026/DSC_${1000 + i}.JPG`,
    mediaKey: `AF1Qip_${i}mockKey`,
  }))

  state.results.fail = [
    {
      path: '/Photos/2026/Corrupt_Archive.raw',
      error: 'Unrecognized codec header or invalid EXIF stream',
    },
    {
      path: '/Photos/2026/Video_Exceeds_Quota.mp4',
      error: 'Google Photos quota exceeded for non-saver mode',
    },
    {
      path: '/Photos/2026/Locked_File.png',
      error: 'Resource temporarily unavailable (os error 11)',
    },
  ]

  state.results.skipped = [
    {
      paths: ['/Photos/2026/IMG_9910.JPG'],
      code: 'already-present',
      reason: 'File checksum matches existing item in Google Photos',
    },
    {
      paths: ['/Photos/2026/IMG_9911.JPG'],
      code: 'already-present',
      reason: 'File checksum matches existing item in Google Photos',
    },
    {
      paths: ['/Photos/2026/Unsupported.xyz'],
      code: 'unsupported-extension',
      reason: 'File extension .xyz is filtered by unsupported files filter',
    },
  ]

  state.results.warnings = [
    {
      paths: ['/Photos/2026/LivePhoto_MissingMOV.HEIC'],
      code: 'incomplete-live-photo',
      reason: 'Live photo image found without matching MOV component',
    },
  ]
}

export function triggerMockCircuitBreaker(reason: 'SIGN_IN_REQUIRED' | 'ACCOUNT_RESTRICTED' | 'RATE_LIMITED') {
  if (!isDevMode()) return
  uploadManager.state.isUploading = false
  if (reason === 'RATE_LIMITED') {
    uploadManager.state.accountBlocked = {
      isBlocked: true,
      reason: 'RATE_LIMITED',
      email: mockState.account,
      message: 'Google asked Lumen to slow down (HTTP 429)',
      resumeAt: Date.now() + 15 * 60 * 1000,
    }
  } else if (reason === 'ACCOUNT_RESTRICTED') {
    uploadManager.state.accountBlocked = {
      isBlocked: true,
      reason: 'ACCOUNT_RESTRICTED',
      email: mockState.account,
      message: 'Google has restricted this account (AccountDisabled)',
    }
  } else {
    uploadManager.state.accountBlocked = {
      isBlocked: true,
      reason: 'SIGN_IN_REQUIRED',
      email: mockState.account,
      message: 'Google is asking you to sign in again (HTTP 401)',
    }
  }
}

export function clearMockCircuitBreaker() {
  uploadManager.clearAccountBlocked()
}
