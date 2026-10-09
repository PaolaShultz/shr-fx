# Native f64 source-frame adapter — 2026-10-03

The current adapter runs the same digital-delay algorithm with f64 samples,
coefficients, interpolation and retained state. Shared `Ring`, `Tap`, `Allpass`
and `Delay` types specialize at compile time for f32 or f64. The standalone
rack keeps its f32 specialization and schema. The five C signatures, first-tap
delay, stereo identity, reset and error behavior are unchanged. This replaces
the initial adapter's f64-to-f32 conversion rather than adding another delay
implementation.

New normal regressions preserve input detail below f32 resolution through the
first tap and feedback, verify direct f64 damping coefficients and repeat after
reset. The maximum-rate allocation test also checks that detail while tracking
all allocations/frees. A separate regression checks the f32 specialization
against exact first-tap and repeated-echo sample bits captured from the original
`88a28ac` release library. Existing rack, stereo, reset, bounds and fault tests
remain in the complete normal suite.

Observed checks with Rust 1.97.1 and `CARGO_INCREMENTAL=0`:

| Check | Result |
|---|---|
| Focused ABI/realtime tests | 11 passed |
| Complete normal suite, `cargo test --locked` | 101 passed; cost matrix remains opt-in |
| `cargo fmt --all -- --check` | Passed |
| `cargo clippy --locked --all-targets -- -D warnings` | Passed |
| `cargo build --release --locked` | Passed |
| Dynamic release ABI, 48-frame blocks | Sub-f32 detail, stereo first tap, feedback and reset passed |
| Opt-in release cost matrix | All ten cases passed finite/fault checks in 95.78 s |
| C header syntax, local Markdown links and staged whitespace | Passed |

The cost matrix ran because this change touches the shared processing primitives
used by the rack. It retains its documented opt-in command below. The one/two
four-effect vocal cases had p99/max processing times of 0.245/0.355 ms and
0.533/0.879 ms. Sixteen halls reached **8.566 ms**, exceeding the nominal
5.333 ms period; they remain unsuitable for a live timing acceptance claim.
These regular-thread simulations do not measure JACK scheduling or hardware
latency. Unchanged renderer/PTY work and audible/hardware tests were skipped in
this module task; GigPies owns the authorized integrated device session.

## Low-latency host continuation — H8 verified with physical qualification

The user rejected the large-prefill latency measured below. Its v7 recordings,
replay and recovery remain evidence for that configuration; they do not establish
live latency acceptance. The continued GigPies work uses the same native f64 FX
source and release-library hash. No FX algorithm or binary changed.

H5 reduced processing to **48 frames / 1 ms**, with zero silent prefill and
**192 frames / 4 ms** wet-return admission. At 144-frame device capacity and FIFO
priority 20, both unrestricted CPU placement and CPU3 passed separate 30 s trials
with exact stored samples and no xruns or missing wet packets. The left physical
reference-to-capture offset was about 5.19–5.35 ms. These were short observations:
the CPU3 fault trial later failed a capture read that took 3.558 ms wall time,
and the two-period retry failed with blocking playback writes.

Increasing capacity to 192 frames, while retaining zero silent prefill, passed
packet/stall and Brain-restart checks at a 249-frame / 5.1875 ms physical offset.
Its subsequent 600 s attempt failed after 36.914 s: a render took 3.597 ms wall
time and 0.191 ms thread CPU. The incomplete take verified exactly. Spare device
capacity and a stable measured offset did not establish long-run reliability.

H6 process memory locking also failed the reliability gate. A separate diagnostic
trace identified **6.219611 ms in `migration_entry_wait_on_locked`** during a load
through SHR PA's `memset` linkage entry. This was a memory-migration wait, not an
FX computation measurement. Both retained failed takes verified exactly, and
process memory locking returned to zero. Trace overhead and the unresolved
migration initiator remain explicit in the owning GigPies evidence.

H7 uses a separately accepted, temporary one-key comparison:
`vm.compact_unevictable_allowed` changes from 1 to 0 while the owned process locks
memory, then returns to its original value after each trial. No persistent tuning
is accepted by this comparison. With GigPies source `1539915`, the first 30 s run
at 48-frame periods, 192-frame capacity, zero silent prefill, 192-frame wet
admission and FIFO 20 on CPU3 retained **1.44 million exact frames**, with zero
xruns or wet loss. All **577 physical windows** measured **249 frames / 5.1875 ms**.

The H7 **600 s / 28.8 million-frame** run completed with zero USB xruns or queue
errors. Direct ADC and all eight stored-stem hashes, dry replay and the journal
matched exactly. All **11977 physical windows** measured **249 frames / 5.1875 ms**,
with no weak windows or timing steps. Render maximum was **299.386 µs**, with no
observed page faults or context switches inside render. Settings were restored.

The **4 ms wet gate failed**: all **600000 returns** arrived, but two expired and
two were missing when due. There were **189 DAC reference differences** against
the intended loss-free replay. Network RTT p99/maximum was **428/4156.374 µs**;
Brain FX processing p99/maximum was **18/77.129 µs**. This does not identify which
network worker was delayed. The completed dry-path/recording and physical-timing
checks do not turn the wet-deadline failure into a full integrated pass.

H8 changed only wet admission to **288 frames / 6 ms**. The same v13 host and
native f64 FX library retained 48 kHz, 48-frame periods, 192-frame device capacity,
zero silent prefill, FIFO 20 on CPU3, process memory locking and the temporary
compaction-key comparison. The **600 s / 28.8 million-frame** run completed with
zero xruns, missing/expired wet packets, queue faults or network errors. All
**600000 returns** arrived; direct ADC/eight PCM hashes, dry and intended-DAC
replay, and the journal matched exactly.

| H8 measured path | p99 | Maximum |
|---|---:|---:|
| PA host render section | 135 µs | 289.665 µs |
| Network RTT | 426 µs | 3732.252 µs |
| Brain FX processing per 48-frame packet | 18 µs | 84.962 µs |

No render page faults, context switches or counter-observation errors were
recorded. Separate 16 s packet-fault and Brain-restart trials each retained
768000 exact frames with raw/dry continuity, no xruns and a gap-free journal.
Both recovered wet output; restart required two acknowledged snapshots and
retained seven expected network errors. The known 20 ms loss faded to zero over
240 frames, with at most 0.621 PCM24 LSB residual error. The deliberately silent
right output stayed silent. The deliberate local xrun and subsequent fresh-run
recovery checks also passed their recorded-waveform verification.

**Physical timing still requires review.** Of 11977 windows, 11975 were trusted
and measured **249–251 frames / 5.1875–5.229167 ms**; two 100 ms windows had weak
correlation. A 10 ms refinement found a one-frame increase near 513.71 s
(correlation 0.580, weak), then another near 520.30 s (0.782, trusted). There were
no persistent steps of at least 24 frames, but the small changes' cause, clock
lock and exact converter sample continuity remain unresolved. The analyzer's
`requires_review: true` is preserved; a perfectly fixed physical offset is not
claimed. This qualification remains separate from the exact digital replay.

Owned scheduling, affinity, memory locking and the original compaction-key value
were restored; resources were released at 22:21 UTC on 2026-10-03, recorded in
exchange ledger `38e31a0`. Detailed private H8 evidence is indexed by GigPies
`artifacts/audio-hardware/2026-10-03/h8-trial-summary.json`,
`h8-fault-assessment.json` and `h8-small-offset-review.json`.

These physical observations use the working left route and include the recorded
host/USB/converter timing. They do not isolate converter delay, measure acoustic
latency or establish the complete standalone FX rack's live acceptance. The fixed
FX echo remains an intentional **960-frame / 20 ms** delay, separate from the host
admission budget. GigPies `docs/AUDIO_HARDWARE.md` owns reservations, exact settings,
restoration reports, recordings and analysis. This documentation checkpoint
checked local Markdown links and whitespace. Module tests, builds and opt-in
cost/audition tests were intentionally skipped because module code was unchanged.

## Native f64 USB integration — 2026-10-03

Historical v7 bench evidence follows; current latency acceptance is described above.

GigPies completed fresh device and sample verification with native f64 FX source
`6510ead9cea8e28a92d992a4147cda17a074c86f`. The release library was retained under
a versioned filename with SHA-256
`6285cb51b156666c3bb33a8f14f428ee676ee619ef2bd704293e981c2e32e2ec`.
The measured host artifact SHA-256 was
`c4b8627790797cda04dc95ff5f3c4454c46ba142f77772f8557123b50694d7c5`.

Pi 4 followed 48-frame / 1 ms stereo packets without an audio device. Pi 5 owned
the AudioBox USB stereo capture/output, PA processing and recording at 48 kHz,
with **384-frame periods and 3072-frame device buffers** (8/64 ms). Wet return
admission was **768 frames / 16 ms**, separate from the fixed delay's intentional
**960 frames / 20 ms**. Float32 send/return encoding remains deliberate; exact
offline replay included that conversion around the native f64 FX core.

The final 600 s run retained **28.8 million frames** with zero xruns, missing or
expired wet packets, or queue errors. All **600000 returns** arrived. Direct ADC
and all eight PCM hashes matched; the journal had no gaps; dry and intended-DAC
replay matched every sample. The eight stems describe two physical ADC channels
plus source, dry and DAC-submitted stereo audit taps.

| Measured path | p99 | Maximum |
|---|---:|---:|
| PA host render section | 1.123 ms | 4.948832 ms |
| Complete post-read host service | 1.290 ms | 5.232607 ms |
| Network RTT | 0.565 ms | 1.977299 ms |
| Pi 4 FX processing per 48-frame packet | 0.029 ms | 0.121814 ms |

Separate 15 s packet/stall and Brain-restart tests each retained 720000 frames
with exact raw/dry continuity and no xruns. Both wet channels recovered, and the
known loss burst faded to zero over 240 frames. A deliberate 100 ms local driver
stall stopped with an explicitly **incomplete** 96000-frame take; its retained
samples verified exactly. These are bounded recovery observations.

The preceding native f64 candidate with a **1536-frame buffer** failed during
Brain restart with a playback underrun at about 5 s: 240000 frames were fully
submitted and 240384 retained in an incomplete take. That failure remains
evidence against accepting the smaller buffer. The final pass uses the larger
3072-frame buffer and does not restore the earlier latency target. The separate
sixteen-hall cost failure above also remains unaccepted.

After the soak, separate generated-only physical checks verified **left output
to input 1**. Two eight-second probes at −72/−54 dBFS completed without faults.
At −54 dBFS, both separated left bursts had correlation 0.799 and the same
**2739-frame / 57.0625 ms** reference-to-capture offset. That offset includes
2688 frames of output prefill, staggered stream starts, USB transfers and
converters. It is not isolated converter delay or capture-to-speaker latency;
subtracting prefill would not isolate those contributions.

The unchanged PA/FX/REC host then completed a generated-only 12 s trial at the
same 384/3072/768 configuration: all 576000 frames, direct ADC/eight PCM hashes,
dry/DAC replay and journal entries matched, without xruns or wet errors. The
997 Hz left return gain was −0.674 dB; the 1499 Hz right return was −69.440 dB,
**68.77 dB below the left**. The right route remains unusable and its analogue
cause unproven. Further work is limited to the verified left channel until the
right connection is resolved. Capture never fed playback in these trials.

This validates one embedded stereo delay and the left physical return under
the recorded bench conditions, not the full rack, standalone JACK performance,
full-show reliability or acoustic quality. The two software channels remain
verified; usable physical stereo return and isolated converter timing do not.

Detailed topology, reservations and retained failures belong to GigPies
`docs/AUDIO_HARDWARE.md`. The final soak's private evidence is under GigPies
`artifacts/audio-hardware/2026-10-03/soak-600s-buffer8-v7/`, including
`verification.json`, host/peer reports and original recordings. The same
integration document indexes the separate private physical probes and analyses.
This update changes documentation only; the measured code and library remain
unchanged.

# Initial f32 source-frame adapter verification — 2026-10-03

The versioned C ABI reuses the existing Digital Delay with a fixed 20 ms first
tap, 0.25 feedback, 0.35 damping and 0.5 wet gain. It preserves L/R identity,
has no dry branch or adapter block delay, and resets tails at the host's source
discontinuity boundary. At 48 kHz the intentional first tap is frame 960.
The f64 ABI converts to existing **f32** DSP and state. This is an explicit
precision limitation against an all-f64 integration target.

Observed offline checks with Rust 1.97.1 on aarch64 Linux:

| Check | Result |
|---|---|
| Focused ABI and realtime tests | 10 passed |
| Complete normal suite, `CARGO_INCREMENTAL=0 cargo test --locked` | 99 passed; 1 historical cost matrix ignored |
| `cargo fmt --all -- --check` | Passed |
| `CARGO_INCREMENTAL=0 cargo clippy --locked --all-targets -- -D warnings` | Passed |
| `CARGO_INCREMENTAL=0 cargo build --release --locked` | Passed |
| Dynamic release-library ABI smoke with 48-frame blocks | Passed: 960-frame onset, L/R identity, feedback and reset |
| C header syntax, local Markdown links and whitespace | Passed |

New normal regressions check exact wet onset at 8, 44.1, 48, 96 and 192 kHz,
plus an 8001 Hz rate requiring frame rounding; independent stereo excitation;
block partition and in-place equivalence; reset determinism; independent handle
tails; NaN/infinity/over-range whole-block silence; bounded null, overlap and
capacity errors; and no process/reset allocation or deallocation at the maximum
prepared rate/block. The complete normal suite also retains rack routing,
callback safety, recovery, schema/storage, controller and UI coverage.

The historical ten-case cost matrix was intentionally skipped: its full-rack
algorithms, capacity and scheduling assumptions did not change. It remains
available with the opt-in command below. Unchanged renderer generation, terminal
PTY smokes, auditions and standalone JACK/MIDI hardware tests were also skipped.
No devices, routes, services or controller settings were changed by this module
work. GigPies owns integrated host/device/network measurements separately; these
offline results do not establish physical latency, audio clock accuracy,
callback deadline margins or listening acceptance.

The release library exports exactly the five documented `shr_fx_v1_*` symbols.
It retains the crate's native ALSA runtime dependency without opening a device.
No publication guard or versioned hook configuration exists in this repository;
the complete staged source/documentation change was reviewed before local commit.

## Embedded stereo USB integration — 2026-10-03

GigPies exercised the release adapter from source commit
`88a28ac4b70670da3499edceb23bba1e0ed396c9` on Pi 4, following source frames in
48-frame / 1 ms stereo packets. Pi 5 owned PA processing, recording and an
AudioBox USB interface opened at 48 kHz with 192-frame periods and 768-frame
buffers. FX remained the fixed wet-only 20 ms / 960-frame stereo delay described
above, with f32 internal processing and no local audio device on Pi 4.

The coordinator reported these observed results:

| Integrated trial | Result |
|---|---|
| Short 3 s, 10 s and 30 s checks; generated-only startup smoke followed by capture plus probe | Exact recorded PCM and PA/FX-to-DAC replay; zero xruns |
| First 600 s run, original 8 ms return admission | 28.8 million dry/recorded frames exact, zero xruns; all 600000 returns arrived, but one expired and one wet packet was missing: zero-loss target **failed** |
| Revised 16 ms admission, 30 s comparison | 1.44 million frames exact; no wet loss |
| Separate 15 s packet/stall and Brain process termination/restart trials | 720000 dry/recorded frames exact in each trial; both wet channels faded to zero over 240 frames and recovered with fresh control state |
| Later 16 ms admission soak with 192-frame periods | Playback EPIPE after about 10.28 s; zero wet loss, recording retained as incomplete |
| Revised 16 ms admission with 384-frame periods / 1536-frame buffers, 600 s | 28.8 million frames; direct ADC/eight PCM hashes, journal, dry and intended-DAC replay exact; zero xruns, wet loss or queue errors |

The 16 ms admission budget is an explicit revision after the failed 8 ms soak;
the delay's intentional 20 ms remains additional. The 384-frame configuration
followed the playback xrun at 192 frames; its 600 s pass is a bounded observation,
not a full-show reliability claim. All these trials used the archived f32
adapter, whose source and library hash remain recorded in the integration
evidence. Fresh native f64 hardware results are not inferred from them.

These results apply to one embedded stereo delay. They do not establish the
standalone JACK rack, two full engines, eight parallel slots per engine, or
acoustic/listening acceptance. The recorded DAC pair is submitted digital PCM,
not a measured physical return; converter-to-output latency remains unresolved
without a return route. GigPies owns the detailed topology, host revisions,
reservations and measurements in its `docs/AUDIO_HARDWARE.md`, with recordings
and exact artifact hashes retained in its private task evidence. The original
measured library remains archived separately from the new native f64 build.

# Performance interface verification — 2026-09-08

Fructal Implement is complete for the local interface/controller scope. This
work includes the eight-slot/exciter baseline and the completed controller/TUI
redesign. [The interaction guide](INTERFACE.md) contains the two
40×13 concepts, chosen rack-to-effect design, the corrected controller map and
normal/recovery behavior. [The renderer gallery](screens/README.md) contains
16 actual renderer states in text and 480×320 PNGs using the installed PSF font.

Observed checks on the final implementation:

| Check | Result |
|---|---|
| Focused UI, controller and storage regressions | Passed |
| Complete normal suite, `cargo test --locked` | **94 passed**, 1 opt-in cost test ignored |
| `cargo fmt --all -- --check` | Passed |
| `cargo clippy --locked --all-targets -- -D warnings` | Passed |
| `cargo build --release --locked` | Passed |
| Release 40×13 PTY smokes | **6 passed**: touch, MultiFX, exciter, guided controller, keyboard, SIGTERM |
| Renderer replay and installed-font glyph checks | **16 previews generated and inspected** |

The corrected hardware has eight pads/buttons total and two rows of eight
rotaries; only 1 and 9 click. Five new normal workflow tests exercise that exact
26-input profile: build the vocal rack, route one stereo return, open and edit
an effect, use all eight slots, return from menus, save/recall, PANIC and Exit
using only those physical roles. They cover absolute rotary-9 pickup in live
and draft contexts, rearming after touch/selection, held clicks across screens,
input loss, rotary placement, and non-overlapping native-size touch targets.
Legacy mapping tests use an explicitly labelled old profile for compatibility;
they do not claim another bank of physical buttons exists.

The new production workflows cover keyboardless construction of the four-effect
vocal rack and one stereo return; all eight pads without selection/page/count/
gain changes; sample-for-sample preservation of unrelated tails for every pad;
all eight slots and empty targets; per-engine/page/focus return; absolute pickup
through context changes, touch and recall while unaffected wet/explicit controls
stay acquired; held/repeated notes through context change and input loss;
26-input guided capture, collision rejection, explicit-binding adoption and
private migration; suppressed capture ownership; same-field conflicts and Keep
live; live tempo preservation; routing/Ports return; failed recall/save with
repair/retry; and controller reach to save/load/routing/PANIC/Exit. Existing
normal tests also cover CC button releases, relative encoding, MIDI overflow,
clock hold, strict schema, routing combinations, DSP finite/wet output, callback
allocation freedom, queue bounds and fault recovery.

The PTY tests run the release executable with neither `--audio` nor `--midi`,
verify no private state is written during browsing, and check restored raw mode,
cursor, mouse capture and alternate screen. Previews use TestBackend cells and
read the existing console font/palette; they do not change the terminal. Only
the explicitly labelled meter example uses synthetic nonzero meter readings.
No audio file is rendered by the gallery.

Intentionally skipped for the interface/doc changes: the expensive DSP cost
matrix. No long soak or audition rendering was added to the normal suite. DSP
algorithms, prepared memory, mixing gain, callback work and matrix assumptions were unchanged by this interface work. The normal
DSP/safety suite still ran in full. The existing opt-in command below remains
available when that protected behavior changes or a measurement is requested.

Hardware-specific unknowns remain: the controller's exact identities and
absolute/relative encoding, note/CC release behavior, physical touch precision,
MIDI timing/reconnect behavior on that device, and live musician acceptance.
Actual source replacement is implemented transactionally but was not exercised against ALSA hardware. Local
verification does not establish JACK deadlines, xruns, thermal headroom, path
latency or external dry-plus-wet listening quality. No live audio, hardware MIDI,
MIDI transmission, recording, installation, service or OS/font changes occurred.

## Documentation closeout — 2026-09-08

The documentation pass checked behavior against `surface.rs`, `ui.rs`,
`model.rs`, `storage.rs`, the CLI and offline tests. It corrected the overview
contract, private schema version, controller assumptions and sixteen-slot live
checklist. PLAN now describes completed scope and the next session; superseded
plans and older test/cost evidence are archived in HISTORY. AGENTS assigns each
current document an owner and requires related updates with behavior changes.

Closeout checks: eight Markdown documents, all local links and code fences;
rack v3/local v2/eight-slot/26-input claims against source; generated gallery
against fresh renderer output; and staged whitespace/file review. The normal
suite, formatting, Clippy, release build and six offline PTY smokes were rerun
for the final commit, with the results shown above. No DSP cost matrix was rerun
for this documentation pass. No additional implementation work is outstanding
for today's local scope; the hardware evidence below remains unmeasured.

## Current production coverage

The normal suite protects:

- every source-pair/return-layout combination with distinguishable signals;
  two-input/four-output operation, non-adjacent mono, deliberate shared gain,
  stereo ordering, missing ports and rejection of duplicates;
- wet-only impulse timing at 8/44.1/48/96/192 kHz, deterministic room output,
  bypass tails, mute/recovery, time crossfades, A/B isolation, ping-pong,
  finite feedback and transitions;
- allocation/deallocation freedom through the complete hardware-free callback
  core, bounded queue overload, priority panic, rate-change silence and stale
  availability publication;
- strict bounded snapshots, failed-load invariance, cancel/retry, private
  atomic saves, button releases/repeats, absolute pickup, relative encoding,
  learn capture, clock loss/stop/hold and internal tap;
- 40×13 screen bounds, shared status/control rows, all eight physical slot
  fields, touch/MIDI action parity, cancellation and keyboardless Exit;
- hardware-free CLI help/version/errors and the release PTY smoke script's
  touch, keyboard and SIGTERM exit with restored terminal modes.

## DSP milestone evidence

The following measurement predates the TUI/controller redesign. Its DSP bounds
remain current; its UI and test counts describe that earlier milestone. Earlier
v0.1/v0.2 results are preserved in [History](HISTORY.md).

### v0.3 eight-slot rack and harmonic exciter

Recorded on 2026-09-08, Rust 1.97.1, aarch64 Linux. The owner requested removal
of the three-slot restriction so one engine can return reverb + delay + chorus
+ exciter through a single stereo pair. The implemented bound is eight parallel
slots per engine, four active by default when selecting MultiFX. No serial
routing or additional physical-output requirement was introduced.

At this DSP milestone, checks passed: **74 normal production tests**, formatting,
Clippy (`--all-targets -D warnings`), release build, and five offline 40×13 PTY
smokes (touch, MultiFX, paged Exciter editing, keyboard and SIGTERM). All terminal
modes were restored and these checks wrote no private user state. The expanded
ten-case release cost matrix was run separately because the DSP and capacity
assumptions changed; it remains ignored in the default suite.

Observed regressions cover every count 2–8, four independent vocal branches,
one input/one physical stereo pair, inactive-output silence, A/B independence
through count changes, all sixteen callback slots without allocation/free,
strict v1/v2 migration without file rewrites, unchanged old mix gain, strict
v3 last-slot/inactive parameters, slot-eight MIDI pickup, paged touch/controller
navigation, draft cancellation and count reduction. Exciter checks cover
8/48/192 kHz, deterministic output, independent stereo channels, distinct even/
odd harmonic emphasis, a nonlinear small-signal response (no linear dry branch),
zero-drive silence, bypass drain, Panic and continuous-control sweeps. A focused
spectral regression confirms that two selected third/fifth-harmonic aliases
are reduced by more than 16 dB relative to direct shaping; this is not a full
spectral or subjective-quality acceptance.

Maximum-rate preparation allocated **70,131,200 heap bytes (66.88 MiB)**, below
the **80 MiB** budget. Exciter filter histories use additional small inline
arrays, not heap allocation. At most sixteen algorithms run, with no second
rack or algorithm overlap during structural transitions.

The offline cost matrix passed finite-output/fault checks for ten cases, each
with one minute of simulated audio (11,250 blocks at 48 kHz / 256 frames).
Runtime for the complete matrix was 101.24 seconds. Timings below are regular-
thread `Processor::process` wall times; the comparison period is 5333.333 µs.
They do not establish JACK scheduling or on-device performance acceptance.

| Case | p50 µs | p95 µs | p99 µs | Maximum µs |
|---|---:|---:|---:|---:|
| One four-effect vocal / stereo return | 252.665 | 289.406 | 297.165 | 355.256 |
| Two four-effect vocal engines | 492.737 | 524.830 | 537.811 | 604.939 |
| Sixteen halls | 1799.801 | 1856.338 | 1889.615 | 9766.572 |
| Sixteen plates | 1804.301 | 1855.763 | 1889.967 | 2183.187 |
| Sixteen tape echoes | 629.680 | 650.921 | 677.032 | 762.105 |
| Sixteen multi-tap delays | 862.123 | 922.493 | 946.771 | 1651.654 |
| Sixteen diffused delays | 672.328 | 703.976 | 725.124 | 929.030 |
| Sixteen exciters | 830.845 | 858.623 | 868.901 | 1239.249 |
| Sixteen ensemble choruses | 763.754 | 793.698 | 805.753 | 881.864 |
| Sixteen slots under rapid edits | 901.104 | 998.603 | 1029.585 | 1541.266 |

The vocal cases use Tape delay, Hall, Ensemble and Bright Exciter, with maximum
configured feedback, chorus depth/rate and exciter drive/tone. The sixteen-hall
case had a **9766.572 µs outlier, exceeding the nominal block period**; its cause
was not isolated. Passing the cost simulator means finite output and no latched
DSP faults, not a timing pass. Eight heavy reverbs per engine are therefore
not live-approved. The intended four-effect vocal combination has substantially
lower observed cost, but real JACK deadlines, xruns, temperature, converter
latency and external dry-plus-wet listening remain unmeasured for this version.

Only local software and offline synthesis into memory were exercised. Live
JACK/MIDI, audible auditions, physical latency, thermal testing, recordings,
services, installation and publication were intentionally skipped.

## Opt-in cost check

The current opt-in cost matrix simulates the ten cases listed in the v0.3
section above, including the four-effect single stereo return and sixteen-slot
stress cases. It reports regular-thread processing time percentiles. It does not attach
to JACK, open MIDI or write audio files. It is deliberately excluded from
`cargo test` after its initial tool validation. Rerun when its assumptions or
protected performance paths change, or for a specifically requested measurement:

```sh
cargo test --release --locked --test soak -- --ignored --nocapture
```

## Next session: separate live authorization required

Use the release executable and the existing JACK configuration. Record actual
sample rate, periods, physical connection names privately, A/B algorithms and
settings. Do not infer physical channel identities from enumeration order.

1. Configure exact sends and returns; inspect missing/extra-port recovery with
   the graph owner. Test separate/shared mono, one stereo source and all return
   layouts supported by the session's ports. Four-output software coverage
   already exists; physical four-output acceptance needs suitable hardware.
2. Listen to mixer dry plus wet at matched return levels. Check delay rhythm,
   mono/stereo phase behavior, ping-pong, each reverb profile's density/decay,
   tape and multi-tap timing, chorus/exciter against external dry, slot balances
   and quiet tails. There is no listening acceptance for any catalog entry yet.
3. Exercise both engines together under rapid time/tempo/parameter controls,
   repeated slot type/mode/count/routing changes, demanding feedback and long
   slot/engine bypass tails. Test the intended four-effect vocal setup first,
   then up to sixteen active slots across A/B, including demanding hall and
   ensemble cases only within the measured session budget.
   Measure callback p50/p95/p99/max against period deadlines, xruns, RSS, CPU
   temperature and sustained thermal headroom. Whole-server CPU load and a
   regular-thread simulation cannot establish callback deadline margins.
4. Measure the physical return path and document converter/JACK contributions
   separately from intentional effect delays. The 5–6 ms allowance remains an
   estimate; do not insert or subtract it as a measured constant.
5. Check touchscreen precision on the unchanged 480×320 tty/font. Verify a
   real controller's eight pads/buttons, two rows of eight rotations and clicks
   on 1 and 9. Learn all 26 inputs with their real message identities/encoding.
   Verify browse/open/value/back, all eight slots, pickup, held/released controls,
   clock jitter/loss/stop, disappearance/reconnect and keyboardless Exit. Check
   menu return, draft Apply/Cancel/conflicts, save/recall, routing, TAP and PANIC,
   with no audible change in unrelated slots. No controller data is forwarded.
6. Exercise JACK shutdown/restart, device loss, sample-rate change, queue stress
   and process signals. Check exact owned connection cleanup and that another
   engine/client is preserved where its required resources remain available.

Only retain expanded spaces, modulation or fixed combinations after both sound
and measured dual-engine cost are acceptable. Repository creation, commit and
push were authorized for the documentation closeout. This does not authorize
live I/O, recording, service installation, autostart or runtime deployment.

## Workflow review

Fructal Implement checks: explicit Apply changes one selected draft; Cancel
retains the active rack; unavailable/duplicate routes retain selection for
repair; shared stereo summing is named; physical and logical configuration are
separate; failed loads retain A/B state; controller and touch use the same
commands; Panic and Exit bypass ordinary queue congestion. Normal and recovery
software tests provide observed evidence. Physical reach, controller timing and
musician listening remain unmeasured rather than presumed validated.

## Task0009 FX-01 read-only ABI — 2026-10-04

Additive capability/status queries retain the five existing v1 signatures and
fixed f64 numerical/error behavior. E07 is the [owner-generated corpus](../tests/fixtures/cfx/v1/README.md),
with real C header/library consumption. Capability identity is
`fx-a/fixed-delay-v1`; writable parameters, rack access and hardware health are
unavailable. Status reports actual process result and history clears, serialized
with the live handle; it is not an atomic concurrent observer.

Focused C API suite: **7 passed**. Full normal production suite:
**105 passed, 1 intentionally ignored** (historical cost matrix). New coverage
includes exact fixed descriptor values/layout, real error/reset history,
invalid size/version/alignment/span, inline and all six owned-buffer overlap
rejection without writes, invalid-version refusal before handle access, and
saturating clear count. Existing wet-only onset, native precision, independent
stereo, in-place, reset and allocation-free recovery regressions pass.
Warning-denied all-target Clippy passed. Commands (coordinated workers also
follow the [shared build-lock policy](https://github.com/PaolaShultz/gigpies/blob/main/docs/PARALLEL_WORK_PLAN.md)):

```sh
CARGO_INCREMENTAL=0 cargo +1.97.1 test --locked -j1 --test c_api
CARGO_INCREMENTAL=0 cargo +1.97.1 test --locked -j1 --all-targets
CARGO_INCREMENTAL=0 cargo +1.97.1 clippy --locked -j1 --all-targets -- -D warnings
```

Historical sixteen-hall/exhaustive cost matrices, auditions, long soaks, physical
JACK/ALSA/MIDI, listening and combined-load tests were intentionally skipped:
no numerical DSP, UI or hardware behavior changed. There are no optional native
or hardware-host Cargo features in this crate. No physical endpoint was opened;
old acceptance evidence is preserved. Exact source/header/library hashes and
build logs belong to the private root-review handoff; root owns publication.

FX-01 final gates also passed: `cargo +1.97.1 fmt --check`,
`CARGO_INCREMENTAL=0 cargo +1.97.1 build --release --locked -j1 --lib`, and
C11 `-Wall -Wextra -Werror` compile/link of `sample.c` against the exact release
library. The actual C run checked all six rates in the corpus, including
960-frame onset at 48 kHz and fault/reset state; its six output rows matched
`corpus.json` exactly. No extra adapter buffering was introduced. The disposable
C executable was removed after retaining its concise output and reproducible
source/command. Local Markdown links and `git diff --check` passed.

## 2026-10-09 — FX-02/FX-03 offline prepared embedding

Additive v2 exposes writable native f64 Digital Delay, existing Room and
single-voice chorus through one typed settings contract. Fixed v1 source,
symbols/layouts and exact sample corpus are retained. Standalone f32 algorithms
use the same generic primitives; storage/controller/routing/UI remain intact.
Architecture and `include/shr_fx.h` own the reviewed lifetime/resource contract.

Verified **113 passed, 2 intentionally ignored** in the full normal suite.
The new offline aggregate embedding cost matrix and historical standalone soak
remain opt-in. Real C v2 caller verifies layouts, parameter descriptors, wet
onsets/feedback, independent Room first-comb/allpass amplitude, chorus delay,
accepted/applied settings, stale/duplicate refusal, cancel/retire/reset, faults
and destruction with pending state. The retained C v1 caller checks all six
rates and exact sub-f32 sample detail. Rust additionally checks independently
computed modulated fractional taps, block partitioning, instance isolation,
input amplitude 16, interrupted transitions, backpressure and bypass tails.
Allocation instrumentation covers maximum-rate preparation bounds and zero
allocation/deallocation during publication, rendering, structural transitions,
reset, sample faults and ownership retirement. All-target warning-denied Clippy,
formatting, local documentation links and whitespace checks pass.

Reproduce without opening endpoints:

```sh
cargo test --locked -j 2
cargo clippy --locked -j 2 --all-targets -- -D warnings
cargo fmt --check
scripts/check_c_api_v2.sh
# Optional regular-thread aggregate cost evidence, not target admission:
cargo test --release --locked -j 2 --test embedding_cost -- --ignored --nocapture
```

This machine has `libjack.so.0` but lacks `jack.pc`. These runs used disposable
pkg-config metadata in `/tmp/shr-fx-build-pc` (Name jack, Version 1.9.22,
Libs `-l:libjack.so.0`) via `PKG_CONFIG_PATH`; no package, service or audio
configuration changed. C checks use the freshly built debug shared library.
No live audio, hardware, deployment, audition, thermal/xrun measurements or
combined module testing ran. A target-specific aggregate 48 kHz/48-frame budget
is still required before live readiness; capabilities report that evidence
unavailable. GigPies/Desk own their consumer adapters. No sibling was modified.
