# Spike #420: what a MIDI controller sends, first the MPK mini Plus. Results on #420.

.DEFAULT_GOAL := help

.PHONY: help build test list monitor identity learn clock stats page

BIN  := target/release/midi-spike
# Port 1 is the MPK's controller; Port 2 its 5-pin MIDI DIN in and out.
PORT ?= Port 1
BPM  ?= 120
SECS ?= 8
LOG  ?= capture.jsonl
URL  := http://localhost:6343/page.html

help: ## Show this help
	@awk 'BEGIN {FS = ":.*## "; printf "Usage: make \033[36m<target>\033[0m\n"} \
		/^[a-zA-Z0-9_-]+:.*## / {printf "  \033[36m%-10s\033[0m %s\n", $$1, $$2}' $(MAKEFILE_LIST)

build: ## Build the tool
	cargo build -q --release

test: ## Test the decoders
	cargo test -q

list: build ## List MIDI ports
	$(BIN) list

# Every input whose name has PORT in it; Ctrl-C stops; also appended to LOG.
monitor: build ## Print every message
	$(BIN) monitor "$(PORT)" 0 $(LOG)

identity: build ## Ask a device who it is
	$(BIN) identity "$(PORT)"

# About three minutes of prompts; writes mpk-mini-plus.toml and learn.jsonl.
learn: build ## Walk through every control
	$(BIN) learn "$(PORT)" mpk-mini-plus.toml learn.jsonl

# For the MPK's arpeggiator set to an External clock.
clock: build ## Send clock at BPM for SECS
	$(BIN) clock $(BPM) $(SECS) "$(PORT)"

stats: build ## Rates and clock jitter of LOG
	$(BIN) stats $(LOG)

# Web MIDI in Chrome on localhost, SysEx included; Ctrl-C stops the server.
page: ## Open the Web MIDI check page
	@(sleep 1 && open -a "Google Chrome" $(URL) 2>/dev/null || open $(URL)) &
	python3 -m http.server 6343 --bind 127.0.0.1
