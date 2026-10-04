# 003: The view

The wide-screen browser view in `web/`. Decision: ADR-0003.

### Requirement 1: Three-area layout [MUST]

The view SHALL fill the window with a transport bar across the top, the **synths** or the **mixer** in the middle with the **MIDI player** (one row per channel) across the bottom, or the **composer** on a screen of its own (a Synths | Mixer | Composer switch in the transport bar). The transport bar's Play and Stop belong to the MIDI file.

**Implementation:** `web/src/App.vue`

### Requirement 2: Power on by gesture [MUST]

Audio SHALL start only from a user gesture (the Power button), creating the AudioContext, compiling `dsp.wasm`, loading `worklet.js` and connecting worklet → analyser → destination. A failure SHALL be shown in the transport bar.

**Implementation:** `web/src/audio/engine.ts::power`, `web/src/components/TransportBar.vue`

### Requirement 3: Play the synths [MUST]

The synths view SHALL be a rail of synth tapes beside one faceplate (Req 9): **+ Synth** SHALL add one (up to 16, reset to the default patch) and × SHALL remove one (never the last), muting the parts that played on it. The selected synth's faceplate has an on-screen keyboard; the computer keyboard (`a`…`;`, C4 upward) SHALL play the selected synth, from the mixer view too, and a held key SHALL release on the synth it started on. Loading a MIDI file SHALL show a synth for each part, and each part SHALL pick its synth or mute.

**Implementation:** `web/src/components/InstrumentsPane.vue`, `web/src/components/synth/` (`SynthRail.vue`, `Keyboard.vue`)

### Requirement 4: Scope [SHOULD]

The transport bar SHALL draw the output waveform from the AnalyserNode.

**Implementation:** `web/src/components/TransportBar.vue::draw`

### Requirement 5: Composer [MUST]

The composer SHALL show the song (ADR-0012, spec 002 Req 6) as a step grid beside its text. Each drum fragment SHALL show a row per lane, a button per step (off, hit, accent; a click cycles them and sends `setStep`), the step each lane plays now, and a selector for the synth its track plays on. The text SHALL be editable and sent with Apply or Ctrl+Enter; a text that does not parse SHALL show its line, column and message, and the grid SHALL keep showing the song that plays. The grid and the text SHALL redraw from what the engine sends back, never from the view's own copy. The composer SHALL have its own Play and Stop (Stop goes back to the top), apart from the MIDI file's, and its BPM and Swing SHALL set the song's tempo and swing through the engine. Without a TR-808 synth the composer SHALL say so. Pitched fragments, the arrangement and generators follow (plan.md MVP 4 and 9).

**Implementation:** `web/src/components/ComposerPane.vue`, `web/src/components/TransportBar.vue`, `web/src/audio/engine.ts` (`song`, `loadSong`, `setStep`, `routeTrack`, `setSongTempo`, `setSongSwing`, `applySong`)

**Tests:** `web/src/audio/song.test.ts`

### Requirement 6: No music logic in the view [MUST]

The view SHALL only send messages and draw; sequencing, generation and synthesis SHALL live in the engine. (ADR-0001)

**Tests:** review

### Requirement 7: Synth setups [SHOULD]

The view SHALL save the synths on screen, each one's kind, model and parameters, the channel routing and the global parameters (`GlobalParam`) as a versioned `.synths.json` file, with parameters keyed by name, not id. **Open…** SHALL take a MIDI file, a setup, or both in either order; with a MIDI file the setup SHALL apply once the parts arrive, alone it SHALL apply to the synths on screen. Applying SHALL reset each listed synth, set its model before its other parameters, then the routes and the globals. A file that isn't JSON or has an unknown version SHALL change nothing; unknown names, models and synths SHALL be skipped and listed in one notice, and a part-count mismatch SHALL warn. The last setup per MIDI file SHALL be kept in `localStorage` and restored when that file opens again, below a picked file and above a shipped one; Demo ships one. A parameter added to the registry SHALL be saved without changes to the setup code; this carries the whole mixer (strips, sends, inserts, processors, equalizer, compressor). The setup SHALL also hold the group buses on screen, each with its strip parameters (`StripParam`) including its Out, and the console layout; a setup without them leaves groups and layout as they are. A setup written before the mixer was central (`EchoSend`, `ReverbSend`, the global `Echo*` and `Reverb*`) SHALL load, migrated to `Send1`, `Send2` and the knobs of P1 and P2 with one notice. The format is built in the view from values the engine reports, which clamps every value it receives (#41).

**Implementation:** `web/src/audio/setup.ts` (`buildSetup`, `parseSetup`, `applyPlan`, `shortF32`), `web/src/audio/engine.ts` (`saveSetup`, `openFiles`), `crates/dsp/src/params.rs::Param::is_global`, `web/public/demo.synths.json`

#### Scenario: round trip

- GIVEN a setup built from the view's values
- WHEN it is written as JSON, read back and applied
- THEN every per-synth parameter, global and route is the same 32-bit float or synth as before

#### Scenario: unknown entries

- GIVEN a setup with an unknown parameter, model, kind, a duplicate and an out-of-range synth
- WHEN it is read
- THEN the rest applies and each skip is listed once

**Tests:** `web/src/audio/setup.test.ts`, `crates/dsp/src/params.rs::tests::typescript_mirror_matches`, `crates/dsp/src/params.rs::tests::ids_round_trip`

### Requirement 8: Mixer console [SHOULD]

The mixer view SHALL be a console: one thin strip per synth side by side (tape with the synth and its model, drive mode and amount, four sends P1–P4, pan, mute, solo, fader with a dB scale and an LED meter, the MIDI channels routed to it), the four effect processors as rack modules (type among off, echo, reverb, chorus and flanger, the knobs of that type with their real units, return and return meter, and on P2–P4 a toggle "← P1" that makes it hear the processor before it, drawn as a link between the two modules), and the master section (equalizer with its response curve, compressor with its transfer curve and gain-reduction meter, master fader, stereo meters and a limiter light). Values shown SHALL be those the engine reports, and changes SHALL go out as messages; the meters SHALL come from the engine's peaks (spec 002 Req 2). A knob SHALL turn by dragging up or down (shift for fine), open a slider when clicked, reset on double-click, and move with the wheel and the arrow keys; a fader SHALL do the same with its taper (0 dB at the top). Group buses (ADR-0010) SHALL be strips too: a group SHALL be added and removed from the console (removing it sends what fed it to the master), and every strip and group SHALL have an Out selector offering only the destinations the engine accepts, a coloured tag under its tape saying which group it feeds, and three insert slots whose panel opens from a slot button with the type and its knobs in real units. A strip SHALL be reordered by dragging its tape onto another, collapsed to a sliver, and hidden (a hidden strip stays in the mix and can be shown again from the bar); the order, collapsed and hidden strips are layout, kept in the setup file and never sent to the engine. Which strips are dimmed SHALL follow the engine's mute and solo rules through groups. Clicking a strip's tape SHALL select its synth for the keyboard, and double-clicking it SHALL show that synth's panel. The synths SHALL stay mounted in the mixer view so the computer keyboard still plays.

**Implementation:** `web/src/components/ConsolePane.vue`, `web/src/components/console/` (`Knob.vue`, `Fader.vue`, `LedMeter.vue`, `ChannelStrip.vue`, `ProcessorModule.vue`, `MasterSection.vue`), `web/src/audio/console.ts`, `web/src/App.vue`, `web/src/components/TransportBar.vue`

#### Scenario: knob and fader maths

- GIVEN a fader position or a knob drag
- WHEN it is turned into a value
- THEN the taper puts 0 dB at the top and a linear level of 1, a drag of 170 px covers the whole range, and the EQ curve reaches each band's gain at its frequency

**Tests:** `web/src/audio/console.test.ts`, `web/src/audio/setup.test.ts`

### Requirement 9: Synth faceplate [SHOULD]

The selected synth SHALL be drawn as one faceplate in the console's hardware style, in the palette of its model (spec 005 Req 8), with the sections, control names and order of that instrument (`web/src/audio/models.ts`): rotary knobs with the console's popover (drag, click for a slider, double-click to reset, wheel and arrow keys, shift for fine), LED switches, stepped selectors (waveforms with a drawn icon), an envelope drawn as a live curve from its attack, decay, sustain and release, and for the models with a patch panel (ARP 2600, MS-20, CS-15) a patch bay: sources down, destinations across, a lit point per connection, and a knob for the amount of each. A rail of tapes SHALL sit beside it, one per synth: its model and number, an LED meter, mute and solo, and a click to select it; a synth's model and preset are chosen in the faceplate's header. "+ Synth" SHALL offer the instrument families (`FAMILIES` in `models.ts`: Mono, Poly, Drums) and their models; a family adds its first model, a model itself, each on its first preset (#132). The view SHALL show the values the engine reports, SHALL send only parameter changes (Req 6), and SHALL keep every control reachable by keyboard. Which control sits on which panel, and how a value is scaled and labelled, is data in `models.ts`; the maths of the drawings (curve points, steps, matrix cells) is pure functions in `web/src/audio/faceplate.ts`.

**Implementation:** `web/src/components/SynthFaceplate.vue`, `web/src/components/synth/` (`Switch.vue`, `Selector.vue`, `EnvGraph.vue`, `PatchBay.vue`, `SynthRail.vue`, `Keyboard.vue`), `web/src/audio/faceplate.ts`, `web/src/audio/models.ts`

#### Scenario: every instrument

- GIVEN one synth of each model
- WHEN each is selected
- THEN its faceplate carries its model's name, palette and sections, every control resolves to a parameter, and an edit sends that parameter

#### Scenario: adding from a family

- GIVEN the "+ Synth" menu
- WHEN Poly or the Juno-106 is chosen
- THEN a synth is added on the lowest free index, selected, as the Prophet-5 or the Juno-106 on that model's first preset

#### Scenario: an envelope curve

- GIVEN an attack, decay, sustain and release
- WHEN the curve is drawn
- THEN it rises over the attack to full level, falls over the decay to the sustain level, holds, and falls over the release to zero, longer times drawing longer segments

#### Scenario: a patch cell

- GIVEN a patch with a source and a destination joined in a slot
- WHEN that cell of the bay is pressed
- THEN the connection is removed, and pressing an empty cell joins them in the first free slot with a middle amount

**Tests:** `web/src/audio/faceplate.test.ts`, `web/src/audio/models.test.ts`, `crates/dsp/src/params.rs::tests::typescript_mirror_matches`

### Requirement 10: Names [SHOULD]

The user SHALL be able to rename every MIDI lane (by channel), synth and group bus in place (#127), a synth also from its tape in the synth rail by a double-click, while a single click still selects it (#178). A synth and its console strip SHALL share one name. An instrument added with + Synth SHALL be named by its family when it is added, with the lowest free number: `Drum N` for the drum machines, `Sampler N` for the samplers, `Synth N` for the synths (#177); the name SHALL be kept, so adding, removing or switching other instruments never renames it, and removing an instrument SHALL free its name. A synth that is not named SHALL take the name of the first lane routed to it, else `Synth N`; a group `Group N`; a lane the file's track name, else `Channel N`. A name SHALL be trimmed and at most 24 characters, and an empty one SHALL restore the default (for an instrument, its family's). Every place a strip or lane is labelled (console tapes and feeds tags, the Out selector, the synth rail and faceplate header, the Player's lanes and route choices) SHALL use the same name. Names are labels only: they SHALL be kept in the view and the setup file (`names`), never in the engine (ADR-0001), and a setup without them SHALL load with the defaults.

**Implementation:** `web/src/audio/names.ts`, `web/src/audio/setup.ts::parseNames`, `web/src/components/EditableName.vue`, `web/src/components/ConsolePane.vue`, `web/src/components/console/ChannelStrip.vue`, `web/src/components/InstrumentsPane.vue`, `web/src/components/PlayerPane.vue`, `web/src/audio/engine.ts` (`addSynth`, `removeSynth`, `renameSynth`)

#### Scenario: a loaded file names its synths

- GIVEN a MIDI file whose first channel is called "Violin I", played on synth 1
- WHEN it loads
- THEN synth 1 and its strip are labelled "Violin I" until the user renames them, and clearing the name brings that back

#### Scenario: names travel with the setup

- GIVEN a renamed synth, group and lane
- WHEN the setup is saved and opened again
- THEN the same names are shown; names that are not a strip or channel are dropped with one warning

**Tests:** `web/src/audio/names.test.ts`, `web/src/audio/setup.test.ts`, `web/src/audio/family-names.test.ts`

### Requirement 11: User presets [SHOULD]

The user SHALL be able to save, load, rename, delete, export and import presets of four kinds (ADR-0014, epic #153): a synth (its model and every synth parameter, loaded on the synth defaults), an insert slot (type and knobs), a processor (type, knobs and return) and a strip (pan, sends and insert slots, never fader, mute, solo or routing). Presets SHALL be stored by name; applying one SHALL touch only its scope; a library file that is not a library SHALL be rejected, and what it cannot place SHALL be dropped with a warning. The library SHALL live in the browser and in `algo-synth.presets.json`, never in a setup.

**Implementation:** `web/src/audio/presets.ts`, `web/src/audio/library.ts`, `web/src/audio/engine.ts::applyPreset`, `crates/dsp/src/engine.rs::Engine::synth_defaults`, `web/src/components/PresetBar.vue` (a picker per target: factory, user, other models or types; Save, Save as, Rename, Delete, Export, Import; a dot when changed), `web/src/components/synth/SysexLoader.vue` (save a DX7 voice), `web/src/components/console/InsertPanel.vue` and `ProcessorModule.vue` (effect presets, compact), `web/src/components/console/StripPanel.vue` (strip presets; Copy and Paste in every preset menu, one clipboard per kind)

#### Scenario: a sound moves to another synth

- GIVEN a Juno-106 pad saved as a synth preset from synth 3
- WHEN it is loaded on synth 6, which plays an ARP 2600 with its fader at −6 dB
- THEN synth 6 is a Juno-106 with every value of the pad, any parameter the preset lacks at its default, and its fader still at −6 dB

#### Scenario: an effect moves to another slot

- GIVEN a compressor saved from strip 4's second insert slot
- WHEN it is loaded into group 3's first slot
- THEN that slot is a compressor with the same knobs and nothing else changes

**Tests:** `web/src/audio/presets.test.ts`, `crates/dsp/src/mono/preset.rs::tests::synth_defaults_reset_the_sound_not_the_strip`
