<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, watch } from 'vue'
import { uploadManager } from './utils/UploadManager'
import M3WavyProgress from './components/ui/M3WavyProgress.vue'
import M3Icon from './components/ui/M3Icon.vue'
import M3LoadingIndicator from './components/ui/M3LoadingIndicator.vue'
import M3Button from './components/ui/M3Button.vue'
import CircuitBreakerAlert from './components/CircuitBreakerAlert.vue'

const emit = defineEmits<{
  (event: 'open-sign-in'): void
  (event: 'account-removed'): void
  (event: 'pick-more'): void
}>()

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

const effectiveTotalFiles = computed(() => {
  if (state.totalFiles > 0) {
    return Math.max(state.totalFiles, state.uploadedFiles + state.results.fail.length)
  }
  return threadsList.value.length + state.uploadedFiles + state.results.fail.length
})

const progressPercent = computed(() => {
  const total = effectiveTotalFiles.value
  if (total === 0) return 0
  return Math.min(100, Math.round((state.uploadedFiles / total) * 100))
})

function formatBytes(bytes: number, decimals = 1): string {
  if (bytes === 0) return '0 B'
  const k = 1024
  const sizes = ['B', 'KB', 'MB', 'GB']
  const i = Math.floor(Math.log(bytes) / Math.log(k))
  return `${parseFloat((bytes / Math.pow(k, i)).toFixed(decimals))} ${sizes[i]}`
}

const speedDisplay = computed(() => {
  if (state.uploadSpeed <= 0) return '--'
  return formatBytes(state.uploadSpeed) + '/s'
})

const etaDisplay = computed(() => {
  if (state.uploadSpeed <= 0) return '--'
  const remainingBytes = Math.max(0, state.totalBytes - state.uploadedBytes)
  if (remainingBytes <= 0) return '0s'
  const seconds = Math.round(remainingBytes / state.uploadSpeed)
  if (seconds < 60) return `${seconds}s`
  const mins = Math.floor(seconds / 60)
  const secs = seconds % 60
  if (mins < 60) return `${mins}m ${secs}s`
  const hours = Math.floor(mins / 60)
  const remMins = mins % 60
  return `${hours}h ${remMins}m`
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

interface QueueRowItem {
  id: string
  name: string
  detail: string
  status: 'uploading' | 'committing' | 'hashing' | 'checking' | 'done' | 'failed' | 'skipped' | 'queued'
  progressPercent?: number
  isQueued?: boolean
}

function getFileExtension(filename: string): string {
  const parts = filename.split('.')
  return parts.length > 1 ? parts.pop()!.toUpperCase() : 'FILE'
}

function getFileType(filename: string): string {
  const ext = getFileExtension(filename)
  if (['JPG', 'JPEG', 'PNG', 'WEBP', 'HEIC', 'AVIF'].includes(ext)) return `${ext} Photo`
  if (['RAW', 'CR2', 'NEF', 'ARW', 'DNG'].includes(ext)) return `${ext} RAW Image`
  if (['MP4', 'MOV', 'M4V', 'AVI', 'MKV'].includes(ext)) return `${ext} Video`
  return `${ext} Media`
}

const queueList = computed<QueueRowItem[]>(() => {
  const list: QueueRowItem[] = []

  // 1. Active worker threads
  for (const thread of threadsList.value) {
    const rawStatus = (thread.Status || '').toLowerCase()
    let status: QueueRowItem['status'] = 'uploading'
    let progressPercent: number | undefined

    if (rawStatus.includes('commit')) {
      status = 'committing'
    } else if (rawStatus.includes('hash')) {
      status = 'hashing'
    } else if (rawStatus.includes('check')) {
      status = 'checking'
    } else if (rawStatus.includes('skip') || rawStatus.includes('duplicate')) {
      status = 'skipped'
    } else if (rawStatus.includes('completed') || rawStatus.includes('done')) {
      status = 'done'
    } else if (rawStatus.includes('fail') || rawStatus.includes('error')) {
      status = 'failed'
    } else {
      status = 'uploading'
      if (thread.BytesTotal > 0) {
        progressPercent = Math.min(100, Math.round((thread.BytesUploaded / thread.BytesTotal) * 100))
      }
    }

    const name = thread.FileName || thread.FilePath.split(/[/\\]/).pop() || `Worker #${thread.WorkerID}`
    const sizeStr = thread.BytesTotal > 0 ? formatBytes(thread.BytesTotal) : ''
    const typeStr = getFileType(name)
    const detail = thread.Message || (sizeStr ? `${sizeStr} • ${typeStr}` : typeStr)

    list.push({
      id: `thread-${thread.WorkerID}`,
      name,
      detail,
      status,
      progressPercent,
    })
  }

  // 2. Failed items
  for (let i = 0; i < state.results.fail.length; i++) {
    const item = state.results.fail[i]
    const path = typeof item === 'string' ? item : item.path
    const name = path.split(/[/\\]/).pop() || `Failed Item #${i + 1}`
    list.push({
      id: `fail-${i}-${name}`,
      name,
      detail: `Upload failed • ${getFileType(name)}`,
      status: 'failed',
    })
  }

  // 3. Skipped items (already in Google Photos / duplicates)
  for (let i = 0; i < state.results.skipped.length; i++) {
    const item = state.results.skipped[i]
    const path = item.paths?.[0] || 'Unknown file'
    const name = path.split(/[/\\]/).pop() || `Skipped Item #${i + 1}`
    const reason = item.reason || 'Already in Google Photos'
    list.push({
      id: `skipped-${i}-${name}`,
      name,
      detail: `${reason} • ${getFileType(name)}`,
      status: 'skipped',
    })
  }

  // 4. Completed items (recent)
  for (let i = 0; i < state.results.success.length; i++) {
    const item = state.results.success[i]
    const path = typeof item === 'string' ? item : item.path
    const name = path.split(/[/\\]/).pop() || `Uploaded Item #${i + 1}`
    list.push({
      id: `success-${i}-${name}`,
      name,
      detail: `Uploaded • ${getFileType(name)}`,
      status: 'done',
    })
  }

  // 5. Queued items
  const activeCount = threadsList.value.length
  const completedCount = state.uploadedFiles
  const total = effectiveTotalFiles.value
  const remaining = Math.max(0, total - (completedCount + activeCount))

  // Cap total items to 200 items cleanly without DOM freeze
  const queuedToAdd = Math.min(remaining, Math.max(0, 200 - list.length))
  for (let i = 0; i < queuedToAdd; i++) {
    const idx = completedCount + activeCount + i + 1
    list.push({
      id: `queued-${idx}`,
      name: `Queue Item #${idx}`,
      detail: 'Waiting in queue',
      status: 'queued',
      isQueued: true,
    })
  }

  return list.slice(0, 200)
})

const totalQueueCount = computed(() => {
  return effectiveTotalFiles.value || queueList.value.length
})
</script>

<template>
  <div class="flex h-full w-full flex-col p-2 max-w-[1600px] mx-auto select-none gap-4">
    <!-- Persistent Circuit Breaker Alert Banner -->
    <CircuitBreakerAlert
      @open-sign-in="emit('open-sign-in')"
      @account-removed="emit('account-removed')"
    />

    <!-- Incremental Folder Scanning Card (Matching Lumen-Kotlin ScanState) -->
    <div
      v-if="state.scanProgress.isScanning"
      class="flex flex-col sm:flex-row sm:items-center justify-between gap-4 rounded-[28px] border border-[var(--md-sys-color-primary)]/40 bg-[var(--md-sys-color-primary-container)] p-5 shadow-sm"
    >
      <div class="flex items-center gap-3.5 min-w-0">
        <M3LoadingIndicator size="md" />
        <div class="flex flex-col min-w-0">
          <div class="flex items-center gap-2">
            <span class="m3-title-medium text-[var(--md-sys-color-on-primary-container)]">
              Indexing Media Folders...
            </span>
            <span class="m3-label-small px-2 py-0.5 rounded-full bg-[var(--md-sys-color-primary)] text-[var(--md-sys-color-on-primary)] tabular-nums">
              {{ state.scanProgress.filesFound }} items found
            </span>
          </div>
          <span
            v-if="state.scanProgress.currentDirectory"
            class="m3-body-medium text-[var(--md-sys-color-on-primary-container)]/85 truncate max-w-md"
            :title="state.scanProgress.currentDirectory"
          >
            {{ state.scanProgress.currentDirectory }}
          </span>
          <span
            v-else
            class="m3-body-medium text-[var(--md-sys-color-on-primary-container)]/85"
          >
            Scanning enabled folders for photos & videos...
          </span>
        </div>
      </div>

      <M3Button
        variant="tonal"
        size="s"
        icon="close"
        @click="uploadManager.cancelScan()"
      >
        Cancel Indexing
      </M3Button>
    </div>

    <!-- Album Creation Status Card (if applicable) -->
    <div
      v-if="state.albumStatus && (state.isCreatingAlbum || state.albumStatus.IsComplete)"
      class="flex flex-col gap-2 rounded-[24px] border border-[var(--md-sys-color-outline-variant)]/40 bg-[var(--md-sys-color-surface-container)] p-4 shadow-xs"
    >
      <div class="flex items-center justify-between">
        <div class="flex items-center gap-2 text-[var(--md-sys-color-primary)]">
          <M3LoadingIndicator
            v-if="state.isCreatingAlbum"
            size="xs"
            color="var(--md-sys-color-primary)"
          />
          <M3Icon
            v-else
            name="folder_special"
            :filled="true"
            :size="16"
          />
          <span class="m3-title-medium text-[var(--md-sys-color-on-surface)]">
            {{ state.isCreatingAlbum ? 'Adding to Google Photos Album...' : 'Album Complete' }}
          </span>
        </div>
        <span class="m3-label-large text-[var(--md-sys-color-primary)] tabular-nums">
          {{ albumProgressPercent }}%
        </span>
      </div>
      <p class="m3-body-medium">
        Album: <span class="m3-body-large text-[var(--md-sys-color-on-surface)]">{{ state.albumStatus.AlbumName }}</span>
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
        <M3Icon
          name="warning"
          :filled="true"
          :size="16"
          class="shrink-0 mt-0.5 text-amber-300"
        />
        <div class="flex flex-col min-w-0">
          <p class="m3-body-medium">
            {{ warning.Message }}
          </p>
          <p class="m3-label-small text-amber-200/70 truncate">
            {{ warningFiles(warning.Paths) }}
          </p>
        </div>
      </div>
    </div>

    <!-- Main Layout: clean flex on narrow/mobile, 2-pane grid on >=860px -->
    <div class="flex flex-col min-[860px]:grid min-[860px]:grid-cols-[380px_1fr] min-[1280px]:grid-cols-[440px_1fr] gap-4 min-[860px]:gap-5 flex-1 min-h-0 w-full min-[860px]:items-start">
      <!-- Left Column: Drop strip + Summary Card -->
      <div class="flex flex-col gap-3 sm:gap-4 w-full shrink-0">
        <!-- Slim drop strip -->
        <div
          data-file-drop-target
          data-drop-zone="regular"
          class="h-11 bg-[var(--md-sys-color-surface-container-low)] border border-dashed border-[var(--md-sys-color-outline)] rounded-2xl flex items-center justify-center space-x-2 m3-body-medium hover:border-[var(--md-sys-color-primary)] hover:text-[var(--md-sys-color-on-surface)] hover:bg-[var(--md-sys-color-surface-container)] cursor-pointer transition select-none flex-shrink-0"
          @click="emit('pick-more')"
        >
          <svg
            class="w-4 h-4 text-[var(--md-sys-color-primary)] shrink-0"
            fill="none"
            stroke="currentColor"
            viewBox="0 0 24 24"
          >
            <path
              stroke-linecap="round"
              stroke-linejoin="round"
              stroke-width="2"
              d="M12 4v16m8-8H4"
            />
          </svg>
          <span class="truncate px-2">Drop more files or folders here to add to the queue</span>
        </div>

        <!-- Summary Header Card (28px rounded, surface-container) -->
        <div class="bg-[var(--md-sys-color-surface-container)] rounded-[28px] p-5 sm:p-6 border border-[var(--md-sys-color-outline-variant)]/40 shadow-sm flex flex-col">
          <div class="flex items-start justify-between gap-3">
            <div class="min-w-0">
              <span class="m3-label-small text-[var(--md-sys-color-primary)] block">
                Uploading batch &bull; {{ progressPercent }}%
              </span>
              <h2 class="m3-display text-[var(--md-sys-color-on-surface)] mt-0.5 tabular-nums truncate">
                {{ displayUploadedFiles }} of {{ effectiveTotalFiles }} items
              </h2>
              <div
                v-if="state.results.skipped.length > 0 || state.results.fail.length > 0"
                class="flex items-center gap-1.5 mt-1 m3-body-medium"
              >
                <span v-if="state.results.success.length > 0">
                  {{ state.results.success.length }} uploaded
                </span>
                <span v-if="state.results.success.length > 0 && state.results.skipped.length > 0">&bull;</span>
                <span v-if="state.results.skipped.length > 0" class="text-[var(--md-sys-color-secondary)]">
                  {{ state.results.skipped.length }} already in Google Photos
                </span>
                <span v-if="state.results.fail.length > 0" class="text-[var(--md-sys-color-error)]">
                  &bull; {{ state.results.fail.length }} failed
                </span>
              </div>
            </div>

            <!-- Cancel Button -->
            <M3Button
              variant="tonal"
              size="s"
              destructive
              :disabled="isCancelling"
              aria-label="Cancel upload batch"
              class="shrink-0"
              @click="handleCancel"
            >
              {{ isCancelling ? 'Cancelling...' : 'Cancel upload' }}
            </M3Button>
          </div>

          <!-- Metrics Row (speed, ETA, transferred with small icons) -->
          <div class="flex flex-wrap items-center gap-x-4 gap-y-2 m3-body-medium mt-3">
            <div class="flex items-center space-x-1.5 shrink-0">
              <M3Icon
                name="bolt"
                :filled="true"
                :size="15"
                class="text-[var(--md-sys-color-primary)]"
              />
              <span class="text-[var(--md-sys-color-on-surface)] tabular-nums">{{ speedDisplay }}</span>
              <span
                v-if="threadsList.length"
                class="m3-label-small text-[var(--md-sys-color-on-surface-variant)] normal-case"
              >({{ threadsList.length }} workers)</span>
            </div>

            <div class="flex items-center space-x-1.5 shrink-0">
              <M3Icon
                name="schedule"
                :filled="false"
                :size="15"
                class="text-[var(--md-sys-color-primary)]"
              />
              <span>ETA: <strong class="text-[var(--md-sys-color-on-surface)] tabular-nums">{{ etaDisplay }}</strong></span>
            </div>

            <span class="hidden sm:inline text-[var(--md-sys-color-outline-variant)]">&bull;</span>
            <span class="tabular-nums shrink-0">{{ bytesDisplay || `${formatBytes(state.uploadedBytes)} / ${formatBytes(state.totalBytes)}` }}</span>
          </div>

          <!-- Existing Wavy Progress Bar Below -->
          <div class="mt-4 sm:mt-5 w-full pt-1">
            <M3WavyProgress :value="progressPercent" />
          </div>
        </div>
      </div>

      <!-- Right Column: File Queue List (fills the height and scrolls) -->
      <div class="flex flex-col min-h-0 flex-1 min-[860px]:h-full max-h-[calc(100vh-320px)] min-[860px]:max-h-[calc(100vh-130px)] bg-[var(--md-sys-color-surface-container)] rounded-[28px] border border-[var(--md-sys-color-outline-variant)]/40 overflow-hidden shadow-sm w-full">
        <div class="px-5 py-3 border-b border-[var(--md-sys-color-outline-variant)]/30 flex items-center justify-between m3-label-small text-[var(--md-sys-color-on-surface-variant)] shrink-0">
          <span>File Queue</span>
          <span>Status &bull; Progress</span>
        </div>

        <div class="divide-y divide-[var(--md-sys-color-outline-variant)]/20 overflow-y-auto flex-1">
          <div
            v-for="item in queueList"
            :key="item.id"
            class="px-5 py-3 flex items-center justify-between gap-3 hover:bg-[var(--md-sys-color-surface-container-high)] transition"
            :class="item.isQueued ? 'opacity-60' : ''"
          >
            <!-- Left: 36px icon tile + Name & details -->
            <div class="flex items-center space-x-3 min-w-0 pr-2">
              <div
                class="w-9 h-9 rounded-xl flex items-center justify-center flex-shrink-0 overflow-hidden"
                :class="
                  item.status === 'done'
                    ? 'bg-[var(--md-sys-color-success-container)] text-[var(--md-sys-color-on-success-container)]'
                    : item.status === 'skipped'
                      ? 'bg-[var(--md-sys-color-secondary-container)] text-[var(--md-sys-color-on-secondary-container)]'
                      : item.status === 'failed'
                        ? 'bg-[var(--md-sys-color-error-container)] text-[var(--md-sys-color-on-error-container)]'
                        : 'bg-[var(--md-sys-color-surface-container-highest)] text-[var(--md-sys-color-primary)]'
                "
              >
                <M3Icon
                  v-if="item.status === 'done'"
                  name="check"
                  :size="16"
                />
                <M3Icon
                  v-else-if="item.status === 'skipped'"
                  name="cloud_done"
                  :size="16"
                />
                <M3Icon
                  v-else-if="item.status === 'failed'"
                  name="warning"
                  :size="16"
                />
                <M3LoadingIndicator
                  v-else
                  size="xs"
                />
              </div>

              <div class="truncate min-w-0">
                <div
                  class="m3-title-medium text-[var(--md-sys-color-on-surface)] truncate"
                  :title="item.name"
                >
                  {{ item.name }}
                </div>
                <div class="m3-body-medium truncate">
                  {{ item.detail }}
                </div>
              </div>
            </div>

            <!-- Right: Progress bar & status chip -->
            <div class="flex items-center space-x-3 flex-shrink-0">
              <!-- Thin progress bar for uploading items -->
              <div
                v-if="item.status === 'uploading' && item.progressPercent !== undefined"
                class="w-24 bg-[var(--md-sys-color-surface-container-highest)] h-1.5 rounded-full overflow-hidden hidden sm:block"
              >
                <div
                  class="bg-[var(--md-sys-color-primary)] h-full rounded-full transition-all duration-200"
                  :style="{ width: `${item.progressPercent}%` }"
                />
              </div>

              <!-- Chips -->
              <span
                v-if="item.status === 'uploading'"
                class="px-2.5 py-0.5 rounded-full bg-[var(--md-sys-color-primary-container)] text-[var(--md-sys-color-on-primary-container)] m3-label-small tabular-nums normal-case"
              >
                {{ item.progressPercent !== undefined ? `${item.progressPercent}%` : 'Uploading' }}
              </span>
              <span
                v-else-if="item.status === 'committing'"
                class="px-2.5 py-0.5 rounded-full bg-[var(--md-sys-color-surface-container-highest)] text-[var(--md-sys-color-primary)] m3-label-small normal-case animate-pulse"
              >
                Committing...
              </span>
              <span
                v-else-if="item.status === 'hashing'"
                class="px-2.5 py-0.5 rounded-full bg-[var(--md-sys-color-surface-container-highest)] text-[var(--md-sys-color-primary)] m3-label-small normal-case animate-pulse"
              >
                Hashing...
              </span>
              <span
                v-else-if="item.status === 'checking'"
                class="px-2.5 py-0.5 rounded-full bg-[var(--md-sys-color-surface-container-highest)] text-[var(--md-sys-color-primary)] m3-label-small normal-case animate-pulse"
              >
                Checking...
              </span>
              <span
                v-else-if="item.status === 'done'"
                class="px-2.5 py-0.5 rounded-full bg-[var(--md-sys-color-success-container)] text-[var(--md-sys-color-on-success-container)] m3-label-small normal-case"
              >
                Uploaded
              </span>
              <span
                v-else-if="item.status === 'skipped'"
                class="px-2.5 py-0.5 rounded-full bg-[var(--md-sys-color-secondary-container)] text-[var(--md-sys-color-on-secondary-container)] m3-label-small normal-case"
              >
                Already in Google Photos
              </span>
              <span
                v-else-if="item.status === 'failed'"
                class="px-2.5 py-0.5 rounded-full bg-[var(--md-sys-color-error-container)] text-[var(--md-sys-color-on-error-container)] m3-label-small normal-case"
              >
                Failed
              </span>
              <span
                v-else-if="item.status === 'queued'"
                class="px-2.5 py-0.5 rounded-full bg-[var(--md-sys-color-surface-container-highest)] text-[var(--md-sys-color-on-surface-variant)] m3-label-small normal-case"
              >
                Queued
              </span>
            </div>
          </div>

          <div
            v-if="totalQueueCount > 200"
            class="px-5 py-3 text-center m3-body-medium bg-[var(--md-sys-color-surface-container-high)]/40"
          >
            + {{ totalQueueCount - 200 }} more items in batch queue
          </div>
        </div>
      </div>
    </div>
  </div>
</template>
