# Current handoff — 2026-10-03

The source-frame integration adapter is implemented around the existing digital
delay. `libshr_fx.so` exposes the versioned C ABI in `include/shr_fx.h`; the
fixed stereo wet preset has exactly 20 ms of intentional echo delay (rounded to
source frames), with no extra adapter buffering. Its f64 boundary uses existing
f32 DSP internally. Host integration, source epochs, transport and hardware
measurements belong to GigPies. Module checks and remaining limits are recorded
in [Acceptance](ACCEPTANCE.md).

The standalone release executable still starts offline. The completed v0.3
UI/controller design has no unfinished implementation item. Actual controller
setup, touch usability, listening and standalone JACK/thermal/latency acceptance
remain separate hardware work. The adapter's offline validation cannot establish
those results or the integrating host's physical output timing.

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
  stay outside sounds. No live audio, hardware MIDI or OS configuration changed.

## Where to resume

| Need | Owning document |
|---|---|
| Build, run offline, operate and stop | [README](../README.md) |
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
