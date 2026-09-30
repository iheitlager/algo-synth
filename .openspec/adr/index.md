# ADR index

| ADR | Title | Status |
|---|---|---|
| [0001](0001-everything-musical-is-wasm.md) | Everything musical is Rust in one wasm module on the audio thread; a C ABI, no wasm-bindgen, a JS shim with no logic | Accepted |
| [0002](0002-real-time-rules.md) | The render loop never allocates, locks or panics; tables and control-rate coefficients instead of per-sample transcendentals | Accepted |
| [0003](0003-vue-view.md) | A Vue + TypeScript view, separate from the engine; the worklet outside the bundler; the scope on an AnalyserNode | Accepted |
| [0004](0004-one-parameter-registry.md) | One parameter and source registry in Rust, mirrored in TypeScript, the mirror checked by a test | Accepted |
| [0005](0005-composition-model.md) | Composition: tracks own one source; fixed insert/send/master mixer; patterns in clips from hand, generator or score; the clock in the engine | Accepted |
| [0006](0006-static-serving.md) | Podman + Caddy serving static files; no backend | Accepted |
