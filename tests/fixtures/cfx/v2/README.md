# Prepared v2 C caller

Run `scripts/check_c_api_v2.sh` from the repository. It builds the offline
shared library and compiles/runs `caller.c` with C11 warning errors, then runs
the retained v1 caller. Both temporary executables are removed on exit.
The caller verifies actual wet samples against independently computed delay,
Room comb/allpass and fixed-depth chorus samples, plus ownership/status recovery.
Rust normal tests independently cover modulated taps and allocation freedom.

The Architecture v2 contract and public header own settings/lifetime semantics.
No v2 settings file format, machine assignments or hardware admission is claimed.

The older `sample.c` and `corpus.json` are frozen evidence for the independently
developed source-timeline delay ABI before its symbols were separated. Use
[the current delay caller](../delay-v2/README.md) with a current library.
