<script setup lang="ts">
import { onMounted, onUnmounted, ref, watch } from 'vue'
import type { Ref } from 'vue'
import type { UploadResults } from '../utils/uploadResults'
import M3Button from './ui/M3Button.vue'
import M3Icon from './ui/M3Icon.vue'

const props = defineProps<{
  results: UploadResults
}>()

const emit = defineEmits<{
  (e: 'copy'): void
  (e: 'clear'): void
  (e: 'retryAll'): void
  (e: 'retryItem', path: string): void
}>()

const showFailedList = ref(false)
const showSkippedList = ref(false)
const copied = ref(false)

const displaySuccess = ref(props.results.success.length)
const displayFail = ref(props.results.fail.length)
const displaySkipped = ref(props.results.skipped.length)

let animFrames: number[] = []

function animateNumber(target: number, currentRef: Ref<number>, duration = 400) {
  if (typeof window !== 'undefined' && window.matchMedia?.('(prefers-reduced-motion: reduce)')?.matches) {
    currentRef.value = target
    return
  }
  const start = currentRef.value
  const diff = target - start
  if (diff === 0) return
  if (Math.abs(diff) <= 2) {
    currentRef.value = target
    return
  }
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
  displaySkipped.value = 0
  animateNumber(props.results.success.length, displaySuccess)
  animateNumber(props.results.fail.length, displayFail)
  animateNumber(props.results.skipped.length, displaySkipped)
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

watch(() => props.results.skipped.length, (newVal) => {
  animateNumber(newVal, displaySkipped)
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
    <!-- Header: responsive wrap for compact windows -->
    <div class="flex flex-col sm:flex-row sm:items-center justify-between gap-3 sm:gap-4 flex-wrap">
      <div class="flex flex-col min-w-0">
        <h2 class="m3-headline-small text-[var(--md-sys-color-on-surface)] truncate">
          Upload Batch Results
        </h2>
        <span class="m3-body-medium">
          Summary of recent upload operations
        </span>
      </div>

      <!-- Connected Action Button Group (M3 Expressive) -->
      <md-button-group
        variant="connected"
        aria-label="Upload results actions"
        class="shrink-0 self-start sm:self-auto"
      >
        <!-- Tonal Copy as JSON Button -->
        <M3Button
          variant="tonal"
          size="s"
          :icon="copied ? 'check' : 'content_copy'"
          aria-label="Copy results as JSON"
          @click="onCopyClick"
        >
          <span>{{ copied ? 'Copied JSON!' : 'Copy as JSON' }}</span>
        </M3Button>

        <!-- Clear / New Upload button -->
        <M3Button
          variant="outlined"
          size="s"
          icon="replay"
          aria-label="Dismiss results and start new upload"
          @click="emit('clear')"
        >
          <span>New Upload</span>
        </M3Button>
      </md-button-group>
    </div>

    <!-- Stat Cards Side by Side (Tonal Containers) -->
    <div
      class="grid gap-4"
      :class="results.skipped.length > 0 ? 'grid-cols-1 sm:grid-cols-2 lg:grid-cols-3' : 'grid-cols-1 sm:grid-cols-2'"
    >
      <!-- Successful in Tertiary Container -->
      <div class="m3-stat-card-1 flex items-center gap-4 rounded-[24px] border border-[var(--md-sys-color-tertiary)]/30 bg-[var(--md-sys-color-tertiary-container)] p-5 text-[var(--md-sys-color-on-tertiary-container)] shadow-xs">
        <div class="flex size-12 shrink-0 items-center justify-center rounded-2xl bg-[var(--md-sys-color-on-tertiary-container)]/15">
          <M3Icon
            name="check_circle"
            :filled="true"
            :size="24"
            class="text-[var(--md-sys-color-on-tertiary-container)]"
          />
        </div>
        <div class="flex flex-col min-w-0">
          <span class="m3-display tabular-nums">
            {{ displaySuccess }}
          </span>
          <span class="m3-label-large opacity-90 leading-tight">
            Successful Uploads
          </span>
        </div>
      </div>

      <!-- Skipped / Already in Google Photos in Secondary Container -->
      <div
        v-if="results.skipped.length > 0"
        class="m3-stat-card-skipped flex items-center gap-4 rounded-[24px] border border-[var(--md-sys-color-secondary)]/30 bg-[var(--md-sys-color-secondary-container)] p-5 text-[var(--md-sys-color-on-secondary-container)] shadow-xs"
      >
        <div class="flex size-12 shrink-0 items-center justify-center rounded-2xl bg-[var(--md-sys-color-on-secondary-container)]/15">
          <M3Icon
            name="cloud_done"
            :filled="true"
            :size="24"
            class="text-[var(--md-sys-color-on-secondary-container)]"
          />
        </div>
        <div class="flex flex-col min-w-0">
          <span class="m3-display tabular-nums">
            {{ displaySkipped }}
          </span>
          <span class="m3-label-large opacity-90 leading-tight">
            Already in Google Photos
          </span>
        </div>
      </div>

      <!-- Failed: Error Container when failed > 0, Neutral Container when failed === 0 -->
      <div
        class="m3-stat-card-2 flex items-center gap-4 rounded-[24px] p-5 shadow-xs transition-colors duration-200"
        :class="results.fail.length > 0
          ? 'border border-[var(--md-sys-color-error)]/30 bg-[var(--md-sys-color-error-container)] text-[var(--md-sys-color-on-error-container)]'
          : 'border border-[var(--md-sys-color-outline-variant)]/40 bg-[var(--md-sys-color-surface-container-high)] text-[var(--md-sys-color-on-surface-variant)]'"
      >
        <div
          class="flex size-12 shrink-0 items-center justify-center rounded-2xl"
          :class="results.fail.length > 0
            ? 'bg-[var(--md-sys-color-on-error-container)]/15 text-[var(--md-sys-color-on-error-container)]'
            : 'bg-[var(--md-sys-color-surface-container-highest)] text-[var(--md-sys-color-on-surface-variant)]'"
        >
          <M3Icon
            :name="results.fail.length > 0 ? 'cancel' : 'task_alt'"
            :filled="true"
            :size="24"
          />
        </div>
        <div class="flex flex-col min-w-0">
          <span
            class="m3-display tabular-nums"
            :class="results.fail.length > 0 ? 'text-[var(--md-sys-color-on-error-container)]' : 'text-[var(--md-sys-color-on-surface)]'"
          >
            {{ displayFail }}
          </span>
          <span class="m3-label-large opacity-90 leading-tight">
            {{ results.fail.length > 0 ? 'Failed Uploads' : 'No Failures' }}
          </span>
        </div>
      </div>
    </div>

    <!-- Secondary stats chips (Skipped & Warnings) -->
    <div class="flex flex-wrap items-center gap-2 pt-1 m3-body-medium">
      <span class="inline-flex items-center rounded-full bg-[var(--md-sys-color-surface-container-high)] px-3 py-1 m3-label-large text-[var(--md-sys-color-on-surface-variant)]">
        Already in Google Photos: <strong class="ml-1 text-[var(--md-sys-color-on-surface)] tabular-nums">{{ results.skipped.length }}</strong>
      </span>
      <span class="inline-flex items-center rounded-full bg-[var(--md-sys-color-surface-container-high)] px-3 py-1 m3-label-large text-[var(--md-sys-color-on-surface-variant)]">
        Warnings: <strong class="ml-1 text-[var(--md-sys-color-on-surface)] tabular-nums">{{ results.warnings.length }}</strong>
      </span>
    </div>

    <!-- Expandable Skipped / Duplicate List -->
    <div
      v-if="results.skipped.length > 0"
      class="flex flex-col rounded-[24px] border border-[var(--md-sys-color-outline-variant)]/40 bg-[var(--md-sys-color-surface-container-low)] overflow-hidden"
    >
      <div class="flex items-center justify-between p-4 bg-[var(--md-sys-color-surface-container-high)]/40">
        <button
          type="button"
          class="flex flex-1 cursor-pointer items-center justify-between m3-title-medium text-[var(--md-sys-color-on-surface)] transition-colors pr-2"
          aria-label="Toggle skipped items list"
          @click="showSkippedList = !showSkippedList"
        >
          <div class="flex items-center gap-2">
            <M3Icon
              name="cloud_done"
              :filled="true"
              :size="16"
              class="text-[var(--md-sys-color-primary)]"
            />
            <span>Already in Google Photos ({{ results.skipped.length }})</span>
          </div>
          <M3Icon
            name="expand_more"
            :size="18"
            class="transition-transform duration-300 ease-[var(--md-sys-motion-easing-spatial)]"
            :class="showSkippedList && 'rotate-180'"
          />
        </button>
      </div>

      <div
        class="grid transition-[grid-template-rows] duration-300 ease-[var(--md-sys-motion-easing-spatial)]"
        :style="{ gridTemplateRows: showSkippedList ? '1fr' : '0fr' }"
      >
        <div class="min-h-0 overflow-hidden">
          <div class="max-h-64 overflow-y-auto p-4 pt-1 space-y-2 border-t border-[var(--md-sys-color-outline-variant)]/20">
            <div
              v-for="(skipItem, idx) in results.skipped"
              :key="idx"
              class="flex items-center justify-between gap-3 rounded-2xl bg-[var(--md-sys-color-surface-container-highest)]/60 p-3 border border-[var(--md-sys-color-outline-variant)]/20"
            >
              <div class="flex items-start gap-2.5 min-w-0 flex-1">
                <M3Icon
                  name="cloud_done"
                  :filled="true"
                  :size="16"
                  class="shrink-0 text-[var(--md-sys-color-primary)] mt-0.5"
                />
                <div class="flex flex-col min-w-0">
                  <span
                    class="m3-mono m3-body-medium text-[var(--md-sys-color-on-surface)] truncate"
                    :title="skipItem.paths?.[0] || 'Unknown file'"
                  >
                    {{ (skipItem.paths?.[0] || 'Unknown file').split(/[/\\]/).pop() }}
                  </span>
                  <span
                    class="m3-body-medium mt-0.5 leading-relaxed"
                  >
                    {{ skipItem.reason || 'Already in Google Photos' }}
                  </span>
                </div>
              </div>
              <span class="px-2 py-0.5 rounded-full bg-[var(--md-sys-color-secondary-container)] text-[var(--md-sys-color-on-secondary-container)] m3-label-small shrink-0">
                Already in library
              </span>
            </div>
          </div>
        </div>
      </div>
    </div>

    <!-- Expandable Failed List with Structured Records & Retry -->
    <div
      v-if="results.fail.length > 0"
      class="flex flex-col rounded-[24px] border border-[var(--md-sys-color-error)]/30 bg-[var(--md-sys-color-surface-container-low)] overflow-hidden"
    >
      <div class="flex items-center justify-between p-4 bg-[var(--md-sys-color-error-container)]/10">
        <button
          type="button"
          class="flex flex-1 cursor-pointer items-center justify-between m3-title-medium text-[var(--md-sys-color-error)] transition-colors pr-2"
          aria-label="Toggle failed items list"
          @click="showFailedList = !showFailedList"
        >
          <div class="flex items-center gap-2">
            <M3Icon
              name="warning"
              :filled="true"
              :size="16"
            />
            <span>View Failed Items ({{ results.fail.length }})</span>
          </div>
          <M3Icon
            name="expand_more"
            :size="18"
            class="transition-transform duration-300 ease-[var(--md-sys-motion-easing-spatial)]"
            :class="showFailedList && 'rotate-180'"
          />
        </button>

        <!-- Batch Retry Button -->
        <M3Button
          variant="filled"
          size="xs"
          icon="replay"
          class="shrink-0"
          aria-label="Retry all failed uploads"
          @click="emit('retryAll')"
        >
          <span>Retry All</span>
        </M3Button>
      </div>

      <div
        class="grid transition-[grid-template-rows] duration-300 ease-[var(--md-sys-motion-easing-spatial)]"
        :style="{ gridTemplateRows: showFailedList ? '1fr' : '0fr' }"
      >
        <div class="min-h-0 overflow-hidden">
          <div class="max-h-64 overflow-y-auto p-4 pt-1 space-y-2 border-t border-[var(--md-sys-color-outline-variant)]/20">
            <div
              v-for="(failItem, idx) in results.fail"
              :key="idx"
              class="flex items-center justify-between gap-3 rounded-2xl bg-[var(--md-sys-color-surface-container-highest)]/60 p-3 border border-[var(--md-sys-color-outline-variant)]/20"
            >
              <div class="flex items-start gap-2.5 min-w-0 flex-1">
                <M3Icon
                  name="cancel"
                  :filled="true"
                  :size="16"
                  class="shrink-0 text-[var(--md-sys-color-error)] mt-0.5"
                />
                <div class="flex flex-col min-w-0">
                  <span
                    class="m3-mono m3-body-medium text-[var(--md-sys-color-on-surface)] truncate"
                    :title="typeof failItem === 'string' ? failItem : failItem.path"
                  >
                    {{ typeof failItem === 'string' ? failItem : failItem.path }}
                  </span>
                  <span
                    v-if="typeof failItem !== 'string' && failItem.error"
                    class="m3-body-medium text-[var(--md-sys-color-error)] mt-0.5 leading-relaxed"
                  >
                    {{ failItem.error }}
                  </span>
                </div>
              </div>

              <!-- Actionable Per-Item Retry Button -->
              <md-icon-button
                icon="replay"
                size="xs"
                title="Retry this file"
                :aria-label="`Retry upload for ${typeof failItem === 'string' ? failItem : failItem.path}`"
                @click="emit('retryItem', typeof failItem === 'string' ? failItem : failItem.path)"
              />
            </div>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>
