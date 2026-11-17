import { i18n } from '../i18n'

export type TranslateFn = (key: string, args?: any) => string

function getT(customT?: TranslateFn): TranslateFn {
  if (customT) return customT
  return (key: string, args?: any) => {
    try {
      return (i18n.global as any).t(key, args)
    } catch {
      return key
    }
  }
}

export function formatSkipReason(reason?: string, code?: string, customT?: TranslateFn): string {
  const t = getT(customT)
  const r = (reason || '').trim()
  const c = (code || '').trim()

  if (c === 'already-present' || r.includes('checksum matches') || r.includes('checksum')) {
    return t('status.checksumMatches')
  }

  const extMatch = r.match(/File extension (\.[a-zA-Z0-9]+|[a-zA-Z0-9]+) is filtered/i)
  if (c === 'unsupported-extension' || extMatch || r.includes('unsupported files filter')) {
    const ext = extMatch ? extMatch[1] : ''
    return t('status.extensionFiltered', { ext })
  }

  if (c === 'incomplete-live-photo' || (r.includes('Live photo') && r.includes('matching MOV'))) {
    return t('status.livePhotoMissingMov')
  }

  if (c === 'hash-cache' || r.includes('local cache')) {
    return t('status.alreadyInPhotosLocalCache')
  }

  if (c === 'remote-dedup' || r.includes('remote dedup')) {
    return t('status.alreadyInPhotosRemoteDedup')
  }

  if (r.includes('Dry run') && (r.includes('Live Photo') || r.includes('Live photo'))) {
    return t('status.dryRunLivePhotoWouldUpload')
  }

  if (r.includes('Dry run')) {
    return t('status.dryRunWouldUpload')
  }

  if (r.includes('already exists remotely')) {
    return t('status.livePhotoRemoteExists')
  }

  if (r.includes('Skipped by backup policy')) {
    return t('status.skippedPolicy')
  }

  if (r.includes('Already in Google Photos') || r === 'Already in library' || c === 'skipped') {
    return t('status.alreadyInPhotos')
  }

  return r || t('status.alreadyInPhotos')
}

export function formatErrorMessage(error?: string, customT?: TranslateFn): string {
  const t = getT(customT)
  const err = (error || '').trim()

  if (err.includes('codec header') || err.includes('invalid EXIF stream')) {
    return t('status.codecHeaderError')
  }

  if (err.includes('quota exceeded') || err.includes('quotaExceeded')) {
    return t('status.quotaExceededError')
  }

  if (err.includes('os error 11') || err.includes('Resource temporarily') || err.includes('temporarily locked')) {
    return t('status.resourceLockedError')
  }

  if (err.includes('invalid argument during decode')) {
    return t('status.decodeError')
  }

  if (err.includes('HTTP 429')) {
    return t('status.http429Error')
  }

  if (err.includes('HTTP 401')) {
    return t('status.http401Error')
  }

  return err || t('status.backupFailed')
}

export function formatThreadMessage(message?: string, customT?: TranslateFn): string {
  const t = getT(customT)
  const msg = (message || '').trim()

  if (msg.includes('full resolution')) {
    return t('status.backingUpFullRes')
  }

  if (msg.includes('Streaming video track')) {
    return t('status.streamingVideo')
  }

  if (msg.includes('cloud duplicate fingerprint') || msg.includes('duplicate fingerprint')) {
    return t('status.checkingDuplicate')
  }

  if (msg.includes('Backed up successfully')) {
    return t('status.backedUpSuccess')
  }

  if (msg === 'Hashing...' || msg === 'Hashing') {
    return t('status.hashing')
  }

  if (msg === 'Checking library...' || msg === 'Checking...' || msg === 'Checking') {
    return t('status.checkingLibrary')
  }

  if (msg.includes('Already in Google Photos (local cache)')) {
    return t('status.alreadyInPhotosLocalCache')
  }

  if (msg.includes('Already in Google Photos (remote dedup)')) {
    return t('status.alreadyInPhotosRemoteDedup')
  }

  if (msg.includes('Already in Google Photos')) {
    return t('status.alreadyInPhotos')
  }

  return msg
}
