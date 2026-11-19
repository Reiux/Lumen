<script setup lang="ts">
import { ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { Browser } from '@wailsio/runtime'
import { ConfigManager } from '../../bindings/app/core'
import type { UpdateInfo } from '../../bindings/app/core/models'
import M3Button from './ui/M3Button.vue'
import M3Icon from './ui/M3Icon.vue'
import { useDialogFocusTrap } from '../utils/useDialogFocusTrap'

const { t } = useI18n()

const props = defineProps<{
  modelValue: boolean
  updateInfo: UpdateInfo | null
}>()

const emit = defineEmits<{
  (e: 'update:modelValue', value: boolean): void
}>()

const dialogCardRef = ref<HTMLElement | null>(null)
useDialogFocusTrap(() => Boolean(props.modelValue && props.updateInfo), dialogCardRef, {
  onEscape: close,
})

const notes = ref('')
const isLoadingNotes = ref(false)
const lastFetchedVersion = ref('')

watch(() => [props.modelValue, props.updateInfo] as const, async ([open, info]) => {
  if (!open) return
  if (info && typeof info === 'object' && 'latestVersion' in info) {
    const update = info as UpdateInfo
    if (update.releaseNotes) {
      notes.value = update.releaseNotes
      lastFetchedVersion.value = update.latestVersion || ''
      return
    }
    if (update.latestVersion && update.latestVersion !== lastFetchedVersion.value) {
      const requestedVersion = update.latestVersion
      notes.value = ''
      isLoadingNotes.value = true
      try {
        const fetched = await ConfigManager.GetReleaseNotes(requestedVersion)
        if (!props.modelValue || props.updateInfo?.latestVersion !== requestedVersion) {
          return
        }
        if (fetched) {
          notes.value = fetched
          lastFetchedVersion.value = requestedVersion
        }
      } catch (err: unknown) {
        if (!props.modelValue || props.updateInfo?.latestVersion !== requestedVersion) {
          return
        }
        const msg = err instanceof Error ? err.message : String(err)
        notes.value = msg || 'Could not load release notes.'
      } finally {
        if (props.updateInfo?.latestVersion === requestedVersion || !props.modelValue) {
          isLoadingNotes.value = false
        }
      }
    }
  }
}, { immediate: true })

function close() {
  emit('update:modelValue', false)
}

function openDownload() {
  if (props.updateInfo?.releaseUrl) {
    Browser.OpenURL(props.updateInfo.releaseUrl).catch(() => {
      window.open(props.updateInfo!.releaseUrl, '_blank', 'noopener,noreferrer')
    })
  }
  close()
}
</script>

<template>
  <Teleport to="body">
    <Transition name="m3-dialog-modal">
      <div
        v-if="modelValue && updateInfo"
        class="m3-dialog-wrapper fixed inset-0 z-50 grid place-items-center p-4 select-none overflow-y-auto"
        style="--wails-draggable: none"
        @click.self="close"
      >
        <div
          ref="dialogCardRef"
          tabindex="-1"
          class="m3-dialog-card relative flex flex-col w-full max-w-lg rounded-[28px] bg-[var(--md-sys-color-surface-container-high)] shadow-2xl overflow-hidden select-none outline-none focus-visible:outline-[3px] focus-visible:outline-[var(--md-sys-color-primary)]"
          role="dialog"
          aria-modal="true"
          aria-labelledby="update-modal-title"
        >
          <!-- M3E Header (tonal hierarchy, no hairline border hacks) -->
          <div class="flex items-start justify-between gap-3 px-6 pt-6 pb-4">
            <div class="flex items-center gap-3.5 min-w-0">
              <div class="flex size-12 shrink-0 items-center justify-center rounded-[16px] bg-[var(--md-sys-color-primary-container)] text-[var(--md-sys-color-on-primary-container)]">
                <M3Icon
                  name="system_update"
                  :filled="true"
                  :size="24"
                />
              </div>
              <div class="flex flex-col min-w-0">
                <h2
                  id="update-modal-title"
                  class="m3-headline-small text-[var(--md-sys-color-on-surface)]"
                >
                  {{ t('updater.newVersion') }}
                </h2>
                <div class="flex flex-wrap items-center gap-2 mt-1 m3-body-medium text-[var(--md-sys-color-on-surface-variant)]">
                  <span>{{ t('updater.currentVersion', { version: updateInfo.currentVersion }) }}</span>
                  <span aria-hidden="true">&rarr;</span>
                  <span class="rounded-full bg-[var(--md-sys-color-secondary-container)] text-[var(--md-sys-color-on-secondary-container)] px-2.5 py-0.5 m3-label-large text-xs">
                    v{{ updateInfo.latestVersion.replace(/^v/i, '') }}
                  </span>
                </div>
              </div>
            </div>

            <M3Button
              variant="text"
              size="s"
              icon="close"
              :aria-label="t('updater.close')"
              class="!size-10 !min-w-0 !p-0 shrink-0"
              @click="close"
            />
          </div>

          <!-- Body with Release Notes -->
          <div class="max-h-80 overflow-y-auto px-6 py-2 flex flex-col gap-2.5">
            <div class="flex items-center justify-between px-1">
              <span class="m3-label-large text-[var(--md-sys-color-on-surface-variant)]">
                {{ t('updater.releaseNotes') }}
              </span>
              <span
                v-if="updateInfo.publishedAt"
                class="m3-body-small text-[var(--md-sys-color-on-surface-variant)]"
              >
                {{ new Date(updateInfo.publishedAt).toLocaleDateString() }}
              </span>
            </div>

            <div class="rounded-[20px] bg-[var(--md-sys-color-surface-container)] p-4 m3-mono m3-body-medium text-[var(--md-sys-color-on-surface)] whitespace-pre-wrap break-words max-h-60 overflow-y-auto leading-relaxed">
              <div
                v-if="isLoadingNotes"
                class="text-[var(--md-sys-color-on-surface-variant)] italic"
              >
                {{ t('updater.loadingNotes') }}
              </div>
              <div v-else>
                {{ notes || updateInfo.releaseNotes || t('updater.noNotes') }}
              </div>
            </div>
          </div>

          <!-- M3E Footer Actions (Text dismiss + Filled primary pill) -->
          <div class="px-6 pt-4 pb-6 flex items-center justify-end gap-2">
            <M3Button
              variant="text"
              size="s"
              @click="close"
            >
              {{ t('updater.later') }}
            </M3Button>
            <M3Button
              variant="filled"
              size="m"
              icon="open_in_new"
              @click="openDownload"
            >
              {{ t('updater.downloadGitHub') }}
            </M3Button>
          </div>
        </div>
      </div>
    </Transition>
  </Teleport>
</template>
