<script setup lang="ts">
import { Browser } from '@wailsio/runtime'
import M3Button from './components/ui/M3Button.vue'
import M3Icon from './components/ui/M3Icon.vue'

const repoUrl = 'https://github.com/Reiux/Lumen'

async function openRepo() {
  try {
    await Browser.OpenURL(repoUrl)
  } catch (err) {
    console.error('Failed to open repository URL:', err)
    window.open(repoUrl, '_blank')
  }
}
</script>

<template>
  <div
    class="flex flex-col flex-1 w-full max-w-4xl mx-auto gap-6 select-none pb-12"
    style="--wails-draggable: none"
  >
    <!-- Hero Banner Card -->
    <div
      class="m3-drop-card m3-card-interactive relative overflow-hidden rounded-[32px] border border-[var(--md-sys-color-outline-variant)]/40 bg-[var(--md-sys-color-surface-container)] p-8 md:p-10 shadow-xs"
      :style="{ '--card-index': 0 }"
    >
      <div class="flex flex-col sm:flex-row items-start sm:items-center justify-between gap-6">
        <div class="flex items-center gap-5">
          <div class="flex size-20 shrink-0 items-center justify-center rounded-[28px] bg-[var(--md-sys-color-primary)] text-[var(--md-sys-color-on-primary)] shadow-md transition-all duration-300 hover:scale-105">
            <M3Icon
              name="light_mode"
              :size="40"
            />
          </div>
          <div class="flex flex-col gap-1">
            <div class="flex items-center gap-3">
              <h1 class="text-3xl font-extrabold tracking-tight text-[var(--md-sys-color-on-surface)]">
                Lumen
              </h1>
              <span class="rounded-full bg-[var(--md-sys-color-primary-container)] px-3 py-0.5 text-xs font-semibold text-[var(--md-sys-color-on-primary-container)]">
                v0.11.0-m3e.1
              </span>
            </div>
            <p class="text-sm text-[var(--md-sys-color-on-surface-variant)] leading-relaxed">
              Fast, privacy-focused desktop companion for Google Photos.
            </p>
          </div>
        </div>

        <M3Button
          variant="filled"
          size="m"
          icon="open_in_new"
          class="shrink-0"
          aria-label="View Lumen on GitHub"
          @click="openRepo"
        >
          <span>View on GitHub</span>
        </M3Button>
      </div>

      <!-- Quick info badges -->
      <div class="mt-8 flex flex-wrap gap-2 pt-6 border-t border-[var(--md-sys-color-outline-variant)]/20 text-xs">
        <span class="inline-flex items-center gap-1.5 rounded-full bg-[var(--md-sys-color-surface-container-high)] px-3.5 py-1 font-medium text-[var(--md-sys-color-on-surface-variant)]">
          <M3Icon
            name="terminal"
            :size="14"
          />
          Wails v3 + Go
        </span>
        <span class="inline-flex items-center gap-1.5 rounded-full bg-[var(--md-sys-color-surface-container-high)] px-3.5 py-1 font-medium text-[var(--md-sys-color-on-surface-variant)]">
          <M3Icon
            name="palette"
            :size="14"
          />
          Material 3 Expressive
        </span>
        <span class="inline-flex items-center gap-1.5 rounded-full bg-[var(--md-sys-color-surface-container-high)] px-3.5 py-1 font-medium text-[var(--md-sys-color-on-surface-variant)]">
          <M3Icon
            name="lock"
            :size="14"
          />
          Local Credential Storage
        </span>
        <span class="inline-flex items-center gap-1.5 rounded-full bg-[var(--md-sys-color-surface-container-high)] px-3.5 py-1 font-medium text-[var(--md-sys-color-on-surface-variant)]">
          <M3Icon
            name="bolt"
            :size="14"
          />
          Multithreaded Pipeline
        </span>
      </div>
    </div>

    <!-- Core Features Section -->
    <div class="flex flex-col gap-3">
      <h2 class="text-base font-bold text-[var(--md-sys-color-on-surface)] px-1">
        Features & Capabilities
      </h2>

      <div class="grid grid-cols-1 md:grid-cols-2 gap-4">
        <div
          class="m3-drop-card m3-card-interactive flex items-start gap-3.5 rounded-[24px] border border-[var(--md-sys-color-outline-variant)]/30 bg-[var(--md-sys-color-surface-container-low)] p-5"
          :style="{ '--card-index': 1 }"
        >
          <div class="flex size-10 shrink-0 items-center justify-center rounded-xl bg-[var(--md-sys-color-primary-container)] text-[var(--md-sys-color-on-primary-container)]">
            <M3Icon
              name="speed"
              :size="22"
            />
          </div>
          <div class="flex flex-col gap-0.5">
            <h3 class="text-sm font-semibold text-[var(--md-sys-color-on-surface)]">
              Multithreaded Chunked Uploads
            </h3>
            <p class="text-xs text-[var(--md-sys-color-on-surface-variant)] leading-relaxed">
              High-throughput parallel uploads with automatic retry, exponential backoff, and live per-thread progress indicators.
            </p>
          </div>
        </div>

        <div
          class="m3-drop-card m3-card-interactive flex items-start gap-3.5 rounded-[24px] border border-[var(--md-sys-color-outline-variant)]/30 bg-[var(--md-sys-color-surface-container-low)] p-5"
          :style="{ '--card-index': 2 }"
        >
          <div class="flex size-10 shrink-0 items-center justify-center rounded-xl bg-[var(--md-sys-color-secondary-container)] text-[var(--md-sys-color-on-secondary-container)]">
            <M3Icon
              name="motion_photos_on"
              :size="22"
            />
          </div>
          <div class="flex flex-col gap-0.5">
            <h3 class="text-sm font-semibold text-[var(--md-sys-color-on-surface)]">
              Apple Live Photos Support
            </h3>
            <p class="text-xs text-[var(--md-sys-color-on-surface-variant)] leading-relaxed">
              Seamlessly detects and pairs .HEIC/.JPG still images with corresponding .MOV video files into full Google Photos Motion Photos.
            </p>
          </div>
        </div>

        <div
          class="m3-drop-card m3-card-interactive flex items-start gap-3.5 rounded-[24px] border border-[var(--md-sys-color-outline-variant)]/30 bg-[var(--md-sys-color-surface-container-low)] p-5"
          :style="{ '--card-index': 3 }"
        >
          <div class="flex size-10 shrink-0 items-center justify-center rounded-xl bg-[var(--md-sys-color-tertiary-container)] text-[var(--md-sys-color-on-tertiary-container)]">
            <M3Icon
              name="folder_special"
              :size="22"
            />
          </div>
          <div class="flex flex-col gap-0.5">
            <h3 class="text-sm font-semibold text-[var(--md-sys-color-on-surface)]">
              Smart Directory & Album Sync
            </h3>
            <p class="text-xs text-[var(--md-sys-color-on-surface-variant)] leading-relaxed">
              Uploads entire directories while automatically creating and organizing photos into Google Photos albums matching local folder names.
            </p>
          </div>
        </div>

        <div
          class="m3-drop-card m3-card-interactive flex items-start gap-3.5 rounded-[24px] border border-[var(--md-sys-color-outline-variant)]/30 bg-[var(--md-sys-color-surface-container-low)] p-5"
          :style="{ '--card-index': 4 }"
        >
          <div class="flex size-10 shrink-0 items-center justify-center rounded-xl bg-[var(--md-sys-color-surface-container-highest)] text-[var(--md-sys-color-on-surface)]">
            <M3Icon
              name="visibility"
              :size="22"
            />
          </div>
          <div class="flex flex-col gap-0.5">
            <h3 class="text-sm font-semibold text-[var(--md-sys-color-on-surface)]">
              Background Folder Watcher
            </h3>
            <p class="text-xs text-[var(--md-sys-color-on-surface-variant)] leading-relaxed">
              Monitors specified folders in the background, waits for stable writes, and automatically uploads new media with account bindings.
            </p>
          </div>
        </div>

        <div
          class="m3-drop-card m3-card-interactive flex items-start gap-3.5 rounded-[24px] border border-[var(--md-sys-color-outline-variant)]/30 bg-[var(--md-sys-color-surface-container-low)] p-5"
          :style="{ '--card-index': 5 }"
        >
          <div class="flex size-10 shrink-0 items-center justify-center rounded-xl bg-[var(--md-sys-color-surface-container-highest)] text-[var(--md-sys-color-on-surface)]">
            <M3Icon
              name="fingerprint"
              :size="22"
            />
          </div>
          <div class="flex flex-col gap-0.5">
            <h3 class="text-sm font-semibold text-[var(--md-sys-color-on-surface)]">
              Account-Keyed Hash Deduplication
            </h3>
            <p class="text-xs text-[var(--md-sys-color-on-surface-variant)] leading-relaxed">
              Tracks confirmed uploads with SHA-256 content hashes to avoid redundant uploads, with per-account cache isolation.
            </p>
          </div>
        </div>

        <div
          class="m3-drop-card m3-card-interactive flex items-start gap-3.5 rounded-[24px] border border-[var(--md-sys-color-outline-variant)]/30 bg-[var(--md-sys-color-surface-container-low)] p-5"
          :style="{ '--card-index': 6 }"
        >
          <div class="flex size-10 shrink-0 items-center justify-center rounded-xl bg-[var(--md-sys-color-surface-container-highest)] text-[var(--md-sys-color-on-surface)]">
            <M3Icon
              name="tune"
              :size="22"
            />
          </div>
          <div class="flex flex-col gap-0.5">
            <h3 class="text-sm font-semibold text-[var(--md-sys-color-on-surface)]">
              Dynamic Rate Limiter & Tray Mode
            </h3>
            <p class="text-xs text-[var(--md-sys-color-on-surface-variant)] leading-relaxed">
              Global live bandwidth throttle (0.5 to 50 MB/s), desktop notifications, and single-instance system tray minimization.
            </p>
          </div>
        </div>
      </div>
    </div>

    <!-- Acknowledgements & Legal Disclaimer -->
    <div
      class="m3-drop-card rounded-[24px] border border-[var(--md-sys-color-outline-variant)]/30 bg-[var(--md-sys-color-surface-container-low)] p-6 flex flex-col gap-3"
      :style="{ '--card-index': 7 }"
    >
      <h3 class="text-xs font-bold uppercase tracking-wider text-[var(--md-sys-color-primary)]">
        Credits & Disclaimer
      </h3>
      <p class="text-xs text-[var(--md-sys-color-on-surface-variant)] leading-relaxed">
        Lumen is a redesign fork of <strong class="text-[var(--md-sys-color-on-surface)]">gotohp</strong>. Built with Wails v3, Go, Vue 3, and the Material 3 Expressive web component system.
      </p>
      <p class="text-[11px] text-[var(--md-sys-color-on-surface-variant)]/80 leading-relaxed border-t border-[var(--md-sys-color-outline-variant)]/20 pt-3">
        Lumen is an independent, unofficial open-source application and is not affiliated with, sponsored by, or endorsed by Google LLC or Google Photos. All trademarks and registered trademarks are the property of their respective owners.
      </p>
    </div>
  </div>
</template>
