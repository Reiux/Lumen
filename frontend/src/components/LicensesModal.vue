<script setup lang="ts">
import { computed, ref } from 'vue'
import licensesData from '../../../docs/THIRD_PARTY_LICENSES.json'
import M3Button from './ui/M3Button.vue'
import M3Icon from './ui/M3Icon.vue'
import M3TextField from './ui/M3TextField.vue'

interface Props {
  open: boolean
}

const props = defineProps<Props>()
const emit = defineEmits<{
  (e: 'update:open', val: boolean): void
}>()

const activeTab = ref<'project' | 'go' | 'npm'>('project')
const searchQuery = ref('')

function close() {
  emit('update:open', false)
}

const filteredGoDeps = computed(() => {
  if (!searchQuery.value) return licensesData.goDependencies
  const q = searchQuery.value.toLowerCase()
  return licensesData.goDependencies.filter(
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
          class="m3-dialog-card relative flex flex-col w-full max-w-4xl max-h-[85vh] rounded-[28px] bg-[var(--md-sys-color-surface-container-high)] border border-[var(--md-sys-color-outline-variant)]/40 shadow-2xl overflow-hidden"
          role="dialog"
          aria-modal="true"
          aria-labelledby="licenses-modal-title"
        >
          <!-- Modal Header -->
          <div class="flex items-center justify-between border-b border-[var(--md-sys-color-outline-variant)]/30 px-6 py-4">
            <div class="flex items-center gap-3">
              <div class="flex size-10 items-center justify-center rounded-[16px] bg-[var(--md-sys-color-primary-container)] text-[var(--md-sys-color-on-primary-container)]">
                <M3Icon name="policy" :size="20" />
              </div>
              <div>
                <h2
                  id="licenses-modal-title"
                  class="m3-headline-small text-[var(--md-sys-color-on-surface)]"
                >
                  Open source licenses
                </h2>
                <p class="m3-body-medium">
                  Third-party components and project copyright notices
                </p>
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

          <!-- Navigation Tabs Bar -->
          <div class="flex items-center justify-between border-b border-[var(--md-sys-color-outline-variant)]/30 px-6 py-2.5 bg-[var(--md-sys-color-surface-container)] gap-4">
            <div class="flex items-center gap-1.5 p-1 rounded-full bg-[var(--md-sys-color-surface-container-high)] border border-[var(--md-sys-color-outline-variant)]/30">
            <M3Button
              size="xs"
              :toggle="true"
              :selected="activeTab === 'project'"
              :variant="activeTab === 'project' ? 'filled' : 'text'"
              icon="copyright"
              @click="activeTab = 'project'"
            >
              Project licenses
            </M3Button>

            <M3Button
              size="xs"
              :toggle="true"
              :selected="activeTab === 'go'"
              :variant="activeTab === 'go' ? 'filled' : 'text'"
              icon="code"
              @click="activeTab = 'go'"
            >
              Go modules ({{ licensesData.goDependencies.length }})
            </M3Button>

            <M3Button
              size="xs"
              :toggle="true"
              :selected="activeTab === 'npm'"
              :variant="activeTab === 'npm' ? 'filled' : 'text'"
              icon="javascript"
              @click="activeTab = 'npm'"
            >
              NPM packages ({{ licensesData.npmDependencies.length }})
            </M3Button>
          </div>

          <div v-if="activeTab !== 'project'" class="w-64">
            <M3TextField
              id="license-search"
              v-model="searchQuery"
              placeholder="Search dependencies..."
              variant="outlined"
              leading-icon="search"
            />
          </div>
        </div>

        <!-- Tab 1: Project & Upstream Licenses -->
        <div
          v-if="activeTab === 'project'"
          class="flex flex-col gap-6 p-6 overflow-y-auto flex-1 min-h-0 m3-body-medium"
        >
          <!-- Lumen License -->
          <div class="flex flex-col gap-2 rounded-2xl bg-[var(--md-sys-color-surface-container-low)] p-5 border border-[var(--md-sys-color-outline-variant)]/30">
            <div class="flex items-center justify-between border-b border-[var(--md-sys-color-outline-variant)]/20 pb-2">
              <span class="m3-title-medium text-[var(--md-sys-color-on-surface)]">{{ licensesData.project.name }}</span>
              <span class="rounded-full bg-[var(--md-sys-color-primary-container)] px-2.5 py-0.5 m3-label-small text-[var(--md-sys-color-on-primary-container)]">
                {{ licensesData.project.license }}
              </span>
            </div>
            <pre class="m3-mono m3-body-medium leading-relaxed whitespace-pre-wrap mt-1">{{ licensesData.project.text }}</pre>
          </div>

          <!-- Upstream gotohp License -->
          <div class="flex flex-col gap-2 rounded-2xl bg-[var(--md-sys-color-surface-container-low)] p-5 border border-[var(--md-sys-color-outline-variant)]/30">
            <div class="flex items-center justify-between border-b border-[var(--md-sys-color-outline-variant)]/20 pb-2">
              <div>
                <span class="m3-title-medium text-[var(--md-sys-color-on-surface)]">Upstream Project: {{ licensesData.upstream.name }}</span>
                <span class="block m3-body-medium mt-0.5">{{ licensesData.upstream.copyright }}</span>
              </div>
              <span class="rounded-full bg-[var(--md-sys-color-secondary-container)] px-2.5 py-0.5 m3-label-small text-[var(--md-sys-color-on-secondary-container)]">
                {{ licensesData.upstream.license }}
              </span>
            </div>
            <pre class="m3-mono m3-body-medium leading-relaxed whitespace-pre-wrap mt-1">{{ licensesData.upstream.text }}</pre>
          </div>
        </div>

        <!-- Tab 2: Go Dependencies -->
        <div
          v-if="activeTab === 'go'"
          class="flex flex-col overflow-y-auto flex-1 min-h-0 divide-y divide-[var(--md-sys-color-outline-variant)]/20"
        >
          <div
            v-for="dep in filteredGoDeps"
            :key="dep.name"
            class="px-6 py-3 flex items-center justify-between gap-4 hover:bg-[var(--md-sys-color-surface-container-high)]/60 transition-colors"
          >
            <div class="flex flex-col min-w-0">
              <span class="m3-mono m3-body-medium text-[var(--md-sys-color-on-surface)] truncate" :title="dep.name">
                {{ dep.name }}
              </span>
              <span class="m3-body-medium">
                version {{ dep.version }}
              </span>
            </div>

            <span
              class="rounded-full px-2.5 py-0.5 m3-label-small shrink-0"
              :class="dep.license === 'unknown'
                ? 'bg-amber-500/20 text-amber-300'
                : 'bg-[var(--md-sys-color-surface-container-highest)] text-[var(--md-sys-color-on-surface)]'"
            >
              {{ dep.license }}
            </span>
          </div>

          <div
            v-if="filteredGoDeps.length === 0"
            class="p-8 text-center m3-body-medium"
          >
            No dependencies match the search query.
          </div>
        </div>

        <!-- Tab 3: NPM Dependencies -->
        <div
          v-if="activeTab === 'npm'"
          class="flex flex-col overflow-y-auto flex-1 min-h-0 divide-y divide-[var(--md-sys-color-outline-variant)]/20"
        >
          <div
            v-for="dep in filteredNpmDeps"
            :key="dep.name"
            class="px-6 py-3 flex items-center justify-between gap-4 hover:bg-[var(--md-sys-color-surface-container-high)]/60 transition-colors"
          >
            <div class="flex flex-col min-w-0">
              <span class="m3-mono m3-body-medium text-[var(--md-sys-color-on-surface)] truncate" :title="dep.name">
                {{ dep.name }}
              </span>
              <span class="m3-body-medium">
                version {{ dep.version }}
              </span>
            </div>

            <span
              class="rounded-full px-2.5 py-0.5 m3-label-small shrink-0"
              :class="dep.license === 'unknown'
                ? 'bg-amber-500/20 text-amber-300'
                : 'bg-[var(--md-sys-color-surface-container-highest)] text-[var(--md-sys-color-on-surface)]'"
            >
              {{ dep.license }}
            </span>
          </div>

          <div
            v-if="filteredNpmDeps.length === 0"
            class="p-8 text-center m3-body-medium"
          >
            No dependencies match the search query.
          </div>
        </div>

        <!-- Modal Footer -->
        <div class="flex items-center justify-between border-t border-[var(--md-sys-color-outline-variant)]/30 px-6 py-3.5 bg-[var(--md-sys-color-surface-container)]">
          <span class="m3-body-medium">
            Licenses retrieved from official project manifests and distribution licenses
            Licenses retrieved from official project manifests and distribution licenses
          </span>
          <M3Button
            variant="filled"
            size="s"
            @click="close"
          >
            Done
          </M3Button>
        </div>
      </div>
    </div>
    </Transition>
  </Teleport>
</template>
