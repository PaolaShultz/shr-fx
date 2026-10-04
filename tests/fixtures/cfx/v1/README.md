# E07 fixed adapter corpus

Owner-generated C-FX:1 evidence for `fx-a/fixed-delay-v1`. `corpus.json` pins
fixed capabilities and exact first taps including sub-f32 detail. `sample.c`
consumes the actual header/library, checks every listed onset, stereo samples,
first feedback repeat, reset silence and numerical fault status. No endpoints
are enumerated or opened. Rust production tests cover invalid query shapes.

Reproduce after `CARGO_INCREMENTAL=0 cargo +1.97.1 build --release --locked -j1 --lib`.
Coordinated lab workers follow the [shared build-lock policy](https://github.com/PaolaShultz/gigpies/blob/main/docs/PARALLEL_WORK_PLAN.md):

```sh
cc -std=c11 -Wall -Wextra -Werror -Iinclude tests/fixtures/cfx/v1/sample.c -Ltarget/release -lshr_fx -Wl,-rpath,"$PWD/target/release" -lm -o /tmp/shr-fx-e07
/tmp/shr-fx-e07
```

The executable is disposable. Exact source/header/library SHA-256 identities
belong to the dated private handoff, not this portable numerical contract.
