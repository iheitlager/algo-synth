# algo-synth
#
# `make` lists the targets by group. `make check` runs every gate CI runs.
# Everything musical is Rust compiled to wasm (ADR-0001); web/ is the Vue view
# and the worklet shim; the container only serves files (ADR-0006).

.DEFAULT_GOAL := help

.PHONY: help check dev build fmt version wasm web install demo-midi test test-rust typecheck lint deny image serve stop clean

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
version: ## Print the workspace version
	@sed -n 's/^version *= *"\([^"]*\)".*/\1/p' Cargo.toml | head -1

##@ Build

wasm: ## Build the engine to wasm
	cargo build --locked --release --target wasm32-unknown-unknown -p algo-dsp
	cp $(WASM_OUT) web/public/dsp.wasm
web: install ## Bundle the Vue app
	cd web && npm run build
install: web/node_modules ## Install web dependencies
# Pachelbel's Canon, written from scratch, so no third-party licence.
demo-midi: ## Regenerate the demo MIDI file
	python3 tools/make_demo_mid.py
web/node_modules: web/package-lock.json
	cd web && npm ci
	@touch web/node_modules

##@ Tests

test: test-rust typecheck ## Engine tests + UI typecheck
test-rust: ## Engine unit tests (native)
	cargo test --locked --workspace
typecheck: install ## vue-tsc over the UI
	cd web && npm run typecheck

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
	podman build -t $(IMAGE) -f Containerfile .
# Localhost is a secure context, so AudioWorklet works without TLS.
serve: image ## Serve on localhost:6340
	podman run --rm -d --name $(IMAGE) -p $(PORT):80 $(IMAGE)
	@echo "http://localhost:$(PORT)"
stop: ## Stop the running container
	@podman stop $(IMAGE) 2>/dev/null && echo "stopped $(IMAGE)" || echo "no $(IMAGE) container running"

##@ Support

clean: ## Remove build artifacts
	rm -rf target web/dist web/public/dsp.wasm
help: ## Show this help
	@awk 'BEGIN {FS = ":.*## "; printf "Usage: make \033[36m<target>\033[0m\n"} \
		/^##@/ {printf "\n\033[1m%s\033[0m\n", substr($$0, 5); next} \
		/^[a-zA-Z0-9_-]+:.*## / {printf "  \033[36m%-12s\033[0m %s\n", $$1, $$2}' $(MAKEFILE_LIST)
