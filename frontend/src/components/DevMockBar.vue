<script setup lang="ts">
import { ref } from 'vue'
import {
  initMockMode,
  isDevMode,
  mockState,
  setMockView,
  toggleMockMode,
} from '../utils/mockMode'
import { replayLaunch } from '../utils/launchState'
import {
  FLICKER_FLAGS,
  isFlickerFlagActive,
  setFlickerFlag,
  type FlickerFlag,
} from '../utils/flickerDebug'
import { ChevronUp, FlaskConical, RotateCcw } from '@lucide/vue'

const isExpanded = ref(true)
const isAvailable = isDevMode()

const flagsState = ref<Record<FlickerFlag, boolean>>({
  noanim: isFlickerFlagActive('noanim'),
  nobackdrop: isFlickerFlagActive('nobackdrop'),
  noripple: isFlickerFlagActive('noripple'),
  nostate: isFlickerFlagActive('nostate'),
  nolaunch: isFlickerFlagActive('nolaunch'),
  nowave: isFlickerFlagActive('nowave'),
})

function toggleFlag(flag: FlickerFlag) {
  const next = !flagsState.value[flag]
  flagsState.value[flag] = next
  setFlickerFlag(flag, next)
}

if (isAvailable) {
  initMockMode()
}
</script>

<template>
  <div
    v-if="isAvailable"
    class="fixed bottom-3 right-4 z-50 flex flex-col items-end gap-1.5 font-sans select-none"
    style="--wails-draggable: none"
  >
    <!-- Collapsible Controls Card -->
    <div
      v-if="isExpanded && mockState.enabled"
      class="flex flex-col gap-2 rounded-2xl border border-[var(--md-sys-color-outline-variant)]/50 bg-[var(--md-sys-color-surface-container-highest)]/95 p-3 shadow-2xl backdrop-blur-md text-xs text-[var(--md-sys-color-on-surface)]"
    >
      <div class="flex items-center justify-between gap-4 border-b border-[var(--md-sys-color-outline-variant)]/30 pb-1.5">
        <span class="font-bold text-[var(--md-sys-color-primary)] flex items-center gap-1.5">
          <FlaskConical class="size-3.5" />
          Dev Mock Previews
        </span>
        <span class="text-[10px] text-[var(--md-sys-color-on-surface-variant)]">No backend calls</span>
      </div>

      <!-- Quick Screen Switcher -->
      <div class="flex flex-wrap gap-1.5 pt-0.5">
        <button
          type="button"
          class="rounded-full px-2.5 py-1 text-[11px] font-semibold transition-colors cursor-pointer"
          :class="mockState.currentView === 'empty'
            ? 'bg-[var(--md-sys-color-primary)] text-[var(--md-sys-color-on-primary)]'
            : 'bg-[var(--md-sys-color-surface-container-high)] hover:bg-[var(--md-sys-color-surface-container)] text-[var(--md-sys-color-on-surface)]'"
          @click="setMockView('empty')"
        >
          Empty
        </button>
        <button
          type="button"
          class="rounded-full px-2.5 py-1 text-[11px] font-semibold transition-colors cursor-pointer"
          :class="mockState.currentView === 'drop'
            ? 'bg-[var(--md-sys-color-primary)] text-[var(--md-sys-color-on-primary)]'
            : 'bg-[var(--md-sys-color-surface-container-high)] hover:bg-[var(--md-sys-color-surface-container)] text-[var(--md-sys-color-on-surface)]'"
          @click="setMockView('drop')"
        >
          Drop Screen
        </button>
        <button
          type="button"
          class="rounded-full px-2.5 py-1 text-[11px] font-semibold transition-colors cursor-pointer"
          :class="mockState.currentView === 'album'
            ? 'bg-[var(--md-sys-color-primary)] text-[var(--md-sys-color-on-primary)]'
            : 'bg-[var(--md-sys-color-surface-container-high)] hover:bg-[var(--md-sys-color-surface-container)] text-[var(--md-sys-color-on-surface)]'"
          @click="setMockView('album')"
        >
          Album Prompt
        </button>
        <button
          type="button"
          class="rounded-full px-2.5 py-1 text-[11px] font-semibold transition-colors cursor-pointer"
          :class="mockState.currentView === 'uploading'
            ? 'bg-[var(--md-sys-color-primary)] text-[var(--md-sys-color-on-primary)]'
            : 'bg-[var(--md-sys-color-surface-container-high)] hover:bg-[var(--md-sys-color-surface-container)] text-[var(--md-sys-color-on-surface)]'"
          @click="setMockView('uploading')"
        >
          Uploading View
        </button>
        <button
          type="button"
          class="rounded-full px-2.5 py-1 text-[11px] font-semibold transition-colors cursor-pointer"
          :class="mockState.currentView === 'results'
            ? 'bg-[var(--md-sys-color-primary)] text-[var(--md-sys-color-on-primary)]'
            : 'bg-[var(--md-sys-color-surface-container-high)] hover:bg-[var(--md-sys-color-surface-container)] text-[var(--md-sys-color-on-surface)]'"
          @click="setMockView('results')"
        >
          Results Screen
        </button>
      </div>

      <!-- Flicker Bisect Flags -->
      <div class="border-t border-[var(--md-sys-color-outline-variant)]/30 pt-1.5 flex flex-col gap-1">
        <span class="text-[10px] font-semibold text-[var(--md-sys-color-primary)]">Flicker Bisect Switches</span>
        <div class="flex flex-wrap gap-1">
          <button
            v-for="flag in FLICKER_FLAGS"
            :key="flag"
            type="button"
            class="rounded px-2 py-0.5 text-[10px] font-mono transition-colors cursor-pointer"
            :class="flagsState[flag]
              ? 'bg-[var(--md-sys-color-tertiary-container)] text-[var(--md-sys-color-on-tertiary-container)] font-bold'
              : 'bg-[var(--md-sys-color-surface-container)] text-[var(--md-sys-color-on-surface-variant)] opacity-70'"
            @click="toggleFlag(flag)"
          >
            ?{{ flag }}=1
          </button>
        </div>
      </div>
    </div>

    <!-- Main Mock Pill Bar -->
    <div class="flex items-center gap-1.5 rounded-full border border-[var(--md-sys-color-outline-variant)]/60 bg-[var(--md-sys-color-surface-container-high)]/90 px-3 py-1.5 shadow-xl backdrop-blur-md">
      <button
        type="button"
        class="flex items-center gap-1.5 text-xs font-semibold cursor-pointer transition-colors"
        :class="mockState.enabled ? 'text-[var(--md-sys-color-primary)]' : 'text-[var(--md-sys-color-on-surface-variant)]'"
        @click="toggleMockMode"
      >
        <FlaskConical class="size-3.5" />
        <span>{{ mockState.enabled ? 'Mock Mode: ON' : 'Mock Mode: OFF' }}</span>
      </button>

      <button
        type="button"
        class="ml-1 flex items-center gap-1 rounded-full bg-[var(--md-sys-color-primary-container)] px-2.5 py-1 text-[11px] font-semibold text-[var(--md-sys-color-on-primary-container)] hover:bg-[var(--md-sys-color-primary)] hover:text-[var(--md-sys-color-on-primary)] transition-colors cursor-pointer"
        title="Replay launch animation (respects ?launchSlow=...)"
        @click="replayLaunch"
      >
        <RotateCcw class="size-3" />
        <span>Replay launch</span>
      </button>

      <button
        v-if="mockState.enabled"
        type="button"
        class="ml-1 flex size-5 cursor-pointer items-center justify-center rounded-full text-[var(--md-sys-color-on-surface-variant)] hover:bg-[var(--md-sys-color-surface-container-highest)]"
        aria-label="Toggle mock menu"
        @click="isExpanded = !isExpanded"
      >
        <ChevronUp
          class="size-3 transition-transform"
          :class="!isExpanded && 'rotate-180'"
        />
      </button>
    </div>
  </div>
</template>
