# algo-synth
#
# `make` lists the targets by group. `make check` runs every gate CI runs.
# Everything musical is Rust compiled to wasm (ADR-0001); web/ is the Vue view
# and the worklet shim; the container only serves files (ADR-0006).

.DEFAULT_GOAL := help

.PHONY: help check dev build fmt release version wasm web install demo-midi samples test test-rust test-tools test-web typecheck bench lint deny image serve stop clean

WASM_OUT := target/wasm32-unknown-unknown/release/algo_dsp.wasm
IMAGE    := algo-synth
# 63xx: out of the way of the usual 3000/5173/8080 dev servers.
PORT     ?= 6340
DEV_PORT ?= 6341

##@ Everyday

check: lint deny test build ## Run all CI gates
# Vite serves web/public, so dsp.wasm is rebuilt first. Hard-refresh the
# browser after a DSP change: the worklet module is cached.
dev: wasm install ## Dev server on localhost:6341
	cd web && npm run dev -- --port $(DEV_PORT) --strictPort
build: wasm web ## Build wasm and web into web/dist
fmt: ## Format the code
	cargo fmt
# Collects changes/*.md into CHANGELOG.md and bumps the version (#186).
release: ## Cut a release from changes/
	python3 tools/release.py
version: ## Print the workspace version
	@sed -n 's/^version *= *"\([^"]*\)".*/\1/p' Cargo.toml | head -1

##@ Build

# The commit goes into the wasm and the page, so the page can tell them apart (#197).
SHA := $(shell git rev-parse --short=8 HEAD 2>/dev/null)
export ALGO_BUILD_SHA ?= $(SHA)

wasm: ## Build the engine to wasm
	cargo build --locked --release --target wasm32-unknown-unknown -p algo-dsp
	cp $(WASM_OUT) web/public/dsp.wasm
web: install ## Bundle the Vue app
	cd web && npm run build
install: web/node_modules ## Install web dependencies
# Pachelbel's Canon, written from scratch, so no third-party licence.
demo-midi: ## Regenerate the demo MIDI file
	python3 tools/make_demo_mid.py
# Downloads the CC0 packs in tools/samples/packs.json (checksummed, cached in
# .cache/samples), converts them to mono WAV and writes web/public/samples/,
# which is gitignored; the static server ships what is there (ADR-0006).
# Unpacking .7z needs 7zz (brew install sevenzip); bsdtar reads only newer ones.
samples: ## Fetch free sample packs
	python3 tools/fetch_samples.py
web/node_modules: web/package-lock.json
	cd web && npm ci
	@touch web/node_modules

##@ Tests

test: test-rust test-web test-tools typecheck ## Engine, UI and tool tests, typecheck
test-tools: ## Python tool tests
	python3 -m unittest discover -s tools -p 'test_*.py'
test-rust: ## Engine unit tests (native)
	cargo test --locked --workspace
test-web: install ## UI unit tests (vitest)
	cd web && npm test
typecheck: install ## vue-tsc over the UI
	cd web && npm run typecheck
# 16 Mono voices in Node's V8 against the 25% budget (plan.md); confirm with
# render capacity in Chrome DevTools' WebAudio panel.
bench: wasm ## Time 16 voices in V8
	node tools/bench.mjs

##@ Code quality

lint: ## clippy, fmt check, no mod.rs
	@found=$$(git ls-files --cached --others --exclude-standard | grep -E '(^|/)mod\.rs$$' | sort); \
	if [ -n "$$found" ]; then echo "mod.rs is not allowed; use name.rs next to name/:"; echo "$$found"; exit 1; fi
	cargo clippy --locked --workspace --all-targets -- -D warnings
	cargo clippy --locked -p algo-dsp --target wasm32-unknown-unknown -- -D warnings
	cargo fmt -- --check
deny: ## cargo-deny checks
	cargo deny check

##@ Container

image: ## Build the Podman image
	podman build --build-arg ALGO_BUILD_SHA=$(SHA) -t $(IMAGE) -f Containerfile .
# Localhost is a secure context, so AudioWorklet works without TLS.
# Replaces a container left over from an earlier serve (running or not).
serve: image ## Serve on localhost:6340
	@podman rm -f --ignore $(IMAGE) >/dev/null
	podman run --rm -d --name $(IMAGE) -p $(PORT):80 $(IMAGE)
	@echo "http://localhost:$(PORT)"
stop: ## Stop the running container
	@podman rm -f --ignore $(IMAGE) | grep -q . && echo "stopped $(IMAGE)" || echo "no $(IMAGE) container running"

##@ Support

clean: ## Remove build artifacts
	rm -rf target web/dist web/public/dsp.wasm
help: ## Show this help
	@awk 'BEGIN {FS = ":.*## "; printf "Usage: make \033[36m<target>\033[0m\n"} \
		/^##@/ {printf "\n\033[1m%s\033[0m\n", substr($$0, 5); next} \
		/^[a-zA-Z0-9_-]+:.*## / {printf "  \033[36m%-12s\033[0m %s\n", $$1, $$2}' $(MAKEFILE_LIST)
