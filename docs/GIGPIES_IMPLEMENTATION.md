# SHR FX: plan and implementation

SHR FX owns wet DSP and prepared embedding. GigPies owns source-clock transport,
dry/wet composition and final integration; Desk owns operator controls.

## Delivered contract and evidence

FX-01 exposes fixed stereo f64 delay and read-only capabilities/status. The actual
library is consumed by GigPies; [acceptance](ACCEPTANCE.md) preserves its exact
scope. The two-engine/eight-slot standalone rack does not establish writable
embedding. [Header](../include/shr_fx.h) and [architecture](ARCHITECTURE.md) are authoritative.

## FX-02 — prepared writable embedding

State: IMPLEMENTED (offline); target acceptance pending. Owner: SHR FX. Outcome: a small versioned f64 wet-effects
interface that GigPies can prepare, control and render without owning FX DSP.
Progress/evidence: offline implementation delivered; native f64 writable Delay,
Room and chorus, prepared ownership and status artifacts. See the 2026-10-09
entry in [Acceptance](ACCEPTANCE.md) and the v2 Architecture/header contract.
Next: separately authorized target-specific aggregate budget and live acceptance;
GigPies/Desk consumer adapters remain owned by those projects.

### Scope and reuse

The standalone application already owns two engines, eight parallel slots per
engine, Delay/Reverb/Chorus/Exciter, controller UI and rack persistence. Preserve
that implementation and its f32 behavior. Embedded v1 exposes only fixed f64
delay; standalone features are not evidence that writable embedding exists.

1. Expose a writable stereo digital delay using the existing shared delay core.
2. Extend shared primitives to f64 where required for one existing room/chamber
   reverb and the existing chorus. Preserve standalone f32 regressions and
   independently verify the new precision path; casting f32 output is insufficient.
3. Admit further existing variants only after their units, wet behavior and
   resource costs meet the same contract. One accepted effect can be integrated
   without waiting for the whole palette or standalone rack exposure.

Start with one effect per independently owned instance. Keep instance limits
explicit in capabilities and host admission. If embedded parallel slots become
necessary, reuse the established bounded parallel model and specify aggregate
headroom/cost first. This plan does not introduce series/mixed chains, replace the
A/B rack, or rebuild its UI. SHR-DAW adoption is a separate consumer task.

GigPies owns sends, return levels, dry/wet summation, source clocks, transport and
final composition. SHR PA remains downstream speaker processing/protection;
it does not host FX under this plan. Desk controls the provider through GigPies.
No FX-owned devices, MIDI ports, transport workers or lighting-parameter interface.

### Contract checklist — before new ABI implementation

Record the reviewed contract in [Architecture](ARCHITECTURE.md) and the versioned
[header](../include/shr_fx.h). Keep this card as the execution/acceptance owner.

- [x] Define stable algorithm IDs and one typed parameter representation: units,
  finite ranges, defaults, sample-rate-dependent bounds and invalid-value policy.
  Separate algorithm identity from settings; reject unsupported/malformed edits
  without silently changing another parameter or the sounding state.
- [x] Define interleaved stereo `double` buffers and native f64 state/coefficients,
  frame capacity, rates, amplitude/headroom bounds, aliasing/in-place rules,
  zero-frame behavior and failure output. Finite values above unity are not
  automatically faults; document the actual supported bound.
- [x] Specify version/size checks, fixed-width C fields, errors and symbol names.
  Preserve every existing v1 signature, layout and sample/error behavior; v2 is
  additive, not a reinterpretation of v1 handles or structs. Contain Rust panics
  at the ABI boundary and state allocation-failure behavior.
- [x] Define create/prepare, validate, cancel, publish, process, reset, retire and
  destroy ownership. Preparation allocates off-thread. Publication transfers a
  complete validated state at a processing boundary. A rejected/stale edit leaves
  active state intact; an accepted edit is distinct from an applied transition.
- [x] Give every active instance one processing owner. Control work must not
  mutate its DSP concurrently. Specify ownership on every success/error path,
  bounded pending/retired capacity, backpressure, cancellation and safe shutdown.
  Free retired state off-thread only after processing relinquishes it; keep the
  library loaded until its objects are destroyed. Do not pass mutable chains or
  unbounded maps into process. No callback allocation/free, locks, I/O or waiting.
- [x] Define continuous smoothing and delay-time changes; specify structural
  replacement and bounded old/new overlap. When overlap exceeds admission limits,
  use deliberate fade-out/swap/in. Include transient memory and CPU in admission.
- [x] Define excitation-off wet bypass with tails, explicit mute/panic/reset,
  independent-instance faults, and tail policy for effect replacement. No dry
  passthrough. Define source-gap/epoch and rate-change handling with the host;
  never report a draining tail as measured silence without evidence.
- [x] Define free-time and tempo-derived settings without adding an FX clock
  owner. GigPies supplies tempo/control updates; source samples advance DSP.
  Report intentional effect delay separately from buffering/transport/device delay.

### Delivery and acceptance

- [x] Deliver writable delay plus a real C caller: prepare, apply, render, query,
  cancel/refuse, reset, retire and destroy. Verify impulse onset, channel isolation,
  feedback/decay, parameter transitions and block-partition behavior against
  independent expected samples; retain exact fixed-v1 regressions.
- [x] Deliver the selected reverb and chorus through the same interface, with
  appropriate independent impulse/decay/modulation checks, deterministic seeds,
  finite bounds and unchanged standalone behavior. Reuse DSP, not UI/device code.
- [x] Cover malformed parameters, unsupported versions, permitted pointer shapes,
  capacity errors, non-finite samples, failed preparation, stale edits, exhausted
  queues, source discontinuities and shutdown with pending/retired states.
  Test actual wet output and preserved healthy instances, not acknowledgements alone.
- [x] Instrument allocation/deallocation freedom for processing, publication,
  transitions and reset. Check preparation memory, maximum work and bounded
  reclamation; preserve standalone storage/controller/routing regressions.
- [ ] Establish a target-specific aggregate budget before claiming live readiness.
  At 48 kHz/48 frames the period is 1 ms; the FX share must leave room for other
  scheduled work and jitter. Record hardware/build, admitted instances/effects,
  worst-case parameters/transitions, memory and p50/p95/p99/max processing cost.
  Do not substitute a per-effect guess or test-count target for this budget.
- [x] Keep normal correctness/recovery tests distinct from opt-in cost matrices.
  Authorized on-device release testing separately measures deadlines, xruns,
  thermal behavior and dry-plus-wet listening. Offline timing does not prove them.

## FX-03 — owner control/status artifacts

State: IMPLEMENTED (offline); reviewed FX-02 contract delivered. Owner: SHR FX.
Outcome: versioned capabilities, parameter descriptors, validated edits and
correlated current/target/fault readback. Progress/evidence: real C caller and
normal recovery/output tests; v1 retained separately (see Acceptance).
Next: consumer integration belongs to shared GP-FX.

- [x] Advertise only supported algorithms/parameters, units/ranges, precision,
  rate/block/instance limits and transition resources. Distinguish unavailable
  features from values legitimately equal to zero.
- [x] Correlate requests with accepted/applied/rejected state and active revision.
  Define stale-base refusal, duplicate/retry behavior and bounded history.
  Report target versus current settings and transition/fault state without
  inventing hardware health. Queries must respect instance ownership.
- [x] Keep machine/device assignments out of effect settings. If serialized
  settings are needed, define one strict versioned representation and round-trip
  fixtures; do not reuse standalone rack version numbers for a different format.
  File loading/saving remains control-thread work, outside the processing ABI.
- [x] Verify capabilities and readback against actual wet output, including failed
  edits, interrupted transitions and recovery; use a real C caller. Unsupported
  changes must preserve the active instance and produce actionable error status.

Shared integration belongs to [GP-FX](https://github.com/PaolaShultz/gigpies/blob/main/docs/MODULE_IMPLEMENTATION_PLAN.md#gp-fx--writable-owner-effects-in-the-console).
Provide reviewed artifacts and evidence there; GigPies and Desk own their adapters.
Neither module implementation nor this plan authorizes sibling edits or hardware use.

## Retained ideas for later selection

Existing tape-delay character, richer modulation and harmonic colour are useful
palette candidates. Start from the existing delay/exciter implementations; select
an addition only for a demonstrated musical need and measured cost. Saturation
needs wet-return semantics, DC/aliasing/headroom checks and external dry-plus-wet
listening. Use character names unless a specific hardware model is substantiated.
A small set of validated example settings can support review and repeatability.

These are candidates, not prerequisites or a second task queue. There is no
scheduled saturation catalogue, arbitrary EQ/clean-boost insert rack, randomizer,
new preset browser or series-chain UI. Select later work in an owning task card.
The standalone application's separate handoff remains [PLAN](PLAN.md).

## Tracking and verification

This is the owning plan for module-only GigPies work. Keep each new task's plan,
implementation state, checklist, evidence and next action together here. Shared
integration tasks live only in the [GigPies integration plan](https://github.com/PaolaShultz/gigpies/blob/main/docs/MODULE_IMPLEMENTATION_PLAN.md);
link to their cards instead of copying status. Follow its task lifecycle and this
repository's AGENTS.md. STATUS/acceptance files hold dated evidence, not another queue.
Preserve closed milestone details in the linked archive; do not reopen old launch cards.
A documentation reconciliation does not rerun tests or qualify hardware.

[Historical plan and milestone evidence](archive/tracking-before-2026-10-09/GIGPIES_IMPLEMENTATION.md). Its launch instructions are retired.
