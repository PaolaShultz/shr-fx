# Current handoff — 2026-10-03

The source-frame integration adapter is implemented around the existing digital
delay. `libshr_fx.so` exposes the versioned C ABI in `include/shr_fx.h`; the
fixed stereo wet preset has exactly 20 ms of intentional echo delay (rounded to
source frames), with no extra adapter buffering. Its shared delay core now uses
native f64 samples, state and coefficients; the standalone rack retains f32.
Host integration, source epochs, transport and hardware
measurements belong to GigPies. Module checks and remaining limits are recorded
in [Acceptance](ACCEPTANCE.md).

GigPies validated the native f64 stereo adapter on Pi 4 against real USB capture,
PA processing and recording on Pi 5. The final 600 s run used 384-frame periods,
3072-frame device buffers and 768-frame wet admission at 48 kHz. All retained
samples, hashes, journal and dry/intended-DAC replay matched, with no xruns or
wet loss. Separate packet and Brain-restart tests preserved raw/dry continuity
and recovered both wet channels. Earlier 8 ms admission and smaller device
buffers had retained failures; the larger final buffer is an explicit change
to the bench latency budget. [Acceptance](ACCEPTANCE.md#native-f64-usb-integration--2026-10-03)
records the measured artifact identities, timing, recovery and remaining limits.

The standalone release executable still starts offline. The completed v0.3
UI/controller design has no unfinished implementation item. Actual controller
setup, touch usability, listening and standalone JACK/thermal/latency acceptance
remain separate hardware work. The adapter's offline validation cannot establish
those results. Subsequent physical probes verified left output to input 1; a
12 s generated-only integrated trial retained exact recorded/replayed samples.
The right return was 68.77 dB weaker and remains unresolved, so further physical
work uses the verified left channel. The observed left loopback offset includes
host prefill/startup, USB and converters; isolated converter latency and acoustic
quality remain unverified.

## Delivered behavior

- Device-independent versioned stereo adapter, explicit delay/precision/channel
  contracts, bounded errors, allocation-free process/reset and source-gap reset.
- Two independent engines, each with up to eight prepared parallel wet slots:
  Delay, Reverb, Chorus and Harmonic Exciter can share one vocal send and one
  stereo return. All supported input/return combinations remain available.
- Rack shows eight cards. Opening a card gives that effect its own screen;
  Back returns to the same slot while the other effects keep sounding.
- The corrected controller has eight pads/buttons total, sixteen rotations in
  two rows of eight, and clicks on 1 and 9. Rotary 1 browses/opens; rotary 9
  edits/goes Back. The other rotations control sound; pads toggle their own
  slots without selecting or resizing. Guided setup learns 26 physical inputs.
- Immediate smoothed performance edits, directional absolute pickup, supported
  relative input, held/released control handling and recovery are implemented.
  Touch, keyboard and MIDI reach the same actions without mandatory chords.
- Structural drafts have Apply/Cancel, retained context and explicit same-field
  live conflict recovery. Failed operations retain active state and editable
  work. Routing/Ports round trips retain the logical routing draft.
- Rack schema v3 and private local schema v2 have strict validation and read-only
  migration. Explicit legacy mappings remain compatible; physical identities
  stay outside sounds. The standalone schema/controller implementation did not
  change during the separate embedded USB integration.

## Where to resume

| Need | Owning document |
|---|---|
| Build and run offline | [README](../README.md) |
| Operate, route and stop | [Operating guide](OPERATING_GUIDE.md) |
| Complete map, setup, value ranges, menu/draft/recovery behavior | [Interaction contract](INTERFACE.md) |
| DSP/control ownership, bounds and persistence schemas | [Architecture](ARCHITECTURE.md) |
| Checks passed, cost evidence and remaining hardware acceptance | [Acceptance](ACCEPTANCE.md) |
| Native renderer output and regeneration command | [Screen gallery](screens/README.md) |
| Earlier milestones and superseded limits/workflows | [History](HISTORY.md) |

The final normal suite, formatting, Clippy, release build and offline PTY smokes
passed; dated counts and commands are recorded only in Acceptance. The gallery
comes from the renderer and installed font, not a hand-maintained mockup.
Historical DSP cost evidence remains available; UI/doc work does not require
rerunning that opt-in matrix.

For the next session, follow the hardware acceptance sequence in Acceptance:
learn the actual controller's messages/encoding, verify touch and controller
operation, then measure/listen with the existing audio setup if authorized.
Do not restart the completed UI redesign from the archived plan.
