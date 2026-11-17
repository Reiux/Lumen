import { describe, expect, it } from 'vitest'
import { formatErrorMessage, formatSkipReason, formatThreadMessage } from './statusTranslation'
import { setLocale } from '../i18n'

describe('statusTranslation', () => {
  it('translates skip reasons across locales', () => {
    // English
    setLocale('en')
    expect(formatSkipReason('File checksum matches existing item in Google Photos')).toBe(
      'File checksum matches existing item in Google Photos'
    )
    expect(formatSkipReason('File extension .xyz is filtered by unsupported files filter')).toBe(
      'File extension .xyz is filtered by unsupported files filter'
    )

    // Japanese
    setLocale('ja')
    expect(formatSkipReason('File checksum matches existing item in Google Photos')).toBe(
      'ファイルのチェックサムがGoogleフォトの既存の項目と一致しました'
    )
    expect(formatSkipReason('File extension .xyz is filtered by unsupported files filter')).toBe(
      'ファイル拡張子 .xyz は未サポートファイルフィルターにより除外されました'
    )

    // Indonesian
    setLocale('id')
    expect(formatSkipReason('File checksum matches existing item in Google Photos')).toBe(
      'Checksum file cocok dengan item yang ada di Google Foto'
    )
    expect(formatSkipReason('File extension .xyz is filtered by unsupported files filter')).toBe(
      'Ekstensi file .xyz difilter oleh filter file yang tidak didukung'
    )
  })

  it('translates error messages across locales', () => {
    setLocale('ja')
    expect(formatErrorMessage('Unrecognized codec header or invalid EXIF stream')).toBe(
      '認識できないコーデックヘッダーまたは無効なEXIFストリームです'
    )
    expect(formatErrorMessage('Google Photos quota exceeded for non-saver mode')).toBe(
      'Googleフォトの保存容量上限を超過しました（オリジナル画質モード）'
    )
    expect(formatErrorMessage('Resource temporarily unavailable (os error 11)')).toBe(
      'リソースが一時的に利用できません（ファイルロック / os error 11）'
    )

    setLocale('id')
    expect(formatErrorMessage('Unrecognized codec header or invalid EXIF stream')).toBe(
      'Header codec tidak dikenali atau stream EXIF tidak valid'
    )
    expect(formatErrorMessage('Google Photos quota exceeded for non-saver mode')).toBe(
      'Kuota Google Foto terlampaui untuk mode non-penghemat'
    )
    expect(formatErrorMessage('Resource temporarily unavailable (os error 11)')).toBe(
      'Sumber daya sementara tidak tersedia (os error 11)'
    )
  })

  it('translates thread messages across locales', () => {
    setLocale('ja')
    expect(formatThreadMessage('Backing up full resolution...')).toBe('元の画質でバックアップ中...')
    expect(formatThreadMessage('Streaming video track...')).toBe('動画トラックを送信中...')
    expect(formatThreadMessage('Checking cloud duplicate fingerprint...')).toBe('クラウド上の重複フィンガープリントを確認中...')
    expect(formatThreadMessage('Backed up successfully')).toBe('正常にバックアップされました')

    setLocale('id')
    expect(formatThreadMessage('Backing up full resolution...')).toBe('Mencadangkan resolusi penuh...')
    expect(formatThreadMessage('Streaming video track...')).toBe('Mengalirkan trek video...')
    expect(formatThreadMessage('Checking cloud duplicate fingerprint...')).toBe('Memeriksa sidik jari duplikat cloud...')
    expect(formatThreadMessage('Backed up successfully')).toBe('Berhasil dicadangkan')

    // Reset to en
    setLocale('en')
  })
})
