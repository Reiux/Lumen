import { beforeEach, describe, expect, it, vi } from 'vitest'

type Handler = (event: { data: any }) => void
const handlers = new Map<string, Handler[]>()

vi.mock('@wailsio/runtime', () => ({
  Clipboard: { SetText: vi.fn() },
  Events: {
    On: vi.fn((event: string, cb: Handler) => {
      const list = handlers.get(event) ?? []
      list.push(cb)
      handlers.set(event, list)
      return () => {}
    }),
    Emit: vi.fn().mockResolvedValue(undefined),
  },
}))

vi.mock('../../bindings/app/core', () => ({
  ConfigManager: {
    CancelScan: vi.fn().mockResolvedValue(undefined),
  },
}))

function emitBackend(event: string, data: any) {
  for (const cb of handlers.get(event) ?? []) {
    cb({ data })
  }
}

describe('UploadManager regression coverage', () => {
  beforeEach(() => {
    handlers.clear()
    vi.resetModules()
  })

  it('adds skipped file SizeBytes to uploadedBytes and clears empty batches', async () => {
    const { uploadManager } = await import('./UploadManager')
    const state = uploadManager.state

    const emptyListener = vi.fn()
    window.addEventListener('uploadEmpty', emptyListener)

    emitBackend('uploadStart', { Total: 0, TotalBytes: 0, IsDryRun: false })
    expect(state.isUploading).toBe(false)
    expect(emptyListener).toHaveBeenCalledTimes(1)

    emitBackend('uploadStart', { Total: 2, TotalBytes: 8192, IsDryRun: false })
    expect(state.isUploading).toBe(true)

    // Queuing mid-upload must not overwrite active scan state with beginPreparation()
    uploadManager.beginPreparation()
    expect(state.scanProgress.isScanning).toBe(false)

    // Skipped file without prior 'uploading' ThreadStatus must still advance uploadedBytes
    emitBackend('FileStatus', {
      MediaKey: 'key_1',
      IsError: false,
      IsLivePhoto: false,
      Skipped: true,
      ErrorMessage: '',
      SkipCode: 'remote-dedup',
      SkipReason: 'Already in Google Photos',
      Path: '/photos/a.jpg',
      Paths: ['/photos/a.jpg'],
      SizeBytes: 4096,
    })

    expect(state.uploadedFiles).toBe(1)
    expect(state.uploadedBytes).toBe(4096)

    // Simulate paused + album activity, then verify reset & next uploadStart clear stale flags
    uploadManager.pauseUpload()
    emitBackend('albumProgress', {
      AlbumName: 'Trip',
      ItemsAdded: 1,
      TotalItems: 2,
      AlbumKeys: ['k1'],
      IsComplete: false,
    })
    expect(state.isPaused).toBe(true)
    expect(state.isCreatingAlbum).toBe(true)

    uploadManager.resetUploadResults()
    expect(state.isPaused).toBe(false)
    expect(state.isCreatingAlbum).toBe(false)
    expect(state.albumStatus).toBeNull()
    expect(state.uploadedBytes).toBe(0)

    uploadManager.pauseUpload()
    emitBackend('uploadStart', { Total: 1, TotalBytes: 1024, IsDryRun: false })
    expect(state.isPaused).toBe(false)

    window.removeEventListener('uploadEmpty', emptyListener)
  })

  it('emits dryRun: false on upload() and retryFailed()', async () => {
    const { Events } = await import('@wailsio/runtime')
    const { uploadManager } = await import('./UploadManager')
    const emitMock = Events.Emit as ReturnType<typeof vi.fn>
    emitMock.mockClear()

    await uploadManager.upload(['/tmp/a.jpg'])
    expect(emitMock).toHaveBeenCalledWith('startUpload', { files: ['/tmp/a.jpg'], dryRun: false })

    emitMock.mockClear()
    await uploadManager.retryFailed(['/tmp/b.jpg'])
    expect(emitMock).toHaveBeenCalledWith('startUpload', { files: ['/tmp/b.jpg'], dryRun: false })
  })

  it('cancelReviewScan does not emit uploadCancel and handles upload:error events', async () => {
    const { Events } = await import('@wailsio/runtime')
    const { ConfigManager } = await import('../../bindings/app/core')
    const { uploadManager } = await import('./UploadManager')
    const emitMock = Events.Emit as ReturnType<typeof vi.fn>
    const cancelScanMock = ConfigManager.CancelScan as ReturnType<typeof vi.fn>

    emitMock.mockClear()
    cancelScanMock.mockClear()

    await uploadManager.cancelReviewScan()
    expect(cancelScanMock).toHaveBeenCalledTimes(1)
    expect(emitMock).not.toHaveBeenCalledWith('uploadCancel')

    const errListener = vi.fn()
    window.addEventListener('uploadError', errListener)
    uploadManager.beginPreparation()
    emitBackend('upload:error', 'Missing credentials for queued batch')
    expect(uploadManager.state.scanProgress.isScanning).toBe(false)
    expect(errListener).toHaveBeenCalledTimes(1)
    window.removeEventListener('uploadError', errListener)
  })
})
