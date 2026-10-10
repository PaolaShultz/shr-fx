# Source-timeline delay v2 C caller

This caller uses the namespaced `shr_fx_delay_v2_*` ABI. The published
`shr_fx_v2_*` interface is a separate prepared Delay/Room/Chorus owner and has
incompatible signatures; never substitute its symbols or handles.

Compile `sample.c` against `include/shr_fx.h` and the current release
`libshr_fx.so` with C11, `-Wall -Wextra -Werror`. Run it without devices.
The historical [delay corpus](../v2/corpus.json) retains its original
2026-10-08 hashes and numerical rows; the symbol namespace changed, so those
artifact hashes do not identify the current library or header.
