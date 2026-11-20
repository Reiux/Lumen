<script setup lang="ts">
import { onMounted, onUnmounted, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import type { Ref } from 'vue'
import type { UploadResults } from '../utils/uploadResults'
import { formatErrorMessage, formatSkipReason } from '../utils/statusTranslation'
import M3Button from './ui/M3Button.vue'
import M3Icon from './ui/M3Icon.vue'

const { t } = useI18n()

const props = withDefaults(defineProps<{
  results: UploadResults
  isDryRun?: boolean
}>(), {
  isDryRun: false,
})

const emit = defineEmits<{
  (e: 'copy'): void
  (e: 'clear'): void
  (e: 'retryAll'): void
  (e: 'retryItem', path: string): void
  (e: 'startUpload'): void
}>()

const showCandidateList = ref(true)
const showFailedList = ref(false)
const showSkippedList = ref(false)
const copied = ref(false)

const displaySuccess = ref(props.results.success.length)
const displayFail = ref(props.results.fail.length)
const displaySkipped = ref(props.results.skipped.length)

const activeAnimFrames = new Map<Ref<number>, number>()
let copyResetTimer: ReturnType<typeof setTimeout> | null = null

function animateNumber(target: number, currentRef: Ref<number>, duration = 400) {
  const prevId = activeAnimFrames.get(currentRef)
  if (prevId !== undefined) {
    cancelAnimationFrame(prevId)
    activeAnimFrames.delete(currentRef)
  }
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
      activeAnimFrames.set(currentRef, id)
    } else {
      currentRef.value = target
      activeAnimFrames.delete(currentRef)
    }
  }
  const id = requestAnimationFrame(step)
  activeAnimFrames.set(currentRef, id)
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
  activeAnimFrames.forEach(id => cancelAnimationFrame(id))
  activeAnimFrames.clear()
  if (copyResetTimer) {
    clearTimeout(copyResetTimer)
    copyResetTimer = null
  }
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
  if (copyResetTimer) clearTimeout(copyResetTimer)
  copyResetTimer = setTimeout(() => {
    copied.value = false
    copyResetTimer = null
  }, 1500)
}
</script>

<template>
  <div class="flex w-full flex-col gap-5 rounded-[32px] border border-[var(--md-sys-color-outline-variant)]/40 bg-[var(--md-sys-color-surface-container)] p-6 shadow-sm select-none">
    <!-- Header: responsive wrap for compact windows -->
    <div class="flex flex-col sm:flex-row sm:items-center justify-between gap-3 sm:gap-4 flex-wrap">
      <div class="flex flex-col min-w-0">
        <h2 class="m3-headline-small text-[var(--md-sys-color-on-surface)] truncate">
          {{ isDryRun ? t('results.dryRunTitle') : t('results.backedUpTitle') }}
        </h2>
        <span class="m3-body-medium">
          {{ isDryRun ? t('results.dryRunDesc') : t('results.backedUpDesc') }}
        </span>
      </div>

      <!-- Action Button Group -->
      <div class="flex items-center gap-2 shrink-0 self-start sm:self-auto flex-wrap">
        <!-- In Dry Run: Primary Start Backup Button -->
        <M3Button
          v-if="isDryRun && results.success.length > 0"
          variant="filled"
          size="s"
          icon="cloud_upload"
          :aria-label="t('results.startBackupAria')"
          @click="emit('startUpload')"
        >
          <span>{{ t('results.startBackup', { count: results.success.length }) }}</span>
        </M3Button>

        <!-- Tonal Copy as JSON Button -->
        <M3Button
          variant="tonal"
          size="s"
          :icon="copied ? 'check' : 'content_copy'"
          :aria-label="t('results.copyJsonAria')"
          @click="onCopyClick"
        >
          <span>{{ copied ? t('results.copiedJson') : t('results.copyJson') }}</span>
        </M3Button>

        <!-- Clear / Back Up More button -->
        <M3Button
          variant="outlined"
          size="s"
          :icon="isDryRun ? 'close' : 'replay'"
          :aria-label="isDryRun ? t('results.dismissDryRunAria') : t('results.dismissResultsAria')"
          @click="emit('clear')"
        >
          <span>{{ isDryRun ? t('results.cancel') : t('results.backUpMore') }}</span>
        </M3Button>
      </div>
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
            :name="isDryRun ? 'cloud_upload' : 'check_circle'"
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
            {{ isDryRun ? t('results.readyToUpload') : t('results.completed') }}
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
            {{ t('results.alreadyInPhotos') }}
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
            {{ results.fail.length > 0 ? t('results.failed') : t('results.noFailures') }}
          </span>
        </div>
      </div>
    </div>

    <!-- Secondary stats chips (Skipped & Warnings) -->
    <div class="flex flex-wrap items-center gap-2 pt-1 m3-body-medium">
      <span class="inline-flex items-center rounded-full bg-[var(--md-sys-color-surface-container-high)] px-3 py-1 m3-label-large text-[var(--md-sys-color-on-surface-variant)]">
        {{ t('results.alreadyInPhotosCount', { count: results.skipped.length }) }}
      </span>
      <span class="inline-flex items-center rounded-full bg-[var(--md-sys-color-surface-container-high)] px-3 py-1 m3-label-large text-[var(--md-sys-color-on-surface-variant)]">
        {{ t('results.warningsCount', { count: results.warnings.length }) }}
      </span>
    </div>

    <!-- Expandable Candidate / Ready to Upload List (Dry Run) -->
    <div
      v-if="isDryRun && results.success.length > 0"
      class="flex flex-col rounded-[24px] border border-[var(--md-sys-color-primary)]/30 bg-[var(--md-sys-color-surface-container-low)] overflow-hidden"
    >
      <div class="flex items-center justify-between p-4 bg-[var(--md-sys-color-primary-container)]/20">
        <button
          type="button"
          class="flex flex-1 cursor-pointer items-center justify-between m3-title-medium text-[var(--md-sys-color-on-surface)] transition-colors pr-2"
          :aria-label="t('results.toggleCandidatesAria')"
          @click="showCandidateList = !showCandidateList"
        >
          <div class="flex items-center gap-2">
            <M3Icon
              name="cloud_upload"
              :filled="true"
              :size="16"
              class="text-[var(--md-sys-color-primary)]"
            />
            <span>{{ t('results.readyCount', { count: results.success.length }) }}</span>
          </div>
          <M3Icon
            name="expand_more"
            :size="18"
            class="text-[var(--md-sys-color-outline)] transition-transform duration-200"
            :class="{ 'rotate-180': showCandidateList }"
          />
        </button>
      </div>

      <div
        v-if="showCandidateList"
        class="flex flex-col divide-y divide-[var(--md-sys-color-outline-variant)]/20 max-h-60 overflow-y-auto"
      >
        <div
          v-for="(item, idx) in results.success"
          :key="idx"
          class="flex items-center justify-between p-3.5 hover:bg-[var(--md-sys-color-surface-container-high)]/30 transition-colors"
        >
          <div class="flex flex-col min-w-0 pr-3">
            <span class="m3-label-large text-[var(--md-sys-color-on-surface)] truncate font-mono">
              {{ item.path?.split(/[/\\]/).pop() || t('results.mediaFile') }}
            </span>
            <span class="m3-body-small text-[var(--md-sys-color-outline)] truncate">
              {{ item.path }}
            </span>
          </div>
          <span class="m3-label-small text-[var(--md-sys-color-primary)] bg-[var(--md-sys-color-primary-container)] px-2 py-0.5 rounded-full shrink-0">
            {{ t('results.willUpload') }}
          </span>
        </div>
      </div>
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
          :aria-label="t('results.toggleSkippedAria')"
          @click="showSkippedList = !showSkippedList"
        >
          <div class="flex items-center gap-2">
            <M3Icon
              name="cloud_done"
              :filled="true"
              :size="16"
              class="text-[var(--md-sys-color-primary)]"
            />
            <span>{{ t('results.skippedCount', { count: results.skipped.length }) }}</span>
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
                    :title="skipItem.paths?.[0] || t('upload.unknownFile')"
                  >
                    {{ (skipItem.paths?.[0] || t('upload.unknownFile')).split(/[/\\]/).pop() }}
                  </span>
                  <span
                    class="m3-body-medium mt-0.5 leading-relaxed"
                  >
                    {{ formatSkipReason(skipItem.reason, skipItem.code, t) }}
                  </span>
                </div>
              </div>
              <span class="px-2 py-0.5 rounded-full bg-[var(--md-sys-color-secondary-container)] text-[var(--md-sys-color-on-secondary-container)] m3-label-small shrink-0">
                {{ t('results.alreadyInLibrary') }}
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
          :aria-label="t('results.toggleFailedAria')"
          @click="showFailedList = !showFailedList"
        >
          <div class="flex items-center gap-2">
            <M3Icon
              name="warning"
              :filled="true"
              :size="16"
            />
            <span>{{ t('results.failedItemsCount', { count: results.fail.length }) }}</span>
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
          :aria-label="t('results.retryFailedAria')"
          @click="emit('retryAll')"
        >
          <span>{{ t('results.retryFailed') }}</span>
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
                    {{ formatErrorMessage(failItem.error, t) }}
                  </span>
                </div>
              </div>

              <!-- Actionable Per-Item Retry Button -->
              <md-icon-button
                icon="replay"
                size="xs"
                :title="t('results.retryFile')"
                :aria-label="t('status.retryItemAria', { path: typeof failItem === 'string' ? failItem : failItem.path })"
                @click="emit('retryItem', typeof failItem === 'string' ? failItem : failItem.path)"
              />
            </div>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>
