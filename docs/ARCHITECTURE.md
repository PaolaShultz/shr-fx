# Ownership and bounded work

## Versioned source-frame adapter

`c_api.rs` and [the C header](../include/shr_fx.h) expose the `shr_fx_v1_*`
symbols in `libshr_fx.so`. The adapter prepares one fixed stereo Digital Delay
using the same `Delay` implementation as the rack. It opens no JACK/ALSA ports
and owns no device, clock, transport, file, thread or dry signal. The host owns
source epochs, ordered frames, transport admission, wet fades and output timing.
There are no sibling dependencies or algorithm copies.

The fixed preset is a **20 ms intentional echo**, rounded to the nearest source
frame, feedback **0.25**, damping **0.35**, wet gain **0.5**, no ping-pong or
channel crossfeed. Input and output are interleaved L/R. At 48 kHz the first wet
sample from an impulse at frame 0 is at frame **960**; `delay_frames` reports
that exact onset. There is no extra adapter block buffering or startup ramp.
Network/host playout delay and physical converter latency are separate. For
example an 8 ms return-admission budget does not turn this echo into an 8 ms
effect. The boundary, sample storage, filter/feedback history, coefficients and
delay arithmetic use **f64** throughout. The existing `Ring`, `Tap`, `Allpass`
and `Delay` implementations are shared through compile-time f32/f64
specialization; no separate delay algorithm was added. The standalone rack
continues to use f32 for the same primitives and retains its existing memory
budget. Only the integration adapter's sample storage doubles. Persisted rack
controls retain their f32 schema; the fixed adapter coefficients are prepared
directly as f64. The host still owns any transport encoding: a float32 wet
packet must be explicitly quantized for exact integration replay.

`create(rate, max_block)` prepares rates 8000–192000 Hz and block bounds
1–8192 frames, returning null on invalid bounds. Creation and destruction
allocate/free off the processing thread. Allocation exhaustion follows the
Rust allocator's process-failure behavior. Process/reset contain no allocation,
deallocation, locks, I/O, waiting or host-clock reads. Work is bounded by the
prepared frame limit; reset logically clears fixed ring indices and filters.
The adapter prepares only one delay's storage, not the complete two-engine rack.

Every handle has one owner. The caller serializes processing, reset and destroy;
the ABI cannot validate dangling pointers or concurrent use. Valid arrays contain
`2 * frames` aligned doubles and do not overlap handle storage. Exact in-place
processing is supported; partial overlap is rejected. `process` returns 0 on
success, -1 for invalid pointer shape, -2 for excess block capacity, and -3 for
non-finite input/output or input amplitude above 16. Capacity/pointer errors
clear history and leave output untouched; sample errors clear history and zero
the complete valid output block. The host must mute every failed block before
publication. Finite input is preflighted before advancing any DSP state. A valid
zero-frame call does not touch audio pointers or state; null audio pointers are
allowed only then. Null reset/destroy are no-ops; null delay query returns zero.

On a source-frame gap, epoch change or worker restart, the host must reset before
the next accepted contiguous source block. This discards old tails immediately;
fresh excitation cannot appear before the declared delay. Reset does not fade,
so a host must mute/fade wet output when discontinuities occur. Duplicate/stale
blocks must be rejected by the host before processing: the ABI deliberately
does not own packet identity or source timestamps. A new sample rate requires
a new prepared handle. This adapter is independently built; the host chooses
its local shared-library path outside committed dependency configuration.

The additive `shr_fx_v1_capabilities(output, version, size)` and
`shr_fx_v1_status(handle, output, version, size)` queries require version 1 and
exact C `sizeof` (112 and 40 bytes respectively on this target). Caller-owned
fixed outputs cross no allocator boundary. Invalid version, size, null,
alignment, overflowing span or status output overlapping inline handle storage
or any of its six owned delay allocations
returns -1 without writes or state changes; valid allocation extent, absence of
aliasing remain caller obligations. A narrow raw-span helper in IntegrationDelay
checks allocation overlap without altering numerical processing.
Keep the supplying library loaded for the complete handle lifetime.
Status queries require a live single-owner handle quiesced relative to process,
reset, query and destroy. These are bounded queries, not thread-safe observers.

Capabilities identify `fx-a/fixed-delay-v1`, f64 stereo, exact fixed coefficients,
rate/block bounds, supported reset and zero adapter buffering. Writable controls
and embedded rack availability are explicitly zero. Status reports prepared
rate/block/onset, last valid-handle process result, most recent history-clear
reason and saturating history-clear count. Initial result/reason/count are zero;
explicit reset preserves the last process result. Successful processing,
including zero frames, sets result to zero; reset reason persists until another
clear. Capacity, pointer and numerical errors record their real clears. A query
failure records nothing. This is numerical history, never measured tail state,
device health, xruns or hardware acceptance. The original five v1 signatures
and audio/error behavior remain unchanged. [E07 corpus](../tests/fixtures/cfx/v1/README.md)
is generated by this owner and consumed against the real library.

## Prepared stereo wet-delay v2

The additive `shr_fx_v2_*` symbols in [the header](../include/shr_fx.h) identify
`fx-a/prepared-delay-v2`. One instance has **two independent digital wet-delay
channels**, each with one prepared ring and its own filter, read heads and
controls. It exposes neither the standalone A/B rack nor eight-slot banks.
GigPies owns source-indexed pairing, sends, return placement, source epochs,
late-return admission and final dry-plus-wet mixing. An unpaired final host
source must have a silent partner. The owner retains the algorithms: v2 reuses
`dsp.rs`'s f64 `Ring`, `Tap` interpolation and digital damping/feedback recurrence.
The standalone schema, controller and f32 rack behavior are unchanged.

| Control | Units and accepted bounds |
|---|---|
| `delay_ms` | finite 1–500 milliseconds; fractional source-frame read positions |
| `feedback` | finite 0–0.85; independent channel recurrence |
| `damping` | finite 0–0.99; per-sample low-pass coefficient, not hertz |
| `wet_gain` | finite 0–1; scales the generated return only |
| `bypass` | integer 0/1; bounded excitation fade, tails permitted |
| `reserved` | zero; other values refused |

Rates are 8000–192000 Hz and blocks 1–8192 frames, justified by shared owner
primitives and synthetic boundary/fractional-rate tests. Input is interleaved
L/R f64 with finite magnitude at most 16. No path casts samples or coefficients
to f32. Initial controls are 20 ms, feedback 0.25, damping 0.35, gain 0.5,
bypass off, applied/settled generation zero and source cursor zero. There is no
startup ramp or block buffering. Unlike v1's rounded fixed onset, v2 uses
fractional interpolation: an impulse first appears at floor(delay_ms × rate /
1000), with its remainder at the next frame. At 48 kHz the default is exactly
960 frames. This is intentional echo time; host/device/network latency is separate.

Two rings each reserve `floor(rate/2)+4` doubles: **384,064 bytes at 48 kHz**,
**1,536,064 bytes at 192 kHz**, plus fixed inline metadata (632 bytes on this
64-bit target) and a 112-byte prepared token. There are at most two read heads
per channel, one feedback/filter history, no scratch buffer, second delay bank,
queue or algorithm overlap. Allocation exhaustion follows the existing Rust
allocator process-failure policy. Maximum-rate allocation is tested. A new rate
requires a new prepared handle; it never resizes a live buffer.

### Lifetime and transaction

All handle calls are serialized by one owner. Control/UI threads never borrow
live mutable DSP concurrently. Creation, preparation, retirement and destruction
are control-thread operations that allocate/free library-owned objects. The
library stays loaded until both handles and all tokens have been released.
The host neither frees these objects itself nor supplies an allocator.

`prepare(config, rate, 2, sizeof(config))` validates and copies the **entire**
config without touching live history. The embedded version and size must also
match, both channels must pass all range/reserved checks, and `generation` must
be greater than `expected_generation`. It returns a library-owned opaque token
or null for invalid input. The token does not reference the caller's config.
Discarded/cancelled, refused and accepted tokens all use `retire` off render.

At a callback boundary, `commit(handle, token, source_frame)` borrows the token,
checks its disjoint span/rate, exact next source frame and generation, then copies
scalars. It never consumes or frees the token; it may be retired immediately
after commit returns. `expected_generation` must equal current applied generation
and the target generation must be strictly greater. Any active channel transition
refuses a new commit, including a no-op commit, rather than exhaust a hidden
transition overlap budget. A changed commit also refuses if its complete finite
transition cannot fit before the source-frame counter overflows; a no-change
commit still settles immediately at that boundary. Refusals change neither
channel nor status.

Each changed channel ramps feedback, damping and return gain and crossfades its
old/new read positions over exactly **ceil(rate/50) source frames** (960 at
48 kHz, 161 at 8001 Hz). Bypass fades new excitation to zero over that same
bound and then permits retained feedback/tails to drain; resume fades excitation
to one. No dry path is added. A channel whose controls compare unchanged receives
no ramp or history edit, preserving its sample bits while its partner transitions.
These are defined transition mechanics, not listening acceptance.

Status reports target controls, remaining frames and transition mask.
`applied_generation`/`applied_source_frame` update at accepted commit;
`settled_generation`/`settled_source_frame` update at the exclusive frame boundary
after the final ramp sample. A no-change commit settles immediately. Settlement
means coefficients, read positions and excitation reached targets; it does **not**
mean tail silence. Generation zero is the initial state, with no pending change.

### Timeline, panic and refusal

`process` requires `source_frame == next_source_frame`; addition overflow is
refused before touching output or DSP. Only a successful block advances the
cursor. Zero frames may use null audio pointers and do not advance ramps/cursor.
The host still rejects stale/duplicate packets and owns source epoch identity.
On a gap/new epoch, the host mutes/fades wet and calls `reset(handle, new_frame)`:
both histories clear logically in constant work, accepted controls snap to their
targets, the transition settles at the supplied frame and the cursor is replaced.
Reset preserves the accepted generation and last process result.

`panic(handle, mask)` accepts masks 1 (L), 2 (R), 3 (both). It immediately clears
only selected ring/filter histories, preserving the other channel, target,
ramp and source cursor. Panic does **not** imply permanent mute: with excitation
enabled, new input may return after the delay. Host mute/rearm/authority remains
host-owned. Reset/panic make no measured tail or device-health claim.

| Result | Meaning and effect |
|---|---|
| 0 | Success |
| -1 | Invalid pointer/alignment/span/version/size/config/rate/mask; failed query or commit leaves state/output exact; process pointer failure clears histories, output untouched |
| -2 | Process exceeds prepared block capacity; clears histories, output untouched |
| -3 | Non-finite/over-range input or non-finite wet result; clears both histories and zeros the complete valid output block |
| -4 | Stale configuration generation; refuses commit unchanged |
| -5 | Active bounded transition; refuses commit unchanged |
| -6 | Wrong source boundary or source-frame overflow; state/output exact |

The host must mute **every failed process block** before publication. Finite
input is preflighted before history/output changes, including exact in-place
use. Partial audio overlap and overlap with inline handle storage or either
owned ring are refused before sample access. Output query and token spans must
also be disjoint from all handle storage. Shape checks cannot prove allocation
lifetime, extent or absence of concurrent access; those remain C caller obligations.

`last_process_result` records every valid-handle process call except timeline
refusals, which leave the snapshot exact. Successful zero frames replace it with
zero. `reset_reason` is 0 initial, 1 reset, 2 capacity, 3 pointer, 4 sample, 5 panic;
the saturating `reset_count` counts history-clear operations, even a one-channel
panic. Reason persists until another clear. These serialized queries do not
claim atomic concurrent observation, hardware health, xruns or tail detection.

ABI version/size checks precede output access. C layouts on this 64-bit target
are channel 40, config 104, capabilities 128 and status 168 bytes, with offset
assertions in the [actual C caller](../tests/fixtures/cfx/v2/sample.c).
The [v2 corpus](../tests/fixtures/cfx/v2/README.md) is generated from this owner,
with exact supplying header/library hashes. V1 symbols, errors, fixture bytes
and numerical behavior remain covered separately. For old-library fallback,
resolve the **complete v2 symbol set** first; if any symbol is missing, use the
complete v1 fixed/read-only path and disable writable controls. Never mix handle
versions or infer a writable/rack capability from a partial symbol set.

## Rack and standalone host

`model.rs` owns versioned scalar rack state, independent input selection and
finite output layouts. `storage.rs` owns strict JSON decoding, file size limits,
private physical identities and atomic file replacement. `dsp.rs` has no JACK,
ALSA, UI, filesystem, network or clock dependency. The implementation is original;
no code or private state was copied from sibling repositories. The reference
architecture documents listed in AGENTS.md informed the separation of wet
returns, publication, fault ownership and measurement from acceptance.

`audio.rs` owns the exact JACK client, four registered inputs and four outputs,
connection edits, the SPSC command producer, atomics and shutdown. Its callback
owns the `Processor` exclusively. The complete core is shared with hardware-free
allocation/recovery tests. JACK adapter code checks missing pointers before
forming slices; each registered port owns a distinct buffer. A missing buffer
removes dependent engines, preserving a healthy separate/shared contribution.
An unsupported block size clears all supplied outputs without processing.

Each engine owns eight independent prepared slots. A slot has a maximum
two-second stereo delay, two delay diffusers per side, 200 ms stereo reverb
predelay, four combs (120 ms capacity) and six reverb allpasses per side, and
40 ms stereo chorus storage. Exciter adds fixed filter/oversampling arrays.
Profiles change scalar lengths within those capacities; no callback resizing occurs. Maximum-rate preparation allocates
70,131,200 heap bytes (66.88 MiB) including stereo scratch, below the 80 MiB
budget enforced by normal allocation tests.
All families remain prepared, including inactive slots.

Single uses slot 1. MultiFX uses 2–8 parallel slots, all reading the
same engine input. Each wet branch has its own smoothed level and excitation
ramp, then 1/max(3, configured slot count) gain. The sum passes through engine
return level and existing physical-return headroom. Slot bypass does not renormalize the sum.
No slot consumes another slot's result and no serial topology is stored.
There are at most sixteen active algorithms and no crossfade overlap of algorithms.

Slot structural edits fade/clear only that slot. Mode/count edits fade/clear
the selected engine. Level/bypass edits preserve other branches. A slot fault
latches the owning engine's fault and removes its complete period contribution,
preserving the other engine even on shared returns. Logical clears reset ring
validity/positions, without walking or freeing sample memory. Buffers are freed
off-thread after deactivation joins the callback. Chorus/tape modulation starts
from fixed per-slot phases; it is repeatable and independent of UI timing.

The callback accepts at most eight Copy commands from an eight-entry SPSC ring
at a block boundary. A full queue refuses ordinary edits before UI state changes.
Mute and Panic use a separate atomic bit mask, applied after bounded queue
intake, so an old command cannot defeat a simultaneous panic. Parameter drafts,
JSON, source discovery, graph audits and formatting live on the control thread.
No DSP is borrowed by that thread. Per-period stereo scratch storage allows a
late engine fault to remove its entire contribution while keeping the healthy
engine even on shared physical outputs. Maximum period is 8192 frames.

Meters publish nonnegative peak bits through atomic max; the UI consumes peak
holds. Fault, missing-port, rate, xrun, callback count and command-consumption
status use atomics. The applied sequence reports command consumption; individual
fade completion can follow by up to a few blocks. JACK CPU load is queried and
labelled as whole-server load off-thread; it is not a measurement of this
callback's maximum cost. No callback timings or temperatures are claimed by
the UI.

A port audit every 250 ms checks exact counterpart names and connection counts.
Notifications immediately invalidate the affected owned slot. An atomic graph
generation prevents an in-flight audit from restoring availability from stale
observations. Unrelated clients' graph edits do not invalidate shr-fx. Unexpected
extra links are visible ambiguity, never a reason to disconnect another owner's
links. Physical Apply preflights before requesting callback silence, retains
exact old connection pairs, and rolls back only pairs this operation changed.
A failed rollback keeps silence until explicit audio Retry.

JACK sample-rate changes publish the new rate, which gates processing against
the prepared rate. Retry creates a new off-thread Processor; JACK is neither
reconfigured nor restarted. A server shutdown callback only stores atomic
state. The terminal and sound storage remain usable without an audio server.

`midi.rs` has a dedicated nonblocking Sequencer input worker, one explicit
source, a 512-packet SPSC queue, and a maximum 256 input events per worker pass.
The UI consumes at most 128 packets per 10 ms pass. Clock smoothing and action
mapping happen off the audio callback. Input source identity is rechecked every
500 ms; exact sender addresses filter foreign traffic. Overflow drops the
bounded queue and requires button release before rearming; no transport stop
or audio-tail cut is inferred from missing MIDI. There is no transmit path.

`ui.rs` defines visible hit targets and dispatch for touch and configured MIDI
navigation. `main.rs` owns CLI parsing, signal flags and terminal restoration.
The interface starts without writing user state. Snapshot reads cap at 64 KiB,
reject unknown/missing schema fields and unsupported versions, and validate the
whole rack before publication. Load errors cannot partially update an engine.
Writes use private temporary files, fsync and atomic rename. Directory fsync is
best effort after commit; once rename succeeds the app reports committed state.

Rack schema v3 stores eight complete effect configurations per engine, Single
or MultiFX mode, and a bounded active MultiFX count of 2–8 (default four).
Families keep independent delay/reverb/chorus/exciter settings. Typed v1 decoding
accepts only original Delay/Room fields and migrates them into slot 1. Typed v2
decoding requires exactly three old-style slots and a count of two or three;
all old settings and 1/3 mixing gain survive. New slots are inactive on migration.
The complete result is validated before publication. Reads never rewrite files.
New saves use v3; physical/controller configuration uses separate v2
schema with read-only migration of v1 explicit bindings. Unknown, missing,
duplicate and unsupported fields remain invalid.

`surface.rs` owns logical physical roles. Guided setup learns 16 rotations,
8 pads and clicks 1/9. `Navigate` and `Value` are UI context roles;
`Press(0)` confirms and `Press(8)` cancels. Rotaries 2–8 use fixed slot levels,
10–16 resolve selected-effect parameters/BPM/return, and pads address their
own slot bypass independently of selection. Empty targets resolve to None;
no pad or unused rotary creates a slot. Legacy Knob/Button roles and explicit
parameter/action mappings remain valid for existing private configurations.
Legacy parameter pages affect those old bindings; the new sound roles have
fixed positions and fit together on the dedicated effect screen.

`Mapper` retains physical note/CC held state independently of target identity.
Pickup is per binding: context changes reset only affected sound controls;
publication compares old/new resolved targets and values, resetting only changed
bindings except the binding delivering that successful edit. The UI resolves
Value to the focused rack card's level or the effect screen's selected field.
In menus it uses a normalized draft value and step with independent absolute
pickup; navigation changes the visible target without editing it. Context
identity prevents crossing from a stale prior position. Relative edits are
bounded. Recalling a sound resets all pickup. Capture suppresses live parameter
ownership while retaining releases; guided setup consumes sound rotations even
after capture. Input loss/overflow drops backlog and requires releases; neither
infers an audio stop. Mapping edits retain known held/released states. A
prepared replacement MIDI subscription is committed only after private saving
succeeds, retaining the old input on open/save failure. No MIDI output exists.

`Main` is an eight-card rack browser (two columns, four rows, 20×2-cell cards).
SelectSlot opens `Play`, the dedicated effect screen, without audio soloing.
Its two rows of eight rotary tiles follow the physical top/bottom rows; only
positions 1 and 9 show click brackets. The selected value, wet level, circular
ASCII input/output LEDs, tempo/return and contextual controls retain context.
Rack returns to the selected card. Home page/field/focus survive menu return;
per-engine slot and legacy parameter-page state also remain intact. Normal
screens reserve rows 11–12 and the shared row 13. Only actual meter/fault state
is shown; bypass permits draining without claiming measured tail completion.

Effects/slot/Timing form one detached engine draft. Routing and Tempo have
separate entry baselines. Apply merges changed scalar fields into current state;
Cancel keeps the live rack. Concurrent changes to the same field cause a visible
conflict and retain the draft. Keep live rebases fields changed live while
preserving other draft edits, followed by another explicit Apply. Engine changes
and unrelated menu shortcuts cannot discard effect/tempo drafts. The performance
MIDI context remains visible in the effect editor banner. A temporary visit to
physical Ports suspends/restores the logical routing draft. Failed applies,
loads and saves preserve active state and selection for retry.

Private local schema v2 adds `Target::Surface(Role)` to the bounded binding list.
The v1 decoder validates old explicit mappings and upgrades in memory; only an
explicit Apply rewrites. Duplicate physical controls, duplicate surface roles,
invalid indices and mismatched control kinds are rejected. Guided setup keeps
26 inputs in a detached local draft. Only Press indices 0 and 8 are valid;
no additional button bank is offered by the wizard. Adopting an existing
explicit physical binding requires the visible Use role action; other explicit mappings survive.
The surface adds no worker threads or callback commands.
See the [interaction contract](INTERFACE.md) and
[renderer evidence](screens/README.md).

## Harmonic excitation

`exciter.rs` is original DSP, with no borrowed plugin or sibling code. Two
high-pass poles select the input region. Cascaded 2x polyphase half-band FIRs
upsample to 4x around a smooth rational nonlinear residual, then decimate.
The 17-tap FIRs use a normalized Hamming-windowed sinc, center coefficient 0.5
and eight nonzero odd coefficients. A DC blocker and two tone low-pass poles
shape the wet return. Each channel has independent history; there is no stereo
crossfeed or linear dry branch. Warm/Bright are character choices, not models
of named analog hardware. Drive zero produces exact silence after its ramp.

Coefficient exponentials are evaluated at most once every 32 samples; bounded
interpolation follows the slot's control ramp. All arrays are prepared inline,
with bounded reset and processing work. FIR group delay is 11.25 base-rate
samples, plus frequency-dependent phase from the tone/input/DC filters. This
is separate from JACK/converter latency. The return may contain residual
fundamental and intermodulation energy; external dry-plus-wet listening remains
necessary. Oversampling addresses aliasing but does not guarantee its absence.
The normal suite checks harmonic character, absence of a small-signal linear
branch, and suppression of two selected folded harmonics versus direct shaping.
The general aliasing/oversampling rationale is discussed in [Parker et al.,
DAFx 2016](https://dafx.de/paper-archive/2016/dafxpapers/20-DAFx-16_paper_41-PN.pdf);
this implementation uses FIR oversampling, not that paper's ADAA algorithm.
