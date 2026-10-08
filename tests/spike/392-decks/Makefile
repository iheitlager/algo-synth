# Spike #392 (throwaway): how many engines can play at once. Results on #392.

.DEFAULT_GOAL := help

.PHONY: help wasm run stop bench workers

ROOT := ../../..
# 6392 sends the cross-origin isolation headers the workers need; 6393 does not.
URL  := http://localhost:6392/

help: ## Show this help
	@awk 'BEGIN {FS = ":.*## "; printf "Usage: make \033[36m<target>\033[0m\n"} \
		/^[a-zA-Z0-9_-]+:.*## / {printf "  \033[36m%-10s\033[0m %s\n", $$1, $$2}' $(MAKEFILE_LIST)

# The engine the page and the benches load: web/public/dsp.wasm.
wasm: ## Build the engine
	$(MAKE) -C $(ROOT) wasm

# Serves the test page and opens it in Chrome (device underruns are Chrome-only); Ctrl-C stops it.
run: wasm stop ## Serve and open the test page
	@(sleep 1 && open -a "Google Chrome" $(URL) 2>/dev/null || open $(URL)) &
	node server.mjs

stop: ## Stop the test server
	@pkill -f "392-decks/server.mjs" 2>/dev/null; pkill -f "^node server.mjs" 2>/dev/null; true

bench: wasm ## Time engines on one thread
	node multi.mjs

# STRESS=0,6 picks how many cores to keep busy; SECONDS sets each run's length.
workers: wasm ## Time engines in worker threads
	node workers.mjs
