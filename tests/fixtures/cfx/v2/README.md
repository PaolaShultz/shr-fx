# Prepared stereo wet-delay v2 owner corpus

`sample.c` is a real C11 consumer of the [supplying header](../../../../include/shr_fx.h)
and `libshr_fx.so`. [corpus.json](corpus.json) contains its actual output rows and
exact header/library/caller SHA256 identities from the 2026-10-08 Pi4 owner run.
The supplying source is the task0020 C1 owner package based on
`5f775e01e6dea3c6fe611745619dc6f4bb91299c`; exact final commit and gate input
manifests are retained in the private handoff. A rebuild on another environment
can have a different library hash: compare numerical/status rows separately and
pin the actual supplying artifact, rather than relabeling this historical hash.

The run checks C sizeof/offsetof, exact first wet sample bits below f32 detail,
independent 1ms left/20ms right delays, immediate token retirement, applied versus
settled state, bypass/panic, busy/stale/timeline/sample errors, reset and source
transition overflow. It prints four small success/error/status JSON rows after
assertions. Additional normal Rust regressions cover fractional/rate bounds,
maximum delay/storage, allocation/free freedom, control ramps, independent
partner history, in-place/span refusal and source-counter recovery. This is one
stereo digital delay, without embedded A/B/eight-slot rack or hardware claims.

Reproduce from the owner root (Linux C compiler/JACK+ALSA development prerequisites):

```sh
CARGO_INCREMENTAL=0 cargo +1.97.1 build --release --locked -j1 --lib
cc -std=c11 -Wall -Wextra -Werror -Iinclude tests/fixtures/cfx/v2/sample.c \
  -Ltarget/release -lshr_fx -Wl,-rpath,"$PWD/target/release" -o /tmp/shr-fx-v2-caller
/tmp/shr-fx-v2-caller
sha256sum include/shr_fx.h target/release/libshr_fx.so
```

Use the actual `CARGO_TARGET_DIR` release directory when an operator has selected
one; coordinated peers hold the shared build lock. The [unchanged v1 corpus](../v1/README.md)
remains separate and must still match its six rate rows. No device is opened by
these callers. Host composition, late-wet admission and old-library symbol-set
fallback belong to GigPies. See [the complete contract](../../../../docs/ARCHITECTURE.md#prepared-stereo-wet-delay-v2).
