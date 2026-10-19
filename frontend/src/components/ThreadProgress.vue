<script setup lang="ts">
import { computed } from 'vue'
import type { ThreadStatus } from '../utils/UploadManager'

const props = defineProps<{
  thread: ThreadStatus
}>()

const statusConfig = computed(() => {
  switch (props.thread.Status) {
    case 'error':
      return {
        color: 'text-[var(--md-sys-color-on-error-container)]',
        bg: 'bg-[var(--md-sys-color-error-container)]',
        label: 'Error',
      }
    case 'completed':
      return {
        color: 'text-[var(--md-sys-color-on-tertiary-container)]',
        bg: 'bg-[var(--md-sys-color-tertiary-container)]',
        label: 'Done',
      }
    case 'skipped':
      return {
        color: 'text-amber-200',
        bg: 'bg-amber-500/20',
        label: 'Skipped',
      }
    case 'uploading':
      if (props.thread.Attempt > 1) {
        return {
          color: 'text-orange-200',
          bg: 'bg-orange-500/20',
          label: `Retry #${props.thread.Attempt - 1}`,
        }
      }
      return {
        color: 'text-[var(--md-sys-color-on-primary-container)]',
        bg: 'bg-[var(--md-sys-color-primary-container)]',
        label: 'Uploading',
      }
    case 'hashing':
      return {
        color: 'text-cyan-200',
        bg: 'bg-cyan-500/20',
        label: 'Hashing',
      }
    case 'checking':
      return {
        color: 'text-yellow-200',
        bg: 'bg-yellow-500/20',
        label: 'Checking',
      }
    case 'finalizing':
      return {
        color: 'text-purple-200',
        bg: 'bg-purple-500/20',
        label: 'Finalizing',
      }
    default:
      return {
        color: 'text-[var(--md-sys-color-on-surface-variant)]',
        bg: 'bg-[var(--md-sys-color-surface-container-high)]',
        label: props.thread.Status,
      }
  }
})

const uploadPercent = computed(() => {
  if (props.thread.Status !== 'uploading' || !props.thread.BytesTotal) {
    return null
  }
  return Math.round((props.thread.BytesUploaded / props.thread.BytesTotal) * 100)
})

function formatBytes(bytes: number): string {
  if (bytes === 0) return '0 B'
  const k = 1024
  const sizes = ['B', 'KB', 'MB', 'GB']
  const i = Math.floor(Math.log(bytes) / Math.log(k))
  return parseFloat((bytes / Math.pow(k, i)).toFixed(1)) + ' ' + sizes[i]
}

const fileSizeDisplay = computed(() => {
  if (props.thread.Status !== 'uploading' || !props.thread.BytesTotal) {
    return null
  }
  return `${formatBytes(props.thread.BytesUploaded)} / ${formatBytes(props.thread.BytesTotal)}`
})

const statusMessage = computed(() => props.thread.Message.replace(/^Error:\s*/, ''))
</script>

<template>
  <div class="flex flex-col gap-2 rounded-[20px] border border-[var(--md-sys-color-outline-variant)]/40 bg-[var(--md-sys-color-surface-container)] p-3.5 shadow-xs transition-colors select-none">
    <div class="flex items-center justify-between gap-2">
      <span class="rounded-full bg-[var(--md-sys-color-surface-container-high)] px-2 py-0.5 text-[10px] font-bold text-[var(--md-sys-color-on-surface-variant)]">
        Worker #{{ thread.WorkerID }}
      </span>
      <span
        class="inline-flex items-center rounded-full px-2.5 py-0.5 text-[10px] font-semibold tracking-wide uppercase"
        :class="[statusConfig.bg, statusConfig.color]"
      >
        {{ statusConfig.label }}
      </span>
    </div>

    <div class="flex flex-col gap-0.5 min-w-0">
      <span
        class="truncate text-xs font-semibold text-[var(--md-sys-color-on-surface)]"
        :title="thread.FileName || thread.FilePath"
      >
        {{ thread.FileName || 'Processing...' }}
      </span>
      <span
        v-if="statusMessage"
        class="truncate text-[11px] text-[var(--md-sys-color-on-surface-variant)]"
      >
        {{ statusMessage }}
      </span>
    </div>

    <!-- Mini Progress for thread -->
    <div
      v-if="uploadPercent !== null"
      class="flex flex-col gap-1 pt-0.5"
    >
      <div class="relative h-1.5 w-full overflow-hidden rounded-full bg-[var(--md-sys-color-surface-container-highest)]">
        <div
          class="h-full rounded-full bg-[var(--md-sys-color-primary)] transition-all duration-200"
          :style="{ width: `${uploadPercent}%` }"
        />
      </div>
      <div class="flex items-center justify-between text-[10px] text-[var(--md-sys-color-on-surface-variant)]">
        <span>{{ fileSizeDisplay }}</span>
        <span class="font-bold">{{ uploadPercent }}%</span>
      </div>
    </div>
  </div>
</template>
