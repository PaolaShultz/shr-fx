# SHR FX: GigPies embedding implementation

Planning baseline **2026-10-04 / GP-2026-10-04.1**. FX-01 is implemented and independently reviewed with software validation;
FX-02/03 remain subsequent work. [Central inventory](https://github.com/PaolaShultz/gigpies/blob/main/docs/MODULE_IMPLEMENTATION_MAP.md) ·
[Agreed contracts](https://github.com/PaolaShultz/gigpies/blob/main/docs/MODULE_CONTRACTS.md). Existing product roadmaps remain authoritative for
unrelated work; this plan owns only the GigPies integration increments below.

## Objective and boundary

Own wet effects and their parameters/lifecycle. Preserve standalone two-engine/eight-slot rack. GigPies owns sends, return scheduling, source epochs and final PA-protected mixing; never import dry mixer/master-strip functionality into FX.

## Source and evidence reviewed

Repository: `/home/shome/p/shr-fx`. Inspected HEAD: `cd943c6a5cbe3c06be4e9968f54f685e97baf84f`.
Clean at inspection; recheck before editing. This dated observation is not a future ownership claim.

Owning documents: README.md; docs/PLAN.md, ARCHITECTURE.md, INTERFACE.md, ACCEPTANCE.md; include/shr_fx.h.

Source inspected: `src/c_api.rs::shr_fx_v1_*`, `src/dsp.rs::{Processor,Delay}`, `src/model.rs`, `src/audio.rs` command/publication boundary.

Standalone rack has prepared DSP, eight-entry command queue, smoothed controls,
strict rack v3/local v2 persistence and controller workflow. Embedded v1 is only
one fixed f64 stereo 20 ms delay; `delay_frames`=960 at 48 kHz, no adapter buffering.
Source/headers and recorded 101-test adapter checkpoint reviewed. H8 host evidence
is qualified; the historical sixteen-hall cost case exceeded its period and must
not become a complete-rack headroom promise.

These are source inspection and previously recorded results, not fresh builds or
physical acceptance. The planning session runs documentation checks only.

## Milestones and tasks

First: truthful independently testable fixed-adapter descriptor for Desk/GigPies. Next: reviewed minimal writable embedding contract, then bounded parameter/lifecycle implementation; full rack widening only after measured need.

Task states are execution dependencies: READY has no missing software provider;
WAITING names its precise prerequisite; DEFERRED has an activation condition.
Source delivery and build reservation are additional launch prerequisites on a
peer. Every row has one owner, the repository named in its Owner column. A later
task starts only after the previous artifact is reviewed, never merely delivered.

| Task / priority / state | Owner | Work area, inputs and required artifact | Output and measurable acceptance |
|---|---|---|---|
| FX-01 / P1 / REVIEW | SHR FX | `src/c_api.rs`, include/shr_fx.h, focused tests and ARCHITECTURE.md; C-FX:1/E07. | Add versioned read-only capability descriptor/query preserving all five v1 signatures and bit behavior. Advertise fixed delay, units/onset, supported reset, read-only parameters and bounds; no fake rack slots. E07 corpus plus header/library hashes for GP-05. |
| FX-02 / P1 / READY | SHR FX | Read existing model/rack ranges and prepared command path, C-FX B-FX; scoped design section in this plan/ARCHITECTURE.md. | Decide exact minimal writable f64 embedding ABI: prepare/apply/readback/retire, precision, time smoothing, tail bypass/panic, bounds and error examples. Strong owner+GigPies review; no API consumer implementation until new contract accepted. This task is a bounded design handoff, not a DSP rewrite. |
| FX-03 / P2 / WAITING | SHR FX | FX-01 and accepted FX-02 header/contract revision. c_api.rs and existing dsp/model paths, tests. | Implement only accepted parameter subset using owned algorithms; allocation/finite/wet-only, source-reset, bounded queue/ramp and independent-engine tests. Preserve fixed v1 compatibility. Full rack availability must be truthful. |
| FX-H1 / P2 / DEFERRED | SHR FX | FX-03 and fresh explicit hardware/load reservation. | Integrated wet latency/failure and standalone JACK/controller/listening/headroom separately; retain failed large-rack cost evidence. |

## Validation and failure behavior

Focused `CARGO_INCREMENTAL=0 cargo +1.97.1 test --locked c_api -j 1`, complete normal `cargo +1.97.1 test --locked --all-targets -j 1` under same environment for ABI/DSP/model changes; fmt and all-target warning-denied Clippy. Release ABI check with exact header after changed ABI. Historical cost matrix only for changed DSP assumptions, using ACCEPTANCE.md opt-in command; no full audition for descriptors.

Test first-tap onset and sub-f32 detail, stereo identity, gap reset, failed-block mute, zero-frame and overlap bounds. Host rejects stale/duplicate packets before processing. Bypass drains without dry leakage; panic precedence and off-thread retirement remain owner safety rules. No second UI owns embedded parameters.

Historical research, auditions, exhaustive matrices, long soaks, full-show renders
and physical/combined-load checks are intentionally outside the normal software
milestones unless their protected behavior changes. Retain their owning documented
on-demand commands; no private media download or test hardware side effect.
Independent builds retain lockfiles and existing repository editions; this plan
does not upgrade dependencies/editions or replace existing intra-repository workspace
paths. The ban is on new sibling-repository path dependencies.

## Resources, review and recovery of work

Wave 2 owner lane rpi5 or a freed Pi4 software lane after exact revision transfer; <768 MiB compiler RSS/≤512 MiB new target growth estimate for FX-01. Jobs=1 host slot; no cost matrix/large renders by default. Larger writable rack preparation requires explicit resource review.

Independent fallback: FX-02 bounded ABI design review, without implementing its unsettled API. No unbounded render or research assignment.
Before builds check free space and target size; below 20 GiB free or above 5 GiB
output is a review, not permission to delete another task's cache. No reduced
coverage/debug information to make a budget appear to pass.

Handoff: exact changed files, commit plus patch hashes or bounded source manifest
if uncommitted, contract IDs/versions and provider-fixture hashes, commands/results,
intentional skipped classes, remaining limits and next task/owner. Stage only named
owned changes if a later implementation session commits; no public push is implied.
Receiving owner reviews independently and writes an immutable private-ledger
acknowledgement. Interrupted work stays visible with last completed acceptance
criterion; never reset/stash/clean another session or replay an uncertain mutation.


## Implementation launch prompt

Host/cwd assignments and source preparation are in GigPies PARALLEL_WORK_PLAN.md.
This is a prompt for a later user-started session; no implementation worker has
been started by the planning pass.

```text
Work only in the current shr-fx checkout. Read AGENTS.md (if present), the
owning docs and docs/GIGPIES_IMPLEMENTATION.md, then the referenced GP-2026-10-04.1 contracts.
Implement only FX-01; keep progress and evidence in this plan. Check hostname,
HEAD/source manifest, live Git state, active ownership and current contract hashes
before edits; preserve other sessions and unrelated work. If this is a delivered
snapshot, verify its handoff manifest and separate receiving acknowledgement first.
Reserve this host's one build slot as described in GigPies PARALLEL_WORK_PLAN.md;
use Rust 1.97.1, Cargo.lock, CARGO_INCREMENTAL=0 and cargo -j 1 in normal target/.
Run focused checks during work and the required normal suite for changed behavior.
No sibling writes, sibling path dependencies or unilateral contract changes.
No audio/MIDI/DMX/playback/device/display/service changes or shared load tests.
When blocked report exact provider/task/version mismatch and continue only the
independent fallback FX-02 bounded ABI design review, without implementing its unsettled API within this repository; do not fake acceptance.
Stop after the scoped task and reviewable handoff, before later milestones,
physical operations, publication or deployment. Do not stage unrelated files or
claim mock, planned or incomplete behavior is a finished engine.
```

## Progress

- 2026-10-04: source and owner documents inspected; plan written. Implementation
  tasks remain in the states above. Physical evidence retains its original limits.

- 2026-10-04 task0009 FX-01: additive fixed capability and quiesced numerical-status queries implemented; E07 corpus supplied. Validation and exact artifact identities are recorded in Acceptance/private handoff. Root owns central contract acceptance and source publication. FX-02/03 remain untouched.
