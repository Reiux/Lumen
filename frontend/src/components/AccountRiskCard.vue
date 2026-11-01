<script setup lang="ts">
import M3Button from './ui/M3Button.vue'
import M3Icon from './ui/M3Icon.vue'

const acknowledged = defineModel<boolean>('modelValue', { default: false })

function acknowledge() {
  acknowledged.value = true
}
</script>

<template>
  <div
    class="flex flex-col gap-3 rounded-2xl border p-4 transition-colors select-none"
    :class="[
      acknowledged
        ? 'border-[var(--md-sys-color-outline-variant)]/40 bg-[var(--md-sys-color-surface-container)] text-[var(--md-sys-color-on-surface-variant)]'
        : 'border-amber-500/50 bg-amber-500/10 text-[var(--md-sys-color-on-surface)] shadow-xs'
    ]"
  >
    <div class="flex items-start gap-3">
      <div
        class="flex size-9 shrink-0 items-center justify-center rounded-xl"
        :class="acknowledged ? 'bg-[var(--md-sys-color-surface-container-high)] text-[var(--md-sys-color-primary)]' : 'bg-amber-500/20 text-amber-500'"
      >
        <M3Icon
          :name="acknowledged ? 'verified_user' : 'warning'"
          :size="20"
        />
      </div>

      <div class="flex flex-col gap-1.5 flex-1">
        <div class="flex items-center justify-between">
          <h3 class="m3-title-medium">
            Heads up
          </h3>
          <span
            v-if="acknowledged"
            class="m3-label-small text-[var(--md-sys-color-primary)] flex items-center gap-1"
          >
            <M3Icon
              name="check"
              :size="14"
            />
            Acknowledged
          </span>
        </div>

        <p class="m3-body-medium leading-relaxed opacity-90">
          Lumen is unofficial and may violate Google's Terms of Service. Google can restrict or disable an account, which can also lock you out of Gmail and Drive. Use an account you can afford to lose, and keep your original photos.
        </p>

        <div
          v-if="!acknowledged"
          class="pt-1.5 flex items-center gap-2"
        >
          <M3Button
            variant="filled"
            size="xs"
            icon="check"
            @click="acknowledge"
          >
            <span>I understand</span>
          </M3Button>
        </div>
      </div>
    </div>
  </div>
</template>
