import { ref } from 'vue'
import { applyDynamicTheme, generateM3Scheme } from '@materialwebunofficial/md3e-web'

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
  applyTheme(currentThemeSeed.value, variant)
}

export function getPresetPreviewColors(preset: ThemePreset, variant: M3PaletteVariant = currentPaletteVariant.value) {
  try {
    const scheme = generateM3Scheme(preset.seed, true, variant)
    if (scheme && scheme['--md-sys-color-primary']) {
      return {
        primary: scheme['--md-sys-color-primary'],
        container: scheme['--md-sys-color-primary-container'],
        tertiary: scheme['--md-sys-color-tertiary'],
      }
    }
  } catch {
    // fallback
  }
  return {
    primary: preset.primary,
    container: preset.container,
    tertiary: preset.tertiary,
  }
}

export function cycleNextTheme() {
  const currentIndex = THEME_PRESETS.findIndex(
    p => p.seed.toLowerCase() === currentThemeSeed.value.toLowerCase()
  )
  const nextIndex = (currentIndex + 1) % THEME_PRESETS.length
  setThemeSeed(THEME_PRESETS[nextIndex].seed)
}

export function initTheme() {
  const seed = getInitialSeed()
  const variant = getInitialVariant()
  currentThemeSeed.value = seed
  currentPaletteVariant.value = variant
  applyTheme(seed, variant)
}
