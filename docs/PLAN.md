# First usable version

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
will supply observed software evidence; physical usability remains unmeasured.

## Delivered first version

Steps 1–7 are implemented and verified; the deliverable is a local `main` commit
containing the application, locked dependencies, this plan, user instructions
and acceptance evidence. The release executable starts offline by default.
The normal suite has 46 passing tests. The new offline cost simulator was
validated once and remains opt-in. Live acceptance and expansion of the sound
palette remain the explicitly separated next stage in `docs/ACCEPTANCE.md`.

## Project naming

The project, Cargo package, executable and local repository directory are
`shr-fx`. The launcher gadget ID remains `fx`; existing JACK/ALSA names and
private data locations continue to use that ID.
