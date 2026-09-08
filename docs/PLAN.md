# Current handoff — 2026-09-08

The local v0.3 implementation and documentation are complete for this session.
The release executable starts offline; there is no unfinished UI/controller
implementation item carried forward. Actual controller setup, touch usability,
listening and JACK/thermal/latency acceptance need a separately authorized
hardware session. They are validation still to perform, not measured results.

## Delivered behavior

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
