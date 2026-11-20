<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { Browser } from '@wailsio/runtime'

const { t } = useI18n()
import { ConfigManager } from '../bindings/app/core'
import type { UpdateInfo } from '../bindings/app/core/models'
import M3Button from './components/ui/M3Button.vue'
import M3Icon from './components/ui/M3Icon.vue'
import M3Switch from './components/ui/M3Switch.vue'
import LumenLogo from './components/LumenLogo.vue'
import UpdateAvailableModal from './components/UpdateAvailableModal.vue'
import LicensesModal from './components/LicensesModal.vue'
import { toast } from 'vue-sonner'
import { preferencesState, refreshLastUpdateCheck, setAutoCheckUpdates } from './utils/configStore'
import { devOptionsState, isDevMode, mockState, setDevOptionsUnlocked, toggleMockMode } from './utils/mockMode'
import reiuxAvatar from './assets/credits/reiux.png'
import xob0tAvatar from './assets/credits/xob0t.png'

const reiuxAvatarError = ref(false)
const xob0tAvatarError = ref(false)

const repoUrl = 'https://github.com/Reiux/Lumen'
const appVersion = ref('2.1.1')
const isCheckingUpdates = ref(false)
const updateResult = ref<UpdateInfo | null>(null)
const showUpdateModal = ref(false)
const showLicensesModal = ref(false)

const tapCount = ref(0)
let tapTimer: ReturnType<typeof setTimeout> | null = null
const logoRotation = ref(0)

const wordmarkScale = ref(1.0)
const wordmarkWeight = computed(() => {
  const progress = Math.min(Math.max((wordmarkScale.value - 1.0) / 0.26, 0), 1)
  return Math.round(800 + progress * 150)
})

let punchAnimId: number | null = null

function triggerWordmarkPunch() {
  if (punchAnimId) cancelAnimationFrame(punchAnimId)
  const startTime = performance.now()
  function frame(now: number) {
    const elapsed = now - startTime
    if (elapsed < 340) {
      // Ease out cubic to 1.26 (tweens in 340ms)
      const t = elapsed / 340
      const ease = 1 - Math.pow(1 - t, 3)
      wordmarkScale.value = 1.0 + 0.26 * ease
      punchAnimId = requestAnimationFrame(frame)
    } else if (elapsed < 750) {
      // Spring oscillation around 1.0 (damping ~0.58, stiffness ~240)
      const t = (elapsed - 340) / 410
      const decay = Math.exp(-t * 5.0)
      const osc = Math.cos(t * Math.PI * 3.2)
      wordmarkScale.value = 1.0 + 0.26 * decay * osc
      punchAnimId = requestAnimationFrame(frame)
    } else {
      wordmarkScale.value = 1.0
      punchAnimId = null
    }
  }
  punchAnimId = requestAnimationFrame(frame)
}

let holdInterval: ReturnType<typeof setInterval> | null = null
let holdStartTime = 0
const isHoldingLogo = ref(false)
let isUnmounted = false

function onLogoMouseDown() {
  if (holdInterval) {
    clearInterval(holdInterval)
    holdInterval = null
  }
  isHoldingLogo.value = true
  holdStartTime = Date.now()
  holdInterval = setInterval(() => {
    const elapsed = Date.now() - holdStartTime
    const speedFactor = Math.min(1 + Math.pow(elapsed / 500, 2), 10)
    logoRotation.value += 6 * speedFactor
  }, 16)
}

function onLogoMouseUp() {
  isHoldingLogo.value = false
  if (holdInterval) {
    clearInterval(holdInterval)
    holdInterval = null
  }
}

const lastChecked = ref<Date | null>(null)
if (typeof localStorage !== 'undefined') {
  const stored = localStorage.getItem('lumen_last_check')
  if (stored) {
    const parsed = new Date(stored)
    if (!Number.isNaN(parsed.getTime())) {
      lastChecked.value = parsed
    }
  }
}

async function handleAutoCheckUpdates(enabled: boolean) {
  try {
    await setAutoCheckUpdates(enabled)
  } catch {
    toast.error(t('settings.saveFailed'))
  }
}

const effectiveLastChecked = computed<Date | null>(() => {
  const backendTs = preferencesState.lastUpdateCheck
  if (backendTs && backendTs > 0) {
    return new Date(backendTs * 1000)
  }
  return lastChecked.value
})

const lastCheckedFormatted = computed(() => {
  if (!effectiveLastChecked.value) {
    return t('about.neverChecked')
  }
  return effectiveLastChecked.value.toLocaleTimeString([], { hour: 'numeric', minute: '2-digit' }) + ' · ' + effectiveLastChecked.value.toLocaleDateString()
})

let startupPunchTimer: ReturnType<typeof setTimeout> | null = null

onMounted(async () => {
  startupPunchTimer = setTimeout(() => {
    triggerWordmarkPunch()
  }, 220)
  void refreshLastUpdateCheck()
  try {
    const v = await ConfigManager.GetAppVersion()
    if (!isUnmounted && v) appVersion.value = v
  } catch (err) {
    console.warn('Failed to get app version:', err)
  }
})

onUnmounted(() => {
  isUnmounted = true
  isHoldingLogo.value = false
  if (startupPunchTimer) {
    clearTimeout(startupPunchTimer)
    startupPunchTimer = null
  }
  if (holdInterval) {
    clearInterval(holdInterval)
    holdInterval = null
  }
  if (tapTimer) {
    clearTimeout(tapTimer)
    tapTimer = null
  }
  if (punchAnimId) {
    cancelAnimationFrame(punchAnimId)
    punchAnimId = null
  }
})

async function openRepo() {
  try {
    await Browser.OpenURL(repoUrl)
  } catch (err) {
    console.error('Failed to open repository URL:', err)
    window.open(repoUrl, '_blank', 'noopener,noreferrer')
  }
}

async function openDevProfile() {
  const url = 'https://github.com/reiux'
  try {
    await Browser.OpenURL(url)
  } catch (err) {
    console.error('Failed to open developer profile:', err)
    window.open(url, '_blank', 'noopener,noreferrer')
  }
}

async function openGotohpRepo() {
  const url = 'https://github.com/xob0t/gotohp'
  try {
    await Browser.OpenURL(url)
  } catch (err) {
    console.error('Failed to open gotohp repository:', err)
    window.open(url, '_blank', 'noopener,noreferrer')
  }
}

async function checkForUpdates() {
  if (isCheckingUpdates.value) return
  isCheckingUpdates.value = true

  try {
    const info = await ConfigManager.CheckForUpdates(true)
    if (isUnmounted) return
    updateResult.value = info
    if (info.error) {
      toast.error(t('about.toastUpdateCheckError', { error: info.error }))
    } else {
      lastChecked.value = new Date()
      preferencesState.lastUpdateCheck = Math.floor(lastChecked.value.getTime() / 1000)
      if (typeof localStorage !== 'undefined') {
        localStorage.setItem('lumen_last_check', lastChecked.value.toISOString())
      }
      if (info.hasUpdate) {
        showUpdateModal.value = true
        toast.info(t('about.toastUpdateAvailable', { version: info.latestVersion.replace(/^v/i, '') }))
      } else {
        toast.success(t('about.toastLatestVersion', { version: info.currentVersion || appVersion.value }))
      }
    }
  } catch (err: unknown) {
    if (isUnmounted) return
    const msg = err instanceof Error ? err.message : String(err)
    toast.error(t('about.toastUpdateCheckError', { error: msg || t('about.toastCouldNotCheck') }))
  } finally {
    if (!isUnmounted) {
      isCheckingUpdates.value = false
    }
  }
}

function onBrandTap() {
  logoRotation.value += 360
  triggerWordmarkPunch()

  if (devOptionsState.unlocked) {
    toast.info(t('about.toastDevOptionsActive'))
    return
  }

  tapCount.value++
  if (tapTimer) clearTimeout(tapTimer)
  tapTimer = setTimeout(() => {
    tapCount.value = 0
  }, 3500)

  const remaining = 7 - tapCount.value
  if (remaining > 0 && remaining <= 3) {
    toast.info(t('about.toastDevOptionsRemaining', { count: remaining }))
  } else if (remaining <= 0) {
    setDevOptionsUnlocked(true)
    tapCount.value = 0
    toast.success(t('about.toastDevOptionsEnabled'))
  }
}

function disableDevOptions() {
  setDevOptionsUnlocked(false)
  toast.info(t('about.toastDevOptionsDisabled'))
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
        class="inline-flex items-center justify-center size-24 cursor-pointer select-none rounded-full origin-center active:scale-95 transition-transform"
        :style="{
          transform: `rotate(${logoRotation}deg)`,
          transition: isHoldingLogo ? 'none' : 'transform 600ms cubic-bezier(0.38, 1.21, 0.22, 1.00)',
        }"
        :title="t('about.tapForDevOptions')"
        @mousedown="onLogoMouseDown"
        @mouseup="onLogoMouseUp"
        @mouseleave="onLogoMouseUp"
        @click="onBrandTap"
      >
        <LumenLogo
          :size="96"
          :transparent="true"
        />
      </div>

      <h1
        class="m3-headline-large text-[var(--md-sys-color-primary)] mt-1 cursor-pointer select-none origin-center"
        :style="{
          transform: `scale(${wordmarkScale})`,
          fontWeight: wordmarkWeight,
          letterSpacing: '-0.5px',
          fontVariationSettings: `'ROND' 100, 'wght' ${wordmarkWeight}`,
        }"
        @click="onBrandTap"
      >
        Lumen
      </h1>

      <span class="rounded-full bg-[var(--md-sys-color-primary-container)] px-3 py-0.5 m3-label-small normal-case text-[var(--md-sys-color-on-primary-container)] mt-1.5">
        v{{ appVersion }}
      </span>

      <p class="m3-body-medium mt-2.5 max-w-sm leading-relaxed">
        {{ t('about.tagline') }}
      </p>

      <!-- Action buttons: Source code, Check for updates & Open source licenses -->
      <div class="flex flex-wrap items-center justify-center gap-3 mt-6">
        <M3Button
          variant="tonal"
          size="m"
          icon="open_in_new"
          :aria-label="t('about.viewSourceGithub')"
          @click="openRepo"
        >
          <span>{{ t('about.sourceCode') }}</span>
        </M3Button>

        <M3Button
          variant="filled"
          size="m"
          icon="refresh"
          :disabled="isCheckingUpdates"
          :loading="isCheckingUpdates"
          :aria-label="t('about.checkForUpdatesAria')"
          @click="checkForUpdates"
        >
          <span>{{ isCheckingUpdates ? t('about.checking') : t('about.checkForUpdates') }}</span>
        </M3Button>

        <M3Button
          variant="tonal"
          size="m"
          icon="policy"
          :aria-label="t('about.viewLicensesAria')"
          @click="showLicensesModal = true"
        >
          <span>{{ t('about.openSourceLicenses') }}</span>
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
          {{ t('about.updates') }}
        </h2>
      </div>

      <div class="flex flex-col gap-2">
        <div class="flex items-center justify-between gap-4 rounded-[18px] bg-[var(--md-sys-color-surface-container-low)] p-4 transition-colors hover:bg-[var(--md-sys-color-surface-container-high)]/60">
          <div class="flex flex-col min-w-0 pr-2">
            <label
              for="auto-updates-toggle"
              class="m3-title-medium text-[var(--md-sys-color-on-surface)] cursor-pointer"
            >
              {{ t('about.autoCheckUpdates') }}
            </label>
            <span class="m3-body-medium mt-0.5 leading-relaxed">
              {{ t('about.autoCheckUpdatesDesc') }}
            </span>
          </div>
          <M3Switch
            id="auto-updates-toggle"
            :model-value="preferencesState.autoCheckUpdates"
            :aria-label="t('about.autoUpdatesAria')"
            @update:model-value="handleAutoCheckUpdates"
          />
        </div>

        <div class="flex items-center justify-between rounded-[18px] bg-[var(--md-sys-color-surface-container-low)] px-4 py-3 m3-body-medium">
          <span class="text-[var(--md-sys-color-on-surface)]">{{ t('about.lastChecked') }}</span>
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
            {{ t('about.devOptions') }}
          </h2>
        </div>
        <span class="rounded-full bg-[var(--md-sys-color-primary-container)] px-2.5 py-0.5 m3-label-small text-[var(--md-sys-color-on-primary-container)]">
          {{ t('about.active') }}
        </span>
      </div>

      <div class="flex flex-col gap-2">
        <!-- Mock Mode Toggle (Debug builds only) -->
        <div
          v-if="isDevMode()"
          class="flex items-center justify-between gap-4 rounded-[18px] bg-[var(--md-sys-color-surface-container-low)] p-4 transition-colors hover:bg-[var(--md-sys-color-surface-container-high)]/60"
        >
          <div class="flex flex-col min-w-0 pr-2">
            <label
              for="mock-mode-toggle"
              class="m3-title-medium text-[var(--md-sys-color-on-surface)] cursor-pointer"
            >
              {{ t('about.mockMode') }}
            </label>
            <span class="m3-body-medium mt-0.5 leading-relaxed">
              {{ t('about.mockModeDesc') }}
            </span>
          </div>
          <M3Switch
            id="mock-mode-toggle"
            :model-value="mockState.enabled"
            :aria-label="t('about.mockModeAria')"
            @update:model-value="toggleMockMode"
          />
        </div>

        <!-- Disable Developer Options Button -->
        <div class="flex items-center justify-between rounded-[18px] bg-[var(--md-sys-color-surface-container-low)] p-4">
          <div class="flex flex-col min-w-0 pr-2">
            <span class="m3-title-medium text-[var(--md-sys-color-on-surface)]">
              {{ t('about.lockDevOptions') }}
            </span>
            <span class="m3-body-medium mt-0.5 leading-relaxed">
              {{ t('about.lockDevOptionsDesc') }}
            </span>
          </div>
          <M3Button
            variant="tonal"
            size="s"
            destructive
            :aria-label="t('about.lockDevOptionsAria')"
            @click="disableDevOptions"
          >
            <span>{{ t('about.lock') }}</span>
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
          {{ t('about.credits') }}
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
                :alt="t('about.reiuxAvatarAlt')"
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
                {{ t('about.reiuxDesc') }}
              </span>
            </div>
          </div>
          <M3Button
            variant="tonal"
            size="s"
            icon="open_in_new"
            :aria-label="t('about.reiuxGithubAria')"
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
                :alt="t('about.xob0tAvatarAlt')"
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
                {{ t('about.xob0tDesc') }}
              </span>
            </div>
          </div>
          <M3Button
            variant="tonal"
            size="s"
            icon="open_in_new"
            :aria-label="t('about.gotohpRepoAria')"
            @click="openGotohpRepo"
          >
            <span>{{ t('about.repository') }}</span>
          </M3Button>
        </div>
      </div>

      <p class="m3-body-medium text-[var(--md-sys-color-on-surface-variant)]/80 leading-relaxed border-t border-[var(--md-sys-color-outline-variant)]/20 pt-2.5 px-1">
        {{ t('about.disclaimer') }}
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
