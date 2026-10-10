// The Spectral Lab in its own window (ADR-0017): its own engine and analysis
// worker, none of the app's state; it talks to the main window only to send it
// a resynthesis.
import { createApp } from 'vue'
import SpectralLab from './components/lab/SpectralLab.vue'
import './style.css'

createApp(SpectralLab).mount('#app')
