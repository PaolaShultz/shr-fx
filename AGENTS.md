# shr-fx — standalone dual-engine send-effects rack

## Product and scope

Build a compact external send/return effects gadget for a mixer or audio
interface. Two independent effect engines, A and B, run in one Rust process.
They receive mixer sends and return added sound. This is a standalone app,
not part of SHR-DAW and not a plugin host or insert processor.

The mixer retains the dry signal. Outputs from this app are **wet-only**.
No compressor, limiter, gate, mastering strip, dry-through bypass, or insert
workflow. Filtering, gain staging, and restrained saturation inside an effect
are allowed because they shape its generated return. Fault muting and bounded
feedback are safety mechanisms, not a user-facing dynamics rack.

## Two engines and all supported routing combinations

Model input selection independently from effect algorithm and return layout.
Each engine can consume either available mono input, an explicit mono sum,
or an available stereo pair. A and B may use separate sends or share a source.
With two inputs support independent mono sends, one shared mono send, and one
stereo source; where four input channels exist also allow two independent
stereo sources. Do not require four inputs to use four outputs.

Each engine produces a prepared stereo wet result; explicit mono folding and
return placement happen afterward. Support the cross-product of valid input
choices with these output layouts, subject only to available physical ports:

| Return layout | Engine A | Engine B | Required outputs |
| --- | --- | --- | --- |
| Dual mono | mono return | separate mono return | 2 |
| Shared stereo | stereo contribution | stereo contribution, summed with A | 2 |
| Dual stereo | separate L/R pair | separate L/R pair | 4 |
| Mono + stereo | mono return | separate L/R pair | 3 |
| Stereo + mono | separate L/R pair | mono return | 3 |
| Single-engine | mono or stereo | off, or reverse A/B | 1 or 2 |

Output port assignment is explicit; mono returns need not be adjacent and
stereo pairs must have distinct assigned L/R channels. Shared stereo is the
only deliberate summing case; prevent accidental duplicate connections.
Use a documented mono fold and deterministic return headroom, with independent
engine return levels. Display missing ports and refuse an invalid apply without
altering the sounding routing. Never silently fold two stereo returns onto a
two-output card or infer routes from enumeration order.

A finite routing matrix is enough; do not turn this into an arbitrary patchbay.
No A/B feedback loop or recursive engine routing. Combinations inside an engine
use bounded, prepared parallel-only effect slots with explicit wet paths.
The owner superseded the earlier three-slot limit: support up to eight slots
per engine, including reverb + delay + chorus + harmonic exciter together.

## Sound palette and cost discipline

Start with useful inexpensive algorithms, then add distinct sounds based on
listening and measured dual-engine headroom:

- Reverbs: small room, chamber, plate-like diffusion, and an economical larger
  space/hall if the target budget supports it. Algorithmic delay networks are
  a sensible starting point. A cathedral label is not a reason to ship an
  expensive or poor-sounding effect; omit/defer anything the Pi cannot sustain.
- Delays: mono/stereo, ping-pong, multi-tap, rhythmic divisions, and tape-echo
  character using bounded feedback, filtering, restrained saturation, and
  economical wow/flutter. No claim of exact hardware emulation.
- Chorus and other inexpensive modulated wet-delay textures; evaluate phaser,
  flanger, diffusion, and spatial movement when they add a useful return.
- Parallel combinations such as reverb + delay + chorus + exciter on one send,
  multiple delays or layered reverbs. Keep the eight-slot per-engine bound and
  explicit CPU/memory budgets. No serial routing between slots for now.
- Parallel harmonic exciter/colour/saturator returns are allowed. Their delay relative to dry
  can change phase and comb filtering, so judge the combined mixer result;
  do not pretend every parallel effect is insensitive to latency.

Do not copy SHR's master strip, dry mixer, compressor, or limiter along with
its reusable effects. An existing insert algorithm must be adapted and tested
for wet-only operation; an internal dry branch can otherwise leak into a send
return. Chorus/flanger especially need listening in the external dry+wet mix.

## Time, tempo, and live control

- Treat approximately **5–6 ms** of I/O/processing latency as the owner's
  initial predelay allowance. It is an estimate, not measured round-trip
  latency. Do not add another fixed 5–6 ms delay on top of the device path.
- Document inherent path latency separately from intentional reverb predelay
  or echo time. If offering total predelay compensation, subtract only a
  measured/configured path value and clamp at zero: the app cannot produce a
  return earlier than its physical path. Short rhythmic delays still need
  honest timing; buffering is not automatically harmless.
- Each engine supports free milliseconds and tempo divisions where meaningful.
  Provide touch/controller TAP and manual BPM adjustment; computer-keyboard
  numeric entry is optional, never the only way to set tempo.
- Support MIDI keyboard/controller operation: configurable notes/buttons for
  tap and actions, CCs/relative encoders for parameters, visible learn, proper
  release/repeat handling, and pickup for supported absolute controls.
  Consume commands locally; no notes or controller traffic sent downstream.
- Provide a clear internal/tap/manual versus external MIDI-clock source with
  optional shared A/B tempo or independent engine tempos. External clock is
  bounded and smoothed; show loss and hold the last valid tempo until explicit
  fallback. MIDI-clock loss/stop must not cut an audible tail unexpectedly.
- Smooth continuous controls. Delay-time changes and tempo changes must have
  defined click-free behavior (e.g. crossfade or intentional tape glide),
  never an accidental buffer jump. Prepare algorithms and structural edits
  off-thread, publish at callback boundaries, and retire memory off-thread.
- Wet bypass stops new excitation and drains tails, then returns silence;
  a visible immediate MUTE/PANIC clears tails. It never passes dry input.
  Independent A/B edits must preserve the other engine. Bound any temporary
  crossfade overlap; if it exceeds budget, use a deliberate fade-out/swap/in.

## UI, persistence, and failure behavior

The performance workflow is Rack → open an effect → adjust → Back. Rack shows
all eight slot states/levels for the selected engine; opening an effect gives
it a dedicated screen without audio soloing. Keep A/B identity and switching
visible, and retain the selected slot, value and focus on menu return. The
effect screen shows input/output circular LED meters, tempo/source and return.
Normal screens retain rows 11–12 for actions and row 13 for shared status/faults.

The owner's corrected hardware has **eight pads/buttons total**, sixteen
rotaries in two rows of eight (1–8 top, 9–16 bottom), and clicks only on 1 and 9.
Do not restore an extra eight-button bank or assume other push switches. The
current map in `docs/INTERFACE.md` uses 1 for browse/open and 9 for value/back;
setup learns 26 physical inputs. Pads toggle their own wet bypass without
selecting or opening a slot. Empty slots stay empty until explicitly configured.
Keep existing explicit/legacy mappings compatible. Model, CC/note identities,
encoder encoding, motorization and controllable LEDs must not be invented.

Engine selection, TAP, wet bypass, Mute/Resume, routing, sound recall/save,
controller setup, PANIC and Exit must remain reachable by touch and configured
MIDI at 40×13. Keep performance edits immediate and smoothed, structural edits
cancellable, live/draft conflicts explicit, and fault feedback above routine
status. Treat bypass as excitation off/tail allowed, never measured tail state.

Store strict versioned rack snapshots with A/B algorithms, parameters, tempo
policy, and logical routing; keep physical device assignments in private local
configuration. Recall validates complete prepared state before changing sound.
Failed load keeps the active rack exact. No required network/catalog account.
Keep CPU/peak/non-finite/xrun diagnostics off-thread. Feedback runaway,
non-finite DSP, missing buffers, or a failed engine must silence the affected
owned return safely and visibly, preserving the other engine where possible.

## Starting references

- `../shr-daw/docs/AUDIO_GRAPH.md`: wet-aux behavior, tails, publication,
  bounded routing and memory, and measurements versus acceptance
- `../shr-daw/src/effects/reverb.rs`, `delay.rs`, `modulated_delay.rs`,
  `phaser.rs`, `tremolo_pan.rs`, `distortion.rs`, and `filter.rs`
- `../shr-daw/src/audio_graph_runtime.rs`, `audio_graph_client.rs`, and
  `jack.rs`: inspect ownership carefully; extract only necessary pieces
- `../shr-tone-over-9000/README.md`: standalone live-input processor,
  background preparation, atomic chain changes, and touch control
- `../shr-sampler/docs/HOST_ARCHITECTURE.md` and `LIVE_PROCESS_CONTRACT.md`:
  bounded host/queue/fault ownership patterns
- `../shr-daw/docs/CONTROLLER_INTERFACE.md` and controller profile/learn code

## Build order and acceptance

First implement two independent bounded wet engines with one delay and one
room/chamber algorithm, the full routing matrix, and keyboardless controls.
Then add tempo/tap/MIDI clock, snapshots, and the broader affordable palette.
Routing completeness is a first-version requirement; the sound catalog may
expand in measured stages. Do not defer the four-output path merely because
the presently connected card has two outputs; test it synthetically.

Use impulses and distinguishable channel signals to test every input/output
combination, independent A/B state, mono folds, stereo isolation, shared-return
headroom, wet-only bypass/tails, time changes, sample-rate changes, faults,
load cancellation, and bounded memory/work. Normal tests cover finite output,
allocation freedom, deterministic seeded modulation, and recovery. Expensive
spectral/cost matrices and audition rendering stay opt-in.

On-device acceptance measures both engines together under rapid controls,
long tails, demanding feedback, and structural transitions: callback cost
percentiles/maxima, xruns, memory, temperature, and actual path latency.
Listening compares external dry plus wet at matched levels. Only retain large
spaces and combinations that meet the measured budget and sound useful.

## Shared platform and working agreements

- Work in this project only. Other Codex sessions will work independently in
  sibling projects. Existing repositories are read-only references: inspect
  their instructions before borrowing code, preserve licence/provenance, and
  copy/adapt only the pieces needed here. Do not modify SHR-DAW, remove its
  features, create shared libraries in sibling folders, or depend on another
  new project's implementation being ready.
- Target this Raspberry Pi 5 (2 GB RAM, active cooling, NVMe), 64-bit Linux,
  Rust, ratatui/crossterm, JACK audio, and ALSA Sequencer MIDI where needed.
  Start with the existing exact Rust 1.97.1 pin; adopt changes deliberately.
  Prefer a small native application with few dependencies.
- The touchscreen terminal is **40 columns × 13 rows**, not a desktop UI
  shrunk afterward. Preserve the existing 480×320 tty and
  `Uni2-TerminusBold24x12` font. Touch is terminal mouse input. Normal screens
  reserve row 13 for one shared useful status/fault row and rows 11–12 for
  contextual controls. A deliberate meter view may use a different body but
  must keep visible actions and the shared status row. Use circular LED meters.
- Essential operation must work without a computer keyboard: touch and a
  configured MIDI controller/keyboard, including selection, confirm, cancel,
  stop, and exit. Supply keyboard equivalents. Keep labels literal, touch
  targets usable, selections stable, and edits cancellable. Optional naming
  must never block operation; generated names are acceptable.
- Device identities, routes, controller mappings, paths, and installation
  choices belong in private configuration. No hardcoded personal equipment.
  Keep user data outside tracked files; never copy sibling private `user/`
  state, mappings, recordings, presets, logs, or downloads.
- Prepare data off the audio thread. Callbacks must not allocate/deallocate,
  lock, access files, log, format, spawn, wait, or perform unbounded work.
  Bound queues, memory, per-period work, and recovery. Publish meters through
  snapshots/atomics without aliasing mutable DSP across threads.
- Attach to the existing JACK server; never start, restart, or reconfigure it
  automatically. Own only this app's clients, processes, and exact connections.
  Missing or ambiguous required routes must be visible and must not silently
  resolve to another channel. Keep the terminal usable when audio is absent.
- Use release artifacts for live performance acceptance. Measure on the Pi;
  compilation, offline renders, and callback simulations do not establish
  whole-system latency, xruns, thermal headroom, or listening quality.
- Agent owns test selection. During implementation use focused production
  regressions; run the full normal suite when engine/rendering, shared schema,
  persistence, routing, concurrency, or safety changes. Keep unit, contract,
  recovery, bounded-audio, and 40×13 interaction tests in the normal suite.
  Historical auditions, exhaustive matrices, long soaks, and evidence
  renderers are opt-in with a documented command. Report what ran and skipped.
- Implement and verify ordinary local work without repeated confirmation.
  Creating this brief does not authorize live audio, MIDI transmission,
  recording, service/autostart changes, or borrowed-hardware tests: prepare
  those paths for a separately authorized on-device session. Do not publish
  new repositories or deploy services merely because a reference repo does.

## Shared launcher boundary

Use stable gadget IDs `rec`, `fx`, and `daw`; `go` is the launcher.
This project and repository are `shr-fx`; its executable is `shr-fx`, while
its stable launcher gadget ID remains `fx`.
New gadgets must run directly without the launcher and return to their caller on a visible
keyboardless Exit action. Expose ordinary executable/arguments, `--help`,
`--version`, a configurable private data root, and a documented graceful
shutdown behavior. Do not require a launcher SDK, daemon, network API, or a
shared crate. The launcher owns launching/waiting; the gadget owns its audio,
state, safe stopping, and terminal restoration. Restore raw mode, cursor,
mouse reporting, and the alternate screen on normal exit and handled errors.

## Documentation maintenance

- Keep `README.md` as the run/operate entry point, `docs/INTERFACE.md` as the
  interaction/controller contract, and `docs/ARCHITECTURE.md` as the owning
  implementation/schema contract. Update the relevant document in the same
  change as behavior, hardware assumptions, persistence or command changes.
- Keep `docs/PLAN.md` a current handoff with completed scope and concrete next
  work. Move superseded plans/limits to `docs/HISTORY.md`; do not leave old
  instructions alongside the current workflow as if both were requirements.
- Record dated check results and hardware limits in `docs/ACCEPTANCE.md`.
  Keep test counts there instead of copying them into multiple current docs.
  Historical results are evidence for their recorded milestone, not fresh runs.
- Generate `docs/screens/` from `examples/screen_gallery.rs` through
  `python3 scripts/screen_gallery.py` after visible renderer changes. Never
  hand-edit generated previews to hide a mismatch. Keep the installed font and
  palette read-only. Check local Markdown links and `git diff --check` before
  committing documentation. A documentation-only pass needs those checks;
  select further tests from actual code changes and the test policy above.

## GigPies task tracking

Use `docs/GIGPIES_IMPLEMENTATION.md` for GigPies work owned here. Keep each task plan,
implementation state, acceptance checklist, evidence and next action in the same
card; update it with the change. Shared integration tasks have one card in
GigPies, linked from contributor plans. STATUS, maps, handoffs and knowledge notes
route to task owners or preserve dated evidence; never mirror current task state.
Archive closed cards once; keep the active queue limited to open work. Reference
projects do not become GigPies runtime modules merely because code is reused.
