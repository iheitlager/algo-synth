# 003: The view

The wide-screen browser view in `web/`. Decision: ADR-0003.

### Requirement 1: Three-area layout [MUST]

The view SHALL fill the window with a transport bar across the top and a Synths | Mixer | Composer switch in it. **Synths**, **Mixer** or **Composer** SHALL fill the middle, with the **arranger** (Req 12) in a bottom pane under each. The transport bar's Play (Pause while the song plays) and Stop SHALL be the app's only transport (ADR-0022), with the song's position as bar, step and, in an arrangement, section.

**Implementation:** `web/src/App.vue`, `web/src/components/TransportBar.vue`

#### Scenario: switching views

- GIVEN audio on and the synths shown, the song playing
- WHEN Composer is chosen and then Pause
- THEN the composer fills the middle with the arranger under it, and the song holds its place until Play

**Tests:** `cd web && npm run typecheck`; review in the browser

### Requirement 2: Power on by gesture [MUST]

Audio SHALL start only from a user gesture (the Power button), creating the AudioContext, compiling `dsp.wasm`, loading `worklet.js` and connecting worklet → analyser → destination. A failure SHALL be shown in the transport bar.

**Implementation:** `web/src/audio/engine.ts::power`, `web/src/components/TransportBar.vue`

#### Scenario: nothing before the gesture

- GIVEN a freshly loaded page
- WHEN nothing has been clicked
- THEN no AudioContext exists and the bar says "click Power on to start audio"; after Power on it shows the sample rate, or the failure

**Tests:** `cd web && npm run typecheck`; review in the browser

#### Scenario: New starts over (#325)

- GIVEN a song, several synths and a changed mix, or a first visit with no kept song
- WHEN New is pressed and confirmed, or audio powers on with nothing kept
- THEN the engine clears the song, every synth, strip and effect (`Engine::clear`), and one Modular synth on `ModularBasic` remains with its song track (ADR-0027), so a reload starts the same way

**Implementation:** `crates/dsp/src/engine.rs::Engine::clear`, `web/src/audio/engine.ts::clearAll`, `web/src/audio/songfile.ts::forgetSong`

**Tests:** `crates/dsp/src/engine/tests.rs::clear_starts_over_with_one_modular_synth`, `web/src/audio/engine.test.ts`, `web/src/audio/songfile.test.ts`

### Requirement 3: Play the synths [MUST]

The synths view SHALL be a rail of synth tapes beside one faceplate (Req 9): **+ Synth** SHALL add one on the lowest free index (up to 16, reset, then on its model's first preset) and **× Remove** SHALL remove one (never the last). The selected synth's faceplate has an on-screen keyboard; the computer keyboard (`a`…`;`, C4 upward) SHALL play the selected synth, from the mixer view too, and a held key SHALL release on the synth it started on. Opening a MIDI file SHALL import it as the song (ADR-0022) and show a synth for each of its tracks; a strip's footer SHALL name the song tracks that play on it.

Adding a synth SHALL add its song track and removing one SHALL remove or mute it (ADR-0027, #360); a synth whose sound the song can't hold all of yet (spec 002 Req 17) SHALL carry a mark on its tape naming what is missing (#361). The view's groups, strip order, collapsed and hidden strips and typed names SHALL be kept in the browser (`viewstate.ts`).

**Implementation:** `web/src/components/InstrumentsPane.vue`, `web/src/components/synth/` (`SynthRail.vue`, `Keyboard.vue`), `web/src/audio/engine.ts` (`addSynth`, `removeSynth`)

#### Scenario: a key from the computer keyboard

- GIVEN synth 2 selected
- WHEN `a` is held, synth 5 selected, and `a` released
- THEN C4 sounds on synth 2 and is released on synth 2

**Tests:** `web/src/audio/family-names.test.ts`, `web/src/audio/models.test.ts`; review in the browser

### Requirement 4: Scope [SHOULD]

The transport bar SHALL draw the output waveform from the AnalyserNode.

**Implementation:** `web/src/components/TransportBar.vue::draw`

#### Scenario: a note on the scope

- GIVEN audio on
- WHEN a synth plays a note
- THEN the scope in the transport bar draws its waveform, and a flat line when it stops

**Tests:** review in the browser

### Requirement 5: Composer [MUST]

The composer SHALL show the song (ADR-0012, spec 002 Req 6) as a step grid beside its text. Each drum fragment SHALL show a row per lane, a button per step (off, hit, accent; a click cycles them and sends `setStep`), the step each lane plays now, and a selector for the synth its track plays on. The text SHALL be editable and SHALL apply as it is typed, half a second after typing stops, Ctrl+Enter at once (ADR-0027, #358). While an edit is not applied the engine's folding SHALL be held, and a song coming back SHALL replace the draft only when it was not typed on since, so a knob never overwrites a keystroke. A text that does not parse SHALL show its line, column and message, keep folding held, and the grid SHALL keep showing the song that plays. The grid and the text SHALL redraw from what the engine sends back, never from the view's own copy. The composer SHALL have no transport of its own (the transport bar's is the only one, Req 1), and its BPM and Swing SHALL set the song's tempo and swing through the engine. Without a drum kit (a TR-808, TR-909 or pad sampler) the composer SHALL say so. The text is edited in the song editor (Req 13). Each fragment's header SHALL have a ▶ that plays the fragment alone, looping (spec 002 Req 4, #375); pressed again, or Stop, it goes back to the song. The fragment playing alone SHALL be outlined, its button lit, only its lanes SHALL show the step playing, and the controls SHALL name it.

Above the grid the composer SHALL list the song's tracks (#213), each with its synth, its model (only the models that play the track's kind, by the engine's rule) and its preset (the model's factory presets, then the song's settings on that model). Picking a model SHALL play its first factory preset; picking a preset or setting SHALL be a message to the engine, which sets the track's synth at once, rewrites the track line and prints the song back. A track's sound SHALL reach the song by itself (spec 002 Req 17): its parameters that differ from its preset (not the model, strip, global or arp parameters, at most 32), as a `setting` named after the track; there is no Save as setting.

A note fragment SHALL be shown as a piano roll of its notes (a row per pitch, a column per sixteenth, the bar lines and the step it plays now). A click on empty grid SHALL add a sixteenth note, a click on a note SHALL remove it, and dragging the handle at a note's end SHALL change its length in whole sixteenths; each edit SHALL be a message to the engine, which keeps the note inside its bar and clear of the next one, prints the song back and sends the notes. A fragment that is a generator call SHALL show its call, its events and a Freeze button, and SHALL NOT be edited until frozen.

**Save song** SHALL download the text the engine prints as a `.song` file (ADR-0015), named after the song or MIDI file opened, and **Open…** SHALL take a `.song` file beside a MIDI file and a setup, and show it in the composer. A song file that does not parse SHALL show its text with its line, column and message, and the playing song SHALL play on. The last song that played SHALL be kept in `localStorage` and loaded when audio powers on; storage that is unavailable SHALL change nothing else (#105).

#### Scenario: save, reload, open

- GIVEN a song that plays
- WHEN it is saved, the page reloaded and the file opened
- THEN the engine parses the same text and the same song plays

#### Scenario: a song file that does not parse

- GIVEN a song that plays
- WHEN a `.song` file with a bad line is opened
- THEN the composer shows that file's text with the error, and the song that played before plays on

The divider between the grid and the text, and in the composer the one between the composer and the arranger, SHALL be dragged to resize them (#373), or moved with the arrow keys when focused, and put back by a double-click; the text SHALL keep at least 260 px and the arranger 120 px, and neither SHALL take more than 70% of its container. The sizes SHALL be kept in the browser with the rest of the view (ADR-0027), never in the song.

**Implementation:** `web/src/components/ComposerPane.vue`, `web/src/components/TrackStrip.vue`, `web/src/audio/trackpick.ts`, `crates/dsp/src/engine.rs::Engine::track_edit`, `web/src/components/NoteRoll.vue`, `web/src/components/TransportBar.vue`, `web/src/audio/songfile.ts`, `web/src/audio/engine.ts` (`song`, `loadSong`, `saveSong`, `openFiles`, `setStep`, `addNote`, `removeNote`, `setNoteLength`, `freezeFrag`, `routeTrack`, `setSongTempo`, `setSongSwing`, `applySong`), `web/src/audio/roll.ts`; the engine side `crates/dsp/src/notes.rs::edit`, `crates/dsp/src/song.rs::Song::edit_note`, `crates/dsp/src/ffi.rs` (`frag_events`, `event_*`, `frag_generated`, `note_add`, `note_remove`, `note_len`), `web/src/components/Splitter.vue`, `web/src/audio/split.ts`

**Tests:** `web/src/audio/song.test.ts`, `web/src/audio/trackpick.test.ts`, `crates/dsp/src/engine/tests.rs::a_track_takes_a_preset_from_the_composer`, `crates/dsp/src/engine/tests.rs::a_tracks_sound_saves_as_a_setting`, `crates/dsp/src/engine/tests.rs::a_setting_past_its_room_is_refused`, `crates/dsp/src/ffi.rs::tests::track_patches_through_the_abi`, `crates/dsp/src/mono/preset.rs::tests::the_views_presets_per_model_match`, `web/src/audio/songfile.test.ts`, `web/src/audio/roll.test.ts`, `crates/dsp/src/song/tests.rs::print_then_parse_is_identity`, `crates/dsp/src/notes/tests.rs::an_added_note_is_a_sixteenth_and_makes_room_for_itself`, `crates/dsp/src/notes/tests.rs::a_length_cannot_run_over_the_next_note_or_the_bar_line`, `crates/dsp/src/notes/tests.rs::every_edit_can_be_written_back_and_plays_the_same`, `crates/dsp/src/song/tests.rs::editing_a_note_rewrites_the_text`, `crates/dsp/src/ffi.rs::tests::notes_are_read_and_edited_through_the_abi`, `web/src/audio/split.test.ts`

### Requirement 6: No music logic in the view [MUST]

The view SHALL only send messages and draw; sequencing, generation, parsing and synthesis SHALL live in the engine. (ADR-0001)

#### Scenario: a song plays

- GIVEN a song in the composer
- WHEN it plays, is edited in the grid, or is highlighted in the editor
- THEN the view has sent only the text, edits and transport messages; the timing, the printed text and the highlighting came from the engine

**Tests:** review

### Requirement 7: Synth setups [SHOULD]

A setup file SHALL be an import only (ADR-0027, #360): the view no longer saves one, and **Open…** SHALL apply it, giving each synth it shows a song track, so its sound is folded into the song. The format SHALL hold the synths on screen, each one's kind, model and parameters, and the global parameters (`GlobalParam`) as a versioned `.synths.json` file, with parameters keyed by name, not id. **Open…** SHALL take a MIDI file, a setup, or both in either order; with a MIDI file the setup SHALL apply once the file is imported as the song (ADR-0022), alone it SHALL apply to the synths on screen. Applying SHALL reset each listed synth, set its model before its other parameters, then the globals; a song track's synth SHALL stay on screen. A file that isn't JSON or has an unknown version SHALL change nothing; unknown names, models and synths SHALL be skipped and listed in one notice, and the MIDI channel routes of an older setup SHALL be ignored with a notice. The Demo is its MIDI file alone: the song it imports picks its synths, and no setup overrides them (#327). A parameter added to the registry SHALL be saved without changes to the setup code; this carries the whole mixer (strips, sends, inserts, processors, equalizer, compressor). The setup SHALL also hold the group buses on screen, each with its strip parameters (`StripParam`) including its Out, and the console layout; a setup without them leaves groups and layout as they are. A setup written before the mixer was central (`EchoSend`, `ReverbSend`, the global `Echo*` and `Reverb*`) SHALL load, migrated to `Send1`, `Send2` and the knobs of P1 and P2 with one notice. The format is built in the view from values the engine reports, which clamps every value it receives (#41).

**Implementation:** `web/src/audio/setup.ts` (`buildSetup`, `parseSetup`, `applyPlan`, `shortF32`), `web/src/audio/engine.ts` (`saveSetup`, `openFiles`), `crates/dsp/src/params.rs::Param::is_global`

#### Scenario: round trip

- GIVEN a setup built from the view's values
- WHEN it is written as JSON, read back and applied
- THEN every per-synth parameter and global is the same 32-bit float as before

#### Scenario: unknown entries

- GIVEN a setup with an unknown parameter, model, kind, a duplicate and an out-of-range synth
- WHEN it is read
- THEN the rest applies and each skip is listed once

**Tests:** `web/src/audio/setup.test.ts`, `crates/dsp/src/params.rs::tests::typescript_mirror_matches`, `crates/dsp/src/params.rs::tests::ids_round_trip`

### Requirement 8: Mixer console [SHOULD]

The mixer view SHALL be a console: one thin strip per synth side by side (tape with the synth and its model, three insert slots, four sends P1–P4 each with pre/post and on/off, pan, mute, solo, fader with a dB scale and an LED meter, the song tracks that play on it), the four effect processors as rack modules (type among off, echo, reverb, chorus and flanger, the knobs of that type with their real units, return and return meter, and on P2–P4 a toggle "← P1" that makes it hear the processor before it, drawn as a link between the two modules), and the master section (equalizer with its response curve, compressor with its transfer curve and gain-reduction meter, master fader, stereo meters and a limiter light). Values shown SHALL be those the engine reports, and changes SHALL go out as messages; the meters SHALL come from the engine's peaks (spec 002 Req 2). A knob SHALL turn by dragging up or down (shift for fine), open a slider when clicked, reset on double-click, and move with the wheel and the arrow keys; a fader SHALL do the same with its taper (0 dB at the top). Group buses (ADR-0010) SHALL be strips too: a group SHALL be added and removed from the console (removing it sends what fed it to the master), and every strip and group SHALL have an Out selector offering only the destinations the engine accepts, a coloured tag under its tape saying which group it feeds, and three insert slots whose panel opens from a slot button with the type and its knobs in real units. A strip SHALL be reordered by dragging its tape onto another, collapsed to a sliver, and hidden (a hidden strip stays in the mix and can be shown again from the bar); the order, collapsed and hidden strips are layout, kept in the setup file and never sent to the engine. Which strips are dimmed SHALL follow the engine's mute and solo rules through groups. Clicking a strip's tape SHALL select its synth for the keyboard, and double-clicking it SHALL show that synth's panel. The synths SHALL stay mounted in the mixer view so the computer keyboard still plays.

**Implementation:** `web/src/components/ConsolePane.vue`, `web/src/components/console/` (`Knob.vue`, `Fader.vue`, `LedMeter.vue`, `ChannelStrip.vue`, `ProcessorModule.vue`, `MasterSection.vue`), `web/src/audio/console.ts`, `web/src/App.vue`, `web/src/components/TransportBar.vue`

#### Scenario: knob and fader maths

- GIVEN a fader position or a knob drag
- WHEN it is turned into a value
- THEN the taper puts 0 dB at the top and a linear level of 1, a drag of 170 px covers the whole range, and the EQ curve reaches each band's gain at its frequency

**Tests:** `web/src/audio/console.test.ts`, `web/src/audio/setup.test.ts`

### Requirement 9: Synth faceplate [SHOULD]

The selected synth SHALL be drawn as one faceplate in the console's hardware style, in the palette of its model (spec 005 Req 8), with the sections, control names and order of that instrument (`web/src/audio/models.ts`): rotary knobs with the console's popover (drag, click for a slider, double-click to reset, wheel and arrow keys, shift for fine), LED switches (on the Minimoog, rocker switches in the Model D's colours, #308), stepped selectors (waveforms with a drawn icon), an envelope drawn as a live curve from its attack, decay, sustain and release, and for the models with a patch panel (ARP 2600, MS-20, CS-15) a patch bay: sources down, destinations across, a lit point per connection, and a knob for the amount of each. A rail of tapes SHALL sit beside it, one per synth: its model and number, an LED meter, mute and solo, and a click to select it; a synth's model and preset are chosen in the faceplate's header. "+ Synth" SHALL offer the instrument families (`FAMILIES` in `models.ts`: Mono, Poly, Samplers, Drums) and their models; a family adds its first model, a model itself, each on its first preset (#132). A drum machine (TR-808, TR-909) SHALL lay its pads left to right in one row that scrolls sideways when narrow, each pad a column of knobs top to bottom, with a pad's out chosen from a pull-down (Master, or a group by its name, #194). Every pad's column SHALL have the same rows in the same order and one width, two knobs to a row (Tune · Decay, Tone · Level, Drive · Pan, then Out across), the kick's Drive in a slot the other pads leave empty, so the columns line up and the instrument fits above the keyboard (#364). A sampler SHALL show its sample panel instead of knobs for the samples: the multisampler its zones on a key and velocity map, the pad sampler its sixteen pads in a grid (spec 007). The view SHALL show the values the engine reports, SHALL send only parameter changes (Req 6), and SHALL keep every control reachable by keyboard. Which control sits on which panel, and how a value is scaled and labelled, is data in `models.ts`; the maths of the drawings (curve points, steps, matrix cells) is pure functions in `web/src/audio/faceplate.ts`.

**Implementation:** `web/src/components/SynthFaceplate.vue`, `web/src/components/synth/` (`Switch.vue`, `Rocker.vue`, `Selector.vue`, `EnvGraph.vue`, `PatchBay.vue`, `SynthRail.vue`, `Keyboard.vue`, `SamplerPane.vue`, `PadGrid.vue`, `SampleSlots.vue`), `web/src/audio/faceplate.ts`, `web/src/audio/models.ts`, `web/src/audio/sampler.ts`

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

**Tests:** `web/src/audio/faceplate.test.ts`, `web/src/audio/models.test.ts`, `web/src/audio/sampler.test.ts`, `crates/dsp/src/params.rs::tests::typescript_mirror_matches`

### Requirement 10: Names [SHOULD]

The user SHALL be able to rename every synth and group bus in place (#127), a synth also from its tape in the synth rail by a double-click, while a single click still selects it (#178). A synth and its console strip SHALL share one name. An instrument added with + Synth SHALL be named by its family when it is added, with the lowest free number: `Drum N` for the drum machines, `Sampler N` for the samplers, `Synth N` for the synths (#177); the name SHALL be kept, so adding, removing or switching other instruments never renames it, and removing an instrument SHALL free its name. A synth that plays a song track SHALL be named after the track, underscores read as spaces (`basso_continuo` is `basso continuo`), unless the user named it; a name a song gave SHALL follow the next song, a typed one SHALL stay (#327). A synth that is not named SHALL be `Synth N`, a group `Group N`. A name SHALL be trimmed and at most 24 characters, and an empty one SHALL restore the default (for an instrument, its family's). Every place a strip is labelled (console tapes and feeds tags, the Out selector, the synth rail and faceplate header, the composer's synth choices) SHALL use the same name. Names are labels only: they SHALL be kept in the view and the setup file (`names`), never in the engine (ADR-0001), and a setup without them SHALL load with the defaults.

**Implementation:** `web/src/audio/names.ts`, `web/src/audio/setup.ts::parseNames`, `web/src/components/EditableName.vue`, `web/src/components/ConsolePane.vue`, `web/src/components/console/ChannelStrip.vue`, `web/src/components/InstrumentsPane.vue`, `web/src/audio/engine.ts` (`addSynth`, `removeSynth`, `renameSynth`)

#### Scenario: a renamed synth

- GIVEN synth 2 renamed "Violin I"
- WHEN its strip is shown in the console and the composer's synth choices
- THEN it reads "Violin I" everywhere, and clearing the name brings back its default

#### Scenario: names travel with the setup

- GIVEN a renamed synth and group
- WHEN the setup is saved and opened again
- THEN the same names are shown; names that are not a strip are dropped with one warning, and an older file's lane names are ignored

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

### Requirement 12: The arranger [SHOULD]

The bottom pane SHALL show the arranger (ADR-0015, #171) under every view, and with it what opening files reported (a MIDI file imported, a setup's skipped entries). Columns SHALL be the arrangement's entries in order, as wide as their bars, each with its section's name and bars; rows SHALL be the song's fragments (in their track's colour), automation lanes and scenes; a lit cell SHALL mean the entry's section plays that row. Clicking a cell SHALL switch the row in that section (so in every entry of it), and a section's bars, the order of entries, adding a section or an entry, and the loop region (shift-click two bars of the ruler; shift-click inside it clears it) SHALL be edits sent to the engine, which changes the song and prints it back. Clicking a bar SHALL move the song there; while the song plays, the current entry SHALL be marked and a playhead SHALL follow it. The arranger SHALL NOT parse the song.

A fragment's row SHALL carry Mute and Solo for its track (#346, #355): the track's own flags in the song (spec 002 Req 11), so its frags stop while its synth plays on for live keys and other tracks; the synth rail and the mixer mute and solo the instrument, apart from them. A track that is not heard (muted, or another track soloed) SHALL have its rows dimmed. Automation and scene rows SHALL have none.

**Implementation:** `web/src/components/ArrangerPane.vue`, `web/src/App.vue`, `web/src/audio/engine.ts` (`arrange`, `applySong`), `web/public/worklet.js` (`arr`, `songSeek`), `crates/dsp/src/song.rs` (`toggle`, `add_section`, `set_bars`, `arrange_insert`, `arrange_remove`, `arrange_move`, `set_loop`), `crates/dsp/src/ffi.rs` (`arr_edit` and the arrangement getters)

#### Scenario: a section gets a fragment

- GIVEN `section intro 2: beat` in the arrangement and a fragment `hats`
- WHEN the cell of `hats` under `intro` is clicked
- THEN the engine's text reads `section intro 2: beat hats` and the cell is lit in every `intro` entry

**Tests:** `crates/dsp/src/song/tests.rs::arranger_edits_change_the_song_and_its_text`, `crates/dsp/src/ffi.rs::tests::song_round_trip_through_the_abi`

### Requirement 13: Song editor [SHOULD]

The composer's text SHALL be edited in a song editor (#203): line numbers in a gutter, the text coloured by kind (keywords, names, numbers, notes, pads, steps, rests, generator calls, `target.Param`, punctuation, comments), and the line and column of a parse error marked. The colours SHALL come from the engine's lexer (ADR-0001), run in a second instance of `dsp.wasm` on the main thread so typing costs the audio thread nothing; until it is ready the text SHALL show plain. The lexer SHALL be total: any text gives spans in order, never overlapping, within the text, counted in UTF-16 units; it SHALL NOT check the song, which is `parse`'s job. Tab SHALL indent by two spaces instead of leaving the editor.

**Implementation:** `crates/dsp/src/song/lex.rs::lex`, `crates/dsp/src/ffi.rs` (`lex_buf`, `lex`, `lex_ptr`), `web/src/audio/lex.ts` (`wasmLexer`, `paint`), `web/src/components/SongEditor.vue`, `web/src/components/ComposerPane.vue`

#### Scenario: a song coloured

- GIVEN a song with a drum lane `bd x.x.`, a line of notes with `c4` and `~`, an `auto` on `kit.Cutoff` and a `# fast` comment
- WHEN it is lexed
- THEN `frag`, `track` and `auto` are keywords, `bd` a pad, `x` a step, `.` and `~` rests, `c4` a note, `kit.Cutoff` a param and `# fast` a comment

#### Scenario: garbage

- GIVEN any text, including multi-byte characters
- WHEN it is lexed and painted
- THEN the spans are ordered and in bounds, and every line, empty ones too, is drawn

**Tests:** `crates/dsp/src/song/lex.rs::tests::keywords_names_and_numbers`, `crates/dsp/src/song/lex.rs::tests::lanes_and_notes`, `crates/dsp/src/song/lex.rs::tests::a_sampler_line_is_lanes_or_notes`, `crates/dsp/src/song/lex.rs::tests::a_sharp_is_not_a_comment`, `crates/dsp/src/song/lex.rs::tests::spans_count_utf16_and_are_ordered`, `crates/dsp/src/song/lex.rs::tests::the_view_has_a_name_per_class`, `crates/dsp/src/song/lex.rs::tests::any_text_lexes_in_bounds`, `web/src/audio/lex.test.ts`

### Requirement 14: Build info [SHOULD]

The transport bar SHALL show which build is running (#197): the page's version, and once audio is on the engine's, with a pop-up giving both versions, their commits and the page's build time, and a Copy button for a bug report. The engine SHALL report its version as a number (`version_code`, major·10000 + minor·100 + patch) and its commit as the first eight hex digits of `ALGO_BUILD_SHA` (`build_id`, 0 when the build did not say). When the page and the engine differ in version, or in a commit both report, or the engine is too old to say, the transport bar SHALL say so and how to fix it.

**Implementation:** `web/src/audio/buildinfo.ts` (`page`, `versionOf`, `buildOf`, `mismatch`, `details`), `web/src/components/TransportBar.vue`, `web/vite.config.ts`, `crates/dsp/src/ffi.rs` (`version_code`, `build_id`)

#### Scenario: a stale engine

- GIVEN a page of v0.32.0 and a cached `dsp.wasm` of v0.31.0
- WHEN audio powers on
- THEN the transport bar says "Page v0.32.0 but engine v0.31.0" with the hint to hard-refresh

**Tests:** `web/src/audio/buildinfo.test.ts`, `crates/dsp/src/ffi.rs::tests::a_build_id_is_the_first_eight_hex_digits_or_zero`, `crates/dsp/src/ffi.rs::tests::exports_drive_the_engine`

### Requirement 15: The Modular code [SHOULD]

The Modular faceplate SHALL show its synth's SuperCollider SynthDef (ADR-0024, superseding the Sound screen of ADR-0020) as the engine hands it out, its knobs' values in its numbers, built as it is typed, half a second after typing stops, Ctrl+Enter at once, and folded into the track's setting (ADR-0027, #359). The code SHALL sit beside the amplifier envelope, with the last error above it, in an editor with line numbers and highlighting from the engine's SuperCollider lexer (`modular::lex`, sharing the song editor's classes and component). Every number of the SynthDef SHALL be a knob on the faceplate, grouped by the UGen it sets, with the range and scale the engine gives it (`knob_list`); turning one SHALL change the sound at once and the number in the text once the knob rests, and an edit not yet applied SHALL be kept (#329). An applied SynthDef SHALL be a message to the engine, which builds it on the synth; one that does not build SHALL show its line, column and message, and the synth SHALL play on as it was. A preset or a song that changes the synth's code SHALL change the text shown. The code SHALL live on the synth like a patch: saving the track's sound as a setting (Req 13) SHALL put it into the song. A setup file and a user synth preset SHALL keep a Modular synth's code and build it after its parameters; code on another model SHALL be ignored with a warning.

**Implementation:** `web/src/components/synth/CodeEditor.vue`, `web/src/audio/engine.ts` (`setCode`, `codes`), `web/src/audio/setup.ts` (`code`), `web/src/audio/presets.ts` (`code`), `web/public/worklet.js` (`setCode`, code in `sendParams`), `crates/dsp/src/engine.rs::Engine::set_code_from_buffer`, `crates/dsp/src/ffi.rs` (`code_*`)

#### Scenario: a SynthDef saved into the song

- GIVEN a Modular track whose synth has code, a knob of it turned
- WHEN the track's sound is saved as a setting
- THEN the song holds the setting with the code under it, the turned knob's value in place of its number

**Tests:** `crates/dsp/src/modular/lex.rs::tests`, `crates/dsp/src/engine/tests.rs::a_modular_synth_lists_its_knobs`, `crates/dsp/src/engine/tests.rs::a_saved_setting_keeps_the_code`, `web/src/audio/engine.test.ts` (a SynthDef sent, the code of each synth and its error kept), `web/src/audio/setup.test.ts` (Modular code), `web/src/audio/presets.test.ts` (Modular synth presets)

### Requirement 16: The Assistant [SHOULD]

The view SHALL have an Assistant (#387, ADR-0028): a pane beside every view, shown and hidden from the transport bar, that pops out into its own window (`assistant.html`). It SHALL talk only to the assist server on the same origin under `/api`: `GET /api/health`, `GET /api/providers` for the providers and models to pick (the server's default selected), and `POST /api/assist` with the current song text, the request, the provider, the model and a fragment to focus on (by default the cued one, #375), read as server-sent events. Progress, tool steps (✓ or ✗ with their summary), the model's text and errors SHALL be drawn as they arrive, and Stop SHALL abort the request; a refusal before the stream (400, 429, 503) SHALL show its message. A proposed song SHALL be shown as a line diff against the song as it is now, with **Apply**, which loads it through `loadSong` so the engine parses it again (ADR-0012), **Discard**, and after Apply **Undo**; when the request is done, its rounds, seconds and tokens (input, cached, output). The requests of the session SHALL stay listed, and one is asked again with a click. Without an answer from `/api/health` the pane SHALL show only how to start the server. The window SHALL get the song from the main window over the `algo-synth-assistant` BroadcastChannel and send Apply back to it, so only the main window touches the engine (ADR-0001); while it is open the pane says so, and when the main window is gone the window SHALL say a song can't be applied, and reconnect when the app opens again. No key and no prompt reach the browser.

**Implementation:** `web/src/components/AssistantPane.vue`, `web/src/components/AssistantWindow.vue`, `web/src/audio/assist.ts` (`health`, `providers`, `assist`, `SseParser`, `toEvent`), `web/src/audio/linediff.ts` (`diffLines`, `collapse`), `web/src/audio/assistlink.ts` (`serveWindow`, `connectMain`), `web/src/App.vue`, `web/src/components/TransportBar.vue`, `web/assistant.html`, `web/vite.config.ts`, `tools/fake-assist.mjs`

#### Scenario: a faster song, applied from the window

- GIVEN the app with audio on and a song at tempo 120, the Assistant popped out into its own window, and an assist server
- WHEN "a little faster" is sent from the window and the server proposes the song at tempo 128
- THEN the window streams the steps and shows `- tempo 120` and `+ tempo 128`, and Apply in the window sets the main window's composer to 128 BPM

#### Scenario: no server

- GIVEN no assist server running
- WHEN the Assistant is shown
- THEN it says the assistant needs its server (`make assist`) and nothing else

**Tests:** `web/src/audio/assist.test.ts`, `web/src/audio/linediff.test.ts`, `web/src/audio/assistlink.test.ts`, `web/src/audio/split.test.ts`

### Requirement 17: The Decks view [SHOULD]

The transport bar SHALL switch to a Decks view (ADR-0029, #391) beside Synths, Mixer and Composer. It SHALL show deck A as the song the rest of the app edits and decks B–D with **Load song…** for a `.song` file, a Start of Next bar, Next phrase or Now (spec 002 Req 18), a **Sync to deck A** switch (on by default) with how far the last bar line pulled the deck or "locked", Play, shown as Cued… until the deck starts, and Stop, the deck's bar and step, a parse error with its line and column, and for every deck a level, a side of the crossfader (Thru, Left, Right), its peak and, for B–D, its dropped blocks; one crossfader runs from Left to Right under them, and the head shows the master's tempo. A worker deck SHALL start the first time a song is loaded into it, after audio is on. The page SHALL be served cross-origin isolated (`Cross-Origin-Opener-Policy: same-origin`, `Cross-Origin-Embedder-Policy: require-corp`) by Caddy and Vite; without isolation the view SHALL say decks B–D can't run and offer deck A only.

**Implementation:** `web/src/components/DecksPane.vue`, `web/src/audio/decks.ts` (`decks`, `loadDeck`, `playDeck`, `stopDeck`, `setDeck`, `setCrossfade`, `onDecks`), `web/src/components/TransportBar.vue`, `web/src/App.vue`, `Caddyfile`, `web/vite.config.ts`

#### Scenario: a second song faded in

- GIVEN audio on, the demo as deck A on the left of the crossfader, and a song loaded into deck B on the right
- WHEN deck B and the transport play and the crossfader moves all the way right
- THEN deck B is heard with no dropped blocks and deck A's peak falls to −∞

**Tests:** `web/src/audio/decks.test.ts`
