import { createApp } from 'vue'
import App from './App.vue'
import { vRipple } from './directives/ripple'
import 'vue-sonner/style.css'

const app = createApp(App)
app.directive('ripple', vRipple)
app.mount('#app')
