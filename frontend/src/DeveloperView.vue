<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue'
import {
  clearMockCircuitBreaker,
  detectDebugBuild,
  initMockMode,
  isDevMode,
  mockState,
  setMockView,
  toggleMockMode,
  triggerMockCircuitBreaker,
} from './utils/mockMode'
import { replayLaunch } from './utils/launchState'
import {
  FLICKER_FLAGS,
  isFlickerFlagActive,
  setFlickerFlag,
  type FlickerFlag,
} from './utils/flickerDebug'
import { ConfigManager } from '../bindings/app/core'
import type { LogEntry } from '../bindings/app/core/models'
import { Events } from '@wailsio/runtime'
import { toast } from 'vue-sonner'
import { preferencesState, refreshPreferences, setNotifyOnComplete } from './utils/configStore'
import M3Button from './components/ui/M3Button.vue'
import M3Icon from './components/ui/M3Icon.vue'
import M3Switch from './components/ui/M3Switch.vue'
import M3TextField from './components/ui/M3TextField.vue'

interface Props {
  liveLogs?: LogEntry[]
}

const props = withDefaults(defineProps<Props>(), {
  liveLogs: () => [],
})

const activeSection = ref<'previews' | 'circuit' | 'logs' | 'flicker'>(isDevMode() ? 'previews' : 'logs')

watch(() => isDevMode(), (dev) => {
  if (!dev) {
    activeSection.value = 'logs'
  }
})

const flagsState = ref<Record<FlickerFlag, boolean>>({
  noanim: isFlickerFlagActive('noanim'),
  nobackdrop: isFlickerFlagActive('nobackdrop'),
  noripple: isFlickerFlagActive('noripple'),
  nostate: isFlickerFlagActive('nostate'),
  nolaunch: isFlickerFlagActive('nolaunch'),
  nowave: isFlickerFlagActive('nowave'),
})

function toggleFlag(flag: FlickerFlag) {
  const next = !flagsState.value[flag]
  flagsState.value[flag] = next
  setFlickerFlag(flag, next)
}

async function openDevTools() {
  try {
    await ConfigManager.ToggleDevTools()
  } catch (err) {
    console.error('Failed to toggle devtools:', err)
  }
}

// Logs section state
const logLevel = ref<'ALL' | 'INFO' | 'WARN' | 'ERROR'>('ALL')
const searchQuery = ref('')
const fetchedLogs = ref<LogEntry[]>([])
const isFetchingLogs = ref(false)
const copied = ref(false)
const maskFilenames = ref(false)

function anonymizeFileName(fileName?: string): string {
  if (!fileName) return ''
  const dotIndex = fileName.lastIndexOf('.')
  const ext = dotIndex !== -1 ? fileName.substring(dotIndex) : ''
  let hash = 0
  for (let i = 0; i < fileName.length; i++) {
    hash = (hash * 31 + fileName.charCodeAt(i)) >>> 0
  }
  const hex = hash.toString(16).padStart(4, '0').slice(-4)
  return `media_${hex}${ext}`
}

function sanitizeUserMessage(msg?: string, fileName?: string): string {
  if (!msg) return ''
  let sanitized = msg
  if (fileName) {
    const masked = anonymizeFileName(fileName)
    sanitized = sanitized.replaceAll(fileName, masked)
  }
  sanitized = sanitized.replace(/(?:[a-zA-Z]:[/\\]|\/)[^\s:]+[/\\]([^/\s:]+)/g, (_match, base) => {
    return anonymizeFileName(base)
  })
  return sanitized
}

async function loadBackendLogs() {
  isFetchingLogs.value = true
  try {
    const res = await ConfigManager.GetLogEntries(1000, 'ALL')
    if (Array.isArray(res)) {
      fetchedLogs.value = res
    }
  } catch {
    // Non-wails / fallback
  } finally {
    isFetchingLogs.value = false
  }
}

const allLogs = computed(() => {
  if (fetchedLogs.value.length > 0) {
    return fetchedLogs.value
  }
  return props.liveLogs
})

const filteredLogs = computed(() => {
  return allLogs.value.filter((entry) => {
    if (logLevel.value !== 'ALL' && entry.level !== logLevel.value) {
      return false
    }
    if (!searchQuery.value) return true
    const q = searchQuery.value.toLowerCase()
    return (
      (entry.userMessage || '').toLowerCase().includes(q) ||
      (entry.rawError || '').toLowerCase().includes(q) ||
      (entry.fileName || '').toLowerCase().includes(q) ||
      (entry.category || '').toLowerCase().includes(q)
    )
  })
})

function copyLogs() {
  const text = filteredLogs.value
    .map((e) => {
      const fileName = maskFilenames.value && e.fileName ? anonymizeFileName(e.fileName) : e.fileName
      const rawMsg = e.userMessage || e.rawError || ''
      const msg = maskFilenames.value ? sanitizeUserMessage(rawMsg, e.fileName) : rawMsg
      return `[${new Date(e.timestamp).toISOString()}] [${e.level}] [${e.category}] ${fileName ? fileName + ': ' : ''}${msg}`
    })
    .join('\n')
  navigator.clipboard.writeText(text)
  copied.value = true
  setTimeout(() => {
    copied.value = false
  }, 1500)
}

function clearLogsView() {
  fetchedLogs.value = []
}

const isTestingNotification = ref(false)

async function testNotification() {
  isTestingNotification.value = true
  try {
    Events.Emit('dev:test-notification')
    toast.info('Test notification sent')
  } catch {
    toast.error('Failed to trigger notification')
  } finally {
    setTimeout(() => {
      isTestingNotification.value = false
    }, 500)
  }
}

onMounted(async () => {
  await detectDebugBuild()
  if (!isDevMode()) {
    activeSection.value = 'logs'
  } else {
    initMockMode()
  }
  loadBackendLogs()
  refreshPreferences()
})
</script>

<template>
  <div class="flex h-full w-full flex-1 flex-col select-none p-2 mx-auto max-w-[1500px] gap-6">
    <!-- Header -->
    <div class="flex flex-col sm:flex-row sm:items-center justify-between gap-4">
      <div class="flex flex-col gap-1">
        <div class="flex items-center gap-2">
          <M3Icon
            name="terminal"
            :size="26"
            class="text-[var(--md-sys-color-primary)]"
          />
          <h1 class="m3-headline-large text-[var(--md-sys-color-on-surface)]">
            Developer tools
          </h1>
          <span
            v-if="isDevMode()"
            class="rounded-full bg-[var(--md-sys-color-tertiary-container)] text-[var(--md-sys-color-on-tertiary-container)] px-2.5 py-0.5 m3-label-small"
          >
            Debug
          </span>
        </div>
        <p class="m3-body-medium leading-relaxed">
          {{ isDevMode() ? 'View logs, preview app screens, and test error handling.' : 'View application logs and system events.' }}
        </p>
      </div>

      <!-- Header Action Buttons -->
      <div class="flex items-center gap-3">
        <M3Button
          v-if="isDevMode()"
          variant="tonal"
          size="s"
          icon="terminal"
          title="Toggle Webview Developer Tools (F12)"
          @click="openDevTools"
        >
          DevTools
        </M3Button>
        <M3Button
          v-if="isDevMode()"
          :variant="mockState.enabled ? 'filled' : 'outlined'"
          size="s"
          icon="science"
          @click="toggleMockMode"
        >
          {{ mockState.enabled ? 'Mock mode: active' : 'Enable mock mode' }}
        </M3Button>
        <M3Button
          variant="tonal"
          size="s"
          icon="replay"
          title="Replay launch splash animation"
          @click="replayLaunch"
        >
          Replay launch
        </M3Button>
      </div>
    </div>

    <!-- Section Navigation Tabs (M3 Expressive segmented group - debug builds only) -->
    <div
      v-if="isDevMode()"
      class="flex flex-wrap items-center gap-2 p-1 rounded-full bg-[var(--md-sys-color-surface-container)] border border-[var(--md-sys-color-outline-variant)]/40 w-fit"
    >
      <M3Button
        size="s"
        :toggle="true"
        :selected="activeSection === 'previews'"
        :variant="activeSection === 'previews' ? 'filled' : 'text'"
        icon="visibility"
        @click="activeSection = 'previews'"
      >
        Previews
      </M3Button>

      <M3Button
        size="s"
        :toggle="true"
        :selected="activeSection === 'circuit'"
        :variant="activeSection === 'circuit' ? 'filled' : 'text'"
        icon="gpp_maybe"
        @click="activeSection = 'circuit'"
      >
        Circuit breakers
      </M3Button>

      <M3Button
        size="s"
        :toggle="true"
        :selected="activeSection === 'logs'"
        :variant="activeSection === 'logs' ? 'filled' : 'text'"
        icon="list_alt"
        @click="activeSection = 'logs'"
      >
        Logs
      </M3Button>

      <M3Button
        size="s"
        :toggle="true"
        :selected="activeSection === 'flicker'"
        :variant="activeSection === 'flicker' ? 'filled' : 'text'"
        icon="tune"
        @click="activeSection = 'flicker'"
      >
        Graphics toggles
      </M3Button>
    </div>

    <!-- SECTION 1: Mock Previews -->
    <div
      v-if="activeSection === 'previews'"
      class="flex flex-col gap-6"
    >
      <div class="rounded-[28px] border border-[var(--md-sys-color-outline-variant)]/40 bg-[var(--md-sys-color-surface-container)] p-6 shadow-xs flex flex-col gap-4">
        <div class="flex flex-col gap-1">
          <h2 class="m3-headline-small text-[var(--md-sys-color-on-surface)] flex items-center gap-2">
            <M3Icon name="preview" :size="20" class="text-[var(--md-sys-color-primary)]" />
            Screen Previews
          </h2>
          <p class="m3-body-medium">
            Preview app screens with test data without uploading any files.
          </p>
        </div>

        <div class="flex flex-wrap gap-2.5 pt-2">
          <M3Button
            size="m"
            :toggle="true"
            :selected="mockState.currentView === 'empty'"
            :variant="mockState.currentView === 'empty' ? 'filled' : 'tonal'"
            icon="crop_square"
            @click="setMockView('empty')"
          >
            Empty state
          </M3Button>

          <M3Button
            size="m"
            :toggle="true"
            :selected="mockState.currentView === 'drop'"
            :variant="mockState.currentView === 'drop' ? 'filled' : 'tonal'"
            icon="cloud_upload"
            @click="setMockView('drop')"
          >
            Drop screen
          </M3Button>

          <M3Button
            size="m"
            :toggle="true"
            :selected="mockState.currentView === 'indexing'"
            :variant="mockState.currentView === 'indexing' ? 'filled' : 'tonal'"
            icon="manage_search"
            @click="setMockView('indexing')"
          >
            Indexing screen
          </M3Button>

          <M3Button
            size="m"
            :toggle="true"
            :selected="mockState.currentView === 'album'"
            :variant="mockState.currentView === 'album' ? 'filled' : 'tonal'"
            icon="folder_special"
            @click="setMockView('album')"
          >
            Album prompt
          </M3Button>

          <M3Button
            size="m"
            :toggle="true"
            :selected="mockState.currentView === 'uploading'"
            :variant="mockState.currentView === 'uploading' ? 'filled' : 'tonal'"
            icon="sync"
            @click="setMockView('uploading')"
          >
            Uploading view
          </M3Button>

          <M3Button
            size="m"
            :toggle="true"
            :selected="mockState.currentView === 'results'"
            :variant="mockState.currentView === 'results' ? 'filled' : 'tonal'"
            icon="check_circle"
            @click="setMockView('results')"
          >
            Results screen
          </M3Button>
        </div>
      </div>

      <!-- Desktop Notifications Test Card -->
      <div class="rounded-[28px] border border-[var(--md-sys-color-outline-variant)]/40 bg-[var(--md-sys-color-surface-container)] p-6 shadow-xs flex flex-col gap-4">
        <div class="flex flex-col sm:flex-row sm:items-center justify-between gap-4">
          <div class="flex flex-col gap-1">
            <h2 class="m3-headline-small text-[var(--md-sys-color-on-surface)] flex items-center gap-2">
              <M3Icon name="notifications" :size="20" class="text-[var(--md-sys-color-primary)]" />
              Desktop Notifications
            </h2>
            <p class="m3-body-medium">
              Toggle desktop notifications and test the system notification daemon.
            </p>
          </div>

          <div class="flex flex-wrap items-center gap-4">
            <div class="flex items-center gap-3 bg-[var(--md-sys-color-surface-container-high)] px-3.5 py-1.5 rounded-full border border-[var(--md-sys-color-outline-variant)]/30">
              <span class="m3-label-large text-[var(--md-sys-color-on-surface)]">
                Notify on complete
              </span>
              <M3Switch
                id="dev-notifications-switch"
                :model-value="preferencesState.notifyOnComplete"
                aria-label="Toggle notifications in settings"
                @update:model-value="setNotifyOnComplete"
              />
            </div>

            <M3Button
              variant="filled"
              size="s"
              icon="notifications_active"
              :disabled="isTestingNotification"
              @click="testNotification"
            >
              Test notification
            </M3Button>
          </div>
        </div>
      </div>
    </div>

    <!-- SECTION 2: Circuit Breaker Simulation -->
    <div
      v-if="activeSection === 'circuit'"
      class="flex flex-col gap-6"
    >
      <div class="rounded-[28px] border border-[var(--md-sys-color-outline-variant)]/40 bg-[var(--md-sys-color-surface-container)] p-6 shadow-xs flex flex-col gap-4">
        <div class="flex flex-col gap-1">
          <h2 class="m3-headline-small text-[var(--md-sys-color-on-surface)] flex items-center gap-2">
            <M3Icon name="gpp_maybe" :size="20" class="text-amber-500" />
            Circuit Breakers
          </h2>
          <p class="m3-body-medium">
            Test error banners, rate limiting pauses, and account restriction handling.
          </p>
        </div>

        <div class="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-3 pt-2">
          <div class="rounded-[20px] bg-[var(--md-sys-color-surface-container-low)] p-4 flex flex-col justify-between gap-3 border border-[var(--md-sys-color-outline-variant)]/20">
            <div>
              <span class="m3-title-medium text-amber-400">401 Sign In Required</span>
              <p class="m3-body-medium mt-1">
                Session expired or revoked. Shows reconnect prompt.
              </p>
            </div>
            <M3Button
              variant="tonal"
              size="s"
              icon="login"
              @click="triggerMockCircuitBreaker('SIGN_IN_REQUIRED')"
            >
              Trigger 401
            </M3Button>
          </div>

          <div class="rounded-[20px] bg-[var(--md-sys-color-surface-container-low)] p-4 flex flex-col justify-between gap-3 border border-[var(--md-sys-color-outline-variant)]/20">
            <div>
              <span class="m3-title-medium text-red-400">403 Account Restricted</span>
              <p class="m3-body-medium mt-1">
                Account restricted or Google storage quota full.
              </p>
            </div>
            <M3Button
              variant="tonal"
              destructive
              size="s"
              icon="block"
              @click="triggerMockCircuitBreaker('ACCOUNT_RESTRICTED')"
            >
              Trigger 403
            </M3Button>
          </div>

          <div class="rounded-[20px] bg-[var(--md-sys-color-surface-container-low)] p-4 flex flex-col justify-between gap-3 border border-[var(--md-sys-color-outline-variant)]/20">
            <div>
              <span class="m3-title-medium text-blue-400">429 Rate Limited</span>
              <p class="m3-body-medium mt-1">
                Upload rate limit reached. Pauses and backs off automatically.
              </p>
            </div>
            <M3Button
              variant="tonal"
              size="s"
              icon="speed"
              @click="triggerMockCircuitBreaker('RATE_LIMITED')"
            >
              Trigger 429
            </M3Button>
          </div>

          <div class="rounded-[20px] bg-[var(--md-sys-color-surface-container-low)] p-4 flex flex-col justify-between gap-3 border border-[var(--md-sys-color-outline-variant)]/20">
            <div>
              <span class="m3-title-medium text-[var(--md-sys-color-on-surface)]">Reset State</span>
              <p class="m3-body-medium mt-1">
                Reset error banners and circuit breakers back to normal.
              </p>
            </div>
            <M3Button
              variant="outlined"
              size="s"
              icon="check_circle"
              @click="clearMockCircuitBreaker"
            >
              Clear state
            </M3Button>
          </div>
        </div>
      </div>
    </div>

    <!-- SECTION 3: Live Logs -->
    <div
      v-if="activeSection === 'logs'"
      class="flex flex-col gap-4 flex-1 min-h-0"
    >
      <div class="rounded-[28px] border border-[var(--md-sys-color-outline-variant)]/40 bg-[var(--md-sys-color-surface-container)] p-6 shadow-xs flex flex-col gap-4 flex-1 min-h-0">
        <!-- Controls Bar -->
        <div class="flex flex-col sm:flex-row sm:items-center justify-between gap-3">
          <!-- Level Filters -->
          <div class="flex items-center gap-1.5 p-1 rounded-full bg-[var(--md-sys-color-surface-container-low)]">
            <M3Button
              v-for="lvl in (['ALL', 'INFO', 'WARN', 'ERROR'] as const)"
              :key="lvl"
              size="xs"
              :toggle="true"
              :selected="logLevel === lvl"
              :variant="logLevel === lvl ? 'filled' : 'text'"
              @click="logLevel = lvl"
            >
              {{ lvl }}
            </M3Button>
          </div>

          <div class="flex flex-wrap items-center gap-2 min-w-0">
            <M3TextField
              id="dev-log-search"
              v-model="searchQuery"
              placeholder="Search logs..."
              variant="outlined"
              leading-icon="search"
              class="w-full sm:w-48 md:w-56"
            />

            <div class="flex flex-wrap items-center gap-2 shrink-0">
              <M3Button
                variant="tonal"
                size="s"
                icon="refresh"
                :disabled="isFetchingLogs"
                @click="loadBackendLogs"
              >
                Refresh
              </M3Button>

              <M3Button
                variant="tonal"
                size="s"
                icon="content_copy"
                @click="copyLogs"
              >
                {{ copied ? 'Copied' : 'Copy' }}
              </M3Button>

              <M3Button
                variant="tonal"
                size="s"
                :toggle="true"
                :selected="maskFilenames"
                :icon="maskFilenames ? 'visibility_off' : 'visibility'"
                @click="maskFilenames = !maskFilenames"
              >
                {{ maskFilenames ? 'Anonymized' : 'Anonymize' }}
              </M3Button>

              <M3Button
                variant="outlined"
                size="s"
                destructive
                icon="clear_all"
                @click="clearLogsView"
              >
                Clear
              </M3Button>
            </div>
          </div>
        </div>

        <!-- Log Stream List -->
        <div class="flex flex-col gap-1.5 overflow-y-auto max-h-[550px] rounded-2xl bg-[var(--md-sys-color-surface-container-low)] p-3 border border-[var(--md-sys-color-outline-variant)]/20 m3-mono m3-body-medium flex-1 min-h-0">
          <div
            v-for="(entry, index) in filteredLogs"
            :key="`${entry.timestamp}-${index}`"
            class="flex items-start gap-2.5 p-2 rounded-lg hover:bg-[var(--md-sys-color-surface-container-high)]/60 transition-colors"
          >
            <span class="m3-label-small text-[var(--md-sys-color-on-surface-variant)]/70 tabular-nums shrink-0 mt-0.5">
              {{ new Date(entry.timestamp).toLocaleTimeString() }}
            </span>

            <span
              class="rounded px-1.5 py-0.2 m3-badge-label shrink-0 mt-0.5"
              :class="{
                'bg-[var(--md-sys-color-primary-container)] text-[var(--md-sys-color-on-primary-container)]': entry.level === 'INFO',
                'bg-amber-500/20 text-amber-300': entry.level === 'WARN',
                'bg-[var(--md-sys-color-error-container)] text-[var(--md-sys-color-on-error-container)]': entry.level === 'ERROR',
              }"
            >
              {{ entry.level }}
            </span>

            <span class="rounded bg-[var(--md-sys-color-surface-container-high)] px-1.5 py-0.2 m3-badge-label text-[var(--md-sys-color-on-surface-variant)] shrink-0 mt-0.5">
              {{ entry.category || 'App' }}
            </span>

            <div class="flex flex-col min-w-0 flex-1">
              <span v-if="entry.fileName" class="m3-title-medium text-[var(--md-sys-color-on-surface)] truncate">
                {{ maskFilenames ? anonymizeFileName(entry.fileName) : entry.fileName }}
              </span>
              <span class="m3-body-medium text-[var(--md-sys-color-on-surface)] break-all">
                {{ maskFilenames ? sanitizeUserMessage(entry.userMessage || entry.rawError, entry.fileName) : (entry.userMessage || entry.rawError || '') }}
              </span>
            </div>
          </div>

          <div
            v-if="filteredLogs.length === 0"
            class="p-8 text-center m3-body-medium"
          >
            No log entries match the current filter.
          </div>
        </div>
      </div>
    </div>

    <!-- SECTION 4: Graphics & Animation Toggles -->
    <div
      v-if="activeSection === 'flicker'"
      class="flex flex-col gap-6"
    >
      <div class="rounded-[28px] border border-[var(--md-sys-color-outline-variant)]/40 bg-[var(--md-sys-color-surface-container)] p-6 shadow-xs flex flex-col gap-4">
        <div class="flex flex-col gap-1">
          <h2 class="m3-headline-small text-[var(--md-sys-color-on-surface)] flex items-center gap-2">
            <M3Icon name="tune" :size="20" class="text-[var(--md-sys-color-primary)]" />
            Graphics & Animation Toggles
          </h2>
          <p class="m3-body-medium">
            Turn off specific animations or visual effects to troubleshoot rendering issues on Linux and Windows.
          </p>
        </div>

        <div class="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-3.5 pt-2">
          <div
            v-for="flag in FLICKER_FLAGS"
            :key="flag"
            class="flex items-center justify-between gap-3 rounded-[20px] bg-[var(--md-sys-color-surface-container-low)] p-4 border border-[var(--md-sys-color-outline-variant)]/20"
          >
            <div class="flex flex-col min-w-0 pr-2">
              <span class="m3-mono m3-title-medium text-[var(--md-sys-color-on-surface)]">
                ?{{ flag }}=1
              </span>
              <span class="m3-body-medium mt-0.5">
                {{
                  flag === 'noanim' ? 'Disable CSS and spring animations' :
                  flag === 'nobackdrop' ? 'Disable backdrop blur filters' :
                  flag === 'noripple' ? 'Disable touch ripple effects' :
                  flag === 'nostate' ? 'Disable hover state layers' :
                  flag === 'nolaunch' ? 'Skip startup splash animation' :
                  'Disable wavy progress bar rendering'
                }}
              </span>
            </div>
            <M3Switch
              :id="`flag-${flag}`"
              :model-value="flagsState[flag]"
              :aria-label="`Toggle ${flag} flicker flag`"
              @update:model-value="toggleFlag(flag)"
            />
          </div>
        </div>
      </div>
    </div>
  </div>
</template>
