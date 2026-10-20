import { createApp } from 'vue'
import App from './App.vue'
import { vRipple } from './directives/ripple'
import { initInputModality } from './utils/modality'
import 'vue-sonner/style.css'

initInputModality()

const app = createApp(App)
app.directive('ripple', vRipple)
app.mount('#app')
