<script setup lang="ts">
import { computed, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import licensesData from '../assets/third-party-licenses.json'
import M3Button from './ui/M3Button.vue'
import M3ButtonGroup from './ui/M3ButtonGroup.vue'
import M3Icon from './ui/M3Icon.vue'
import M3TextField from './ui/M3TextField.vue'
import { useDialogFocusTrap } from '../utils/useDialogFocusTrap'

interface Props {
  open: boolean
}

const props = defineProps<Props>()
const emit = defineEmits<{
  (e: 'update:open', val: boolean): void
}>()

const dialogCardRef = ref<HTMLElement | null>(null)
useDialogFocusTrap(() => props.open, dialogCardRef, { onEscape: close })

const { t } = useI18n()

const activeTab = ref<'project' | 'rust' | 'npm'>('project')
const searchQuery = ref('')

function close() {
  emit('update:open', false)
}

const rustDeps = computed(() => {
  return (licensesData as { rustDependencies?: Array<{ name: string; version: string; license: string }> }).rustDependencies || []
})

const filteredRustDeps = computed(() => {
  if (!searchQuery.value) return rustDeps.value
  const q = searchQuery.value.toLowerCase()
  return rustDeps.value.filter(
    (d) => d.name.toLowerCase().includes(q) || d.license.toLowerCase().includes(q)
  )
})

const filteredNpmDeps = computed(() => {
  if (!searchQuery.value) return licensesData.npmDependencies
  const q = searchQuery.value.toLowerCase()
  return licensesData.npmDependencies.filter(
    (d) => d.name.toLowerCase().includes(q) || d.license.toLowerCase().includes(q)
  )
})
</script>

<template>
  <Teleport to="body">
    <Transition name="m3-dialog-modal">
      <div
        v-if="open"
        class="m3-dialog-wrapper fixed inset-0 z-50 grid place-items-center p-4 select-none overflow-y-auto"
        @click.self="close"
      >
        <div
          ref="dialogCardRef"
          tabindex="-1"
          class="m3-dialog-card relative flex flex-col w-full max-w-4xl max-h-[85vh] rounded-[28px] bg-[var(--md-sys-color-surface-container-high)] shadow-2xl overflow-hidden outline-none focus-visible:outline-[3px] focus-visible:outline-[var(--md-sys-color-primary)]"
          role="dialog"
          aria-modal="true"
          aria-labelledby="licenses-modal-title"
        >
          <!-- M3E Modal Header -->
          <div class="flex items-center justify-between px-6 pt-5 pb-4">
            <div class="flex items-center gap-3.5">
              <div class="flex size-12 shrink-0 items-center justify-center rounded-[16px] bg-[var(--md-sys-color-primary-container)] text-[var(--md-sys-color-on-primary-container)]">
                <M3Icon
                  name="policy"
                  :filled="true"
                  :size="24"
                />
              </div>
              <div>
                <h2
                  id="licenses-modal-title"
                  class="m3-headline-small text-[var(--md-sys-color-on-surface)]"
                >
                  {{ t('licenses.title') }}
                </h2>
                <p class="m3-body-medium text-[var(--md-sys-color-on-surface-variant)]">
                  {{ t('licenses.subtitle') }}
                </p>
              </div>
            </div>

            <M3Button
              variant="text"
              size="s"
              icon="close"
              :aria-label="t('licenses.closeAria')"
              class="!size-10 !min-w-0 !p-0 shrink-0"
              @click="close"
            />
          </div>

          <!-- M3E Connected Button Group Navigation Bar -->
          <div class="flex flex-wrap items-center justify-between px-6 py-3 bg-[var(--md-sys-color-surface-container)] gap-4">
            <M3ButtonGroup :connected="true">
              <M3Button
                size="s"
                :toggle="true"
                :selected="activeTab === 'project'"
                :variant="activeTab === 'project' ? 'tonal' : 'outlined'"
                icon="copyright"
                @click="activeTab = 'project'"
              >
                {{ t('licenses.projectTab') }}
              </M3Button>

              <M3Button
                size="s"
                :toggle="true"
                :selected="activeTab === 'rust'"
                :variant="activeTab === 'rust' ? 'tonal' : 'outlined'"
                icon="build"
                @click="activeTab = 'rust'"
              >
                {{ t('licenses.rustTab', { count: rustDeps.length }) }}
              </M3Button>

              <M3Button
                size="s"
                :toggle="true"
                :selected="activeTab === 'npm'"
                :variant="activeTab === 'npm' ? 'tonal' : 'outlined'"
                icon="javascript"
                @click="activeTab = 'npm'"
              >
                {{ t('licenses.npmTab', { count: licensesData.npmDependencies.length }) }}
              </M3Button>
            </M3ButtonGroup>

            <div
              v-if="activeTab !== 'project'"
              class="w-full sm:w-64"
            >
              <M3TextField
                id="license-search"
                v-model="searchQuery"
                :placeholder="t('licenses.searchPlaceholder')"
                variant="outlined"
                leading-icon="search"
              />
            </div>
          </div>

          <!-- Tab 1: Project & Upstream Licenses -->
          <div
            v-if="activeTab === 'project'"
            class="flex flex-col gap-4 p-6 overflow-y-auto flex-1 min-h-0 m3-body-medium"
          >
            <!-- Lumen License -->
            <div class="flex flex-col gap-3 rounded-[20px] bg-[var(--md-sys-color-surface-container)] p-5">
              <div class="flex items-center justify-between gap-2">
                <span class="m3-title-medium text-[var(--md-sys-color-on-surface)]">{{ licensesData.project.name }}</span>
                <span class="rounded-full bg-[var(--md-sys-color-primary-container)] px-3 py-1 m3-label-large text-xs text-[var(--md-sys-color-on-primary-container)]">
                  {{ licensesData.project.license }}
                </span>
              </div>
              <pre class="m3-mono m3-body-medium leading-relaxed whitespace-pre-wrap text-[var(--md-sys-color-on-surface-variant)]">{{ licensesData.project.text }}</pre>
            </div>

            <!-- Upstream gotohp License -->
            <div class="flex flex-col gap-3 rounded-[20px] bg-[var(--md-sys-color-surface-container)] p-5">
              <div class="flex items-center justify-between gap-2">
                <div>
                  <span class="m3-title-medium text-[var(--md-sys-color-on-surface)]">{{ t('licenses.upstreamProject', { name: licensesData.upstream.name }) }}</span>
                  <span class="block m3-body-medium text-[var(--md-sys-color-on-surface-variant)] mt-0.5">{{ licensesData.upstream.copyright }}</span>
                </div>
                <span class="rounded-full bg-[var(--md-sys-color-secondary-container)] px-3 py-1 m3-label-large text-xs text-[var(--md-sys-color-on-secondary-container)]">
                  {{ licensesData.upstream.license }}
                </span>
              </div>
              <pre class="m3-mono m3-body-medium leading-relaxed whitespace-pre-wrap text-[var(--md-sys-color-on-surface-variant)]">{{ licensesData.upstream.text }}</pre>
            </div>
          </div>

          <!-- Tab 2: Rust Dependencies (M3E grouped segmented list) -->
          <div
            v-if="activeTab === 'rust'"
            class="flex flex-col gap-1 p-6 overflow-y-auto flex-1 min-h-0"
          >
            <div
              v-for="(dep, idx) in filteredRustDeps"
              :key="`${dep.name}@${dep.version}`"
              class="px-4 py-3 flex items-center justify-between gap-4 bg-[var(--md-sys-color-surface-container)] hover:bg-[var(--md-sys-color-surface-container-highest)] transition-colors"
              :class="[
                filteredRustDeps.length === 1
                  ? 'rounded-[20px]'
                  : idx === 0
                    ? 'rounded-t-[20px] rounded-b-[4px]'
                    : idx === filteredRustDeps.length - 1
                      ? 'rounded-t-[4px] rounded-b-[20px]'
                      : 'rounded-[4px]',
              ]"
            >
              <div class="flex flex-col min-w-0">
                <span
                  class="m3-mono m3-body-medium text-[var(--md-sys-color-on-surface)] truncate"
                  :title="dep.name"
                >
                  {{ dep.name }}
                </span>
                <span class="m3-body-small text-[var(--md-sys-color-on-surface-variant)]">
                  {{ t('licenses.version', { version: dep.version }) }}
                </span>
              </div>

              <span
                class="rounded-full px-3 py-1 m3-label-large text-xs shrink-0"
                :class="dep.license === 'unknown'
                  ? 'bg-[var(--md-sys-color-tertiary-container)] text-[var(--md-sys-color-on-tertiary-container)]'
                  : 'bg-[var(--md-sys-color-secondary-container)] text-[var(--md-sys-color-on-secondary-container)]'"
              >
                {{ dep.license }}
              </span>
            </div>

            <div
              v-if="filteredRustDeps.length === 0"
              class="p-8 text-center m3-body-medium text-[var(--md-sys-color-on-surface-variant)]"
            >
              {{ t('licenses.noDependencies') }}
            </div>
          </div>

          <!-- Tab 3: NPM Dependencies (M3E grouped segmented list) -->
          <div
            v-if="activeTab === 'npm'"
            class="flex flex-col gap-1 p-6 overflow-y-auto flex-1 min-h-0"
          >
            <div
              v-for="(dep, idx) in filteredNpmDeps"
              :key="`${dep.name}@${dep.version}`"
              class="px-4 py-3 flex items-center justify-between gap-4 bg-[var(--md-sys-color-surface-container)] hover:bg-[var(--md-sys-color-surface-container-highest)] transition-colors"
              :class="[
                filteredNpmDeps.length === 1
                  ? 'rounded-[20px]'
                  : idx === 0
                    ? 'rounded-t-[20px] rounded-b-[4px]'
                    : idx === filteredNpmDeps.length - 1
                      ? 'rounded-t-[4px] rounded-b-[20px]'
                      : 'rounded-[4px]',
              ]"
            >
              <div class="flex flex-col min-w-0">
                <span
                  class="m3-mono m3-body-medium text-[var(--md-sys-color-on-surface)] truncate"
                  :title="dep.name"
                >
                  {{ dep.name }}
                </span>
                <span class="m3-body-small text-[var(--md-sys-color-on-surface-variant)]">
                  {{ t('licenses.version', { version: dep.version }) }}
                </span>
              </div>

              <span
                class="rounded-full px-3 py-1 m3-label-large text-xs shrink-0"
                :class="dep.license === 'unknown'
                  ? 'bg-[var(--md-sys-color-tertiary-container)] text-[var(--md-sys-color-on-tertiary-container)]'
                  : 'bg-[var(--md-sys-color-secondary-container)] text-[var(--md-sys-color-on-secondary-container)]'"
              >
                {{ dep.license }}
              </span>
            </div>

            <div
              v-if="filteredNpmDeps.length === 0"
              class="p-8 text-center m3-body-medium text-[var(--md-sys-color-on-surface-variant)]"
            >
              {{ t('licenses.noDependencies') }}
            </div>
          </div>

          <!-- M3E Modal Footer (tonal container, no border hack) -->
          <div class="flex items-center justify-between gap-4 px-6 py-4 bg-[var(--md-sys-color-surface-container)]">
            <span class="m3-body-medium text-[var(--md-sys-color-on-surface-variant)]">
              {{ t('licenses.notice') }}
            </span>
            <M3Button
              variant="filled"
              size="m"
              @click="close"
            >
              {{ t('licenses.done') }}
            </M3Button>
          </div>
        </div>
      </div>
    </Transition>
  </Teleport>
</template>
