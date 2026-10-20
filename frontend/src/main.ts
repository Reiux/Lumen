import { createApp } from 'vue'
import App from './App.vue'
import { vRipple } from './directives/ripple'
import { initInputModality } from './utils/modality'
import { initTheme } from './utils/theme'
import '@materialwebunofficial/md3e-web'
import 'vue-sonner/style.css'

initInputModality()

// Apply Material 3 Expressive dynamic theme from user choice or default seed
initTheme()

if (import.meta.env.DEV) {
  import('./utils/flickerDebug').then(({ initFlickerDebug }) => {
    initFlickerDebug()
  })
}

const app = createApp(App)
app.directive('ripple', vRipple)
app.mount('#app')

