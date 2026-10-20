<script setup lang="ts">
import { onMounted, onUnmounted, ref, watch } from 'vue'
import type { Ref } from 'vue'
import {
  AlertCircle,
  Check,
  CheckCircle2,
  ChevronDown,
  Copy,
  RotateCcw,
  XCircle,
} from '@lucide/vue'
import type { UploadResults } from '../utils/uploadResults'
import Button from './ui/button/Button.vue'

const props = defineProps<{
  results: UploadResults
}>()

const emit = defineEmits<{
  (e: 'copy'): void
  (e: 'clear'): void
}>()

const showFailedList = ref(false)
const copied = ref(false)

const displaySuccess = ref(props.results.success.length)
const displayFail = ref(props.results.fail.length)

let animFrames: number[] = []

function animateNumber(target: number, currentRef: Ref<number>, duration = 400) {
  if (typeof window !== 'undefined' && window.matchMedia?.('(prefers-reduced-motion: reduce)')?.matches) {
    currentRef.value = target
    return
  }
  const start = currentRef.value
  const diff = target - start
  if (diff === 0) return
  const startTime = performance.now()
  function step(now: number) {
    const elapsed = now - startTime
    const progress = Math.min(elapsed / duration, 1)
    const ease = 1 - Math.pow(1 - progress, 3)
    currentRef.value = Math.round(start + diff * ease)
    if (progress < 1) {
      const id = requestAnimationFrame(step)
      animFrames.push(id)
    } else {
      currentRef.value = target
    }
  }
  const id = requestAnimationFrame(step)
  animFrames.push(id)
}

onMounted(() => {
  displaySuccess.value = 0
  displayFail.value = 0
  animateNumber(props.results.success.length, displaySuccess)
  animateNumber(props.results.fail.length, displayFail)
})

onUnmounted(() => {
  animFrames.forEach(id => cancelAnimationFrame(id))
  animFrames = []
})

watch(() => props.results.success.length, (newVal) => {
  animateNumber(newVal, displaySuccess)
})

watch(() => props.results.fail.length, (newVal) => {
  animateNumber(newVal, displayFail)
})

function onCopyClick() {
  emit('copy')
  copied.value = true
  setTimeout(() => {
    copied.value = false
  }, 1500)
}
</script>

<template>
  <div class="flex w-full flex-col gap-5 rounded-[32px] border border-[var(--md-sys-color-outline-variant)]/40 bg-[var(--md-sys-color-surface-container)] p-6 shadow-sm select-none">
    <!-- Header -->
    <div class="flex items-center justify-between">
      <div class="flex flex-col">
        <h2 class="text-xl font-bold tracking-tight text-[var(--md-sys-color-on-surface)]">
          Upload Batch Results
        </h2>
        <span class="text-xs text-[var(--md-sys-color-on-surface-variant)]">
          Summary of recent upload operations
        </span>
      </div>

      <!-- Action buttons -->
      <div class="flex items-center gap-2">
        <!-- Tonal Copy as JSON Button with Icon morph -->
        <Button
          type="button"
          class="h-9 rounded-full px-4 text-xs font-semibold shadow-xs cursor-pointer select-none bg-[var(--md-sys-color-surface-container-high)] text-[var(--md-sys-color-on-surface)] hover:bg-[var(--md-sys-color-surface-container-highest)] border border-[var(--md-sys-color-outline-variant)]/40 relative"
          aria-label="Copy results as JSON"
          @click="onCopyClick"
        >
          <div class="relative size-3.5 mr-1.5 shrink-0 flex items-center justify-center">
            <Copy
              class="size-3.5 absolute transition-[transform,opacity] duration-200 ease-[var(--md-sys-motion-easing-spatial)]"
              :class="copied ? 'scale-0 rotate-90 opacity-0' : 'scale-100 rotate-0 opacity-100'"
            />
            <Check
              class="size-3.5 absolute text-[var(--md-sys-color-tertiary)] transition-[transform,opacity] duration-200 ease-[var(--md-sys-motion-easing-spatial)]"
              :class="copied ? 'scale-110 rotate-0 opacity-100' : 'scale-0 -rotate-90 opacity-0'"
            />
          </div>
          <span>{{ copied ? 'Copied JSON!' : 'Copy as JSON' }}</span>
        </Button>

        <!-- Clear / New Upload button -->
        <Button
          variant="outline"
          type="button"
          class="h-9 rounded-full px-3.5 text-xs font-semibold shadow-xs cursor-pointer select-none"
          aria-label="Dismiss results and start new upload"
          @click="emit('clear')"
        >
          <RotateCcw class="size-3.5 mr-1.5" />
          <span>New Upload</span>
        </Button>
      </div>
    </div>

    <!-- Two Stat Cards Side by Side -->
    <div class="grid grid-cols-1 sm:grid-cols-2 gap-4">
      <!-- Successful in Tertiary Container -->
      <div class="m3-stat-card-1 flex items-center gap-4 rounded-[24px] border border-[var(--md-sys-color-tertiary)]/30 bg-[var(--md-sys-color-tertiary-container)] p-5 text-[var(--md-sys-color-on-tertiary-container)] shadow-xs">
        <div class="flex size-12 shrink-0 items-center justify-center rounded-2xl bg-[var(--md-sys-color-on-tertiary-container)]/15">
          <CheckCircle2 class="size-6 text-[var(--md-sys-color-on-tertiary-container)]" />
        </div>
        <div class="flex flex-col min-w-0">
          <span class="text-3xl font-extrabold tabular-nums tracking-tight">
            {{ displaySuccess }}
          </span>
          <span class="text-xs font-semibold opacity-90 truncate">
            Successful Uploads
          </span>
        </div>
      </div>

      <!-- Failed in Error Container -->
      <div class="m3-stat-card-2 flex items-center gap-4 rounded-[24px] border border-[var(--md-sys-color-error)]/30 bg-[var(--md-sys-color-error-container)] p-5 text-[var(--md-sys-color-on-error-container)] shadow-xs">
        <div class="flex size-12 shrink-0 items-center justify-center rounded-2xl bg-[var(--md-sys-color-on-error-container)]/15">
          <XCircle class="size-6 text-[var(--md-sys-color-on-error-container)]" />
        </div>
        <div class="flex flex-col min-w-0">
          <span class="text-3xl font-extrabold tabular-nums tracking-tight">
            {{ displayFail }}
          </span>
          <span class="text-xs font-semibold opacity-90 truncate">
            Failed Uploads
          </span>
        </div>
      </div>
    </div>

    <!-- Secondary stats chips (Skipped & Warnings) -->
    <div class="flex flex-wrap items-center gap-2 pt-1 text-xs">
      <span class="inline-flex items-center rounded-full bg-[var(--md-sys-color-surface-container-high)] px-3 py-1 font-medium text-[var(--md-sys-color-on-surface-variant)]">
        Skipped: <strong class="ml-1 text-[var(--md-sys-color-on-surface)] tabular-nums">{{ results.skipped.length }}</strong>
      </span>
      <span class="inline-flex items-center rounded-full bg-[var(--md-sys-color-surface-container-high)] px-3 py-1 font-medium text-[var(--md-sys-color-on-surface-variant)]">
        Warnings: <strong class="ml-1 text-[var(--md-sys-color-on-surface)] tabular-nums">{{ results.warnings.length }}</strong>
      </span>
    </div>

    <!-- Expandable Failed List -->
    <div
      v-if="results.fail.length > 0"
      class="flex flex-col rounded-[24px] border border-[var(--md-sys-color-error)]/30 bg-[var(--md-sys-color-surface-container-low)] overflow-hidden"
    >
      <button
        type="button"
        class="flex w-full cursor-pointer items-center justify-between p-4 text-xs font-semibold text-[var(--md-sys-color-error)] hover:bg-[var(--md-sys-color-error-container)]/20 transition-colors"
        aria-label="Toggle failed items list"
        @click="showFailedList = !showFailedList"
      >
        <div class="flex items-center gap-2">
          <AlertCircle class="size-4" />
          <span>View Failed Items ({{ results.fail.length }})</span>
        </div>
        <ChevronDown
          class="size-4 transition-transform duration-300 ease-[var(--md-sys-motion-easing-spatial)]"
          :class="showFailedList && 'rotate-180'"
        />
      </button>

      <div
        class="grid transition-[grid-template-rows] duration-300 ease-[var(--md-sys-motion-easing-spatial)]"
        :style="{ gridTemplateRows: showFailedList ? '1fr' : '0fr' }"
      >
        <div class="min-h-0 overflow-hidden">
          <div class="max-h-60 overflow-y-auto p-4 pt-0 space-y-2 border-t border-[var(--md-sys-color-outline-variant)]/20">
            <div
              v-for="(failItem, idx) in results.fail"
              :key="idx"
              class="flex items-start gap-2.5 rounded-xl bg-[var(--md-sys-color-surface-container-highest)]/50 p-2.5 text-xs text-[var(--md-sys-color-on-surface)]"
            >
              <XCircle class="size-4 shrink-0 text-[var(--md-sys-color-error)] mt-0.5" />
              <span class="break-all font-mono text-[11px] leading-relaxed">{{ failItem }}</span>
            </div>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>
