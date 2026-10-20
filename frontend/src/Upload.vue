<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, watch } from 'vue'
import { AlertTriangle, Clock, FolderPlus, X, Zap } from '@lucide/vue'
import { uploadManager } from './utils/UploadManager'
import ThreadProgress from './components/ThreadProgress.vue'
import M3WavyProgress from './components/ui/M3WavyProgress.vue'
import Button from './components/ui/button/Button.vue'
import { ScrollArea } from './components/ui/scroll-area'

const { state } = uploadManager

// Elapsed time ticker
const elapsedSeconds = ref(0)
let elapsedInterval: ReturnType<typeof setInterval> | null = null

// Smooth count tween for uploaded files
const displayUploadedFiles = ref(state.uploadedFiles)
let countAnimationId: number | null = null

watch(() => state.uploadedFiles, (newVal) => {
  if (countAnimationId) cancelAnimationFrame(countAnimationId)
  if (typeof window !== 'undefined' && window.matchMedia?.('(prefers-reduced-motion: reduce)')?.matches) {
    displayUploadedFiles.value = newVal
    return
  }
  const startVal = displayUploadedFiles.value
  const diff = newVal - startVal
  if (diff === 0) return
  if (Math.abs(diff) <= 2) {
    displayUploadedFiles.value = newVal
    return
  }
  const startTime = performance.now()
  const duration = 250 // ms

  function step(now: number) {
    const elapsed = now - startTime
    const progress = Math.min(elapsed / duration, 1)
    // ease-out cubic
    const ease = 1 - Math.pow(1 - progress, 3)
    displayUploadedFiles.value = Math.round(startVal + diff * ease)
    if (progress < 1) {
      countAnimationId = requestAnimationFrame(step)
    } else {
      displayUploadedFiles.value = newVal
    }
  }
  countAnimationId = requestAnimationFrame(step)
})

onMounted(() => {
  elapsedInterval = setInterval(() => {
    if (state.startTime > 0) {
      elapsedSeconds.value = Math.floor((Date.now() - state.startTime) / 1000)
    }
  }, 1000)
})

onUnmounted(() => {
  if (elapsedInterval) {
    clearInterval(elapsedInterval)
  }
  if (countAnimationId) {
    cancelAnimationFrame(countAnimationId)
  }
})

const threadsList = computed(() => {
  return Array.from(state.threads.values())
    .filter(thread => thread.Status !== 'idle')
    .sort((a, b) => a.WorkerID - b.WorkerID)
})

const progressPercent = computed(() => {
  if (state.totalFiles === 0) return 0
  return Math.round((state.uploadedFiles / state.totalFiles) * 100)
})

function formatBytes(bytes: number, decimals = 1): string {
  if (bytes === 0) return '0 B'
  const k = 1024
  const sizes = ['B', 'KB', 'MB', 'GB']
  const i = Math.floor(Math.log(bytes) / Math.log(k))
  return parseFloat((bytes / Math.pow(k, i)).toFixed(decimals)) + ' ' + sizes[i]
}

const speedDisplay = computed(() => {
  if (state.uploadSpeed <= 0) return '--'
  return formatBytes(state.uploadSpeed) + '/s'
})

const elapsedDisplay = computed(() => {
  const seconds = elapsedSeconds.value
  if (seconds < 60) return `${seconds}s`
  const mins = Math.floor(seconds / 60)
  const secs = seconds % 60
  if (mins < 60) return `${mins}m ${secs}s`
  const hours = Math.floor(mins / 60)
  const remainingMins = mins % 60
  return `${hours}h ${remainingMins}m`
})

const bytesDisplay = computed(() => {
  if (state.totalBytes === 0) return ''
  return `${formatBytes(state.uploadedBytes)} / ${formatBytes(state.totalBytes)}`
})

const albumProgressPercent = computed(() => {
  if (!state.albumStatus || state.albumStatus.TotalItems === 0) return 0
  return Math.round((state.albumStatus.ItemsAdded / state.albumStatus.TotalItems) * 100)
})

function warningFiles(paths: string[]): string {
  return paths.map(path => path.split(/[\\/]/).pop()).filter(Boolean).join(', ')
}

const isCancelling = ref(false)

async function handleCancel() {
  if (isCancelling.value) return
  isCancelling.value = true
  try {
    await uploadManager.cancelUpload()
  } finally {
    setTimeout(() => {
      isCancelling.value = false
    }, 600)
  }
}
</script>

<template>
  <div class="flex h-full w-full flex-col p-2 max-w-5xl mx-auto select-none gap-5">
    <!-- Top Progress Card -->
    <div class="flex flex-col gap-4 rounded-[32px] border border-[var(--md-sys-color-outline-variant)]/40 bg-[var(--md-sys-color-surface-container)] p-6 shadow-sm">
      <div class="flex flex-col sm:flex-row sm:items-center sm:justify-between gap-4">
        <!-- Large N of M Counter -->
        <div class="flex flex-col">
          <div class="flex items-baseline gap-2">
            <span class="text-4xl sm:text-5xl font-extrabold tracking-tight tabular-nums text-[var(--md-sys-color-on-surface)]">
              {{ displayUploadedFiles }}
            </span>
            <span class="text-2xl sm:text-3xl font-normal text-[var(--md-sys-color-on-surface-variant)]">
              / {{ state.totalFiles }}
            </span>
          </div>
          <span class="text-xs font-semibold uppercase tracking-wider text-[var(--md-sys-color-primary)] mt-1">
            Items Processed &bull; {{ progressPercent }}%
          </span>
        </div>

        <!-- Pill-shaped Tonal Error Cancel Button -->
        <Button
          type="button"
          class="h-10 rounded-full px-6 text-xs font-semibold shadow-xs cursor-pointer select-none bg-[var(--md-sys-color-error-container)] text-[var(--md-sys-color-on-error-container)] hover:bg-[var(--md-sys-color-error-container)]/80 self-start sm:self-auto"
          aria-label="Cancel current upload batch"
          :loading="isCancelling"
          @click="handleCancel"
        >
          <X
            v-if="!isCancelling"
            class="size-4 mr-1.5"
          />
          <span>{{ isCancelling ? 'Cancelling...' : 'Cancel Upload' }}</span>
        </Button>
      </div>

      <!-- Wavy Linear Progress Indicator -->
      <div class="w-full pt-1">
        <M3WavyProgress :value="progressPercent" />
      </div>

      <!-- Stats Bar (Chips) -->
      <div class="flex flex-wrap items-center gap-2 pt-1 text-xs text-[var(--md-sys-color-on-surface-variant)]">
        <span class="inline-flex items-center gap-1.5 rounded-full bg-[var(--md-sys-color-surface-container-high)] px-3 py-1 font-medium">
          <Zap class="size-3.5 text-[var(--md-sys-color-primary)]" />
          <span class="tabular-nums text-[var(--md-sys-color-on-surface)] font-semibold">{{ speedDisplay }}</span>
        </span>
        <span class="inline-flex items-center gap-1.5 rounded-full bg-[var(--md-sys-color-surface-container-high)] px-3 py-1 font-medium">
          <Clock class="size-3.5 text-[var(--md-sys-color-primary)]" />
          <span class="tabular-nums text-[var(--md-sys-color-on-surface)] font-semibold">{{ elapsedDisplay }}</span>
        </span>
        <span
          v-if="bytesDisplay"
          class="inline-flex items-center gap-1.5 rounded-full bg-[var(--md-sys-color-surface-container-high)] px-3 py-1 font-medium"
        >
          <span class="tabular-nums text-[var(--md-sys-color-on-surface)]">{{ bytesDisplay }}</span>
        </span>
      </div>
    </div>

    <!-- Album Creation Status Card (if applicable) -->
    <div
      v-if="state.albumStatus && (state.isCreatingAlbum || state.albumStatus.IsComplete)"
      class="flex flex-col gap-2 rounded-[24px] border border-[var(--md-sys-color-outline-variant)]/40 bg-[var(--md-sys-color-surface-container)] p-4 shadow-xs"
    >
      <div class="flex items-center justify-between">
        <div class="flex items-center gap-2 text-[var(--md-sys-color-primary)]">
          <FolderPlus class="size-4" />
          <span class="text-xs font-semibold text-[var(--md-sys-color-on-surface)]">
            {{ state.isCreatingAlbum ? 'Adding to Google Photos Album...' : 'Album Complete' }}
          </span>
        </div>
        <span class="text-xs font-bold text-[var(--md-sys-color-primary)] tabular-nums">
          {{ albumProgressPercent }}%
        </span>
      </div>
      <p class="text-xs text-[var(--md-sys-color-on-surface-variant)]">
        Album: <span class="font-medium text-[var(--md-sys-color-on-surface)]">{{ state.albumStatus.AlbumName }}</span>
        ({{ state.albumStatus.ItemsAdded }} of {{ state.albumStatus.TotalItems }} items added)
      </p>
      <div class="relative h-2 w-full overflow-hidden rounded-full bg-[var(--md-sys-color-surface-container-highest)] mt-1">
        <div
          class="h-full rounded-full bg-[var(--md-sys-color-secondary)] transition-[width] duration-300"
          :style="{ width: `${albumProgressPercent}%` }"
        />
      </div>
    </div>

    <!-- Warnings List -->
    <div
      v-if="state.warnings.length"
      class="space-y-2"
    >
      <div
        v-for="(warning, index) in state.warnings"
        :key="`${warning.Code}-${index}`"
        class="flex items-start gap-2.5 rounded-[20px] border border-amber-500/30 bg-amber-500/10 p-3 text-amber-200"
      >
        <AlertTriangle class="size-4 shrink-0 mt-0.5 text-amber-300" />
        <div class="flex flex-col min-w-0">
          <p class="text-xs font-medium">
            {{ warning.Message }}
          </p>
          <p class="text-[10px] text-amber-200/70 truncate">
            {{ warningFiles(warning.Paths) }}
          </p>
        </div>
      </div>
    </div>

    <!-- Active Threads (Responsive Grid) -->
    <div class="flex flex-1 flex-col gap-3 min-h-0">
      <div class="flex items-center justify-between px-1">
        <h3 class="text-sm font-semibold text-[var(--md-sys-color-on-surface)]">
          Active Worker Threads ({{ threadsList.length }})
        </h3>
        <span class="text-xs text-[var(--md-sys-color-on-surface-variant)]">
          Dynamic concurrency
        </span>
      </div>

      <ScrollArea class="flex-1 pr-2">
        <div class="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-3.5 pb-4">
          <ThreadProgress
            v-for="(thread, index) in threadsList"
            :key="thread.WorkerID"
            :thread="thread"
            class="m3-thread-card"
            :style="{ '--thread-index': index }"
          />
        </div>
      </ScrollArea>
    </div>
  </div>
</template>
