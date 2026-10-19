<script setup lang="ts">
import { ref } from 'vue'
import {
  initMockMode,
  isDevMode,
  mockState,
  setMockView,
  toggleMockMode,
} from '../utils/mockMode'
import { ChevronUp, FlaskConical } from '@lucide/vue'

const isExpanded = ref(true)
const isAvailable = isDevMode()

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
