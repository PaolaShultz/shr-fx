# Delivery history and superseded evidence

This is an archive, not the current implementation plan. Slot limits, schema
versions, UI workflows, test counts and publication boundaries below describe
their recorded milestones. Use [PLAN.md](PLAN.md) for the current handoff,
[INTERFACE.md](INTERFACE.md) for operation, and [ACCEPTANCE.md](ACCEPTANCE.md)
for current verification and the next authorized hardware session. Historical
benchmarks are retained as observations, not current performance acceptance.

## v0.3 DSP milestone (before the controller redesign)

The owner wants Delay + Reverb + Chorus + Harmonic Exciter together on one
send/engine and one stereo output pair. The earlier three-slot cap is replaced
by an eight-slot per-engine bound, retaining prepared storage and bounded CPU
work. A single engine can feed its entire parallel mix to A stereo / B off;
Shared stereo remains available for two independent engine contributions.
Physical port choices stay private and explicit.

Implemented:

1. Strict schema v3: eight slots, 2–8 active in MultiFX (four by default), all
   families assignable to every slot. Typed v1/v2 migration preserves old
   values and gains without rewriting files. Private configuration was v1 at this milestone; current local saves use v2.
2. Harmonic Exciter with Warm/Bright character, Tune, Drive, Tone and wet level.
   Original filtered nonlinear residual with 4x FIR oversampling, independent
   stereo histories, smooth controls and no linear dry branch. Drive zero is
   silence; bypass drains and Panic clears the owning engine.
3. Mixing headroom is 1/max(3, configured count): legacy two/three-slot sounds
   keep 1/3; four-slot mixes use 1/4. Slot level/bypass never renormalize other
   branches. Count changes use the existing selected-engine fade/clear.
4. The initial v0.3 UI used three rows per slot page on the overview and
   Effects draft. Touch and MIDI reach slot eight. Returning preserves the edited
   slot's page; shrinking a draft count clamps navigation while retaining
   inactive settings. Apply merges edited scalars; Cancel retains active sound.
5. Sixteen prepared slots, at most sixteen active algorithms and no structural
   overlap. Maximum-rate heap allocation is 70,131,200 bytes (66.88 MiB), below
   the 80 MiB cap. Exciter's short histories are additional inline arrays.
6. All 74 normal tests, formatting, Clippy, release build and five offline PTY
   smokes pass. New checks cover four effects through one stereo return, every
   slot count, last-slot storage/MIDI, legacy migration, paging/cancel/resize,
   callback allocation freedom, harmonic character, selected alias suppression,
   zero-drive silence and control continuity. Release cost evidence is recorded
   in ACCEPTANCE.md; the ten-case matrix remains opt-in afterward.

Fructal Implement: the unnecessary three-slot restriction is removed while the
AGENTS.md requirements for bounded callback work, wet-only output, exact routing
ownership, keyboardless reach and state preservation remain active. Observed
software checks cover normal editing, cancellation, recovery, controller
navigation and independent engine state. The selected page survives return;
count changes visibly state their headroom effect. Physical touch usability,
external dry-plus-wet listening, real JACK deadlines, temperature and measured
path latency remain outside the completed local verification.

## v0.2 parallel MultiFX delivery

The owner requested a broader reverb/delay palette, chorus, and up to three
**parallel-only** effects per A/B engine. This supersedes the earlier tentative
serial/parallel design. One engine can provide Delay + Reverb + Chorus from the
same send, with independent slot parameters, wet levels and tail-draining
bypass. Duplicate effect types are supported. Single mode retains slot 1;
MultiFX selects two or three active slots with a fixed 1/3 contribution gain.

Implemented work:

1. Versioned three-slot model, strict full-state validation and read-only
   migration from original v1 Delay/Room snapshots. Private physical/controller
   identities remain separate; loading does not write or partially apply state.
2. Five original reverb profiles (Room, Small room, Chamber, Plate, Hall), four
   delay characters (Digital, Tape echo, Multi-tap, Diffused) with ping-pong,
   and wet-only Chorus/Ensemble. Every family is prepared for all six slots.
3. Slot-local parameter smoothing, time crossfades, structural fade/clear,
   deterministic modulation and bounded bypass tails. Mode/count edits affect
   only the selected engine. No serial path, recursive routing or DSP allocation
   in the callback. Maximum-rate prepared storage is about 25.16 MiB, capped
   at 32 MiB; at most six algorithms run without structural overlap.
4. Native 40×13 effects/slot/timing draft workflow, direct slot access from the
   MultiFX overview, and explicit controller targets for every slot. Apply
   merges only edited fields; Cancel, engine switching, Panic and concurrent
   controller/clock activity have regression coverage.
5. Production palette/parallel/legacy-storage/UI regressions, expanded
   allocation checks, a release PTY MultiFX smoke, and an opt-in release cost
   matrix covering both engines fully loaded. All 65 normal tests, formatting,
   Clippy, release build, four PTY smokes and the cost matrix pass. Evidence at that milestone and outstanding on-device work are recorded in `ACCEPTANCE.md`.

Fructal Implement: the musician edits one engine draft through Slots and Timing;
returning retains the draft and Cancel leaves the active rack exact. The
callback receives validated scalars only; slots share the source, never one
another's output. Keyboard/touch/MIDI reach the same actions. Failure retains
selection for repair and Apply preserves live mute/tempo and unrelated edits.
Observed software checks cover those contracts; sound quality, physical reach,
actual JACK deadlines and thermal headroom remain unmeasured. The expanded
palette is a software candidate until that acceptance is performed.

The following section records the completed original baseline.

## v0.1 delivery

The supplied AGENTS.md is the product contract. This checkout initially
contained only that brief; no application or user state existed.

1. Establish a standalone Rust 1.97.1 executable, private data root and Git.
2. Implement strict rack/routing models and all input/output combinations,
   including synthetic four-port operation. Keep physical identities separate.
3. Build two independently owned wet engines: stereo delay and room reverb.
   Preallocate both algorithms, smooth controls, bound feedback, fade structural
   changes, drain bypass tails, and latch faults per engine. No dry branch.
4. Attach only to an existing JACK server on explicit invocation. Own four
   stable input/output slots, exact user-selected physical connections, bounded
   control publication, atomic meters and fault status. Keep offline UI useful.
5. Provide a 40×13 touch/keyboard/controller interface: both engines, parameters,
   routing draft/apply/cancel, physical ports, tempo/tap/clock, save/load, learn,
   mute, panic and Exit. Reserve the last three rows for controls and status.
6. Store strict versioned snapshots by atomic replacement; failed recall keeps
   active state. MIDI Sequencer is input-only and explicitly enabled. Provide
   button edges, release handling, relative controls and absolute pickup.
7. Run focused regressions while developing, then the complete normal suite,
   formatting, Clippy, release build and a hardware-free terminal smoke test.
   Document invocation, behavior, limitations and acceptance still required.
8. Review staged files and commit the usable version locally. No remote publish.

First-version palette deliberately stops at delay and room. Ping-pong is a
bounded delay option. Larger spaces, chorus and combinations follow listening
and measured dual-engine budgets. No claims of hardware emulation.

Validation includes impulses/distinguishable channels, routing cross-product,
non-adjacent mono returns, A/B isolation, wet-only tails/mute, time changes,
finite output, rate changes, missing buffers/ports, control overload, snapshot
rejection and cancellation, callback allocation freedom, controller edges,
clock loss, and 40×13 reachability. Long soaks/cost renders are opt-in.

Live JACK/ALSA hardware testing, audible listening, recordings, latency/xrun/
thermal measurements, services and installation are outside this local task's
existing authorization. Software evidence does not substitute for them.

Fructal mode: Implement. Actors: musician, launcher, audio callback, JACK graph
owner and controller. Necessary constraints come from AGENTS.md: wet-only
returns, explicit ownership, bounded callback work and keyboardless operation.
The design keeps unavailable ports visible, rejects invalid drafts before
publication, keeps selection on failure, and puts Cancel/Retry beside editing.
The initial state has no prior interaction to preserve. Normal/recovery tests
supplied observed software evidence; physical usability remains unmeasured.

### Delivered first version

Steps 1–7 are implemented and verified; the deliverable is a local `main` commit
containing the application, locked dependencies, this plan, user instructions
and acceptance evidence. The release executable starts offline by default.
That baseline had 46 passing tests. The new offline cost simulator was
validated once and remains opt-in. Live acceptance and expansion of the sound
palette remain the explicitly separated next stage in `docs/ACCEPTANCE.md`.

### Project naming

The project, Cargo package, executable and local repository directory are
`shr-fx`. The launcher gadget ID remains `fx`; existing JACK/ALSA names and
private data locations continue to use that ID.

## Earlier verification evidence

### v0.2 parallel MultiFX evidence (historical)

Recorded on 2026-09-08, Rust 1.97.1, aarch64 Linux. The owner authorized the
expanded palette and three parallel slots per engine. Serial routing is absent.
The original physical-routing and launcher contracts remain in place.

Final checks passed: formatting, Clippy (`--all-targets -D warnings`), **65
normal production tests**, release build, and all four offline 40×13 PTY
smokes (touch, MultiFX touch, keyboard and SIGTERM). The opt-in cost matrix
also passed, separately from the normal suite.

Observed DSP/storage checks cover every flavor at 8/48/192 kHz, wet-only output,
deterministic modulation, distinct impulse responses, decaying reverbs, chorus
control continuity, three independent delay times, vocal branch summing, fixed
slot gain, slot-local edits/bypass, shared-return fault removal and recovery.
Strict v1 sound migration preserves original parameters without rewriting files;
v2 rejects invalid counts, extra slots, serial topology, unknown/missing fields
and invalid inactive settings. Local MIDI mappings retain their separate schema.

The complete callback allocation/deallocation test exercises all six slots under
edits, faults and Panic. Maximum-rate prepared DSP allocation is **26,381,120
bytes (25.16 MiB)** against a **32 MiB** cap. The old 8 MiB cap below belongs to
the two-effect v0.1 baseline. Work is bounded to six active algorithms; structural
changes never run a second algorithm or rack concurrently.

The release offline cost matrix passed: eight cases, each with one minute of
simulated audio (11,250 blocks, 48 kHz / 256 frames). These are regular-thread
wall times for `Processor::process`, **not real JACK callback deadlines**.
The nominal comparison period is **5333.333 µs**.

| Case | p50 µs | p95 µs | p99 µs | Maximum µs |
|---|---:|---:|---:|---:|
| Two vocal MultiFX engines | 377.553 | 415.350 | 422.683 | 1192.049 |
| Six halls | 637.440 | 663.034 | 689.052 | 4611.881 |
| Six plates | 635.052 | 654.885 | 679.089 | 748.607 |
| Six tape echoes | 200.370 | 242.239 | 255.165 | 308.313 |
| Six multi-tap delays | 305.072 | 319.554 | 323.813 | 395.220 |
| Six diffused delays | 239.147 | 242.610 | 255.221 | 3462.536 |
| Six ensemble choruses | 267.887 | 269.610 | 271.240 | 281.683 |
| Six slots under rapid edits | 316.109 | 360.164 | 372.386 | 804.847 |

The isolated maxima for halls and diffused delays are much larger than their
p99 values. Their causes were not isolated and these measurements do not
establish a live deadline margin. Keep them in the evidence; do not substitute
p99 for a worst-case scheduling guarantee. The expanded palette remains a
software candidate pending matched-level listening and actual JACK/thermal
acceptance.

Normal tests also cover every new 40×13 page, controller traversal, direct slot
parameters/pickup, a complete vocal draft, cancellation, invalid-Apply repair,
engine selection, live clock display, and concurrent CC/Panic preservation.
The release PTY smoke now includes creating a three-slot MultiFX mix, selecting
Ensemble in slot 3, applying, and exiting by touch. No JACK or MIDI port is
opened by these checks, and no audio files are rendered.

The cost matrix was explicitly run because DSP and its benchmark assumptions
changed. It stays opt-in; subsequent UI-only fixes do not require rerunning it.
Live audio/MIDI, physical latency, xruns, thermal tests, listening, recording,
services, installation and publication remain unexercised.

### Original v0.1 software evidence (historical)

Recorded on 2026-09-07 with Rust 1.97.1, aarch64 Linux:

- Formatting and Clippy (`--all-targets -D warnings`) passed.
- Full normal production suite: **46 passed**, one soak excluded by default.
- Release build passed. The 40×13 PTY smoke passed touch, keyboard and SIGTERM
  exit, including raw mode, cursor, mouse reporting and alternate-screen cleanup.
- The new opt-in soak was run once to validate the simulator itself: one minute
  of simulated audio, 11,250 blocks at 48 kHz / 256 frames. Regular-thread
  processor timings were p50 **77.795 µs**, p99 **114.333 µs**, max **142.073 µs**
  against a 5333.333 µs period. These are offline process timings, not real JACK
  callback measurements or live performance acceptance.
- Callback-core allocation/deallocation checks passed. Maximum-rate DSP
  preparation stayed within the tested **8 MiB** allocation budget.

Live JACK/ALSA integration, audible auditions, path latency, xruns, thermal
headroom, recordings and service changes were intentionally not exercised.
