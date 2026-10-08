// Spike #392: burn one core, as other load on the machine would.
import { parentPort } from 'node:worker_threads'
let x = 0
parentPort.on('message', () => process.exit(0))
setImmediate(function spin() { const end = Date.now() + 50; while (Date.now() < end) x += Math.sqrt(x + 1); setImmediate(spin) })
