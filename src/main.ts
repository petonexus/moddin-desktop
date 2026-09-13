import { createApp } from 'vue'
import Root from './Root.vue'
import { installDebugInstrumentation } from './debug'
import { i18n } from './i18n'
import './styles/index.css'

const app = createApp(Root)
installDebugInstrumentation(app)
app.use(i18n).mount('#app')
