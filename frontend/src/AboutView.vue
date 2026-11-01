<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { Browser } from '@wailsio/runtime'
import { ConfigManager } from '../bindings/app/core'
import type { UpdateInfo } from '../bindings/app/core/models'
import M3Button from './components/ui/M3Button.vue'
import M3Icon from './components/ui/M3Icon.vue'
import M3Switch from './components/ui/M3Switch.vue'
import LumenLogo from './components/LumenLogo.vue'
import UpdateAvailableModal from './components/UpdateAvailableModal.vue'
import LicensesModal from './components/LicensesModal.vue'
import { toast } from 'vue-sonner'
import { preferencesState, setAutoCheckUpdates } from './utils/configStore'
import { devOptionsState, mockState, setDevOptionsUnlocked, toggleMockMode } from './utils/mockMode'
import reiuxAvatar from './assets/credits/reiux.png'
import xob0tAvatar from './assets/credits/xob0t.png'

const reiuxAvatarError = ref(false)
const xob0tAvatarError = ref(false)

const repoUrl = 'https://github.com/Reiux/Lumen'
const appVersion = ref('1.0.0')
const isCheckingUpdates = ref(false)
const updateResult = ref<UpdateInfo | null>(null)
const showUpdateModal = ref(false)
const showLicensesModal = ref(false)

const tapCount = ref(0)
let tapTimer: ReturnType<typeof setTimeout> | null = null
const logoRotation = ref(0)

const lastChecked = ref<Date | null>(null)
if (typeof localStorage !== 'undefined') {
  const stored = localStorage.getItem('lumen_last_check')
  if (stored) {
    try {
      lastChecked.value = new Date(stored)
    } catch {
      // ignore
    }
  }
}

const lastCheckedFormatted = computed(() => {
  if (!lastChecked.value) {
    return 'Never checked in this session'
  }
  return lastChecked.value.toLocaleTimeString([], { hour: 'numeric', minute: '2-digit' }) + ' · ' + lastChecked.value.toLocaleDateString()
})

onMounted(async () => {
  try {
    const v = await ConfigManager.GetAppVersion()
    if (v) appVersion.value = v
  } catch (err) {
    console.warn('Failed to get app version:', err)
  }
})

async function openRepo() {
  try {
    await Browser.OpenURL(repoUrl)
  } catch (err) {
    console.error('Failed to open repository URL:', err)
    window.open(repoUrl, '_blank')
  }
}

async function openDevProfile() {
  const url = 'https://github.com/reiux'
  try {
    await Browser.OpenURL(url)
  } catch (err) {
    console.error('Failed to open developer profile:', err)
    window.open(url, '_blank')
  }
}

async function openGotohpRepo() {
  const url = 'https://github.com/xob0t/gotohp'
  try {
    await Browser.OpenURL(url)
  } catch (err) {
    console.error('Failed to open gotohp repository:', err)
    window.open(url, '_blank')
  }
}

async function checkForUpdates() {
  if (isCheckingUpdates.value) return
  isCheckingUpdates.value = true
  lastChecked.value = new Date()
  if (typeof localStorage !== 'undefined') {
    localStorage.setItem('lumen_last_check', lastChecked.value.toISOString())
  }

  try {
    const info = await ConfigManager.CheckForUpdates(true)
    updateResult.value = info
    if (info.error) {
      toast.error(`Update check: ${info.error}`)
    } else if (info.hasUpdate) {
      showUpdateModal.value = true
      toast.info(`New version v${info.latestVersion} is available!`)
    } else {
      toast.success(`You're on the latest version (v${info.currentVersion || appVersion.value})`)
    }
  } catch (err: unknown) {
    const msg = err instanceof Error ? err.message : String(err)
    toast.error(`Update check: ${msg || 'Could not check for updates'}`)
  } finally {
    isCheckingUpdates.value = false
  }
}

function onLogoTap() {
  logoRotation.value += 360

  if (devOptionsState.unlocked) {
    toast.info('Developer options are already active')
    return
  }

  tapCount.value++
  if (tapTimer) clearTimeout(tapTimer)
  tapTimer = setTimeout(() => {
    tapCount.value = 0
  }, 3500)

  const remaining = 7 - tapCount.value
  if (remaining > 0 && remaining <= 3) {
    toast.info(`${remaining} more tap${remaining > 1 ? 's' : ''} to enable developer options`)
  } else if (remaining <= 0) {
    setDevOptionsUnlocked(true)
    tapCount.value = 0
    toast.success('Developer options enabled')
  }
}

function disableDevOptions() {
  setDevOptionsUnlocked(false)
  toast.info('Developer options disabled')
}
</script>

<template>
  <div
    class="flex flex-col flex-1 w-full max-w-xl mx-auto gap-6 select-none pb-12"
    style="--wails-draggable: none"
  >
    <!-- Header: Large transparent logo, app name, version, short description -->
    <div class="flex flex-col items-center text-center pt-8">
      <div
        class="inline-flex items-center justify-center size-24 cursor-pointer select-none rounded-full origin-center"
        :style="{
          transform: `rotate(${logoRotation}deg)`,
          transition: 'transform 600ms cubic-bezier(0.38, 1.21, 0.22, 1.00)',
        }"
        title="Tap 7 times for developer options"
        @click="onLogoTap"
      >
        <LumenLogo
          :size="96"
          :transparent="true"
        />
      </div>

      <h1 class="m3-headline-large text-[var(--md-sys-color-on-surface)] mt-1">
        Lumen
      </h1>

      <span class="rounded-full bg-[var(--md-sys-color-primary-container)] px-3 py-0.5 m3-label-small normal-case text-[var(--md-sys-color-on-primary-container)] mt-1.5">
        v{{ appVersion }}
      </span>

      <p class="m3-body-medium mt-2.5 max-w-sm leading-relaxed">
        Fast, privacy-focused desktop companion for Google Photos.
      </p>

      <!-- Action buttons: Source code, Check for updates & Open source licenses -->
      <div class="flex flex-wrap items-center justify-center gap-3 mt-6">
        <M3Button
          variant="tonal"
          size="m"
          icon="open_in_new"
          aria-label="View source code on GitHub"
          @click="openRepo"
        >
          <span>Source code</span>
        </M3Button>

        <M3Button
          variant="filled"
          size="m"
          icon="refresh"
          :disabled="isCheckingUpdates"
          :loading="isCheckingUpdates"
          aria-label="Check for updates"
          @click="checkForUpdates"
        >
          <span>{{ isCheckingUpdates ? 'Checking...' : 'Check for updates' }}</span>
        </M3Button>

        <M3Button
          variant="tonal"
          size="m"
          icon="policy"
          aria-label="View open source licenses"
          @click="showLicensesModal = true"
        >
          <span>Open source licenses</span>
        </M3Button>
      </div>
    </div>

    <!-- Updates Group Card -->
    <div class="flex flex-col gap-3 rounded-[24px] border border-[var(--md-sys-color-outline-variant)]/40 bg-[var(--md-sys-color-surface-container)] p-5 shadow-xs">
      <div class="flex items-center gap-2.5 px-1 text-[var(--md-sys-color-primary)]">
        <M3Icon
          name="update"
          :size="20"
        />
        <h2 class="m3-headline-small text-[var(--md-sys-color-on-surface)]">
          Updates
        </h2>
      </div>

      <div class="flex flex-col gap-2">
        <div class="flex items-center justify-between gap-4 rounded-[18px] bg-[var(--md-sys-color-surface-container-low)] p-4 transition-colors hover:bg-[var(--md-sys-color-surface-container-high)]/60">
          <div class="flex flex-col min-w-0 pr-2">
            <label
              for="auto-updates-toggle"
              class="m3-title-medium text-[var(--md-sys-color-on-surface)] cursor-pointer"
            >
              Check for updates automatically
            </label>
            <span class="m3-body-medium mt-0.5 leading-relaxed">
              Periodically check GitHub Releases for new versions.
            </span>
          </div>
          <M3Switch
            id="auto-updates-toggle"
            :model-value="preferencesState.autoCheckUpdates"
            aria-label="Check for updates automatically toggle"
            @update:model-value="setAutoCheckUpdates"
          />
        </div>

        <div class="flex items-center justify-between rounded-[18px] bg-[var(--md-sys-color-surface-container-low)] px-4 py-3 m3-body-medium">
          <span class="text-[var(--md-sys-color-on-surface)]">Last checked:</span>
          <span class="tabular-nums text-[var(--md-sys-color-on-surface)]">{{ lastCheckedFormatted }}</span>
        </div>
      </div>
    </div>

    <!-- Developer Options Group (Shown when unlocked) -->
    <div
      v-if="devOptionsState.unlocked"
      class="flex flex-col gap-3 rounded-[24px] border border-[var(--md-sys-color-primary)]/40 bg-[var(--md-sys-color-surface-container)] p-5 shadow-xs transition-all"
    >
      <div class="flex items-center justify-between px-1">
        <div class="flex items-center gap-2.5 text-[var(--md-sys-color-primary)]">
          <M3Icon
            name="terminal"
            :size="20"
          />
          <h2 class="m3-headline-small text-[var(--md-sys-color-on-surface)]">
            Developer options
          </h2>
        </div>
        <span class="rounded-full bg-[var(--md-sys-color-primary-container)] px-2.5 py-0.5 m3-label-small text-[var(--md-sys-color-on-primary-container)]">
          Active
        </span>
      </div>

      <div class="flex flex-col gap-2">
        <!-- Mock Mode Toggle -->
        <div class="flex items-center justify-between gap-4 rounded-[18px] bg-[var(--md-sys-color-surface-container-low)] p-4 transition-colors hover:bg-[var(--md-sys-color-surface-container-high)]/60">
          <div class="flex flex-col min-w-0 pr-2">
            <label
              for="mock-mode-toggle"
              class="m3-title-medium text-[var(--md-sys-color-on-surface)] cursor-pointer"
            >
              Mock mode
            </label>
            <span class="m3-body-medium mt-0.5 leading-relaxed">
              Simulate upload batches, states, error scenarios, and activity logs.
            </span>
          </div>
          <M3Switch
            id="mock-mode-toggle"
            :model-value="mockState.enabled"
            aria-label="Mock mode toggle"
            @update:model-value="toggleMockMode"
          />
        </div>

        <!-- Disable Developer Options Button -->
        <div class="flex items-center justify-between rounded-[18px] bg-[var(--md-sys-color-surface-container-low)] p-4">
          <div class="flex flex-col min-w-0 pr-2">
            <span class="m3-title-medium text-[var(--md-sys-color-on-surface)]">
              Lock developer options
            </span>
            <span class="m3-body-medium mt-0.5 leading-relaxed">
              Hide developer options until tapped 7 times again.
            </span>
          </div>
          <M3Button
            variant="tonal"
            size="s"
            destructive
            aria-label="Disable developer options"
            @click="disableDevOptions"
          >
            <span>Lock</span>
          </M3Button>
        </div>
      </div>
    </div>

    <!-- Credits Card -->
    <div class="flex flex-col gap-3 rounded-[24px] border border-[var(--md-sys-color-outline-variant)]/40 bg-[var(--md-sys-color-surface-container)] p-5 shadow-xs">
      <div class="flex items-center gap-2.5 px-1 text-[var(--md-sys-color-primary)]">
        <M3Icon
          name="favorite"
          :size="20"
        />
        <h2 class="m3-headline-small text-[var(--md-sys-color-on-surface)]">
          Credits
        </h2>
      </div>

      <div class="flex flex-col gap-2">
        <!-- Developer credit (Reiux) -->
        <div class="flex items-center justify-between gap-4 rounded-[18px] bg-[var(--md-sys-color-surface-container-low)] p-4 transition-colors hover:bg-[var(--md-sys-color-surface-container-high)]/60">
          <div class="flex items-center gap-3.5 min-w-0 pr-2">
            <!-- Round 40px avatar with fallback -->
            <div class="relative size-10 shrink-0 overflow-hidden rounded-full border border-[var(--md-sys-color-outline-variant)] bg-[var(--md-sys-color-surface-container-high)] flex items-center justify-center">
              <img
                v-if="!reiuxAvatarError"
                :src="reiuxAvatar"
                alt="Reiux profile avatar"
                class="size-full object-cover"
                @error="reiuxAvatarError = true"
              >
              <span
                v-else
                class="m3-title-medium text-[var(--md-sys-color-primary)]"
              >
                R
              </span>
            </div>

            <div class="flex flex-col min-w-0">
              <span class="m3-title-medium text-[var(--md-sys-color-on-surface)] truncate">
                Reiux
              </span>
              <span class="m3-body-medium mt-0.5">
                Creator and developer of Lumen
              </span>
            </div>
          </div>
          <M3Button
            variant="tonal"
            size="s"
            icon="open_in_new"
            aria-label="Open Reiux's GitHub profile"
            @click="openDevProfile"
          >
            <span>GitHub</span>
          </M3Button>
        </div>

        <!-- Core protocol logic credit (xob0t / gotohp) -->
        <div class="flex items-center justify-between gap-4 rounded-[18px] bg-[var(--md-sys-color-surface-container-low)] p-4 transition-colors hover:bg-[var(--md-sys-color-surface-container-high)]/60">
          <div class="flex items-center gap-3.5 min-w-0 pr-2">
            <!-- Round 40px avatar with fallback -->
            <div class="relative size-10 shrink-0 overflow-hidden rounded-full border border-[var(--md-sys-color-outline-variant)] bg-[var(--md-sys-color-surface-container-high)] flex items-center justify-center">
              <img
                v-if="!xob0tAvatarError"
                :src="xob0tAvatar"
                alt="xob0t profile avatar"
                class="size-full object-cover"
                @error="xob0tAvatarError = true"
              >
              <span
                v-else
                class="m3-title-medium text-[var(--md-sys-color-primary)]"
              >
                X
              </span>
            </div>

            <div class="flex flex-col min-w-0">
              <span class="m3-title-medium text-[var(--md-sys-color-on-surface)] truncate">
                xob0t
              </span>
              <span class="m3-body-medium mt-0.5">
                gotohp • Google Photos upload protocol and session reverse engineering
              </span>
            </div>
          </div>
          <M3Button
            variant="tonal"
            size="s"
            icon="open_in_new"
            aria-label="Open gotohp repository on GitHub"
            @click="openGotohpRepo"
          >
            <span>Repository</span>
          </M3Button>
        </div>
      </div>

      <p class="m3-body-medium text-[var(--md-sys-color-on-surface-variant)]/80 leading-relaxed border-t border-[var(--md-sys-color-outline-variant)]/20 pt-2.5 px-1">
        Lumen is an independent open-source project and is not affiliated with Google or Google Photos.
      </p>
    </div>

    <!-- Update Available Modal -->
    <UpdateAvailableModal
      v-model="showUpdateModal"
      :update-info="updateResult"
    />

    <!-- Open Source Licenses Modal -->
    <LicensesModal
      v-model:open="showLicensesModal"
    />
  </div>
</template>
