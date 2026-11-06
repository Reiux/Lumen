<script setup lang="ts">
import { computed } from 'vue'

interface Props {
  size?: number | string
  animated?: boolean
  blade1Color?: string
  blade2Color?: string
  blade3Color?: string
  blade4Color?: string
  centerColor?: string
}

const props = withDefaults(defineProps<Props>(), {
  size: 40,
  animated: false,
})

const sizeValue = computed(() => (typeof props.size === 'number' ? `${props.size}px` : props.size))
</script>

<template>
  <div
    class="lumen-logo inline-flex shrink-0 items-center justify-center select-none bg-transparent"
    :style="{
      width: sizeValue,
      height: sizeValue,
    }"
    aria-hidden="true"
  >
    <svg
      xmlns="http://www.w3.org/2000/svg"
      viewBox="0 0 108 108"
      width="100%"
      height="100%"
      class="block size-full overflow-visible"
    >
      <!-- Single wrapper group for whole-logo animation -->
      <g
        class="aperture-group"
        :class="{ 'is-launching': animated }"
      >
        <!-- Blade 1: Top-Right (0 deg) - Primary -->
        <g transform="rotate(0 54 54)">
          <path
            :fill="blade1Color || 'var(--md-sys-color-primary, #38BDF8)'"
            d="M 54,26 C 65,26 76,36 76,47 C 76,52 70,54 63,54 C 59,44 54,35 54,26 Z"
            class="blade blade-1"
          />
        </g>
        <!-- Blade 2: Bottom-Right (90 deg) - Secondary -->
        <g transform="rotate(90 54 54)">
          <path
            :fill="blade2Color || 'var(--md-sys-color-secondary, #60A5FA)'"
            d="M 54,26 C 65,26 76,36 76,47 C 76,52 70,54 63,54 C 59,44 54,35 54,26 Z"
            class="blade blade-2"
          />
        </g>
        <!-- Blade 3: Bottom-Left (180 deg) - Tertiary -->
        <g transform="rotate(180 54 54)">
          <path
            :fill="blade3Color || 'var(--md-sys-color-tertiary, #818CF8)'"
            d="M 54,26 C 65,26 76,36 76,47 C 76,52 70,54 63,54 C 59,44 54,35 54,26 Z"
            class="blade blade-3"
          />
        </g>
        <!-- Blade 4: Top-Left (270 deg) - Primary Container -->
        <g transform="rotate(270 54 54)">
          <path
            :fill="blade4Color || 'var(--md-sys-color-primary-container, #A78BFA)'"
            d="M 54,26 C 65,26 76,36 76,47 C 76,52 70,54 63,54 C 59,44 54,35 54,26 Z"
            class="blade blade-4"
          />
        </g>
        <!-- Center Core: Radiant lens core -->
        <circle
          cx="54"
          cy="54"
          r="7"
          :fill="centerColor || 'var(--md-sys-color-on-surface, #FFFFFF)'"
          class="aperture-core"
        />
      </g>
    </svg>
  </div>
</template>

<style scoped>
.lumen-logo {
  background: transparent !important;
  border: none !important;
  border-radius: 0 !important;
}

/* Whole-logo animation wrapper: rotates around center 50% 50% of view-box */
.aperture-group {
  transform-box: view-box;
  transform-origin: 50% 50%;
}

/* Smooth color morphing when theme changes */
.blade,
.aperture-core {
  transition: fill var(--md-sys-motion-duration-slow-effects, 300ms) var(--md-sys-motion-easing-effects, cubic-bezier(0.2, 0, 0, 1));
}

/* Android Splash Animation (avd_splash_icon.xml):
   - Whole logo rotates clockwise 0 -> 360 deg over 500 ms with cubic-bezier(0.4, 0, 0.2, 1) (fast_out_slow_in)
   - Simultaneously scales 0.7 -> 1.0 with overshoot curve cubic-bezier(0.34, 1.56, 0.64, 1)
*/
.is-launching {
  animation:
    splash-rotate 500ms cubic-bezier(0.4, 0, 0.2, 1) forwards,
    splash-scale 500ms cubic-bezier(0.34, 1.56, 0.64, 1) forwards;
}

@keyframes splash-rotate {
  0% {
    rotate: 0deg;
  }
  100% {
    rotate: 360deg;
  }
}

@keyframes splash-scale {
  0% {
    scale: 0.7;
  }
  100% {
    scale: 1;
  }
}

@media (prefers-reduced-motion: reduce) {
  .is-launching,
  .blade,
  .aperture-core {
    animation: none !important;
    transition: none !important;
  }
}
</style>
