<script setup lang="ts">
import { computed } from 'vue'

const props = withDefaults(defineProps<{
  value?: number
}>(), {
  value: 0,
})

const clampedPercent = computed(() => {
  return Math.max(0, Math.min(100, props.value))
})
</script>

<template>
  <div
    class="relative h-6 w-full overflow-hidden rounded-full select-none"
    role="progressbar"
    :aria-valuenow="clampedPercent"
    aria-valuemin="0"
    aria-valuemax="100"
  >
    <!-- Background track -->
    <svg
      class="absolute inset-0 h-full w-full"
      preserveAspectRatio="none"
      viewBox="0 0 1000 24"
    >
      <line
        x1="0"
        y1="12"
        x2="1000"
        y2="12"
        stroke="var(--md-sys-color-surface-container-highest)"
        stroke-width="6"
        stroke-linecap="round"
      />
    </svg>

    <!-- Active wavy fill -->
    <div
      class="absolute inset-y-0 left-0 overflow-hidden transition-all duration-300 ease-out"
      :style="{ width: `${clampedPercent}%` }"
    >
      <!-- Normal wavy animated track -->
      <div class="wavy-track relative h-full w-[2000px]">
        <svg
          class="h-full w-full animate-wavy-slide"
          preserveAspectRatio="none"
          viewBox="0 0 2000 24"
        >
          <path
            d="M 0 12 Q 25 3 50 12 T 100 12 T 150 12 T 200 12 T 250 12 T 300 12 T 350 12 T 400 12 T 450 12 T 500 12 T 550 12 T 600 12 T 650 12 T 700 12 T 750 12 T 800 12 T 850 12 T 900 12 T 950 12 T 1000 12 T 1050 12 T 1100 12 T 1150 12 T 1200 12 T 1250 12 T 1300 12 T 1350 12 T 1400 12 T 1450 12 T 1500 12 T 1550 12 T 1600 12 T 1650 12 T 1700 12 T 1750 12 T 1800 12 T 1850 12 T 1900 12 T 1950 12 T 2000 12"
            fill="none"
            stroke="var(--md-sys-color-primary)"
            stroke-width="6"
            stroke-linecap="round"
          />
        </svg>
      </div>

      <!-- Static straight track (shown when ?nowave=1 / data-nowave is set) -->
      <div class="static-track hidden h-full w-full items-center">
        <div class="h-1.5 w-full rounded-full bg-[var(--md-sys-color-primary)]" />
      </div>
    </div>
  </div>
</template>

<style scoped>
@keyframes wavySlide {
  0% { transform: translateX(0); }
  100% { transform: translateX(-100px); }
}

.animate-wavy-slide {
  animation: wavySlide 2s linear infinite;
}

:global(html[data-nowave]) .wavy-track {
  display: none !important;
}

:global(html[data-nowave]) .static-track {
  display: flex !important;
}

@media (prefers-reduced-motion: reduce) {
  .animate-wavy-slide {
    animation: none;
  }
}
</style>
