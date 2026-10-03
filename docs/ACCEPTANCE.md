# Source-frame adapter verification — 2026-10-03

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
