<script setup lang="ts">
import { nextTick, onMounted, onUnmounted, ref, watch } from 'vue'
import { Browser, Clipboard } from '@wailsio/runtime'
import { toast } from 'vue-sonner'
import {
  AlertTriangle,
  ChevronDown,
  ClipboardPaste,
  ExternalLink,
  KeyRound,
  Loader2,
  X,
} from '@lucide/vue'
import { ConfigManager } from '../../bindings/app/core'
import Button from './ui/button/Button.vue'

const embeddedSetupURL = 'https://accounts.google.com/EmbeddedSetup'

const emit = defineEmits<{
  (event: 'account-added'): void
}>()

const isOpen = defineModel<boolean>('open', { default: false })
const oauthToken = ref('')
const rawCredential = ref('')
const isConnecting = ref(false)
const isAddingRawCredential = ref(false)
const showAdvanced = ref(false)

// Visible validation & server errors
const tokenError = ref('')
const connectionError = ref('')
const rawCredentialError = ref('')

let openerElement: HTMLElement | null = null

watch(isOpen, (open) => {
  if (open) {
    openerElement = (document.activeElement as HTMLElement) || null
    nextTick(() => {
      const input = document.getElementById('google-oauth-token')
      input?.focus()
    })
  } else {
    oauthToken.value = ''
    rawCredential.value = ''
    showAdvanced.value = false
    tokenError.value = ''
    connectionError.value = ''
    rawCredentialError.value = ''
    if (openerElement && typeof openerElement.focus === 'function') {
      openerElement.focus()
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
})

onUnmounted(() => {
  window.removeEventListener('keydown', handleKeydown)
})

watch(oauthToken, (val) => {
  if (val.trim()) {
    tokenError.value = ''
  }
  connectionError.value = ''
})

watch(rawCredential, (val) => {
  if (val.trim()) {
    rawCredentialError.value = ''
  }
})

async function openEmbeddedSetup() {
  try {
    await Browser.OpenURL(embeddedSetupURL)
  } catch (error) {
    toast.error('Could not open the system browser', {
      description: error instanceof Error ? error.message : String(error),
    })
  }
}

async function pasteToken() {
  try {
    let text = ''
    try {
      text = await Clipboard.Text()
    } catch {
      text = await navigator.clipboard.readText()
    }
    if (text) {
      oauthToken.value = text.trim()
      tokenError.value = ''
      toast.success('Pasted from clipboard')
    }
  } catch (err) {
    console.warn('Clipboard read failed:', err)
  }
}

async function pasteRawCredential() {
  try {
    let text = ''
    try {
      text = await Clipboard.Text()
    } catch {
      text = await navigator.clipboard.readText()
    }
    if (text) {
      rawCredential.value = text.trim()
      rawCredentialError.value = ''
      toast.success('Pasted from clipboard')
    }
  } catch (err) {
    console.warn('Clipboard read failed:', err)
  }
}

async function connectAccount() {
  const normalizedToken = oauthToken.value.trim()
  if (!normalizedToken) {
    tokenError.value = 'Please enter or paste the oauth_token cookie value.'
    return
  }

  isConnecting.value = true
  connectionError.value = ''
  try {
    const connectedEmail = await ConfigManager.AddGoogleAccount(normalizedToken)
    oauthToken.value = ''
    isOpen.value = false
    emit('account-added')
    toast.success('Google account connected.', {
      description: connectedEmail,
    })
  } catch (error) {
    const errMsg = error instanceof Error ? error.message : String(error)
    connectionError.value = errMsg || 'Could not connect Google account. Please verify the token.'
  } finally {
    isConnecting.value = false
  }
}

async function addRawCredential() {
  const credential = rawCredential.value.trim()
  if (!credential) {
    rawCredentialError.value = 'Please enter or paste a valid captured credential string.'
    return
  }

  isAddingRawCredential.value = true
  rawCredentialError.value = ''
  try {
    await ConfigManager.AddCredentials(credential)
    rawCredential.value = ''
    isOpen.value = false
    emit('account-added')
    toast.success('Credentials added.')
  } catch (error) {
    const errMsg = error instanceof Error ? error.message : String(error)
    rawCredentialError.value = errMsg || 'Could not add credentials. Check the captured format.'
  } finally {
    isAddingRawCredential.value = false
  }
}
</script>

<template>
  <Teleport to="body">
    <Transition name="m3-dialog-modal">
      <div
        v-if="isOpen"
        class="fixed inset-0 z-50 grid place-items-center bg-black/65 backdrop-blur-xs p-4 select-none overflow-y-auto"
        style="--wails-draggable: none"
        @click.self="isOpen = false"
      >
        <!-- Centered M3 Modal Dialog -->
        <div
          class="m3-dialog-card relative w-full max-w-lg rounded-[32px] border border-[var(--md-sys-color-outline-variant)]/50 bg-[var(--md-sys-color-surface-container)] p-6 shadow-2xl outline-none max-h-[90vh] overflow-y-auto select-none"
          role="dialog"
          aria-modal="true"
          aria-labelledby="google-auth-title"
          style="--wails-draggable: none"
        >
          <!-- Header -->
          <div class="flex items-start justify-between pb-4">
            <div class="flex items-center gap-3">
              <div class="flex size-11 items-center justify-center rounded-2xl bg-[var(--md-sys-color-primary-container)] text-[var(--md-sys-color-on-primary-container)]">
                <KeyRound class="size-5" />
              </div>
              <div class="flex flex-col">
                <h2
                  id="google-auth-title"
                  class="text-lg font-bold tracking-tight text-[var(--md-sys-color-on-surface)]"
                >
                  Connect Google Photos
                </h2>
                <p class="text-xs text-[var(--md-sys-color-on-surface-variant)]">
                  Embedded setup sign-in & credential management
                </p>
              </div>
            </div>

            <button
              type="button"
              class="flex size-8 cursor-pointer items-center justify-center rounded-full text-[var(--md-sys-color-on-surface-variant)] transition-colors hover:bg-[var(--md-sys-color-surface-container-high)] hover:text-[var(--md-sys-color-on-surface)]"
              aria-label="Close dialog"
              @click="isOpen = false"
            >
              <X class="size-4" />
            </button>
          </div>

          <div class="flex flex-col gap-6 pt-1">
            <!-- Step 1: Open Google Sign-in -->
            <div class="flex flex-col gap-3">
              <div class="flex items-center gap-2.5">
                <span class="flex size-6 shrink-0 items-center justify-center rounded-full bg-[var(--md-sys-color-primary)] text-xs font-bold text-[var(--md-sys-color-on-primary)] shadow-xs">
                  1
                </span>
                <h2 class="text-sm font-semibold text-[var(--md-sys-color-on-surface)]">
                  Open the Google sign-in page
                </h2>
              </div>

              <!-- Tonal card instructions -->
              <div class="rounded-2xl border border-[var(--md-sys-color-outline-variant)]/30 bg-[var(--md-sys-color-surface-container-low)] p-4 text-xs leading-relaxed text-[var(--md-sys-color-on-surface-variant)]">
                <ol class="list-decimal space-y-1.5 pl-4">
                  <li>
                    Click the button below to open Google's Embedded Setup page in your default browser.
                  </li>
                  <li>
                    Sign in with your account and click <strong class="text-[var(--md-sys-color-on-surface)]">I agree</strong>. (The page may keep loading with a spinner; that is expected.)
                  </li>
                  <li>
                    Open DevTools (<kbd class="rounded bg-[var(--md-sys-color-surface-container-highest)] px-1 font-mono text-[10px]">F12</kbd>), navigate to <strong class="text-[var(--md-sys-color-on-surface)]">Application</strong> &rarr; <strong class="text-[var(--md-sys-color-on-surface)]">Cookies</strong> &rarr; <code class="font-mono text-[11px] text-[var(--md-sys-color-on-surface)]">accounts.google.com</code>.
                  </li>
                  <li>
                    Copy the cookie value for <code class="rounded-full bg-[var(--md-sys-color-surface-container-highest)] px-2.5 py-0.5 font-mono text-[11px] font-semibold text-[var(--md-sys-color-primary)]">oauth_token</code>.
                  </li>
                </ol>
              </div>

              <Button
                type="button"
                variant="outline"
                class="h-10 rounded-full px-5 text-xs font-semibold self-start cursor-pointer select-none"
                :disabled="isConnecting"
                aria-label="Open Google sign-in page in default browser"
                @click="openEmbeddedSetup"
              >
                <ExternalLink class="size-4 mr-2 text-[var(--md-sys-color-primary)]" />
                <span>Open Google Sign-in</span>
              </Button>
            </div>

            <!-- Step 2: Paste Token -->
            <div class="flex flex-col gap-3">
              <div class="flex items-center gap-2.5">
                <span class="flex size-6 shrink-0 items-center justify-center rounded-full bg-[var(--md-sys-color-primary)] text-xs font-bold text-[var(--md-sys-color-on-primary)] shadow-xs">
                  2
                </span>
                <h2 class="text-sm font-semibold text-[var(--md-sys-color-on-surface)]">
                  Paste the oauth_token cookie value
                </h2>
              </div>

              <!-- Outlined Token Field with Floating Label & Paste Action -->
              <div class="flex flex-col gap-1">
                <div
                  class="m3-outlined-field relative rounded-2xl border bg-[var(--md-sys-color-surface-container-low)] transition-all focus-within:border-2"
                  :class="[
                    tokenError
                      ? 'border-[var(--md-sys-color-error)] focus-within:border-[var(--md-sys-color-error)] m3-field-error'
                      : 'border-[var(--md-sys-color-outline)] focus-within:border-[var(--md-sys-color-primary)]',
                  ]"
                >
                  <input
                    id="google-oauth-token"
                    v-model="oauthToken"
                    type="password"
                    autocomplete="off"
                    spellcheck="false"
                    placeholder=" "
                    class="peer w-full rounded-2xl bg-transparent px-4 pt-5 pb-2 pr-12 text-sm text-[var(--md-sys-color-on-surface)] outline-none"
                    :disabled="isConnecting"
                    @keydown.enter="connectAccount"
                  >
                  <label
                    for="google-oauth-token"
                    class="pointer-events-none absolute left-4 top-2 text-[11px] font-medium transition-all peer-placeholder-shown:top-3.5 peer-placeholder-shown:text-xs peer-focus:top-2 peer-focus:text-[11px]"
                    :class="tokenError
                      ? 'text-[var(--md-sys-color-error)]'
                      : 'text-[var(--md-sys-color-primary)] peer-placeholder-shown:text-[var(--md-sys-color-on-surface-variant)]'"
                  >
                    oauth_token Cookie Value
                  </label>

                  <!-- Paste Action Icon Button -->
                  <button
                    type="button"
                    class="absolute right-2.5 top-2.5 flex size-8 cursor-pointer items-center justify-center rounded-full text-[var(--md-sys-color-on-surface-variant)] transition-colors hover:bg-[var(--md-sys-color-surface-container-high)] hover:text-[var(--md-sys-color-on-surface)]"
                    aria-label="Paste token from clipboard"
                    title="Paste from clipboard"
                    @click="pasteToken"
                  >
                    <ClipboardPaste class="size-4" />
                  </button>
                </div>

                <!-- Visible validation error -->
                <p
                  v-if="tokenError"
                  class="m3-error-text px-2 text-xs font-medium text-[var(--md-sys-color-error)]"
                >
                  {{ tokenError }}
                </p>
              </div>

              <!-- Error-container Snackbar for Server Failures -->
              <div
                v-if="connectionError"
                class="flex items-start gap-2.5 rounded-2xl border border-[var(--md-sys-color-error)]/50 bg-[var(--md-sys-color-error-container)] p-3.5 text-xs text-[var(--md-sys-color-on-error-container)] shadow-xs"
                role="alert"
              >
                <AlertTriangle class="size-4 shrink-0 mt-0.5 text-[var(--md-sys-color-error)]" />
                <div class="flex flex-col min-w-0">
                  <span class="font-bold">Connection Failed</span>
                  <span class="text-[11px] leading-relaxed opacity-95">{{ connectionError }}</span>
                </div>
              </div>

              <!-- Connect Account Button -->
              <Button
                type="button"
                class="mt-1 h-11 w-full rounded-full text-sm font-semibold shadow-sm cursor-pointer select-none bg-[var(--md-sys-color-primary)] text-[var(--md-sys-color-on-primary)] hover:bg-[var(--md-sys-color-primary)]/90 disabled:opacity-40 disabled:cursor-not-allowed"
                :disabled="!oauthToken.trim() || isConnecting"
                :loading="isConnecting"
                aria-label="Connect Google account"
                @click="connectAccount"
              >
                <span>{{ isConnecting ? 'Connecting...' : 'Connect Account' }}</span>
              </Button>
            </div>
          </div>

          <!-- Collapsible Section: Paste a captured credential instead -->
          <div class="mt-6 border-t border-[var(--md-sys-color-outline-variant)]/30 pt-4">
            <button
              type="button"
              class="flex w-full cursor-pointer items-center justify-between text-xs font-semibold text-[var(--md-sys-color-on-surface-variant)] transition-colors hover:text-[var(--md-sys-color-on-surface)]"
              aria-label="Toggle captured credential input section"
              :aria-expanded="showAdvanced"
              @click="showAdvanced = !showAdvanced"
            >
              <span>Paste a captured credential instead</span>
              <ChevronDown
                class="size-4 transition-transform duration-200"
                :class="showAdvanced && 'rotate-180'"
              />
            </button>

            <div
              v-if="showAdvanced"
              class="mt-4 flex flex-col gap-3"
            >
              <div
                class="relative rounded-2xl border bg-[var(--md-sys-color-surface-container-low)] transition-all focus-within:border-2"
                :class="rawCredentialError
                  ? 'border-[var(--md-sys-color-error)] focus-within:border-[var(--md-sys-color-error)]'
                  : 'border-[var(--md-sys-color-outline)] focus-within:border-[var(--md-sys-color-primary)]'"
              >
                <input
                  id="google-raw-credential"
                  v-model="rawCredential"
                  type="password"
                  autocomplete="off"
                  spellcheck="false"
                  placeholder=" "
                  class="peer w-full rounded-2xl bg-transparent px-4 pt-5 pb-2 pr-12 text-sm text-[var(--md-sys-color-on-surface)] outline-none"
                  :disabled="isAddingRawCredential"
                  @keydown.enter="addRawCredential"
                >
                <label
                  for="google-raw-credential"
                  class="pointer-events-none absolute left-4 top-2 text-[11px] font-medium transition-all peer-placeholder-shown:top-3.5 peer-placeholder-shown:text-xs peer-focus:top-2 peer-focus:text-[11px]"
                  :class="rawCredentialError
                    ? 'text-[var(--md-sys-color-error)]'
                    : 'text-[var(--md-sys-color-primary)] peer-placeholder-shown:text-[var(--md-sys-color-on-surface-variant)]'"
                >
                  Captured credential (androidId=...&Email=...)
                </label>

                <button
                  type="button"
                  class="absolute right-2.5 top-2.5 flex size-8 cursor-pointer items-center justify-center rounded-full text-[var(--md-sys-color-on-surface-variant)] transition-colors hover:bg-[var(--md-sys-color-surface-container-high)] hover:text-[var(--md-sys-color-on-surface)]"
                  aria-label="Paste raw credential from clipboard"
                  title="Paste from clipboard"
                  @click="pasteRawCredential"
                >
                  <ClipboardPaste class="size-4" />
                </button>
              </div>

              <!-- Visible raw credential error -->
              <p
                v-if="rawCredentialError"
                class="px-2 text-xs font-medium text-[var(--md-sys-color-error)]"
              >
                {{ rawCredentialError }}
              </p>

              <Button
                type="button"
                variant="outline"
                class="h-10 rounded-full px-5 text-xs font-semibold self-start cursor-pointer select-none"
                :disabled="!rawCredential.trim() || isAddingRawCredential"
                aria-label="Add captured credential"
                @click="addRawCredential"
              >
                <Loader2
                  v-if="isAddingRawCredential"
                  class="size-3.5 mr-2 animate-spin"
                />
                <span>{{ isAddingRawCredential ? 'Adding...' : 'Add Credential' }}</span>
              </Button>
            </div>
          </div>
        </div>
      </div>
    </Transition>
  </Teleport>
</template>
