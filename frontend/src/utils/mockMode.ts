import { reactive } from 'vue'
import { uploadManager } from './UploadManager'

export interface MockState {
  enabled: boolean
  currentView: 'empty' | 'drop' | 'uploading' | 'results' | 'album'
  account: string
  accounts: string[]
}

export const mockState = reactive<MockState>({
  enabled: false,
  currentView: 'drop',
  account: 'clara.oswald@gmail.com',
  accounts: ['clara.oswald@gmail.com', 'lumen.preview@example.com'],
})

// Timer for simulated upload
let mockProgressInterval: ReturnType<typeof setInterval> | null = null

export function isDevMode(): boolean {
  return import.meta.env.DEV
}

export function initMockMode(): boolean {
  if (!import.meta.env.DEV) return false

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
  if (!import.meta.env.DEV) return
  mockState.currentView = view

  if (mockProgressInterval) {
    clearInterval(mockProgressInterval)
    mockProgressInterval = null
  }

  if (view === 'uploading') {
    simulateUploadProgress()
  } else if (view === 'results') {
    simulateResults()
  } else {
    uploadManager.state.isUploading = false
    uploadManager.resetUploadResults()
  }
}

export function toggleMockMode(): boolean {
  if (!import.meta.env.DEV) return false
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
      state.uploadedBytes = Math.min(state.totalBytes, state.uploadedBytes + 4 * 1024 * 1024)
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
