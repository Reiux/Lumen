<script setup lang="ts">
import { computed, defineComponent, h, type PropType, type VNode } from 'vue'
import { Browser } from '@wailsio/runtime'
import { parseMarkdown, type MdInline } from '../../utils/markdown'
import M3Icon from './M3Icon.vue'

const props = defineProps<{
  content: string
}>()

const blocks = computed(() => parseMarkdown(props.content))

function openExternalLink(href: string) {
  Browser.OpenURL(href).catch(() => {
    window.open(href, '_blank', 'noopener,noreferrer')
  })
}

function renderInlineNodes(nodes: MdInline[]): (string | VNode)[] {
  return nodes.map((node, i) => {
    switch (node.type) {
      case 'text':
        return node.text
      case 'code':
        return h(
          'code',
          {
            key: i,
            class:
              'm3-mono rounded-[6px] bg-[var(--md-sys-color-surface-container-highest)] text-[var(--md-sys-color-on-surface)] px-1.5 py-0.5 text-[0.85em]',
          },
          node.text,
        )
      case 'strong':
        return h(
          'strong',
          { key: i, class: 'font-semibold text-[var(--md-sys-color-on-surface)]' },
          renderInlineNodes(node.children),
        )
      case 'em':
        return h('em', { key: i, class: 'italic' }, renderInlineNodes(node.children))
      case 'del':
        return h('del', { key: i, class: 'line-through opacity-75' }, renderInlineNodes(node.children))
      case 'link':
        return h(
          'a',
          {
            key: i,
            href: node.href,
            target: '_blank',
            rel: 'noopener noreferrer',
            class:
              'text-[var(--md-sys-color-primary)] underline decoration-[var(--md-sys-color-primary)]/40 underline-offset-2 hover:decoration-[var(--md-sys-color-primary)] transition-colors cursor-pointer break-all',
            onClick: (e: MouseEvent) => {
              e.preventDefault()
              openExternalLink(node.href)
            },
          },
          renderInlineNodes(node.children),
        )
    }
  })
}

const MdInlineRenderer = defineComponent({
  name: 'MdInlineRenderer',
  props: {
    nodes: {
      type: Array as PropType<MdInline[]>,
      required: true,
    },
  },
  setup(inlineProps) {
    return () => renderInlineNodes(inlineProps.nodes)
  },
})
</script>

<template>
  <div class="flex flex-col gap-3 text-[var(--md-sys-color-on-surface-variant)] select-text">
    <template
      v-for="(block, idx) in blocks"
      :key="idx"
    >
      <!-- Headings -->
      <h1
        v-if="block.type === 'heading' && block.level === 1"
        class="m3-title-large text-[var(--md-sys-color-on-surface)] font-semibold tracking-tight first:mt-0 mt-1"
      >
        <MdInlineRenderer :nodes="block.children" />
      </h1>
      <h2
        v-else-if="block.type === 'heading' && block.level === 2"
        class="m3-title-medium text-[var(--md-sys-color-on-surface)] font-semibold first:mt-0 mt-1.5"
      >
        <MdInlineRenderer :nodes="block.children" />
      </h2>
      <h3
        v-else-if="block.type === 'heading' && block.level === 3"
        class="m3-title-medium text-[var(--md-sys-color-primary)] font-semibold first:mt-0 mt-1"
      >
        <MdInlineRenderer :nodes="block.children" />
      </h3>
      <h4
        v-else-if="block.type === 'heading'"
        class="m3-label-large text-[var(--md-sys-color-on-surface)] font-semibold first:mt-0 mt-1"
      >
        <MdInlineRenderer :nodes="block.children" />
      </h4>

      <!-- Horizontal Rule -->
      <hr
        v-else-if="block.type === 'hr'"
        class="border-0 h-px bg-[var(--md-sys-color-outline-variant)]/50 my-1"
      >

      <!-- Fenced Code Block -->
      <pre
        v-else-if="block.type === 'code'"
        class="rounded-[12px] bg-[var(--md-sys-color-surface-container-highest)] text-[var(--md-sys-color-on-surface)] p-3.5 overflow-x-auto m3-mono text-xs leading-relaxed"
      ><code>{{ block.code }}</code></pre>

      <!-- Blockquote -->
      <blockquote
        v-else-if="block.type === 'blockquote'"
        class="rounded-r-[12px] border-l-[3px] border-[var(--md-sys-color-primary)] bg-[var(--md-sys-color-surface-container-low)] px-3.5 py-2.5 m3-body-medium leading-relaxed"
      >
        <MdInlineRenderer :nodes="block.children" />
      </blockquote>

      <!-- Unordered / Task List -->
      <ul
        v-else-if="block.type === 'list' && !block.ordered"
        class="flex flex-col gap-2 pl-1"
      >
        <li
          v-for="(item, itemIdx) in block.items"
          :key="itemIdx"
          class="flex items-start gap-2.5 m3-body-medium leading-relaxed"
        >
          <M3Icon
            v-if="item.checked !== undefined"
            :name="item.checked ? 'check_box' : 'check_box_outline_blank'"
            :filled="item.checked"
            :size="18"
            class="shrink-0 mt-0.5 text-[var(--md-sys-color-primary)]"
          />
          <span
            v-else
            class="mt-2 size-1.5 shrink-0 rounded-full bg-[var(--md-sys-color-primary)]"
            aria-hidden="true"
          />
          <span class="min-w-0 flex-1 break-words">
            <MdInlineRenderer :nodes="item.children" />
          </span>
        </li>
      </ul>

      <!-- Ordered List -->
      <ol
        v-else-if="block.type === 'list' && block.ordered"
        class="flex flex-col gap-2 pl-5 list-decimal marker:text-[var(--md-sys-color-primary)] marker:font-medium"
      >
        <li
          v-for="(item, itemIdx) in block.items"
          :key="itemIdx"
          class="m3-body-medium leading-relaxed pl-1 break-words"
        >
          <MdInlineRenderer :nodes="item.children" />
        </li>
      </ol>

      <!-- Paragraph -->
      <p
        v-else-if="block.type === 'paragraph'"
        class="m3-body-medium leading-relaxed break-words"
      >
        <MdInlineRenderer :nodes="block.children" />
      </p>
    </template>
  </div>
</template>
