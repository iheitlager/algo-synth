// The Assistant in its own window (#387): the same pane as in the app, talking
// to the main window over a BroadcastChannel. It loads no engine (only the main
// window owns one), so none of the app's audio code is imported here.
import { createApp } from 'vue'
import AssistantWindow from './components/AssistantWindow.vue'
import './style.css'
import './console.css'

createApp(AssistantWindow).mount('#app')
