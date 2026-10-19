<script setup lang="ts">
import type { HTMLAttributes } from 'vue'
import { cn } from '@/lib/utils'
import { Primitive, type PrimitiveProps } from 'reka-ui'
import { type ButtonVariants, buttonVariants } from '.'
import { vRipple } from '@/directives/ripple'
import { Loader2 } from '@lucide/vue'

interface Props extends PrimitiveProps {
  variant?: ButtonVariants['variant']
  size?: ButtonVariants['size']
  class?: HTMLAttributes['class']
  loading?: boolean
}

const props = withDefaults(defineProps<Props>(), {
  as: 'button',
  loading: false,
})
</script>

<template>
  <Primitive
    v-ripple
    data-slot="button"
    :as="as"
    :as-child="asChild"
    :disabled="loading || ($attrs.disabled as boolean)"
    style="--wails-draggable: none"
    :class="cn(
      `cursor-pointer m3-interactive m3-btn-morph select-none`,
      buttonVariants({ variant, size }),
      loading && 'is-loading opacity-80 pointer-events-none',
      props.class
    )"
  >
    <Loader2
      v-if="loading"
      class="size-4 shrink-0 animate-spin m3-spinner-fade mr-1.5"
    />
    <slot />
  </Primitive>
</template>
