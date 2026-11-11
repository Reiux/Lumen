import { ref } from 'vue'
import { applyDynamicTheme, generateM3Scheme } from '@materialwebunofficial/md3e-web'
import { tauriInvoke, Events } from '../tauri-compat/wails-runtime'

export type M3PaletteVariant = 'tonal-spot' | 'vibrant' | 'expressive'

export interface VariantOption {
  id: M3PaletteVariant
  label: string
  description: string
  icon: string
}

export const PALETTE_VARIANTS: VariantOption[] = [
  {
    id: 'tonal-spot',
    label: 'Tonal spot',
    description: 'Android standard, calm and balanced tones',
    icon: 'palette',
  },
  {
    id: 'vibrant',
    label: 'Vibrant',
    description: 'High chroma, rich and punchy colors',
    icon: 'auto_awesome',
  },
  {
    id: 'expressive',
    label: 'Expressive',
    description: 'Complementary tertiary hues, high contrast',
    icon: 'flare',
  },
]

export interface ThemePreset {
  id: string
  name: string
  seed: string
  primary: string
  container: string
  tertiary: string
}

export const THEME_PRESETS: ThemePreset[] = [
  {
    id: 'azure',
    name: 'Azure Blue',
    seed: '#00639B',
    primary: '#00639B',
    container: '#D0E4FF',
    tertiary: '#6A5779',
  },
  {
    id: 'purple',
    name: 'Iris Purple',
    seed: '#6750A4',
    primary: '#6750A4',
    container: '#EADDFF',
    tertiary: '#7D5260',
  },
  {
    id: 'green',
    name: 'Forest Green',
    seed: '#006C4C',
    primary: '#006C4C',
    container: '#8EF7C7',
    tertiary: '#3E6374',
  },
  {
    id: 'amber',
    name: 'Amber Gold',
    seed: '#855300',
    primary: '#855300',
    container: '#FFDDB3',
    tertiary: '#53643E',
  },
  {
    id: 'coral',
    name: 'Coral Terracotta',
    seed: '#9C4332',
    primary: '#9C4332',
    container: '#FFDAD3',
    tertiary: '#6F5D2F',
  },
  {
    id: 'berry',
    name: 'Crimson Berry',
    seed: '#984061',
    primary: '#984061',
    container: '#FFD9E2',
    tertiary: '#7C5635',
  },
  {
    id: 'teal',
    name: 'Ocean Teal',
    seed: '#00687A',
    primary: '#00687A',
    container: '#AFECF8',
    tertiary: '#575C7E',
  },
  {
    id: 'slate',
    name: 'Charcoal Slate',
    seed: '#555F6C',
    primary: '#555F6C',
    container: '#D9E3F3',
    tertiary: '#625E62',
  },
]

const SEED_STORAGE_KEY = 'lumen_theme_seed'
const VARIANT_STORAGE_KEY = 'lumen_theme_variant'
const THEME_MODE_STORAGE_KEY = 'lumen_theme_mode'

export const isMatugenSupported = ref(false)
export const isMatugenActive = ref(false)
export const matugenSource = ref<string | null>(null)
export const matugenColors = ref<Record<string, string> | null>(null)

function getInitialSeed(): string {
  if (typeof localStorage !== 'undefined') {
    const saved = localStorage.getItem(SEED_STORAGE_KEY)
    if (saved && THEME_PRESETS.some(p => p.seed.toLowerCase() === saved.toLowerCase())) {
      return saved
    }
  }
  return '#00639B'
}

function getInitialVariant(): M3PaletteVariant {
  if (typeof localStorage !== 'undefined') {
    const saved = localStorage.getItem(VARIANT_STORAGE_KEY)
    if (saved === 'tonal-spot' || saved === 'vibrant' || saved === 'expressive') {
      return saved
    }
  }
  return 'tonal-spot'
}

export const currentThemeSeed = ref<string>(getInitialSeed())
export const currentPaletteVariant = ref<M3PaletteVariant>(getInitialVariant())
export const themePulseCounter = ref(0)

export function updateAppFavicon(primaryColor: string) {
  if (typeof document === 'undefined') return
  try {
    const svg = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 108 108"><rect width="108" height="108" rx="24" fill="${primaryColor}"/><g transform="rotate(0 54 54)"><path fill="#38BDF8" d="M 54,26 C 65,26 76,36 76,47 C 76,52 70,54 63,54 C 59,44 54,35 54,26 Z"/></g><g transform="rotate(90 54 54)"><path fill="#60A5FA" d="M 54,26 C 65,26 76,36 76,47 C 76,52 70,54 63,54 C 59,44 54,35 54,26 Z"/></g><g transform="rotate(180 54 54)"><path fill="#818CF8" d="M 54,26 C 65,26 76,36 76,47 C 76,52 70,54 63,54 C 59,44 54,35 54,26 Z"/></g><g transform="rotate(270 54 54)"><path fill="#A78BFA" d="M 54,26 C 65,26 76,36 76,47 C 76,52 70,54 63,54 C 59,44 54,35 54,26 Z"/></g><circle cx="54" cy="54" r="7" fill="#FFFFFF"/></svg>`
    const blob = new Blob([svg], { type: 'image/svg+xml' })
    const url = URL.createObjectURL(blob)
    let link = document.querySelector<HTMLLinkElement>("link[rel*='icon']")
    if (!link) {
      link = document.createElement('link')
      link.rel = 'icon'
      document.head.appendChild(link)
    }
    link.type = 'image/svg+xml'
    link.href = url
  } catch (e) {
    console.warn('Failed to update favicon:', e)
  }
}

export function applyMatugenPalette(colors: Record<string, string>) {
  if (typeof document === 'undefined') return
  const root = document.documentElement

  for (const [key, val] of Object.entries(colors)) {
    if (typeof val === 'string' && val.startsWith('#')) {
      const token = key.startsWith('--') ? key : `--md-sys-color-${key.replace(/_/g, '-')}`
      root.style.setProperty(token, val)
    }
  }

  // Synchronize CSS custom property semantic mappings
  const surface = colors.surface || colors['--md-sys-color-surface']
  const onSurface = colors.on_surface || colors['--md-sys-color-on-surface']
  const surfaceContainer = colors.surface_container || colors['--md-sys-color-surface-container']
  const surfaceContainerHigh = colors.surface_container_high || colors['--md-sys-color-surface-container-high']
  const primary = colors.primary || colors['--md-sys-color-primary']
  const onPrimary = colors.on_primary || colors['--md-sys-color-on-primary']
  const secondary = colors.secondary_container || colors.secondary || colors['--md-sys-color-secondary-container']
  const onSecondary = colors.on_secondary_container || colors.on_secondary || colors['--md-sys-color-on-secondary-container']
  const outlineVariant = colors.outline_variant || colors['--md-sys-color-outline-variant']
  const error = colors.error || colors['--md-sys-color-error']
  const onError = colors.on_error || colors['--md-sys-color-on-error']

  if (surface) root.style.setProperty('--background', surface)
  if (onSurface) root.style.setProperty('--foreground', onSurface)
  if (surfaceContainer) root.style.setProperty('--card', surfaceContainer)
  if (surfaceContainerHigh) root.style.setProperty('--popover', surfaceContainerHigh)
  if (primary) {
    root.style.setProperty('--primary', primary)
    root.style.setProperty('--ring', primary)
    updateAppFavicon(primary)
  }
  if (onPrimary) root.style.setProperty('--primary-foreground', onPrimary)
  if (secondary) root.style.setProperty('--secondary', secondary)
  if (onSecondary) root.style.setProperty('--secondary-foreground', onSecondary)
  if (outlineVariant) {
    root.style.setProperty('--border', outlineVariant)
    root.style.setProperty('--input', outlineVariant)
  }
  if (error) root.style.setProperty('--destructive', error)
  if (onError) root.style.setProperty('--destructive-foreground', onError)

  themePulseCounter.value++
}

export function applyTheme(seed: string = currentThemeSeed.value, variant: M3PaletteVariant = currentPaletteVariant.value) {
  try {
    applyDynamicTheme(seed, true, variant)
  } catch (e) {
    console.warn('Failed to apply dynamic theme:', e)
  }
  updateAppFavicon(seed)
  themePulseCounter.value++
}

export function setThemeSeed(seed: string) {
  if (isMatugenActive.value) {
    isMatugenActive.value = false
    if (typeof localStorage !== 'undefined') {
      localStorage.setItem(THEME_MODE_STORAGE_KEY, 'preset')
    }
  }
  currentThemeSeed.value = seed
  if (typeof localStorage !== 'undefined') {
    localStorage.setItem(SEED_STORAGE_KEY, seed)
  }
  applyTheme(seed, currentPaletteVariant.value)
}

export function setPaletteVariant(variant: M3PaletteVariant) {
  currentPaletteVariant.value = variant
  if (typeof localStorage !== 'undefined') {
    localStorage.setItem(VARIANT_STORAGE_KEY, variant)
  }
  if (!isMatugenActive.value) {
    applyTheme(currentThemeSeed.value, variant)
  }
}

export async function setMatugenEnabled(enabled: boolean) {
  isMatugenActive.value = enabled
  if (typeof localStorage !== 'undefined') {
    localStorage.setItem(THEME_MODE_STORAGE_KEY, enabled ? 'matugen' : 'preset')
  }

  if (enabled) {
    if (matugenColors.value) {
      applyMatugenPalette(matugenColors.value)
    } else {
      const colors = await tauriInvoke<Record<string, string>>('get_matugen_colors')
      if (colors) {
        matugenColors.value = colors
        applyMatugenPalette(colors)
      }
    }
  } else {
    // Clear custom style overrides and restore chosen preset theme
    if (typeof document !== 'undefined') {
      const root = document.documentElement
      root.removeAttribute('style')
    }
    applyTheme(currentThemeSeed.value, currentPaletteVariant.value)
  }
}

const presetPreviewCache = new Map<string, { primary: string; container: string; tertiary: string }>()

export function getPresetPreviewColors(preset: ThemePreset, variant: M3PaletteVariant = currentPaletteVariant.value) {
  const cacheKey = `${preset.id}:${variant}`
  const cached = presetPreviewCache.get(cacheKey)
  if (cached) {
    return cached
  }

  let result = {
    primary: preset.primary,
    container: preset.container,
    tertiary: preset.tertiary,
  }

  try {
    const scheme = generateM3Scheme(preset.seed, true, variant)
    if (scheme && scheme['--md-sys-color-primary']) {
      result = {
        primary: scheme['--md-sys-color-primary'],
        container: scheme['--md-sys-color-primary-container'],
        tertiary: scheme['--md-sys-color-tertiary'],
      }
    }
  } catch {
    // fallback
  }

  presetPreviewCache.set(cacheKey, result)
  return result
}

export function cycleNextTheme() {
  if (isMatugenSupported.value) {
    if (!isMatugenActive.value && matugenColors.value) {
      // Switch into Matugen dynamic
      setMatugenEnabled(true)
      return
    }
  }

  isMatugenActive.value = false
  if (typeof localStorage !== 'undefined') {
    localStorage.setItem(THEME_MODE_STORAGE_KEY, 'preset')
  }

  const currentIndex = THEME_PRESETS.findIndex(
    p => p.seed.toLowerCase() === currentThemeSeed.value.toLowerCase()
  )
  const nextIndex = (currentIndex + 1) % THEME_PRESETS.length
  setThemeSeed(THEME_PRESETS[nextIndex].seed)
}

export async function initTheme() {
  const seed = getInitialSeed()
  const variant = getInitialVariant()
  currentThemeSeed.value = seed
  currentPaletteVariant.value = variant

  // Initial preset application
  applyTheme(seed, variant)

  // Query Linux Matugen support
  try {
    interface MatugenStatusResponse {
      supported: boolean
      available: boolean
      active_path?: string
      colors?: Record<string, string>
    }
    const status = await tauriInvoke<MatugenStatusResponse>('get_matugen_status')
    if (status && status.supported) {
      isMatugenSupported.value = true
      matugenSource.value = status.active_path || null
      matugenColors.value = status.colors || null

      const savedMode = typeof localStorage !== 'undefined' ? localStorage.getItem(THEME_MODE_STORAGE_KEY) : null
      // If user selected matugen or this is first run on Linux with available matugen colors, enable dynamic theming
      const shouldActivate = savedMode === 'matugen' || (savedMode === null && status.available)
      if (shouldActivate && status.colors) {
        isMatugenActive.value = true
        applyMatugenPalette(status.colors)
      }

      // Wire live wallpaper / color change listener
      Events.On('matugen:colors-changed', (event: { data: Record<string, string> }) => {
        if (event && event.data) {
          matugenColors.value = event.data
          if (isMatugenActive.value) {
            applyMatugenPalette(event.data)
          }
        }
      })
    }
  } catch (e) {
    console.debug('Matugen dynamic theme not active:', e)
  }
}
