<script setup lang="ts">
import { ref, watch } from 'vue'
import { Browser } from '@wailsio/runtime'
import { ConfigManager } from '../../bindings/app/core'
import type { UpdateInfo } from '../../bindings/app/core/models'
import M3Button from './ui/M3Button.vue'
import M3Icon from './ui/M3Icon.vue'

const props = defineProps<{
  modelValue: boolean
  updateInfo: UpdateInfo | null
}>()

const emit = defineEmits<{
  (e: 'update:modelValue', value: boolean): void
}>()

const notes = ref('')
const isLoadingNotes = ref(false)

watch(() => [props.modelValue, props.updateInfo], async ([open, info]) => {
  if (open && info && typeof info === 'object' && 'latestVersion' in info) {
    const update = info as UpdateInfo
    if (update.releaseNotes) {
      notes.value = update.releaseNotes
      return
    }
    if (update.latestVersion && !notes.value) {
      isLoadingNotes.value = true
      try {
        const fetched = await ConfigManager.GetReleaseNotes(update.latestVersion)
        if (fetched) {
          notes.value = fetched
        }
      } catch (err: unknown) {
        const msg = err instanceof Error ? err.message : String(err)
        notes.value = msg || 'Could not load release notes.'
      } finally {
        isLoadingNotes.value = false
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
      window.open(props.updateInfo!.releaseUrl, '_blank')
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
        class="m3-dialog-wrapper fixed inset-0 z-50 grid place-items-center bg-black/70 p-4 select-none overflow-y-auto"
        style="--wails-draggable: none"
        @click.self="close"
      >
        <div
          class="m3-dialog-card relative flex flex-col w-full max-w-lg rounded-[28px] border border-[var(--md-sys-color-outline-variant)]/50 bg-[var(--md-sys-color-surface-container-high)] shadow-2xl overflow-hidden select-none"
          role="dialog"
          aria-modal="true"
          aria-labelledby="update-modal-title"
        >
          <!-- Header -->
          <div class="flex items-center justify-between border-b border-[var(--md-sys-color-outline-variant)]/30 px-6 py-5">
            <div class="flex items-center gap-3">
              <div class="flex size-11 shrink-0 items-center justify-center rounded-2xl bg-[var(--md-sys-color-primary-container)] text-[var(--md-sys-color-on-primary-container)]">
                <M3Icon
                  name="system_update"
                  :filled="true"
                  :size="24"
                />
              </div>
              <div class="flex flex-col">
                <h2
                  id="update-modal-title"
                  class="m3-headline-small text-[var(--md-sys-color-on-surface)]"
                >
                  New Version Available
                </h2>
                <div class="flex items-center gap-2 mt-0.5 m3-body-medium">
                  <span>Current: v{{ updateInfo.currentVersion }}</span>
                  <span>&rarr;</span>
                  <span class="m3-title-medium text-[var(--md-sys-color-primary)]">v{{ updateInfo.latestVersion }}</span>
                </div>
              </div>
            </div>

            <M3Button
              variant="text"
              size="xs"
              icon="close"
              aria-label="Close dialog"
              class="!size-9 !min-w-0 !p-0"
              @click="close"
            />
          </div>

          <!-- Body with Release Notes -->
          <div class="max-h-80 overflow-y-auto px-6 py-5 flex flex-col gap-3">
            <div class="flex items-center justify-between m3-label-small text-[var(--md-sys-color-on-surface-variant)]">
              <span>Release Notes</span>
              <span
                v-if="updateInfo.publishedAt"
                class="opacity-75"
              >
                {{ new Date(updateInfo.publishedAt).toLocaleDateString() }}
              </span>
            </div>

            <div class="rounded-2xl border border-[var(--md-sys-color-outline-variant)]/30 bg-[var(--md-sys-color-surface-container)] p-4 m3-mono m3-body-medium text-[var(--md-sys-color-on-surface)] whitespace-pre-wrap break-words max-h-60 overflow-y-auto">
              <div
                v-if="isLoadingNotes"
                class="text-[var(--md-sys-color-on-surface-variant)] italic"
              >
                Loading release notes...
              </div>
              <div v-else>
                {{ notes || updateInfo.releaseNotes || 'No release notes provided.' }}
              </div>
            </div>
          </div>

          <!-- Footer Actions -->
          <div class="border-t border-[var(--md-sys-color-outline-variant)]/30 bg-[var(--md-sys-color-surface-container-high)] px-6 py-4 flex items-center justify-end gap-2.5">
            <M3Button
              variant="outlined"
              size="s"
              @click="close"
            >
              Later
            </M3Button>
            <M3Button
              variant="filled"
              size="s"
              icon="open_in_new"
              @click="openDownload"
            >
              Download on GitHub
            </M3Button>
          </div>
        </div>
      </div>
    </Transition>
  </Teleport>
</template>
