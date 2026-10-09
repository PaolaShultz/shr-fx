# SHR FX: plan and implementation

SHR FX owns wet DSP and prepared embedding. GigPies owns source-clock transport,
dry/wet composition and final integration; Desk owns operator controls.

## Delivered contract and evidence

FX-01 exposes fixed stereo f64 delay and read-only capabilities/status. The actual
library is consumed by GigPies; [acceptance](ACCEPTANCE.md) preserves its exact
scope. The two-engine/eight-slot standalone rack does not establish writable
embedding. [Header](../include/shr_fx.h) and [architecture](ARCHITECTURE.md) are authoritative.

## FX-02 — prepared writable embedding

State: PLANNED. Owner: SHR FX. Outcome: a minimal reviewed f64 prepared-state ABI
for useful wet effects, preserving existing v1 bytes/behavior and source-frame timing.
Next: design the exact supported algorithms/parameters, units/ranges, preparation,
publication/retirement, smooth edits, tail/bypass/fault semantics and resource bounds.
Reuse owner DSP where suitable; no audio-device owner or standalone UI requirement.
Acceptance: real C caller, independent wet-sample references, no callback allocation,
finite/fault behavior, bounded transitions and unchanged fixed-v1 regressions.
Progress/evidence: no writable embedding implemented; fixed-v1 evidence is separate.

## FX-03 — owner control/status artifacts

State: PLANNED; depends on reviewed FX-02. Owner: SHR FX. Outcome: exact versioned
capabilities, prepared edits and current/target/fault readback for the new ABI.
Next: define artifacts with FX-02, then update this card during implementation.
Acceptance: malformed/unsupported edits preserve the sounding state; actual output
and readback agree; lifetime/retirement/retry bounds are explicit. GigPies/Desk
integration is shared GP-FX, not another implementation of these provider tasks.

The standalone rack's separate plan is [PLAN](PLAN.md); do not copy it into this
GigPies task queue or count it as completed embedding.

## Tracking and verification

This is the owning plan for module-only GigPies work. Keep each new task's plan,
implementation state, checklist, evidence and next action together here. Shared
integration tasks live only in the [GigPies integration plan](https://github.com/PaolaShultz/gigpies/blob/main/docs/MODULE_IMPLEMENTATION_PLAN.md);
link to their cards instead of copying status. Follow its task lifecycle and this
repository's AGENTS.md. STATUS/acceptance files hold dated evidence, not another queue.
Preserve closed milestone details in the linked archive; do not reopen old launch cards.
A documentation reconciliation does not rerun tests or qualify hardware.

[Historical plan and milestone evidence](archive/tracking-before-2026-10-09/GIGPIES_IMPLEMENTATION.md). Its launch instructions are retired.
