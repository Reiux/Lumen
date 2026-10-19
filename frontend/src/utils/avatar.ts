export function getEmailInitial(email: string): string {
  if (!email) return '?'
  return email.trim().charAt(0).toUpperCase()
}

export function displayNameFromEmail(email: string): string {
  if (!email) return 'Guest'
  const namePart = email.split('@')[0]
  if (!namePart) return email
  return namePart
    .split(/[._-]/)
    .filter(Boolean)
    .map(word => word.charAt(0).toUpperCase() + word.slice(1).toLowerCase())
    .join(' ')
}

export function getAvatarColor(email: string): { bg: string; text: string } {
  if (!email) {
    return {
      bg: 'var(--md-sys-color-primary-container)',
      text: 'var(--md-sys-color-on-primary-container)',
    }
  }

  // Pre-selected M3 tonal color pairs (bg, text) in dark theme
  const palettes = [
    { bg: 'oklch(0.34 0.11 245)', text: 'oklch(0.92 0.06 245)' }, // Blue
    { bg: 'oklch(0.32 0.10 160)', text: 'oklch(0.92 0.06 160)' }, // Mint / Green
    { bg: 'oklch(0.33 0.11 290)', text: 'oklch(0.92 0.06 290)' }, // Purple
    { bg: 'oklch(0.34 0.11 40)', text: 'oklch(0.92 0.06 40)' },   // Amber / Orange
    { bg: 'oklch(0.32 0.10 200)', text: 'oklch(0.92 0.06 200)' }, // Cyan
    { bg: 'oklch(0.34 0.12 340)', text: 'oklch(0.92 0.06 340)' }, // Rose
  ]

  let hash = 0
  for (let i = 0; i < email.length; i++) {
    hash = (hash << 5) - hash + email.charCodeAt(i)
    hash |= 0
  }
  const index = Math.abs(hash) % palettes.length
  return palettes[index]
}

export function getCustomAvatar(email: string): string | null {
  if (!email) return null
  try {
    return localStorage.getItem(`lumen_avatar_${email.toLowerCase()}`)
  } catch {
    return null
  }
}

export function saveCustomAvatar(email: string, dataUrl: string): boolean {
  if (!email) return false
  try {
    localStorage.setItem(`lumen_avatar_${email.toLowerCase()}`, dataUrl)
    return true
  } catch {
    return false
  }
}

export function processImageFile(file: File): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader()
    reader.onerror = () => reject(new Error('Failed to read file'))
    reader.onload = (e) => {
      const img = new Image()
      img.onerror = () => reject(new Error('Failed to parse image'))
      img.onload = () => {
        const size = 128
        const canvas = document.createElement('canvas')
        canvas.width = size
        canvas.height = size
        const ctx = canvas.getContext('2d')
        if (!ctx) {
          reject(new Error('Canvas context unavailable'))
          return
        }

        const minDim = Math.min(img.width, img.height)
        const sx = (img.width - minDim) / 2
        const sy = (img.height - minDim) / 2

        ctx.drawImage(img, sx, sy, minDim, minDim, 0, 0, size, size)
        resolve(canvas.toDataURL('image/jpeg', 0.85))
      }
      img.src = e.target?.result as string
    }
    reader.readAsDataURL(file)
  })
}
