import path from 'node:path'
import tailwindcss from '@tailwindcss/vite'
import vue from '@vitejs/plugin-vue'
import { defineConfig, type Plugin } from 'vite'

function devFlickerFlagsPlugin(): Plugin {
  return {
    name: 'dev-flicker-flags',
    apply: 'serve',
    transformIndexHtml(html: string) {
      const raw = process.env.LUMEN_FLAGS || process.env.FLAGS || ''
      if (!raw) return html
      const cleaned = raw.startsWith('?') ? raw.slice(1) : raw
      const pairs = cleaned.split(/[&,]/)
      let attrs = ''
      for (const pair of pairs) {
        const [k, v] = pair.split('=')
        if (k && k.trim()) {
          attrs += ` data-${k.trim()}="${v ? v.trim() : '1'}"`
        }
      }
      return html.replace('<html', `<html${attrs}`)
    },
  }
}

export default defineConfig({
  plugins: [vue(), tailwindcss(), devFlickerFlagsPlugin()],
  server: {
    host: '127.0.0.1',
  },
  resolve: {
    alias: {
      '@': path.resolve(import.meta.dirname, './src'),
    },
  },
})

