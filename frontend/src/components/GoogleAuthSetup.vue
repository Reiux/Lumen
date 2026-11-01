<script setup lang="ts">
import { computed, nextTick, onMounted, onUnmounted, ref, watch } from 'vue'
import { Browser, Clipboard, Events } from '@wailsio/runtime'
import { toast } from 'vue-sonner'
import { ConfigManager } from '../../bindings/app/core'
import M3Button from './ui/M3Button.vue'
import M3Icon from './ui/M3Icon.vue'
import M3TextField from './ui/M3TextField.vue'
import AccountRiskCard from './AccountRiskCard.vue'
import { displayNameFromEmail, getAvatarColor, getEmailInitial } from '../utils/avatar'

const embeddedSetupURL = 'https://accounts.google.com/EmbeddedSetup'

const emit = defineEmits<{
  (event: 'account-added'): void
}>()

const isOpen = defineModel<boolean>('open', { default: false })

// Workflow state
const authMode = ref<'oauth' | 'direct'>('oauth')
const oauthToken = ref('')
const rawCredential = ref('')
const hasAcknowledgedRisk = ref(false)
const isValidating = ref(false)
const validationError = ref('')
const verifiedUser = ref<{ email: string } | null>(null)

// Browser automation state
const browserState = ref<'idle' | 'waiting' | 'error'>('idle')
const browserErrorMessage = ref('')
const browserCloseFallbackMessage = ref('')
const hasSupportedBrowser = ref(true)
const detectedBrowserName = ref('')
const showManualFallback = ref(false)
let unlistenCloseFailed: (() => void) | null = null

const dialogBodyRef = ref<HTMLElement | null>(null)
const tokenFieldRef = ref<{ focus?: () => void } | null>(null)
let openerElement: HTMLElement | null = null
let successDismissTimer: ReturnType<typeof setTimeout> | null = null

async function checkBrowserSupport() {
  try {
    hasSupportedBrowser.value = await ConfigManager.HasSupportedBrowser()
    if (hasSupportedBrowser.value) {
      detectedBrowserName.value = (await ConfigManager.GetSupportedBrowserName()) || ''
    }
  } catch {
    hasSupportedBrowser.value = false
    detectedBrowserName.value = ''
  }
  if (!hasSupportedBrowser.value) {
    showManualFallback.value = true
  }
}

watch(hasAcknowledgedRisk, (ack) => {
  ConfigManager.SetAccountRiskAcknowledged(ack).catch(() => {})
})

async function handleBrowserSignIn() {
  if (!hasAcknowledgedRisk.value) {
    validationError.value = 'Please check the account safety notice above before signing in.'
    return
  }

  browserState.value = 'waiting'
  browserErrorMessage.value = ''
  browserCloseFallbackMessage.value = ''
  validationError.value = ''

  try {
    await ConfigManager.SetAccountRiskAcknowledged(true)
    const connectedEmail = await ConfigManager.StartBrowserSignIn()
    if (!connectedEmail) {
      throw new Error("Couldn't verify your account: Google didn't return an email.")
    }

    browserState.value = 'idle'
    verifiedUser.value = { email: connectedEmail }

    successDismissTimer = setTimeout(() => {
      isOpen.value = false
      emit('account-added')
      toast.success('Google account connected', {
        description: connectedEmail,
      })
    }, 1500)
  } catch (error: unknown) {
    const errMsg = error instanceof Error ? error.message : String(error)
    if (errMsg.toLowerCase().includes('cancel')) {
      browserState.value = 'idle'
      return
    }
    browserState.value = 'error'
    browserErrorMessage.value = errMsg || "Couldn't complete Google sign-in."
  }
}

async function handleCancelBrowserSignIn() {
  try {
    await ConfigManager.CancelBrowserSignIn()
  } catch {
    // ignore
  }
  browserState.value = 'idle'
}

function handleFallbackToManual() {
  browserState.value = 'idle'
  showManualFallback.value = true
  nextTick(() => {
    tokenFieldRef.value?.focus?.()
  })
}

watch(isOpen, (open) => {
  if (open) {
    openerElement = (document.activeElement as HTMLElement) || null
    validationError.value = ''
    verifiedUser.value = null
    isValidating.value = false
    browserState.value = 'idle'
    browserErrorMessage.value = ''
    browserCloseFallbackMessage.value = ''
    checkBrowserSupport()
    nextTick(() => {
      if (dialogBodyRef.value) {
        dialogBodyRef.value.scrollTop = 0
      }
      tokenFieldRef.value?.focus?.()
    })
  } else {
    if (browserState.value === 'waiting') {
      ConfigManager.CancelBrowserSignIn().catch(() => {})
    }
    oauthToken.value = ''
    rawCredential.value = ''
    validationError.value = ''
    verifiedUser.value = null
    isValidating.value = false
    browserState.value = 'idle'
    browserErrorMessage.value = ''
    if (successDismissTimer) {
      clearTimeout(successDismissTimer)
      successDismissTimer = null
    }
    if (openerElement && typeof openerElement.focus === 'function') {
      openerElement.focus({ preventScroll: true })
    }
    openerElement = null
  }
})

function handleKeydown(e: KeyboardEvent) {
  if (e.key === 'Escape' && isOpen.value) {
    e.stopPropagation()
    isOpen.value = false
  }
}

onMounted(() => {
  window.addEventListener('keydown', handleKeydown)
  unlistenCloseFailed = Events.On('browser-signin:close-failed', () => {
    browserCloseFallbackMessage.value = 'Signed in, you can close the browser'
  })
})

onUnmounted(() => {
  window.removeEventListener('keydown', handleKeydown)
  if (successDismissTimer) clearTimeout(successDismissTimer)
  if (unlistenCloseFailed) {
    unlistenCloseFailed()
    unlistenCloseFailed = null
  }
})

watch([oauthToken, rawCredential], () => {
  if (validationError.value) {
    validationError.value = ''
  }
})

async function openEmbeddedSetup() {
  try {
    await Browser.OpenURL(embeddedSetupURL)
  } catch (error) {
    toast.error('Could not open system browser', {
      description: error instanceof Error ? error.message : String(error),
    })
  }
}

// Automatic clipboard auto-detection
async function pasteFromClipboard() {
  try {
    let text = ''
    try {
      text = await Clipboard.Text()
    } catch {
      text = await navigator.clipboard.readText()
    }
    text = (text || '').trim()
    if (!text) {
      toast.info('Clipboard is empty')
      return
    }

    // Smart detection: Direct credential string
    if (text.includes('Email=') && text.includes('Token=')) {
      authMode.value = 'direct'
      rawCredential.value = text
      toast.success('Detected credential string from clipboard')
      return
    }

    // Smart detection: oauth_token cookie string
    if (text.includes('oauth_token=')) {
      authMode.value = 'oauth'
      const match = text.match(/oauth_token=([^;]+)/)
      oauthToken.value = (match ? match[1] : text).trim().replace(/^["']|["']$/g, '')
      toast.success('Detected oauth_token from clipboard')
      return
    }

    // If active in direct tab and looks like key=value
    if (authMode.value === 'direct') {
      rawCredential.value = text
      toast.success('Pasted into credential field')
    } else {
      oauthToken.value = text
      toast.success('Pasted into token field')
    }
  } catch (err) {
    console.warn('Clipboard read failed:', err)
    toast.error('Could not read clipboard')
  }
}

const isFormValid = computed(() => {
  if (!hasAcknowledgedRisk.value) return false
  if (authMode.value === 'oauth') {
    return oauthToken.value.trim().length > 0
  }
  return rawCredential.value.trim().length > 0
})

async function handleConnect() {
  if (!hasAcknowledgedRisk.value) {
    validationError.value = 'Please check the account safety notice above before signing in.'
    return
  }

  const payload = authMode.value === 'oauth' ? oauthToken.value.trim() : rawCredential.value.trim()
  if (!payload) {
    validationError.value = 'Please enter or paste your token to continue.'
    return
  }

  isValidating.value = true
  validationError.value = ''

  try {
    // Perform immediate live validation probe & storage via backend
    const connectedEmail = await ConfigManager.ValidateAndAddCredentials(payload)
    if (!connectedEmail) {
      throw new Error('Verification failed: Google did not return an authenticated email.')
    }

    // Visual confirmation state
    verifiedUser.value = { email: connectedEmail }
    isValidating.value = false

    // Auto-advance after brief visual confirmation
    successDismissTimer = setTimeout(() => {
      isOpen.value = false
      emit('account-added')
      toast.success('Google account connected', {
        description: connectedEmail,
      })
    }, 1500)
  } catch (error) {
    isValidating.value = false
    const errMsg = error instanceof Error ? error.message : String(error)
    validationError.value = errMsg || 'Could not connect Google account. Please verify credentials.'
  }
}

function handleDoneSuccess() {
  if (successDismissTimer) clearTimeout(successDismissTimer)
  isOpen.value = false
  emit('account-added')
  if (verifiedUser.value?.email) {
    toast.success('Google account connected', {
      description: verifiedUser.value.email,
    })
  }
}
</script>

<template>
  <Teleport to="body">
    <Transition name="m3-dialog-modal">
      <div
        v-if="isOpen"
        class="m3-dialog-wrapper fixed inset-0 z-50 grid place-items-center bg-black/70 p-4 select-none overflow-y-auto"
        style="--wails-draggable: none"
        @click.self="isOpen = false"
      >
        <!-- Centered M3 Modal Dialog -->
        <div
          class="m3-dialog-card relative flex flex-col w-full max-w-lg rounded-[28px] border border-[var(--md-sys-color-outline-variant)]/50 bg-[var(--md-sys-color-surface-container-high)] shadow-2xl outline-none max-h-[calc(100dvh-32px)] overflow-hidden select-none"
          role="dialog"
          aria-modal="true"
          aria-labelledby="google-auth-title"
          style="--wails-draggable: none"
        >
          <!-- Sticky Header -->
          <div class="sticky top-0 z-20 flex shrink-0 items-center justify-between border-b border-[var(--md-sys-color-outline-variant)]/30 bg-[var(--md-sys-color-surface-container-high)] px-6 py-4">
            <div class="flex items-center gap-3">
              <div class="flex size-10 shrink-0 items-center justify-center rounded-2xl bg-[var(--md-sys-color-primary-container)] text-[var(--md-sys-color-primary)]">
                <M3Icon
                  name="lock"
                  :filled="true"
                  :size="20"
                />
              </div>
              <div class="flex flex-col">
                <h2
                  id="google-auth-title"
                  class="m3-headline-small text-[var(--md-sys-color-on-surface)]"
                >
                  Connect Google Photos
                </h2>
                <p class="m3-body-medium">
                  Sign in with your browser to connect your Google Photos account.
                </p>
              </div>
            </div>

            <M3Button
              variant="text"
              size="xs"
              icon="close"
              aria-label="Close dialog"
              class="!size-9 !min-w-0 !p-0"
              @click="isOpen = false"
            />
          </div>

          <!-- Scrollable Body -->
          <div
            ref="dialogBodyRef"
            class="m3-dialog-body flex-1 overflow-y-auto px-6 py-5 flex flex-col gap-5"
          >
            <!-- Step 1: Account Safety Gate -->
            <div class="flex flex-col gap-2">
              <div class="flex items-center justify-between px-1">
                <span class="m3-label-small text-[var(--md-sys-color-primary)]">
                  Step 1: Account Safety Gate
                </span>
                <span
                  v-if="hasAcknowledgedRisk"
                  class="m3-label-small text-emerald-500 flex items-center gap-1"
                >
                  <M3Icon
                    name="check_circle"
                    :size="14"
                  />
                  Confirmed
                </span>
              </div>
              <AccountRiskCard v-model="hasAcknowledgedRisk" />
            </div>

            <!-- SUCCESS STATE: Visual Confirmation -->
            <div
              v-if="verifiedUser"
              class="flex flex-col items-center justify-center gap-4 rounded-2xl bg-[var(--md-sys-color-surface-container)] p-6 border border-emerald-500/40 text-center animate-fade-in"
            >
              <div class="relative">
                <div
                  class="flex size-20 items-center justify-center rounded-full m3-headline-large shadow-lg ring-4 ring-emerald-500/20"
                  :style="{
                    backgroundColor: getAvatarColor(verifiedUser.email).bg,
                    color: getAvatarColor(verifiedUser.email).text,
                  }"
                >
                  {{ getEmailInitial(verifiedUser.email) }}
                </div>
                <div class="absolute -bottom-1 -right-1 flex size-7 items-center justify-center rounded-full bg-emerald-500 text-white shadow-md">
                  <M3Icon
                    name="check"
                    :size="16"
                  />
                </div>
              </div>

              <div class="flex flex-col gap-1">
                <h3 class="m3-title-medium text-[var(--md-sys-color-on-surface)]">
                  {{ displayNameFromEmail(verifiedUser.email) }}
                </h3>
                <p class="m3-body-medium m3-mono">
                  {{ verifiedUser.email }}
                </p>
                <div class="mt-2 inline-flex items-center gap-1.5 rounded-full bg-emerald-500/15 px-3 py-1 m3-label-large text-emerald-500 mx-auto">
                  <M3Icon
                    name="verified"
                    :size="14"
                  />
                  <span>Google Photos access verified</span>
                </div>
                <p
                  v-if="browserCloseFallbackMessage"
                  class="mt-2 m3-body-medium text-[var(--md-sys-color-primary)] text-center animate-fade-in"
                >
                  {{ browserCloseFallbackMessage }}
                </p>
              </div>
            </div>

            <!-- Step 2: Sign-in Options (Gated until risk is acknowledged) -->
            <div
              v-else
              class="flex flex-col gap-4 transition-all duration-200"
              :class="!hasAcknowledgedRisk && 'opacity-40 pointer-events-none select-none filter blur-[0.5px]'"
            >
              <div class="flex items-center justify-between px-1">
                <span class="m3-label-small text-[var(--md-sys-color-primary)]">
                  Step 2: Sign In
                </span>
              </div>

              <!-- Automated Browser Sign-in Section (when supported browser is present) -->
              <div
                v-if="hasSupportedBrowser"
                class="flex flex-col gap-3 rounded-2xl bg-[var(--md-sys-color-surface-container)] p-4 border border-[var(--md-sys-color-outline-variant)]/30"
              >
                <!-- State: IDLE -->
                <div
                  v-if="browserState === 'idle'"
                  class="flex flex-col gap-3"
                >
                  <div class="flex flex-col gap-1">
                    <span class="m3-title-medium text-[var(--md-sys-color-on-surface)]">
                      Automated Sign-in
                    </span>
                    <span class="m3-body-medium leading-relaxed">
                      Opens your Chromium browser to sign in with Google. Lumen detects your sign-in token automatically and closes the window when done.
                    </span>
                  </div>

                  <M3Button
                    variant="filled"
                    size="m"
                    icon="open_in_browser"
                    class="w-full justify-center py-2.5 shadow-xs"
                    :disabled="!hasAcknowledgedRisk || isValidating"
                    @click="handleBrowserSignIn"
                  >
                    <span>Sign in with Google</span>
                  </M3Button>
                </div>

                <!-- State: WAITING -->
                <div
                  v-else-if="browserState === 'waiting'"
                  class="flex flex-col items-center justify-center gap-3 py-3 text-center"
                >
                  <div class="flex size-10 items-center justify-center rounded-full bg-[var(--md-sys-color-primary-container)] text-[var(--md-sys-color-primary)] animate-pulse">
                    <M3Icon
                      name="sync"
                      :size="22"
                      class="animate-spin"
                    />
                  </div>

                  <div class="flex flex-col gap-1">
                    <h4 class="m3-title-medium text-[var(--md-sys-color-on-surface)]">
                      Finish signing in in the browser
                    </h4>
                    <p class="m3-body-medium max-w-sm">
                      Sign in to your Google account in the window that opened. Lumen will connect your account as soon as you finish.
                    </p>
                  </div>

                  <M3Button
                    variant="outlined"
                    size="s"
                    icon="close"
                    class="mt-1"
                    @click="handleCancelBrowserSignIn"
                  >
                    <span>Cancel Sign-in</span>
                  </M3Button>
                </div>

                <!-- State: ERROR -->
                <div
                  v-else-if="browserState === 'error'"
                  class="flex flex-col gap-3 rounded-xl border border-[var(--md-sys-color-error)]/40 bg-[var(--md-sys-color-error-container)]/30 p-3.5"
                >
                  <div class="flex items-start gap-2.5">
                    <M3Icon
                      name="error"
                      :filled="true"
                      :size="18"
                      class="text-[var(--md-sys-color-error)] shrink-0 mt-0.5"
                    />
                    <div class="flex flex-col gap-0.5 min-w-0">
                      <span class="m3-title-medium text-[var(--md-sys-color-error)]">
                        Couldn't sign in
                      </span>
                      <span class="m3-body-medium text-[var(--md-sys-color-on-surface)] leading-relaxed">
                        {{ browserErrorMessage }}
                      </span>
                    </div>
                  </div>

                  <div class="flex items-center gap-2 pt-1">
                    <M3Button
                      variant="filled"
                      size="xs"
                      icon="refresh"
                      @click="handleBrowserSignIn"
                    >
                      <span>Try again</span>
                    </M3Button>
                    <M3Button
                      variant="tonal"
                      size="xs"
                      icon="key"
                      @click="handleFallbackToManual"
                    >
                      <span>Enter token manually</span>
                    </M3Button>
                  </div>
                </div>
              </div>

              <!-- Browser Not Found Notice (when FindBrowser fails) -->
              <div
                v-else
                class="flex items-start gap-2.5 rounded-xl border border-[var(--md-sys-color-outline-variant)]/40 bg-[var(--md-sys-color-surface-container-low)] p-3 m3-body-medium leading-relaxed"
              >
                <M3Icon
                  name="info"
                  :size="16"
                  class="shrink-0 mt-0.5 text-[var(--md-sys-color-primary)]"
                />
                <span>
                  No supported Chromium browser (Chrome, Edge, Brave, or Chromium) was found on your system. You can sign in using manual token entry below.
                </span>
              </div>

              <!-- Manual Fallback Toggle Button (when browser is present and idle) -->
              <div
                v-if="hasSupportedBrowser && browserState === 'idle'"
                class="flex items-center justify-between px-1"
              >
                <M3Button
                  variant="text"
                  size="xs"
                  :icon="showManualFallback ? 'expand_less' : 'expand_more'"
                  @click="showManualFallback = !showManualFallback"
                >
                  {{ showManualFallback ? 'Hide manual token entry' : 'Or enter token manually' }}
                </M3Button>

                <M3Button
                  v-if="showManualFallback"
                  variant="text"
                  size="xs"
                  icon="content_paste"
                  class="text-[var(--md-sys-color-primary)] shrink-0 whitespace-nowrap"
                  @click="pasteFromClipboard"
                >
                  <span>Paste from Clipboard</span>
                </M3Button>
              </div>

              <!-- MANUAL FORM (Visible if showManualFallback is true OR if no browser found) -->
              <div
                v-if="showManualFallback || !hasSupportedBrowser"
                class="flex flex-col gap-4 animate-fade-in"
              >
                <!-- Segmented Method Tabs -->
                <div class="flex rounded-full bg-[var(--md-sys-color-surface-container)] p-1 border border-[var(--md-sys-color-outline-variant)]/30 gap-1">
                  <M3Button
                    size="s"
                    :toggle="true"
                    :selected="authMode === 'oauth'"
                    :variant="authMode === 'oauth' ? 'tonal' : 'text'"
                    icon="open_in_browser"
                    class="flex-1 justify-center"
                    @click="authMode = 'oauth'"
                  >
                    <span>Browser Sign-in</span>
                    <span class="ml-2 inline-flex items-center rounded-full bg-[var(--md-sys-color-primary-container)] px-2 py-0.5 m3-label-small text-[var(--md-sys-color-on-primary-container)]">Manual</span>
                  </M3Button>

                  <M3Button
                    size="s"
                    :toggle="true"
                    :selected="authMode === 'direct'"
                    :variant="authMode === 'direct' ? 'tonal' : 'text'"
                    icon="terminal"
                    class="flex-1 justify-center"
                    @click="authMode = 'direct'"
                  >
                    <span>Direct Token / String</span>
                  </M3Button>
                </div>

                <!-- Option A: Browser Sign-in Tab -->
                <div
                  v-if="authMode === 'oauth'"
                  class="flex flex-col gap-3 rounded-2xl bg-[var(--md-sys-color-surface-container)] p-4 border border-[var(--md-sys-color-outline-variant)]/30"
                >
                  <div class="flex flex-col sm:flex-row items-start sm:items-center justify-between gap-3">
                    <div class="flex flex-col">
                      <span class="m3-title-medium text-[var(--md-sys-color-on-surface)]">
                        1. Open Google Sign-in Page
                      </span>
                      <span class="m3-body-medium">
                        Opens Google Embedded Setup in your browser
                      </span>
                    </div>
                    <M3Button
                      variant="tonal"
                      size="s"
                      icon="open_in_new"
                      class="shrink-0 whitespace-nowrap"
                      @click="openEmbeddedSetup"
                    >
                      <span>Open Sign-in</span>
                    </M3Button>
                  </div>

                  <div class="rounded-xl border border-[var(--md-sys-color-outline-variant)]/20 bg-[var(--md-sys-color-surface-container-low)] p-3 m3-body-medium leading-relaxed">
                    <ol class="list-decimal space-y-1 pl-4">
                      <li>Sign in and click <strong class="text-[var(--md-sys-color-on-surface)]">I agree</strong>.</li>
                      <li>Open DevTools (<kbd class="rounded bg-[var(--md-sys-color-surface-container-highest)] px-1 m3-mono m3-label-small">F12</kbd>) &rarr; <strong class="text-[var(--md-sys-color-on-surface)]">Application/Storage</strong> &rarr; <strong class="text-[var(--md-sys-color-on-surface)]">Cookies</strong>.</li>
                      <li>Copy and paste the <code class="rounded bg-[var(--md-sys-color-surface-container-highest)] px-1 m3-mono m3-title-medium text-[var(--md-sys-color-primary)]">oauth_token</code> cookie value below.</li>
                    </ol>
                  </div>

                  <div class="relative w-full pt-1">
                    <M3TextField
                      id="google-oauth-token"
                      ref="tokenFieldRef"
                      v-model="oauthToken"
                      type="password"
                      label="oauth_token Cookie Value"
                      placeholder="Paste oauth_token here"
                      variant="outlined"
                      leading-icon="key"
                      :disabled="isValidating"
                      @keydown.enter="handleConnect"
                    />
                    <M3Button
                      variant="text"
                      size="xs"
                      icon="content_paste"
                      aria-label="Paste from clipboard"
                      title="Paste from clipboard"
                      class="!absolute right-2 top-3.5 !size-9 !min-w-0 !p-0"
                      @click="pasteFromClipboard"
                    />
                  </div>
                </div>

                <!-- Option B: Direct Token / Credential Tab -->
                <div
                  v-else
                  class="flex flex-col gap-3 rounded-2xl bg-[var(--md-sys-color-surface-container)] p-4 border border-[var(--md-sys-color-outline-variant)]/30"
                >
                  <div class="flex flex-col">
                    <span class="m3-title-medium text-[var(--md-sys-color-on-surface)]">
                      Paste Captured Credential
                    </span>
                    <span class="m3-body-medium mt-0.5 leading-relaxed">
                      Import credentials from ReVanced, Android logcat, or HTTP Toolkit. Both full query strings (<code class="m3-mono m3-body-medium">androidId=...&Email=...&Token=...</code>) and shorthand (<code class="m3-mono m3-body-medium">Email=...&Token=...</code>) are supported.
                    </span>
                  </div>

                  <div class="relative w-full">
                    <M3TextField
                      id="google-raw-credential"
                      v-model="rawCredential"
                      type="password"
                      label="Credential String or Token"
                      placeholder="Email=user@gmail.com&Token=..."
                      variant="outlined"
                      leading-icon="terminal"
                      :disabled="isValidating"
                      @keydown.enter="handleConnect"
                    />
                    <M3Button
                      variant="text"
                      size="xs"
                      icon="content_paste"
                      aria-label="Paste from clipboard"
                      title="Paste from clipboard"
                      class="!absolute right-2 top-2.5 !size-9 !min-w-0 !p-0"
                      @click="pasteFromClipboard"
                    />
                  </div>
                </div>
              </div>

              <!-- Error Banner -->
              <div
                v-if="validationError"
                class="flex items-start gap-2.5 rounded-xl border border-[var(--md-sys-color-error)]/50 bg-[var(--md-sys-color-error-container)] p-3 m3-body-medium text-[var(--md-sys-color-on-error-container)] shadow-xs"
                role="alert"
              >
                <M3Icon
                  name="error"
                  :filled="true"
                  :size="18"
                  class="shrink-0 mt-0.5 text-[var(--md-sys-color-error)]"
                />
                <div class="flex flex-col min-w-0">
                  <span class="m3-title-medium">Validation Failed</span>
                  <span class="m3-body-medium leading-relaxed opacity-95">{{ validationError }}</span>
                </div>
              </div>
            </div>
          </div>

          <!-- Sticky Footer -->
          <div class="sticky bottom-0 z-20 shrink-0 border-t border-[var(--md-sys-color-outline-variant)]/30 bg-[var(--md-sys-color-surface-container-high)] px-6 py-4 flex items-center justify-end gap-2.5">
            <template v-if="verifiedUser">
              <M3Button
                variant="filled"
                size="m"
                class="w-full"
                @click="handleDoneSuccess"
              >
                <span>Done</span>
              </M3Button>
            </template>
            <template v-else>
              <M3Button
                variant="outlined"
                size="m"
                :disabled="isValidating"
                @click="isOpen = false"
              >
                <span>Cancel</span>
              </M3Button>
              <M3Button
                variant="filled"
                size="m"
                class="min-w-36 shrink-0 whitespace-nowrap"
                :disabled="!isFormValid || isValidating"
                :loading="isValidating"
                aria-label="Validate and connect Google account"
                @click="handleConnect"
              >
                <span>{{ isValidating ? 'Verifying with Google...' : 'Verify & Connect' }}</span>
              </M3Button>
            </template>
          </div>
        </div>
      </div>
    </Transition>
  </Teleport>
</template>
