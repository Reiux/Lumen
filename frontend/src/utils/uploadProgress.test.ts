import { describe, expect, it } from 'vitest'

function calculateProgressPercent(
  totalFiles: number,
  uploadedFiles: number,
  totalBytes: number,
  uploadedBytes: number
): number {
  if (totalFiles > 0 && uploadedFiles >= totalFiles) {
    return 100
  }
  if (totalBytes > 0) {
    return Math.min(100, Math.max(0, Math.round((uploadedBytes / totalBytes) * 100)))
  }
  if (totalFiles === 0) return 0
  return Math.min(100, Math.max(0, Math.round((uploadedFiles / totalFiles) * 100)))
}

function anonymizeFileName(fileName?: string): string {
  if (!fileName) return ''
  const dotIndex = fileName.lastIndexOf('.')
  const ext = dotIndex !== -1 ? fileName.substring(dotIndex) : ''
  let hash = 0
  for (let i = 0; i < fileName.length; i++) {
    hash = (hash * 31 + fileName.charCodeAt(i)) >>> 0
  }
  const hex = hash.toString(16).padStart(4, '0').slice(-4)
  return `media_${hex}${ext}`
}

function sanitizeUserMessage(msg?: string, fileName?: string): string {
  if (!msg) return ''
  let sanitized = msg
  if (fileName) {
    const masked = anonymizeFileName(fileName)
    sanitized = sanitized.replaceAll(fileName, masked)
  }
  sanitized = sanitized.replace(/(?:[a-zA-Z]:[/\\]|\/)[^\s:]+[/\\]([^/\s:]+)/g, (_match, base) => {
    return anonymizeFileName(base)
  })
  return sanitized
}

describe('Upload Progress Calculation', () => {
  it('calculates progress based on bytes rather than item count when totalBytes is present', () => {
    // 131 of 139 files done, but only 1.3 GB of 14.5 GB uploaded
    const totalFiles = 139
    const uploadedFiles = 131
    const totalBytes = 14.5 * 1024 * 1024 * 1024
    const uploadedBytes = 1.3 * 1024 * 1024 * 1024

    const percent = calculateProgressPercent(totalFiles, uploadedFiles, totalBytes, uploadedBytes)
    // 1.3 / 14.5 = 8.96% -> 9%
    expect(percent).toBe(9)
  })

  it('reports 100% when all files are completed regardless of slight byte rounding', () => {
    const totalFiles = 50
    const uploadedFiles = 50
    const totalBytes = 1000
    const uploadedBytes = 998

    const percent = calculateProgressPercent(totalFiles, uploadedFiles, totalBytes, uploadedBytes)
    expect(percent).toBe(100)
  })

  it('falls back to item count when totalBytes is 0', () => {
    const totalFiles = 20
    const uploadedFiles = 10
    const totalBytes = 0
    const uploadedBytes = 0

    const percent = calculateProgressPercent(totalFiles, uploadedFiles, totalBytes, uploadedBytes)
    expect(percent).toBe(50)
  })

  it('returns 0 when there are no files or bytes', () => {
    expect(calculateProgressPercent(0, 0, 0, 0)).toBe(0)
  })
})

describe('Log Anonymization', () => {
  it('masks sensitive file names while preserving file extensions', () => {
    const masked1 = anonymizeFileName('Screenshot_20261001_093542_TikTok.jpg')
    expect(masked1).toMatch(/^media_[0-9a-f]{4}\.jpg$/)
    expect(masked1).not.toContain('TikTok')
    expect(masked1).not.toContain('20261001')

    const masked2 = anonymizeFileName('My_Vacation_Video.mp4')
    expect(masked2).toMatch(/^media_[0-9a-f]{4}\.mp4$/)
    expect(masked2).not.toContain('Vacation')
  })

  it('sanitizes embedded file paths in user messages', () => {
    const fileName = 'secret_photo.png'
    const msg = 'Uploaded successfully from /home/user/Pictures/secret_photo.png'
    const sanitized = sanitizeUserMessage(msg, fileName)

    expect(sanitized).not.toContain('secret_photo.png')
    expect(sanitized).toContain('media_')
  })
})
