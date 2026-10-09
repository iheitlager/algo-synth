Spike #420: what a MIDI controller sends, first the Akai MPK mini Plus.

A standalone Rust tool (its own workspace; midir for I/O, wmidi beside a small
decoder of its own). `make help` lists the targets: list, monitor, identity,
learn, clock, stats, and page (Chrome's Web MIDI, SysEx included).

mpk-mini-plus.toml is the MPK's default map, captured from the device.
Findings and the recommendation are on #420. Capture logs (*.jsonl) stay local.
