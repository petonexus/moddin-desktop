import { createApp } from 'vue'
import App from './App.vue'
import { installDebugInstrumentation } from './debug'
import { i18n } from './i18n'
import './style.css'
import './environment.css'

const app = createApp(App)
installDebugInstrumentation(app)
app.use(i18n).mount('#app')
