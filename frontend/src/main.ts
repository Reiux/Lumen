import { createApp } from 'vue'
import App from './App.vue'
import { vRipple } from './directives/ripple'
import { initInputModality } from './utils/modality'
import '@materialwebunofficial/md3e-web'
import { applyDynamicTheme } from '@materialwebunofficial/md3e-web'
import 'vue-sonner/style.css'

initInputModality()

// Apply Material 3 Expressive dynamic theme from saturated Google Photos Azure seed
try {
  applyDynamicTheme('#00639B', true, 'tonal-spot')
} catch (e) {
  console.warn('Failed to apply dynamic M3 theme:', e)
}

if (import.meta.env.DEV) {
  import('./utils/flickerDebug').then(({ initFlickerDebug }) => {
    initFlickerDebug()
  })
}

const app = createApp(App)
app.directive('ripple', vRipple)
app.mount('#app')

