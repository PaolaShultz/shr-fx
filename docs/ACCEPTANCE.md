# Acceptance evidence and remaining work

The user authorized a local repository, plan, usable first version and commit.
AGENTS.md explicitly separates that work from live audio, MIDI transmission,
recording, service/autostart changes and borrowed-hardware tests. No server or
service was started, no hardware routes were exercised and no recordings made.

## Software checks

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

The opt-in soak simulates one minute of demanding dual-engine audio and rapid
edits, reporting regular-thread processing time percentiles. It does not attach
to JACK, open MIDI or write audio files. It is deliberately excluded from
`cargo test` after its initial tool validation. Rerun when its assumptions or
protected performance paths change, or for a specifically requested measurement:

```sh
cargo test --release --locked --test soak -- --ignored --nocapture
```

## Authorized live session, still required

Use the release executable and the existing JACK configuration. Record actual
sample rate, periods, physical connection names privately, A/B algorithms and
settings. Do not infer physical channel identities from enumeration order.

1. Configure exact sends and returns; inspect missing/extra-port recovery with
   the graph owner. Test separate/shared mono, one stereo source and all return
   layouts supported by the session's ports. Four-output software coverage
   already exists; physical four-output acceptance needs suitable hardware.
2. Listen to mixer dry plus wet at matched return levels. Check delay rhythm,
   mono/stereo phase behavior, ping-pong, room density/decay and quiet tails.
   There is no listening acceptance for either algorithm yet.
3. Exercise both engines together under rapid time/tempo/parameter controls,
   repeated algorithm/routing changes, demanding feedback and long bypass tails.
   Measure callback p50/p95/p99/max against period deadlines, xruns, RSS, CPU
   temperature and sustained thermal headroom. Whole-server CPU load and a
   regular-thread simulation cannot establish callback deadline margins.
4. Measure the physical return path and document converter/JACK contributions
   separately from intentional effect delays. The 5–6 ms allowance remains an
   estimate; do not insert or subtract it as a measured constant.
5. Check touchscreen precision on the unchanged 480×320 tty/font. Verify a
   real controller's note/CC releases, encoder encoding, pickup and learn, clock
   jitter/loss/stop, controller disappearance and keyboardless Exit. This app
   never sends controller data downstream.
6. Exercise JACK shutdown/restart, device loss, sample-rate change, queue stress
   and process signals. Check exact owned connection cleanup and that another
   engine/client is preserved where its required resources remain available.

Only retain expanded spaces, modulation or fixed combinations after both sound
and measured dual-engine cost are acceptable. No service installation, autostart,
remote repository, package publication or deployment is part of this version.

## Workflow review

Fructal Implement checks: explicit Apply changes one selected draft; Cancel
retains the active rack; unavailable/duplicate routes retain selection for
repair; shared stereo summing is named; physical and logical configuration are
separate; failed loads retain A/B state; controller and touch use the same
commands; Panic and Exit bypass ordinary queue congestion. Normal and recovery
software tests provide observed evidence. Physical reach, controller timing and
musician listening remain unmeasured rather than presumed validated.
