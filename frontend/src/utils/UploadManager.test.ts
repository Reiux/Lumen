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

    window.removeEventListener('uploadEmpty', emptyListener)
  })
})
