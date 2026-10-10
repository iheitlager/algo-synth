# algo-synth
#
# `make` lists the targets by group. `make check` runs every gate CI runs.
# Everything musical is Rust compiled to wasm (ADR-0001); web/ is the Vue view
# and the worklet shim; the container only serves files (ADR-0006).

.DEFAULT_GOAL := help

.PHONY: help check dev build fmt release version wasm web install demo-midi samples test test-rust test-tools test-web coverage-web typecheck bench filters syntax-test lint deny image serve stop clean env-check

WASM_OUT := target/wasm32-unknown-unknown/release/algo_dsp.wasm
IMAGE    := algo-synth
# One server (ADR-0030): 6340 for make serve, 6341 for make dev; 63xx: out of
# the way of the usual 3000/5173/8080 dev servers.
PORT     ?= 6340
DEV_PORT ?= 6341
# The assistant's keys come from 1Password (ADR-0028, #386): op.env holds only
# op:// references, and `op run` fills them in for one process. OP= skips
# 1Password: without keys the app serves and the assistant has no provider.
OP       ?= op run --env-file=op.env --
KEYS     := ANTHROPIC_API_KEY MISTRAL_API_KEY GEMINI_API_KEY OPENROUTER_API_KEY ASSIST_SELF_HOSTED_URL ASSIST_SELF_HOSTED_MODELS

##@ Everyday

check: lint deny test build ## Run all CI gates
# One server (ADR-0030): algo-synth serve on 127.0.0.1:6341, the app and the
# assistant, while vite build --watch keeps web/dist current; reload to see a
# change. dsp.wasm is built first (make wasm again after a DSP change). Ctrl-C
# stops both.
dev: wasm install ## App and assistant on localhost:6341
	@(cd web && exec ./node_modules/.bin/vite build --watch --logLevel warn) & build=$$!; \
	trap 'kill $$build 2>/dev/null' EXIT INT TERM; \
	until [ -f web/dist/index.html ]; do sleep 1; done; \
	ALGO_BIND=127.0.0.1:$(DEV_PORT) $(OP) cargo run --release -p algo-assist -- serve
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
# Line coverage of the UI (vitest, v8); the HTML report is web/coverage/index.html.
coverage-web: install ## UI test coverage
	cd web && npm run test:coverage
typecheck: install ## vue-tsc over the UI
	cd web && npm run typecheck
# 16 Mono voices in Node's V8 against the 25% budget (plan.md); confirm with
# render capacity in Chrome DevTools' WebAudio panel.
bench: wasm ## Time 16 voices in V8
	node tools/bench.mjs
# A harmonic table per ladder voicing (#319); pass ARGS="--stages-only" or
# ARGS="--wav <dir>" for the stage types alone or a WAV per voicing.
filters: ## Compare the ladder voicings
	cargo run --release --locked -p algo-dsp --example filter_compare -- $(ARGS)

# The editor syntaxes (#482) against the engine: the tree-sitter corpus, the
# generated parser up to date, Zed's copy of the queries, and Vim's and
# tree-sitter's colours compared with song-lsp's over the examples. Needs
# tree-sitter, nvim and a C compiler, so CI does not run it.
syntax-test: ## Check the editor syntaxes
	cargo build --locked -q -p song-lsp
	cd syntax/tree-sitter-song && tree-sitter generate && git diff --exit-code -- src
	cd syntax/tree-sitter-song && tree-sitter test -p . && tree-sitter build -o song.so .
	diff -r syntax/tree-sitter-song/queries syntax/zed/languages/song --exclude='*.toml'
	python3 syntax/compare.py vim target/debug/song-lsp examples/*.song
	python3 syntax/compare.py ts target/debug/song-lsp examples/*.song

##@ Code quality

lint: ## clippy, fmt check, no mod.rs
	@found=$$(git ls-files --cached --others --exclude-standard | grep -E '(^|/)mod\.rs$$' | sort); \
	if [ -n "$$found" ]; then echo "mod.rs is not allowed; use name.rs next to name/:"; echo "$$found"; exit 1; fi
	cargo clippy --locked --workspace --all-targets -- -D warnings
	cargo clippy --locked -p algo-dsp --target wasm32-unknown-unknown -- -D warnings
	cargo fmt -- --check
deny: ## cargo-deny checks
	cargo deny check

##@ Assistant

# Which of op.env's keys 1Password resolves: ok or missing, never a value.
env-check: ## Check the 1Password keys
	@grep -E '^[A-Z_]+=op://' op.env | while IFS='=' read -r name ref; do \
		if op read --no-newline "$$ref" >/dev/null 2>&1; then echo "ok       $$name"; \
		else echo "missing  $$name  ($$ref)"; fi; \
	done

##@ Container

# The commit goes in as ALGO_BUILD_SHA, declared after the source copies in
# Containerfile, so a new commit rebuilds the wasm and the page instead of
# reusing a cached layer from older sources (#198); no --no-cache needed.
image: ## Build the Podman image
	podman build --build-arg ALGO_BUILD_SHA=$(SHA) -t $(IMAGE) -f Containerfile .
# Localhost is a secure context, so AudioWorklet works without TLS.
# Replaces a container left over from an earlier serve (running or not).
# One container, one port on the host's 127.0.0.1 (ADR-0030, ADR-0028). The
# keys pass in by name (`-e NAME`) from `op run`, never on the command line;
# without 1Password it starts without them.
serve: image ## Serve on localhost:6340
	@podman rm -f --ignore $(IMAGE) >/dev/null
	@$(OP) podman run --rm -d --name $(IMAGE) -p 127.0.0.1:$(PORT):6340 $(foreach k,$(KEYS),-e $(k)) $(IMAGE) \
		|| podman run --rm -d --name $(IMAGE) -p 127.0.0.1:$(PORT):6340 $(IMAGE)
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
