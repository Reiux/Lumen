<script setup lang="ts">
import { computed, onUnmounted, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { uploadManager } from './utils/UploadManager'
import { formatErrorMessage, formatSkipReason, formatThreadMessage } from './utils/statusTranslation'
import M3WavyProgress from './components/ui/M3WavyProgress.vue'
import M3Icon from './components/ui/M3Icon.vue'
import M3LoadingIndicator from './components/ui/M3LoadingIndicator.vue'
import M3Button from './components/ui/M3Button.vue'
import CircuitBreakerAlert from './components/CircuitBreakerAlert.vue'

const { t } = useI18n()

const props = withDefaults(defineProps<{
  isDraggingFiles?: boolean
}>(), {
  isDraggingFiles: false,
})

const emit = defineEmits<{
  (event: 'open-sign-in'): void
  (event: 'account-removed'): void
  (event: 'pick-more'): void
}>()

const { state } = uploadManager
const isUploadBlocked = computed(() => state.accountBlocked.isBlocked)
const isLocalDragOver = ref(false)
const isQueueDropActive = computed(() => !isUploadBlocked.value && (props.isDraggingFiles || isLocalDragOver.value))

function onQueueDragEnter(e: DragEvent) {
  if (isUploadBlocked.value) {
    e.preventDefault()
    if (e.dataTransfer) e.dataTransfer.dropEffect = 'none'
    isLocalDragOver.value = false
    return
  }
  e.preventDefault()
  isLocalDragOver.value = true
}

function onQueueDragOver(e: DragEvent) {
  e.preventDefault()
  if (isUploadBlocked.value) {
    if (e.dataTransfer) e.dataTransfer.dropEffect = 'none'
    isLocalDragOver.value = false
    return
  }
  if (e.dataTransfer) e.dataTransfer.dropEffect = 'copy'
  isLocalDragOver.value = true
}

function onQueueDragLeave(e: DragEvent) {
  const current = e.currentTarget as HTMLElement | null
  const related = e.relatedTarget as Node | null
  if (current && related && current.contains(related)) return
  isLocalDragOver.value = false
}

function onQueueDrop() {
  isLocalDragOver.value = false
}

function handleQueueDropClick() {
  if (isUploadBlocked.value) return
  emit('pick-more')
}

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

onUnmounted(() => {
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
    return Math.max(state.totalFiles, state.uploadedFiles)
  }
  return threadsList.value.length + state.uploadedFiles
})

const progressPercent = computed(() => {
  if (state.totalBytes > 0) {
    return Math.min(100, Math.max(0, Math.round((state.uploadedBytes / state.totalBytes) * 100)))
  }
  const total = effectiveTotalFiles.value
  if (total === 0) return 0
  return Math.min(100, Math.max(0, Math.round((state.uploadedFiles / total) * 100)))
})

function formatBytes(bytes: number, decimals = 1): string {
  if (!Number.isFinite(bytes) || bytes <= 0) return '0 B'
  const k = 1024
  const sizes = ['B', 'KB', 'MB', 'GB', 'TB']
  const i = Math.min(sizes.length - 1, Math.floor(Math.log(bytes) / Math.log(k)))
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
  status: 'uploading' | 'committing' | 'hashing' | 'checking' | 'paused' | 'retrying' | 'done' | 'failed' | 'skipped' | 'queued'
  progressPercent?: number
  isQueued?: boolean
}

function getFileExtension(filename: string): string {
  const parts = filename.split('.')
  return parts.length > 1 ? parts.pop()!.toUpperCase() : 'FILE'
}

function getFileType(filename: string): string {
  const ext = getFileExtension(filename)
  if (['JPG', 'JPEG', 'PNG', 'WEBP', 'HEIC', 'AVIF'].includes(ext)) return t('upload.photoType', { ext })
  if (['RAW', 'CR2', 'NEF', 'ARW', 'DNG'].includes(ext)) return t('upload.rawType', { ext })
  if (['MP4', 'MOV', 'M4V', 'AVI', 'MKV'].includes(ext)) return t('upload.videoType', { ext })
  return t('upload.mediaType', { ext })
}

const queueList = computed<QueueRowItem[]>(() => {
  const list: QueueRowItem[] = []

  // 1. Active worker threads
  for (const thread of threadsList.value) {
    const rawStatus = (thread.Status || '').toLowerCase()
    let status: QueueRowItem['status']
    let progressPercent: number | undefined

    if (rawStatus.includes('commit') || rawStatus.includes('finaliz')) {
      status = 'committing'
    } else if (rawStatus.includes('pause')) {
      status = 'paused'
    } else if (rawStatus.includes('retry')) {
      status = 'retrying'
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

    const name = thread.FileName || thread.FilePath.split(/[/\\]/).pop() || t('thread.worker', { id: thread.WorkerID })
    const sizeStr = thread.BytesTotal > 0 ? formatBytes(thread.BytesTotal) : ''
    const typeStr = getFileType(name)
    const msg = formatThreadMessage(thread.Message, t)
    const detail = msg || (sizeStr ? `${sizeStr} • ${typeStr}` : typeStr)

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
    const name = path.split(/[/\\]/).pop() || t('upload.failedItem', { number: i + 1 })
    const errText = typeof item !== 'string' && item.error ? formatErrorMessage(item.error, t) : t('upload.failed')
    list.push({
      id: `fail-${i}-${name}`,
      name,
      detail: `${errText} • ${getFileType(name)}`,
      status: 'failed',
    })
  }

  // 3. Skipped items (already in Google Photos / duplicates)
  for (let i = 0; i < state.results.skipped.length; i++) {
    const item = state.results.skipped[i]
    const path = item.paths?.[0] || t('upload.unknownFile')
    const name = path.split(/[/\\]/).pop() || t('upload.skippedItem', { number: i + 1 })
    const reason = formatSkipReason(item.reason, item.code, t)
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
    const name = path.split(/[/\\]/).pop() || t('upload.backedUpItem', { number: i + 1 })
    list.push({
      id: `success-${i}-${name}`,
      name,
      detail: `${t('upload.completed')} • ${getFileType(name)}`,
      status: 'done',
    })
  }

  // 5. Queued items
  const activeCount = threadsList.value.filter((th) => {
    const s = (th.Status || '').toLowerCase()
    return (
      !s.includes('completed') &&
      !s.includes('done') &&
      !s.includes('skip') &&
      !s.includes('duplicate') &&
      !s.includes('fail') &&
      !s.includes('error')
    )
  }).length
  const completedCount = state.uploadedFiles
  const total = effectiveTotalFiles.value
  const remaining = Math.max(0, total - (completedCount + activeCount))

  // Cap total items to 200 items cleanly without DOM freeze
  const queuedToAdd = Math.min(remaining, Math.max(0, 200 - list.length))
  for (let i = 0; i < queuedToAdd; i++) {
    const idx = completedCount + activeCount + i + 1
    list.push({
      id: `queued-${idx}`,
      name: `${t('upload.queued')} #${idx}`,
      detail: t('upload.waitingInQueue'),
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
              {{ state.scanProgress.phase === 'preparing' ? t('backup.preparingTitle') : t('backup.indexingTitle') }}
            </span>
            <span class="m3-label-small px-2 py-0.5 rounded-full bg-[var(--md-sys-color-primary)] text-[var(--md-sys-color-on-primary)] tabular-nums">
              {{ t('backup.filesFound', { count: state.scanProgress.filesFound }) }}
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
            {{ state.scanProgress.phase === 'preparing' ? t('backup.preparingFiles') : t('backup.scanningFolders') }}
          </span>
        </div>
      </div>

      <M3Button
        variant="tonal"
        size="s"
        icon="close"
        @click="uploadManager.cancelScan()"
      >
        {{ t('backup.cancelIndexing') }}
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
            {{ state.isCreatingAlbum ? t('upload.addingToAlbum') : t('upload.albumComplete') }}
          </span>
        </div>
        <span class="m3-label-large text-[var(--md-sys-color-primary)] tabular-nums">
          {{ albumProgressPercent }}%
        </span>
      </div>
      <p class="m3-body-medium">
        {{ t('upload.albumSummary', { name: state.albumStatus.AlbumName, added: state.albumStatus.ItemsAdded, total: state.albumStatus.TotalItems }) }}
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

    <!-- Main Layout: clean flex on narrow/mobile, symmetrical 2-pane grid on >=860px -->
    <div class="flex flex-col min-[860px]:grid min-[860px]:grid-cols-[380px_1fr] min-[1280px]:grid-cols-[440px_1fr] gap-4 min-[860px]:gap-5 flex-1 min-h-0 min-[860px]:max-h-[calc(100vh-130px)] w-full min-[860px]:items-stretch">
      <!-- Left Column: Hero Summary Card + M3E Queue Drop Card below (flush height with File Queue) -->
      <div class="flex flex-col gap-4 w-full min-[860px]:h-full min-h-0 shrink-0">
        <!-- Summary Header Card (28px rounded, surface-container) -->
        <div class="bg-[var(--md-sys-color-surface-container)] rounded-[28px] p-5 sm:p-6 border border-[var(--md-sys-color-outline-variant)]/40 shadow-sm flex flex-col">
          <div class="flex items-start justify-between gap-3">
            <div class="min-w-0">
              <span class="m3-label-small text-[var(--md-sys-color-primary)] block">
                {{ state.isPaused ? t('upload.backupPaused') : t('upload.backingUpMedia') }}
              </span>
              <h2 class="text-2xl sm:text-3xl font-bold text-[var(--md-sys-color-on-surface)] mt-0.5 tabular-nums">
                {{ t('upload.itemsCount', { uploaded: displayUploadedFiles, total: effectiveTotalFiles }) }}
              </h2>
              <div
                v-if="state.results.skipped.length > 0 || state.results.fail.length > 0"
                class="flex items-center gap-1.5 mt-1 m3-body-medium"
              >
                <span v-if="state.results.success.length > 0">
                  {{ t('upload.completedCount', { count: state.results.success.length }) }}
                </span>
                <span v-if="state.results.success.length > 0 && state.results.skipped.length > 0">&bull;</span>
                <span v-if="state.results.skipped.length > 0" class="text-[var(--md-sys-color-secondary)]">
                  {{ t('upload.skippedCount', { count: state.results.skipped.length }) }}
                </span>
                <span v-if="state.results.fail.length > 0" class="text-[var(--md-sys-color-error)]">
                  <template v-if="state.results.success.length > 0 || state.results.skipped.length > 0">&bull; </template>{{ t('upload.failedCount', { count: state.results.fail.length }) }}
                </span>
              </div>
            </div>

            <!-- Action Buttons: Pause/Resume and Cancel -->
            <div class="flex items-center gap-2 shrink-0">
              <M3Button
                variant="tonal"
                size="s"
                :aria-label="state.isPaused ? t('upload.resume') : t('upload.pause')"
                class="shrink-0"
                @click="uploadManager.togglePause()"
              >
                <div class="flex items-center space-x-1.5">
                  <M3Icon :name="state.isPaused ? 'play_arrow' : 'pause'" :size="16" />
                  <span>{{ state.isPaused ? t('upload.resume') : t('upload.pause') }}</span>
                </div>
              </M3Button>

              <M3Button
                variant="tonal"
                size="s"
                destructive
                :disabled="isCancelling"
                :aria-label="t('upload.cancelBatchAria')"
                class="shrink-0"
                @click="handleCancel"
              >
                {{ isCancelling ? t('upload.cancelling') : t('upload.cancelBackup') }}
              </M3Button>
            </div>
          </div>

          <!-- Metrics Row (speed on left, ETA on right - stable line that never wraps) -->
          <div class="flex items-center justify-between gap-2 m3-body-medium text-[var(--md-sys-color-on-surface-variant)] mt-3 sm:mt-4">
            <div class="flex items-center space-x-1.5 min-w-0">
              <M3Icon
                name="bolt"
                :filled="true"
                :size="15"
                class="text-[var(--md-sys-color-primary)] shrink-0"
              />
              <span class="text-[var(--md-sys-color-on-surface)] tabular-nums">{{ speedDisplay }}</span>
              <span
                v-if="threadsList.length"
                class="m3-label-small text-[var(--md-sys-color-on-surface-variant)] normal-case truncate"
              >{{ t('upload.workersCount', { count: threadsList.length }) }}</span>
            </div>

            <div class="flex items-center space-x-1.5 shrink-0">
              <M3Icon
                name="schedule"
                :filled="false"
                :size="15"
                class="text-[var(--md-sys-color-primary)] shrink-0"
              />
              <span>{{ t('upload.eta', { time: etaDisplay }) }}</span>
            </div>
          </div>

          <!-- Progress Section: MB count on left, bold % on right, Wavy Progress Bar below -->
          <div class="mt-4 sm:mt-5 w-full flex flex-col gap-1.5">
            <div class="flex items-baseline justify-between gap-2">
              <span class="m3-body-medium text-[var(--md-sys-color-on-surface-variant)] tabular-nums">
                {{ bytesDisplay || `${formatBytes(state.uploadedBytes)} / ${formatBytes(state.totalBytes)}` }}
              </span>
              <span class="text-xl sm:text-2xl font-bold text-[var(--md-sys-color-primary)] tabular-nums tracking-tight">
                {{ progressPercent }}%
              </span>
            </div>
            <M3WavyProgress :value="progressPercent" />
          </div>
        </div>

        <!-- M3 Expressive Queue Drop Card (positioned beneath hero card, fills remaining column space) -->
        <div
          data-file-drop-target
          data-drop-zone="regular"
          role="button"
          :tabindex="isUploadBlocked ? -1 : 0"
          :aria-disabled="isUploadBlocked ? 'true' : 'false'"
          :aria-label="t('upload.dropMore')"
          class="m3-queue-drop-card group relative flex flex-1 min-h-[168px] w-full flex-col items-center justify-center gap-3 p-6 text-center select-none focus-visible:outline-[3px] focus-visible:outline-[var(--md-sys-color-primary)] focus-visible:outline-offset-2"
          :class="[
            isUploadBlocked
              ? 'is-disabled opacity-45 pointer-events-none cursor-not-allowed bg-[var(--md-sys-color-surface-container-low)] border-2 border-dashed border-[var(--md-sys-color-outline-variant)]'
              : isQueueDropActive
                ? 'is-drag-active cursor-copy bg-[var(--md-sys-color-primary-container)] text-[var(--md-sys-color-on-primary-container)] border-2 border-solid border-[var(--md-sys-color-primary)] ring-4 ring-[var(--md-sys-color-primary)]/20 shadow-lg'
                : 'cursor-pointer bg-[var(--md-sys-color-surface-container-low)] text-[var(--md-sys-color-on-surface-variant)] border-2 border-dashed border-[var(--md-sys-color-outline-variant)] hover:bg-[var(--md-sys-color-surface-container)] hover:border-[var(--md-sys-color-primary)]/70 hover:text-[var(--md-sys-color-on-surface)]',
          ]"
          @click="handleQueueDropClick"
          @keydown.enter.prevent="handleQueueDropClick"
          @keydown.space.prevent="handleQueueDropClick"
          @dragenter="onQueueDragEnter"
          @dragover="onQueueDragOver"
          @dragleave="onQueueDragLeave"
          @drop="onQueueDrop"
        >
          <div
            class="m3-queue-drop-icon flex size-14 shrink-0 items-center justify-center"
            :class="
              isQueueDropActive
                ? 'is-drag-active bg-[var(--md-sys-color-primary)] text-[var(--md-sys-color-on-primary)] shadow-md'
                : 'bg-[var(--md-sys-color-secondary-container)] text-[var(--md-sys-color-on-secondary-container)] group-hover:bg-[var(--md-sys-color-primary-container)] group-hover:text-[var(--md-sys-color-on-primary-container)]'
            "
          >
            <M3Icon
              :name="isQueueDropActive ? 'arrow_downward' : 'add_photo_alternate'"
              :filled="isQueueDropActive"
              :size="26"
            />
          </div>

          <div class="flex flex-col items-center gap-1 max-w-[300px]">
            <span
              class="m3-title-medium"
              :class="isQueueDropActive ? 'text-[var(--md-sys-color-on-primary-container)]' : 'text-[var(--md-sys-color-on-surface)]'"
            >
              {{ isQueueDropActive ? t('backup.releaseFiles') : t('upload.dropMore') }}
            </span>
            <span
              v-if="isQueueDropActive"
              class="m3-body-medium !text-[var(--md-sys-color-on-primary-container)]/85"
            >
              {{ t('backup.dropFiles') }}
            </span>
          </div>
        </div>
      </div>

      <!-- Right Column: File Queue List (fills the height and scrolls, symmetrical with left column) -->
      <div class="flex flex-col min-h-[260px] flex-1 min-[860px]:h-full max-h-[calc(100vh-320px)] min-[860px]:max-h-none bg-[var(--md-sys-color-surface-container)] rounded-[28px] border border-[var(--md-sys-color-outline-variant)]/40 overflow-hidden shadow-sm w-full">
        <div class="px-5 py-3 border-b border-[var(--md-sys-color-outline-variant)]/30 flex items-center justify-between m3-label-small text-[var(--md-sys-color-on-surface-variant)] shrink-0">
          <span>{{ t('upload.fileQueue') }}</span>
          <span>{{ t('upload.statusProgress') }}</span>
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
                <M3Icon
                  v-else-if="item.status === 'paused'"
                  name="pause_circle"
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
                {{ item.progressPercent !== undefined ? `${item.progressPercent}%` : t('upload.backingUp') }}
              </span>
              <span
                v-else-if="item.status === 'committing'"
                class="px-2.5 py-0.5 rounded-full bg-[var(--md-sys-color-surface-container-highest)] text-[var(--md-sys-color-primary)] m3-label-small normal-case animate-pulse"
              >
                {{ t('upload.committing') }}
              </span>
              <span
                v-else-if="item.status === 'hashing'"
                class="px-2.5 py-0.5 rounded-full bg-[var(--md-sys-color-surface-container-highest)] text-[var(--md-sys-color-primary)] m3-label-small normal-case animate-pulse"
              >
                {{ t('upload.hashing') }}
              </span>
              <span
                v-else-if="item.status === 'checking'"
                class="px-2.5 py-0.5 rounded-full bg-[var(--md-sys-color-surface-container-highest)] text-[var(--md-sys-color-primary)] m3-label-small normal-case animate-pulse"
              >
                {{ t('upload.checking') }}
              </span>
              <span
                v-else-if="item.status === 'paused'"
                class="px-2.5 py-0.5 rounded-full bg-[var(--md-sys-color-surface-container-highest)] text-[var(--md-sys-color-on-surface-variant)] m3-label-small normal-case"
              >
                {{ t('upload.paused') }}
              </span>
              <span
                v-else-if="item.status === 'retrying'"
                class="px-2.5 py-0.5 rounded-full bg-[var(--md-sys-color-tertiary-container)] text-[var(--md-sys-color-on-tertiary-container)] m3-label-small normal-case animate-pulse"
              >
                {{ t('upload.retrying') }}
              </span>
              <span
                v-else-if="item.status === 'done'"
                class="px-2.5 py-0.5 rounded-full bg-[var(--md-sys-color-success-container)] text-[var(--md-sys-color-on-success-container)] m3-label-small normal-case"
              >
                {{ t('upload.completed') }}
              </span>
              <span
                v-else-if="item.status === 'skipped'"
                class="px-2.5 py-0.5 rounded-full bg-[var(--md-sys-color-secondary-container)] text-[var(--md-sys-color-on-secondary-container)] m3-label-small normal-case"
              >
                {{ t('upload.alreadyInPhotos') }}
              </span>
              <span
                v-else-if="item.status === 'failed'"
                class="px-2.5 py-0.5 rounded-full bg-[var(--md-sys-color-error-container)] text-[var(--md-sys-color-on-error-container)] m3-label-small normal-case"
              >
                {{ t('upload.failed') }}
              </span>
              <span
                v-else-if="item.status === 'queued'"
                class="px-2.5 py-0.5 rounded-full bg-[var(--md-sys-color-surface-container-highest)] text-[var(--md-sys-color-on-surface-variant)] m3-label-small normal-case"
              >
                {{ t('upload.queued') }}
              </span>
            </div>
          </div>

          <div
            v-if="totalQueueCount > 200"
            class="px-5 py-3 text-center m3-body-medium bg-[var(--md-sys-color-surface-container-high)]/40"
          >
            {{ t('upload.moreInQueue', { count: totalQueueCount - 200 }) }}
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
/* M3 Expressive Queue Drop Card: spatial spring for transform/shape, effects spring (damping 1.0) for color/border */
.m3-queue-drop-card {
  border-radius: var(--md-sys-shape-corner-extra-large, 28px);
  transition:
    transform var(--md-sys-motion-duration-fast-spatial, 350ms) var(--md-sys-motion-easing-fast-spatial, cubic-bezier(0.42, 1.67, 0.21, 0.90)),
    border-radius var(--md-sys-motion-duration-fast-spatial, 350ms) var(--md-sys-motion-easing-fast-spatial, cubic-bezier(0.42, 1.67, 0.21, 0.90)),
    background-color var(--md-sys-motion-duration-default-effects, 200ms) var(--md-sys-motion-easing-default-effects, cubic-bezier(0.34, 0.80, 0.34, 1.00)),
    border-color var(--md-sys-motion-duration-default-effects, 200ms) var(--md-sys-motion-easing-default-effects, cubic-bezier(0.34, 0.80, 0.34, 1.00)),
    color var(--md-sys-motion-duration-default-effects, 200ms) var(--md-sys-motion-easing-default-effects, cubic-bezier(0.34, 0.80, 0.34, 1.00)),
    box-shadow var(--md-sys-motion-duration-default-effects, 200ms) var(--md-sys-motion-easing-default-effects, cubic-bezier(0.34, 0.80, 0.34, 1.00));
}

.m3-queue-drop-card:active:not(.is-disabled) {
  transform: scale(0.985);
  border-radius: var(--md-sys-shape-corner-large-increased, 20px);
}

.m3-queue-drop-card.is-drag-active {
  transform: scale(1.015);
  border-radius: var(--md-sys-shape-corner-extra-large-increased, 32px);
}

/* Nested corner radius: outer 28px -> inner icon tile 20px resting, morphing to full pill on drag-over */
.m3-queue-drop-icon {
  border-radius: var(--md-sys-shape-corner-large-increased, 20px);
  transition:
    transform var(--md-sys-motion-duration-fast-spatial, 350ms) var(--md-sys-motion-easing-fast-spatial, cubic-bezier(0.42, 1.67, 0.21, 0.90)),
    border-radius var(--md-sys-motion-duration-fast-spatial, 350ms) var(--md-sys-motion-easing-fast-spatial, cubic-bezier(0.42, 1.67, 0.21, 0.90)),
    background-color var(--md-sys-motion-duration-fast-effects, 150ms) var(--md-sys-motion-easing-fast-effects, cubic-bezier(0.31, 0.94, 0.34, 1.00)),
    color var(--md-sys-motion-duration-fast-effects, 150ms) var(--md-sys-motion-easing-fast-effects, cubic-bezier(0.31, 0.94, 0.34, 1.00));
}

/* Exact circle radius (28px for 56px box) instead of 9999px so the overshooting
   spatial spring never underflows border-radius to 0px (sharp corners) on release. */
.m3-queue-drop-icon.is-drag-active {
  border-radius: 28px;
  transform: scale(1.1) translateY(2px);
  animation: m3-drop-bob 900ms var(--md-sys-motion-easing-default-spatial, cubic-bezier(0.38, 1.21, 0.22, 1.00)) infinite alternate;
}

@keyframes m3-drop-bob {
  0% {
    transform: scale(1.08) translateY(-2px);
  }
  100% {
    transform: scale(1.12) translateY(4px);
  }
}

@media (prefers-reduced-motion: reduce) {
  .m3-queue-drop-card,
  .m3-queue-drop-icon {
    transition:
      background-color 120ms linear,
      border-color 120ms linear,
      color 120ms linear;
  }
  .m3-queue-drop-card.is-drag-active,
  .m3-queue-drop-card:active:not(.is-disabled),
  .m3-queue-drop-icon.is-drag-active {
    transform: none;
    animation: none;
  }
}
</style>
