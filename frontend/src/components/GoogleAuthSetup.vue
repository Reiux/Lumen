<script setup lang="ts">
import { computed, nextTick, onMounted, onUnmounted, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { Browser, Clipboard, Events } from '@wailsio/runtime'
import { toast } from 'vue-sonner'
import { ConfigManager } from '../../bindings/app/core'
import M3Button from './ui/M3Button.vue'
import M3Icon from './ui/M3Icon.vue'
import M3TextField from './ui/M3TextField.vue'
import AccountRiskCard from './AccountRiskCard.vue'
import { displayNameFromEmail, getAvatarColor, getEmailInitial } from '../utils/avatar'
import { useDialogFocusTrap } from '../utils/useDialogFocusTrap'

const { t } = useI18n()

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

// Sign-in state
const browserState = ref<'idle' | 'waiting' | 'waiting-external' | 'error'>('idle')
const browserErrorMessage = ref('')
const browserCloseFallbackMessage = ref('')
const isInlineEmbeddedSupported = ref(false)
const isEmbeddedViewOpen = ref(false)
const hasExternalChromium = ref(false)
const detectedBrowserName = ref('')
const showManualFallback = ref(false)
let unlistenCloseFailed: (() => void) | null = null

const dialogCardRef = ref<HTMLElement | null>(null)
const dialogBodyRef = ref<HTMLElement | null>(null)
const embeddedHostRef = ref<HTMLElement | null>(null)
const tokenFieldRef = ref<{ focus?: () => void } | null>(null)
let successDismissTimer: ReturnType<typeof setTimeout> | null = null
let embeddedResizeObserver: ResizeObserver | null = null
let embeddedResizeRaf: number | null = null

useDialogFocusTrap(
  () => isOpen.value,
  dialogCardRef,
  {
    onEscape: () => {
      if (isEmbeddedViewOpen.value) {
        handleCancelBrowserSignIn()
      } else {
        isOpen.value = false
      }
    },
    initialFocus: () => tokenFieldRef.value,
  },
)

function readEmbeddedHostBounds() {
  const el = embeddedHostRef.value
  if (!el) return null
  const rect = el.getBoundingClientRect()
  if (rect.width < 100 || rect.height < 100) return null
  return {
    x: Math.round(rect.left),
    y: Math.round(rect.top),
    width: Math.round(rect.width),
    height: Math.round(rect.height),
  }
}

function syncEmbeddedBounds() {
  if (!isEmbeddedViewOpen.value) return
  if (embeddedResizeRaf !== null) {
    cancelAnimationFrame(embeddedResizeRaf)
  }
  embeddedResizeRaf = requestAnimationFrame(() => {
    embeddedResizeRaf = null
    if (!isEmbeddedViewOpen.value) return
    const bounds = readEmbeddedHostBounds()
    if (bounds) {
      ConfigManager.UpdateEmbeddedSignInBounds(bounds).catch(() => {})
    }
  })
}

function stopEmbeddedBoundsSync() {
  if (embeddedResizeRaf !== null) {
    cancelAnimationFrame(embeddedResizeRaf)
    embeddedResizeRaf = null
  }
  if (embeddedResizeObserver) {
    embeddedResizeObserver.disconnect()
    embeddedResizeObserver = null
  }
  if (typeof window !== 'undefined') {
    window.removeEventListener('resize', syncEmbeddedBounds)
  }
}

function startEmbeddedBoundsSync() {
  stopEmbeddedBoundsSync()
  if (typeof ResizeObserver !== 'undefined' && embeddedHostRef.value) {
    embeddedResizeObserver = new ResizeObserver(() => {
      syncEmbeddedBounds()
    })
    embeddedResizeObserver.observe(embeddedHostRef.value)
  }
  if (typeof window !== 'undefined') {
    window.addEventListener('resize', syncEmbeddedBounds)
  }
}

async function checkBrowserSupport() {
  try {
    const [inlineSupported, hasChromium] = await Promise.all([
      ConfigManager.IsInlineEmbeddedSignInSupported().catch(() => false),
      ConfigManager.HasSupportedBrowser().catch(() => false),
    ])
    const browserName = hasChromium ? (await ConfigManager.GetSupportedBrowserName().catch(() => '')) || 'Chrome' : ''
    if (!isOpen.value) return
    isInlineEmbeddedSupported.value = !!inlineSupported
    hasExternalChromium.value = !!hasChromium
    detectedBrowserName.value = browserName
  } catch {
    if (!isOpen.value) return
    isInlineEmbeddedSupported.value = false
    hasExternalChromium.value = false
    detectedBrowserName.value = ''
  }
}

watch(hasAcknowledgedRisk, (ack) => {
  ConfigManager.SetAccountRiskAcknowledged(ack).catch(() => {})
})

let browserAttemptSeq = 0

function handleSignInSuccess(connectedEmail: string) {
  stopEmbeddedBoundsSync()
  isEmbeddedViewOpen.value = false
  browserState.value = 'idle'
  verifiedUser.value = { email: connectedEmail }

  successDismissTimer = setTimeout(() => {
    if (!verifiedUser.value) return
    const email = verifiedUser.value.email
    verifiedUser.value = null
    isOpen.value = false
    emit('account-added')
    toast.success(t('googleAuth.toastConnected'), {
      description: email,
    })
  }, 1500)
}

async function handleEmbeddedSignIn() {
  if (!hasAcknowledgedRisk.value) {
    validationError.value = t('googleAuth.validationSafetyNotice')
    return
  }

  const attemptSeq = ++browserAttemptSeq
  browserErrorMessage.value = ''
  browserCloseFallbackMessage.value = ''
  validationError.value = ''
  showManualFallback.value = false

  try {
    await ConfigManager.SetAccountRiskAcknowledged(true)
    if (!isOpen.value || attemptSeq !== browserAttemptSeq) return

    let bounds: { x: number; y: number; width: number; height: number } | null = null
    if (isInlineEmbeddedSupported.value) {
      isEmbeddedViewOpen.value = true
      browserState.value = 'waiting'
      await nextTick()
      await new Promise<void>((resolve) => {
        requestAnimationFrame(() => requestAnimationFrame(() => resolve()))
      })
      if (!isOpen.value || attemptSeq !== browserAttemptSeq) return
      bounds = readEmbeddedHostBounds()
      if (bounds) {
        startEmbeddedBoundsSync()
      } else {
        isEmbeddedViewOpen.value = false
      }
    } else {
      isEmbeddedViewOpen.value = false
      browserState.value = 'waiting'
    }

    const connectedEmail = await ConfigManager.StartEmbeddedSignIn(bounds)
    if (attemptSeq !== browserAttemptSeq) return
    if (!isOpen.value) {
      stopEmbeddedBoundsSync()
      isEmbeddedViewOpen.value = false
      if (connectedEmail) {
        emit('account-added')
      }
      return
    }
    if (!connectedEmail) {
      throw new Error(t('googleAuth.couldNotVerifyNoEmail'))
    }

    handleSignInSuccess(connectedEmail)
  } catch (error: unknown) {
    stopEmbeddedBoundsSync()
    isEmbeddedViewOpen.value = false
    if (!isOpen.value || attemptSeq !== browserAttemptSeq) return
    const errMsg = error instanceof Error ? error.message : String(error)
    if (errMsg.toLowerCase().includes('cancel')) {
      browserState.value = 'idle'
      return
    }
    browserState.value = 'error'
    browserErrorMessage.value = errMsg || t('googleAuth.couldNotSignIn')
  }
}

async function handleExternalBrowserSignIn() {
  if (!hasAcknowledgedRisk.value) {
    validationError.value = t('googleAuth.validationSafetyNotice')
    return
  }

  const attemptSeq = ++browserAttemptSeq
  stopEmbeddedBoundsSync()
  isEmbeddedViewOpen.value = false
  try {
    await ConfigManager.CancelBrowserSignIn()
  } catch {
    // ignore prior session cleanup errors
  }
  if (!isOpen.value || attemptSeq !== browserAttemptSeq) return

  browserState.value = 'waiting-external'
  browserErrorMessage.value = ''
  browserCloseFallbackMessage.value = ''
  validationError.value = ''
  showManualFallback.value = false

  try {
    await ConfigManager.SetAccountRiskAcknowledged(true)
    if (!isOpen.value || attemptSeq !== browserAttemptSeq) return
    const connectedEmail = await ConfigManager.StartExternalBrowserSignIn()
    if (attemptSeq !== browserAttemptSeq) return
    if (!isOpen.value) {
      if (connectedEmail) {
        emit('account-added')
      }
      return
    }
    if (!connectedEmail) {
      throw new Error(t('googleAuth.couldNotVerifyNoEmail'))
    }

    handleSignInSuccess(connectedEmail)
  } catch (error: unknown) {
    if (!isOpen.value || attemptSeq !== browserAttemptSeq) return
    const errMsg = error instanceof Error ? error.message : String(error)
    if (errMsg.toLowerCase().includes('cancel')) {
      browserState.value = 'idle'
      return
    }
    browserState.value = 'error'
    browserErrorMessage.value = errMsg || t('googleAuth.couldNotSignIn')
  }
}

async function handleCancelBrowserSignIn() {
  browserAttemptSeq++
  stopEmbeddedBoundsSync()
  isEmbeddedViewOpen.value = false
  browserState.value = 'idle'
  try {
    await ConfigManager.CancelBrowserSignIn()
  } catch {
    // ignore
  }
}

async function handleFallbackToManual() {
  if (isEmbeddedViewOpen.value || browserState.value === 'waiting' || browserState.value === 'waiting-external') {
    await handleCancelBrowserSignIn()
  }
  browserState.value = 'idle'
  showManualFallback.value = true
  nextTick(() => {
    tokenFieldRef.value?.focus?.()
  })
}

watch(isOpen, (open) => {
  if (open) {
    validationError.value = ''
    verifiedUser.value = null
    isValidating.value = false
    browserState.value = 'idle'
    isEmbeddedViewOpen.value = false
    browserErrorMessage.value = ''
    browserCloseFallbackMessage.value = ''
    showManualFallback.value = false
    checkBrowserSupport()
    nextTick(() => {
      if (dialogBodyRef.value) {
        dialogBodyRef.value.scrollTop = 0
      }
    })
  } else {
    stopEmbeddedBoundsSync()
    if (isEmbeddedViewOpen.value || browserState.value === 'waiting' || browserState.value === 'waiting-external') {
      browserAttemptSeq++
      isEmbeddedViewOpen.value = false
      ConfigManager.CancelBrowserSignIn().catch(() => {})
    }
    if (verifiedUser.value) {
      const email = verifiedUser.value.email
      verifiedUser.value = null
      emit('account-added')
      toast.success(t('googleAuth.toastConnected'), {
        description: email,
      })
    }
    oauthToken.value = ''
    rawCredential.value = ''
    validationError.value = ''
    verifiedUser.value = null
    isValidating.value = false
    browserState.value = 'idle'
    browserErrorMessage.value = ''
    browserCloseFallbackMessage.value = ''
    showManualFallback.value = false
    if (successDismissTimer) {
      clearTimeout(successDismissTimer)
      successDismissTimer = null
    }
  }
})

onMounted(() => {
  unlistenCloseFailed = Events.On('browser-signin:close-failed', () => {
    browserCloseFallbackMessage.value = t('googleAuth.signedInCanClose')
  })
})

onUnmounted(() => {
  stopEmbeddedBoundsSync()
  if (isEmbeddedViewOpen.value || browserState.value === 'waiting' || browserState.value === 'waiting-external') {
    ConfigManager.CancelBrowserSignIn().catch(() => {})
  }
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
    toast.error(t('googleAuth.couldNotOpenBrowser'), {
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
      toast.info(t('googleAuth.toastClipboardEmpty'))
      return
    }

    // Smart detection: Direct credential string
    if (text.includes('Email=') && text.includes('Token=')) {
      authMode.value = 'direct'
      rawCredential.value = text
      toast.success(t('googleAuth.toastDetectedCredential'))
      return
    }

    // Smart detection: oauth_token cookie string
    if (text.includes('oauth_token=')) {
      authMode.value = 'oauth'
      const match = text.match(/oauth_token=([^;]+)/)
      oauthToken.value = (match ? match[1] : text).trim().replace(/^["']|["']$/g, '')
      toast.success(t('googleAuth.toastDetectedToken'))
      return
    }

    // If active in direct tab and looks like key=value
    if (authMode.value === 'direct') {
      rawCredential.value = text
      toast.success(t('googleAuth.toastPastedCredential'))
    } else {
      oauthToken.value = text
      toast.success(t('googleAuth.toastPastedToken'))
    }
  } catch (err) {
    console.warn('Clipboard read failed:', err)
    toast.error(t('googleAuth.toastClipboardError'))
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
  if (isValidating.value || verifiedUser.value) return
  if (!hasAcknowledgedRisk.value) {
    validationError.value = t('googleAuth.validationSafetyNotice')
    return
  }

  const payload = authMode.value === 'oauth' ? oauthToken.value.trim() : rawCredential.value.trim()
  if (!payload) {
    validationError.value = t('googleAuth.validationEnterToken')
    return
  }

  isValidating.value = true
  validationError.value = ''

  try {
    // Perform immediate live validation probe & storage via backend
    const connectedEmail = await ConfigManager.ValidateAndAddCredentials(payload)
    if (!isOpen.value) {
      if (connectedEmail) {
        emit('account-added')
      }
      return
    }
    if (!connectedEmail) {
      throw new Error(t('googleAuth.couldNotVerifyNoEmail'))
    }

    // Visual confirmation state
    verifiedUser.value = { email: connectedEmail }
    isValidating.value = false

    // Auto-advance after brief visual confirmation
    successDismissTimer = setTimeout(() => {
      if (!verifiedUser.value) return
      const email = verifiedUser.value.email
      verifiedUser.value = null
      isOpen.value = false
      emit('account-added')
      toast.success(t('googleAuth.toastConnected'), {
        description: email,
      })
    }, 1500)
  } catch (error) {
    if (!isOpen.value) return
    isValidating.value = false
    const errMsg = error instanceof Error ? error.message : String(error)
    validationError.value = errMsg || t('googleAuth.validationErrorGeneric')
  }
}

function handleDoneSuccess() {
  if (successDismissTimer) clearTimeout(successDismissTimer)
  const email = verifiedUser.value?.email
  verifiedUser.value = null
  isOpen.value = false
  emit('account-added')
  if (email) {
    toast.success(t('googleAuth.toastConnected'), {
      description: email,
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
          ref="dialogCardRef"
          tabindex="-1"
          class="m3-dialog-card relative flex flex-col w-full rounded-[28px] border border-[var(--md-sys-color-outline-variant)]/50 bg-[var(--md-sys-color-surface-container-high)] shadow-2xl outline-none max-h-[calc(100dvh-32px)] overflow-hidden select-none transition-all duration-200"
          :class="isEmbeddedViewOpen ? 'max-w-[540px] h-[min(740px,calc(100dvh-32px))]' : 'max-w-lg'"
          role="dialog"
          aria-modal="true"
          aria-labelledby="google-auth-title"
          style="--wails-draggable: none"
        >
          <!-- M3E Embedded Sign-In View (inline child WebView host on Windows & macOS) -->
          <template v-if="isEmbeddedViewOpen">
            <div class="flex shrink-0 items-center justify-between border-b border-[var(--md-sys-color-outline-variant)]/30 bg-[var(--md-sys-color-surface-container-high)] px-4 py-3">
              <div class="flex items-center gap-2.5 min-w-0">
                <M3Button
                  variant="text"
                  size="xs"
                  icon="arrow_back"
                  :aria-label="t('googleAuth.backToOptionsAria')"
                  class="!size-9 !min-w-0 !p-0 shrink-0"
                  @click="handleCancelBrowserSignIn"
                />
                <div class="flex size-8 shrink-0 items-center justify-center rounded-xl bg-[var(--md-sys-color-primary-container)] text-[var(--md-sys-color-primary)]">
                  <M3Icon
                    name="lock"
                    :filled="true"
                    :size="16"
                  />
                </div>
                <div class="flex flex-col min-w-0">
                  <h2
                    id="google-auth-title"
                    class="m3-title-medium text-[var(--md-sys-color-on-surface)] truncate"
                  >
                    {{ t('googleAuth.automatedSignIn') }}
                  </h2>
                  <p class="m3-body-small text-[var(--md-sys-color-on-surface-variant)] truncate">
                    {{ t('googleAuth.embeddedSignInStatus') }}
                  </p>
                </div>
              </div>

              <M3Button
                variant="text"
                size="xs"
                icon="close"
                :aria-label="t('googleAuth.closeAria')"
                class="!size-9 !min-w-0 !p-0 shrink-0"
                @click="isOpen = false"
              />
            </div>

            <!-- Native Child WebView Host Rectangle -->
            <div
              ref="embeddedHostRef"
              class="relative flex-1 w-full bg-[#121316] flex flex-col items-center justify-center gap-3"
            >
              <div class="flex size-10 items-center justify-center rounded-full bg-[var(--md-sys-color-primary-container)] text-[var(--md-sys-color-primary)] animate-pulse">
                <M3Icon
                  name="sync"
                  :size="22"
                  class="animate-spin"
                />
              </div>
              <span class="m3-body-medium text-[var(--md-sys-color-on-surface-variant)]">
                {{ t('googleAuth.loadingGoogleSignIn') }}
              </span>
            </div>

            <!-- M3E Embedded Footer Fallback Bar -->
            <div class="flex shrink-0 items-center justify-between gap-2 border-t border-[var(--md-sys-color-outline-variant)]/30 bg-[var(--md-sys-color-surface-container)] px-4 py-2.5">
              <M3Button
                variant="text"
                size="xs"
                @click="handleCancelBrowserSignIn"
              >
                <span>{{ t('googleAuth.cancel') }}</span>
              </M3Button>

              <M3Button
                v-if="hasExternalChromium"
                variant="tonal"
                size="xs"
                icon="open_in_browser"
                @click="handleExternalBrowserSignIn"
              >
                <span>{{ t('googleAuth.externalBrowserFallback', { browser: detectedBrowserName || 'Chrome' }) }}</span>
              </M3Button>
              <M3Button
                v-else
                variant="text"
                size="xs"
                icon="key"
                @click="handleFallbackToManual"
              >
                <span>{{ t('googleAuth.havingTroubleManual') }}</span>
              </M3Button>
            </div>
          </template>

          <template v-else>
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
                    {{ t('googleAuth.title') }}
                  </h2>
                  <p class="m3-body-medium">
                    {{ t('googleAuth.automatedSignInDesc') }}
                  </p>
                </div>
              </div>

              <M3Button
                variant="text"
                size="xs"
                icon="close"
                :aria-label="t('googleAuth.closeAria')"
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
                    {{ t('googleAuth.step1Title') }}
                  </span>
                  <span
                    v-if="hasAcknowledgedRisk"
                    class="m3-label-small text-emerald-500 flex items-center gap-1"
                  >
                    <M3Icon
                      name="check_circle"
                      :size="14"
                    />
                    {{ t('googleAuth.confirmed') }}
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
                    <span>{{ t('googleAuth.verified') }}</span>
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
                    {{ t('googleAuth.step2Title') }}
                  </span>
                </div>

                <!-- Primary In-App Google Sign-in Card -->
                <div class="flex flex-col gap-3 rounded-2xl bg-[var(--md-sys-color-surface-container)] p-4 border border-[var(--md-sys-color-outline-variant)]/30">
                  <!-- State: IDLE -->
                  <div
                    v-if="browserState === 'idle'"
                    class="flex flex-col gap-3"
                  >
                    <div class="flex flex-col gap-1">
                      <span class="m3-title-medium text-[var(--md-sys-color-on-surface)]">
                        {{ t('googleAuth.automatedSignIn') }}
                      </span>
                      <span class="m3-body-medium leading-relaxed">
                        {{ t('googleAuth.automatedSignInDesc') }}
                      </span>
                    </div>

                    <M3Button
                      variant="filled"
                      size="m"
                      icon="login"
                      class="w-full justify-center py-2.5 shadow-xs"
                      :disabled="!hasAcknowledgedRisk || isValidating"
                      @click="handleEmbeddedSignIn"
                    >
                      <span>{{ t('googleAuth.signInWithBrowser') }}</span>
                    </M3Button>
                  </div>

                  <!-- State: WAITING (Linux popup WebView or External Chromium Fallback) -->
                  <div
                    v-else-if="browserState === 'waiting' || browserState === 'waiting-external'"
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
                        {{ t('googleAuth.finishInBrowser') }}
                      </h4>
                      <p class="m3-body-medium max-w-sm">
                        {{ t('googleAuth.finishInBrowserDesc') }}
                      </p>
                    </div>

                    <div class="flex flex-wrap items-center justify-center gap-2 mt-1">
                      <M3Button
                        variant="outlined"
                        size="s"
                        icon="close"
                        @click="handleCancelBrowserSignIn"
                      >
                        <span>{{ t('googleAuth.cancelSignIn') }}</span>
                      </M3Button>
                      <M3Button
                        v-if="browserState === 'waiting' && hasExternalChromium"
                        variant="text"
                        size="s"
                        icon="open_in_browser"
                        @click="handleExternalBrowserSignIn"
                      >
                        <span>{{ t('googleAuth.externalBrowserFallback', { browser: detectedBrowserName || 'Chrome' }) }}</span>
                      </M3Button>
                    </div>
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
                          {{ t('googleAuth.couldNotSignIn') }}
                        </span>
                        <span class="m3-body-medium text-[var(--md-sys-color-on-surface)] leading-relaxed">
                          {{ browserErrorMessage }}
                        </span>
                      </div>
                    </div>

                    <div class="flex flex-wrap items-center gap-2 pt-1">
                      <M3Button
                        variant="filled"
                        size="xs"
                        icon="refresh"
                        @click="handleEmbeddedSignIn"
                      >
                        <span>{{ t('googleAuth.tryAgain') }}</span>
                      </M3Button>
                      <M3Button
                        v-if="hasExternalChromium"
                        variant="tonal"
                        size="xs"
                        icon="open_in_browser"
                        @click="handleExternalBrowserSignIn"
                      >
                        <span>{{ t('googleAuth.signInWithExternalBrowser', { browser: detectedBrowserName || 'Chrome' }) }}</span>
                      </M3Button>
                      <M3Button
                        variant="text"
                        size="xs"
                        icon="key"
                        @click="handleFallbackToManual"
                      >
                        <span>{{ t('googleAuth.enterTokenManually') }}</span>
                      </M3Button>
                    </div>
                  </div>
                </div>

                <!-- Secondary Fallback Links (Manual + External Browser) -->
                <div
                  v-if="browserState === 'idle'"
                  class="flex flex-wrap items-center justify-between gap-2 px-1"
                >
                  <M3Button
                    variant="text"
                    size="xs"
                    :icon="showManualFallback ? 'expand_less' : 'expand_more'"
                    @click="showManualFallback = !showManualFallback"
                  >
                    {{ showManualFallback ? t('googleAuth.hideManual') : t('googleAuth.showManual') }}
                  </M3Button>

                  <div class="flex items-center gap-1">
                    <M3Button
                      v-if="!showManualFallback && hasExternalChromium"
                      variant="text"
                      size="xs"
                      icon="open_in_browser"
                      class="shrink-0 whitespace-nowrap"
                      @click="handleExternalBrowserSignIn"
                    >
                      <span>{{ t('googleAuth.signInWithExternalBrowser', { browser: detectedBrowserName || 'Chrome' }) }}</span>
                    </M3Button>
                    <M3Button
                      v-if="showManualFallback"
                      variant="text"
                      size="xs"
                      icon="content_paste"
                      class="text-[var(--md-sys-color-primary)] shrink-0 whitespace-nowrap"
                      @click="pasteFromClipboard"
                    >
                      <span>{{ t('googleAuth.pasteClipboard') }}</span>
                    </M3Button>
                  </div>
                </div>

                <!-- MANUAL FORM (Visible if showManualFallback is true) -->
                <div
                  v-if="showManualFallback"
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
                      <span>{{ t('googleAuth.tabBrowser') }}</span>
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
                      <span>{{ t('googleAuth.tabDirect') }}</span>
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
                          {{ t('googleAuth.step1Browser') }}
                        </span>
                        <span class="m3-body-medium">
                          {{ t('googleAuth.step1BrowserDesc') }}
                        </span>
                      </div>
                      <M3Button
                        variant="tonal"
                        size="s"
                        icon="open_in_new"
                        class="shrink-0 whitespace-nowrap"
                        @click="openEmbeddedSetup"
                      >
                        <span>{{ t('googleAuth.openSignIn') }}</span>
                      </M3Button>
                    </div>

                    <div class="rounded-xl border border-[var(--md-sys-color-outline-variant)]/20 bg-[var(--md-sys-color-surface-container-low)] p-3 m3-body-medium leading-relaxed">
                      <ol class="list-decimal space-y-1 pl-4">
                        <li>{{ t('googleAuth.instr1') }} <strong class="text-[var(--md-sys-color-on-surface)]">{{ t('googleAuth.instr1Bold') }}</strong>.</li>
                        <li>{{ t('googleAuth.instr2') }}</li>
                        <li>{{ t('googleAuth.instr3') }}</li>
                      </ol>
                    </div>

                    <div class="relative w-full pt-1">
                      <M3TextField
                        id="google-oauth-token"
                        ref="tokenFieldRef"
                        v-model="oauthToken"
                        type="password"
                        :label="t('googleAuth.cookieLabel')"
                        :placeholder="t('googleAuth.cookiePlaceholder')"
                        variant="outlined"
                        leading-icon="key"
                        :disabled="isValidating"
                        @keydown.enter="handleConnect"
                      />
                      <M3Button
                        variant="text"
                        size="xs"
                        icon="content_paste"
                        :aria-label="t('googleAuth.pasteClipboard')"
                        :title="t('googleAuth.pasteClipboard')"
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
                        {{ t('googleAuth.directTitle') }}
                      </span>
                      <span class="m3-body-medium mt-0.5 leading-relaxed">
                        {{ t('googleAuth.directDesc') }}
                      </span>
                    </div>

                    <div class="relative w-full">
                      <M3TextField
                        id="google-raw-credential"
                        v-model="rawCredential"
                        type="password"
                        :label="t('googleAuth.directLabel')"
                        :placeholder="t('googleAuth.directPlaceholder')"
                        variant="outlined"
                        leading-icon="terminal"
                        :disabled="isValidating"
                        @keydown.enter="handleConnect"
                      />
                      <M3Button
                        variant="text"
                        size="xs"
                        icon="content_paste"
                        :aria-label="t('googleAuth.pasteClipboard')"
                        :title="t('googleAuth.pasteClipboard')"
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
                    <span class="m3-title-medium">{{ t('googleAuth.validationFailed') }}</span>
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
                  <span>{{ t('googleAuth.done') }}</span>
                </M3Button>
              </template>
              <template v-else>
                <M3Button
                  variant="outlined"
                  size="m"
                  :disabled="isValidating"
                  @click="isOpen = false"
                >
                  <span>{{ t('googleAuth.cancel') }}</span>
                </M3Button>
                <M3Button
                  variant="filled"
                  size="m"
                  class="min-w-36 shrink-0 whitespace-nowrap"
                  :disabled="!isFormValid || isValidating"
                  :loading="isValidating"
                  :aria-label="t('googleAuth.verifyConnect')"
                  @click="handleConnect"
                >
                  <span>{{ isValidating ? t('googleAuth.verifying') : t('googleAuth.verifyConnect') }}</span>
                </M3Button>
              </template>
            </div>
          </template>
        </div>
      </div>
    </Transition>
  </Teleport>
</template>
