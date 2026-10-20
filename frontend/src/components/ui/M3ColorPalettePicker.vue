<script setup lang="ts">
import { computed } from 'vue'
import {
  THEME_PRESETS,
  PALETTE_VARIANTS,
  currentThemeSeed,
  currentPaletteVariant,
  setThemeSeed,
  setPaletteVariant,
  getPresetPreviewColors,
  type M3PaletteVariant,
} from '../../utils/theme'
import M3Icon from './M3Icon.vue'

const currentPreset = computed(() => {
  return THEME_PRESETS.find(p => p.seed.toLowerCase() === currentThemeSeed.value.toLowerCase()) || THEME_PRESETS[0]
})

const activeVariantMeta = computed(() => {
  return PALETTE_VARIANTS.find(v => v.id === currentPaletteVariant.value) || PALETTE_VARIANTS[0]
})

function selectPreset(seed: string) {
  setThemeSeed(seed)
}

function selectVariant(variant: M3PaletteVariant) {
  setPaletteVariant(variant)
}
</script>

<template>
  <div class="flex flex-col gap-4 w-full select-none overflow-visible">
    <!-- Header with Active Info -->
    <div class="flex items-center justify-between px-0.5">
      <div class="flex flex-col">
        <span class="text-xs font-semibold text-[var(--md-sys-color-on-surface)]">
          Color Palette
        </span>
        <span class="text-[11px] text-[var(--md-sys-color-on-surface-variant)]">
          Active: <strong class="text-[var(--md-sys-color-primary)] font-medium">{{ currentPreset.name }}</strong> ({{ activeVariantMeta.label }})
        </span>
      </div>
      <span class="rounded-full bg-[var(--md-sys-color-primary-container)] px-2.5 py-0.5 text-[10px] font-semibold text-[var(--md-sys-color-on-primary-container)] uppercase tracking-wider">
        Material You
      </span>
    </div>

    <!-- Palette Style Segmented Control (Tonal spot / Vibrant / Expressive) -->
    <div class="flex flex-col gap-1.5">
      <div class="flex items-center justify-between px-0.5">
        <span class="text-[11px] font-medium text-[var(--md-sys-color-on-surface-variant)]">
          Palette Style
        </span>
        <span class="text-[10px] text-[var(--md-sys-color-on-surface-variant)]/80 italic">
          {{ activeVariantMeta.description }}
        </span>
      </div>
      <div
        class="grid grid-cols-3 gap-1 rounded-full bg-[var(--md-sys-color-surface-container)] p-1 border border-[var(--md-sys-color-outline-variant)]/40"
        role="tablist"
        aria-label="Palette variant options"
      >
        <button
          v-for="variant in PALETTE_VARIANTS"
          :key="variant.id"
          type="button"
          role="tab"
          :aria-selected="currentPaletteVariant === variant.id"
          class="no-state-layer relative flex items-center justify-center gap-1.5 rounded-full py-1.5 px-2 text-xs transition-all duration-200 cursor-pointer focus:outline-none"
          :class="currentPaletteVariant === variant.id
            ? 'bg-[var(--md-sys-color-secondary-container)] text-[var(--md-sys-color-on-secondary-container)] font-semibold shadow-xs'
            : 'text-[var(--md-sys-color-on-surface-variant)] hover:text-[var(--md-sys-color-on-surface)] hover:bg-[var(--md-sys-color-surface-container-highest)]/60 font-medium'"
          @click="selectVariant(variant.id)"
        >
          <M3Icon
            :name="variant.icon"
            :size="15"
            :filled="currentPaletteVariant === variant.id"
          />
          <span class="truncate">{{ variant.label }}</span>
        </button>
      </div>
    </div>

    <!-- Android-style 4-quadrant Palette Swatches Grid (4 columns x 2 rows, ample safety margin) -->
    <div
      class="grid grid-cols-4 gap-x-2 gap-y-3 p-2 overflow-visible"
      role="radiogroup"
      aria-label="Material You Color Palette Options"
    >
      <button
        v-for="preset in THEME_PRESETS"
        :key="preset.id"
        type="button"
        role="radio"
        :aria-checked="currentThemeSeed.toLowerCase() === preset.seed.toLowerCase()"
        :aria-label="`${preset.name} theme color`"
        :title="preset.name"
        class="no-state-layer group relative flex flex-col items-center gap-1 p-2 rounded-[20px] focus:outline-none cursor-pointer transition-colors duration-200 hover:bg-[var(--md-sys-color-surface-container-high)]/60"
        style="overflow: visible !important;"
        @click="selectPreset(preset.seed)"
      >
        <!-- Swatch outer container (size-14 layout box provides clearance for 44px circle + ring-3 + ring-offset-2) -->
        <div class="relative flex size-14 items-center justify-center overflow-visible">
          <!-- Swatch Circle (44px, strictly circular corners) -->
          <div
            class="relative flex size-11 items-center justify-center rounded-full overflow-hidden transition-all duration-300 ease-[var(--md-sys-motion-easing-spatial)]"
            :class="currentThemeSeed.toLowerCase() === preset.seed.toLowerCase()
              ? 'scale-105 ring-3 ring-offset-2 ring-offset-[var(--md-sys-color-surface-container-low)] ring-[var(--md-sys-color-primary)] shadow-md'
              : 'group-hover:scale-105 group-hover:shadow-sm border border-[var(--md-sys-color-outline-variant)]/40 opacity-90 group-hover:opacity-100'"
          >
            <!-- 4-Quadrant Split Background rotated 45deg like Android (dynamically computed per variant) -->
            <div class="absolute inset-0 grid grid-cols-2 grid-rows-2 -rotate-45 scale-125 pointer-events-none">
              <div :style="{ backgroundColor: getPresetPreviewColors(preset, currentPaletteVariant).primary }" />
              <div :style="{ backgroundColor: getPresetPreviewColors(preset, currentPaletteVariant).container }" />
              <div :style="{ backgroundColor: getPresetPreviewColors(preset, currentPaletteVariant).tertiary }" />
              <div :style="{ backgroundColor: getPresetPreviewColors(preset, currentPaletteVariant).primary }" />
            </div>

            <!-- Selected Checkmark Badge -->
            <div
              v-if="currentThemeSeed.toLowerCase() === preset.seed.toLowerCase()"
              class="relative z-10 flex size-5 items-center justify-center rounded-full bg-[var(--md-sys-color-surface)] text-[var(--md-sys-color-primary)] shadow-xs transition-transform duration-200 scale-100"
            >
              <M3Icon
                name="check"
                :size="14"
              />
            </div>
          </div>
        </div>

        <!-- Preset Label -->
        <span
          class="text-[10px] font-medium tracking-tight truncate max-w-[72px] text-center transition-colors"
          :class="currentThemeSeed.toLowerCase() === preset.seed.toLowerCase()
            ? 'font-bold text-[var(--md-sys-color-primary)]'
            : 'text-[var(--md-sys-color-on-surface-variant)] group-hover:text-[var(--md-sys-color-on-surface)]'"
        >
          {{ preset.name }}
        </span>
      </button>
    </div>
  </div>
</template>

<style scoped>
button:focus-visible > div > div {
  outline: 3px solid var(--md-sys-color-primary);
  outline-offset: 3px;
  border-radius: 9999px;
}
</style>
