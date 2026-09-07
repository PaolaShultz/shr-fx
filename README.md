# shr-fx

A standalone, wet-only send/return rack for a 40×13 touchscreen terminal.
The Cargo package, executable and repository are named `shr-fx`.
The launcher gadget ID stays `fx`; JACK/ALSA identities, `FX_DATA_ROOT` and
the existing private data directories retain that stable ID.

Two independent engines provide a stereo delay (including ping-pong) and a
compact room reverb. Rust, ratatui/crossterm, JACK audio and input-only ALSA
Sequencer MIDI. The mixer keeps the dry signal.

Version 0.1.0 is a usable software baseline. Hardware-free tests cover DSP,
routing, controls, storage and terminal cleanup. Live audio quality, JACK
headroom, xruns, temperature and physical latency still require an authorized
on-device session. No live audio or MIDI hardware tests were run during creation.

## Build and run

The exact toolchain is Rust **1.97.1**. Native build requirements are a C
compiler, pkg-config, JACK development headers and ALSA development headers
(on Debian: `build-essential pkg-config libjack-jackd2-dev libasound2-dev`).

```sh
cargo build --release --locked
./target/release/shr-fx
```

Default startup is **offline**: the complete interface, sound storage and
logical routing work without opening JACK or ALSA. No signal is synthesized or
played. The app does not change the terminal font, tty setup or JACK settings.

For an explicitly authorized live session, start it with:

```sh
./target/release/shr-fx --audio --midi
```

Both flags are independent. JACK must already be running; shr-fx never starts,
restarts or reconfigures it. With no saved physical assignments, shr-fx attaches
with unconnected, silent ports. The exact client name is `fx`; an existing
client of that name causes a visible failure instead of a suffixed duplicate.

`--help`, `--version`, `--data-root DIR` and `FX_DATA_ROOT` are supported.
Without an override, data lives in `$XDG_DATA_HOME/fx`, or
`$HOME/.local/share/fx`. Use a private directory outside this checkout.
`local.json` holds physical port identities and controller settings;
`sounds/sound-01.json` through `sound-16.json` hold rack snapshots. No accounts,
network, launcher SDK or sibling repository are required.

## Touch and keyboard operation

The overview shows both engines, source/return slots, tempo, input/output
circular LED meters, and the selected engine's five main controls. Tap the A
or B row to select it. Tap a parameter and use `-` / `+`; **Apply** publishes
that field, and **Cancel** discards it. Selecting another field cancels its
previous uncommitted draft. Editing free delay milliseconds turns sync off.
Return levels run from 0 to 100%, before fixed routing headroom.

The last two control rows change with context. Row 13 is the shared status or
fault row. **PANIC** and **Exit** remain visible on every page. A smaller
terminal shows a visible Exit instead of clipping the working interface.

- **TAP** sets internal tempo after two valid taps; subsequent taps average up
  to four intervals. **More → Tempo** offers manual BPM, internal/MIDI clock,
  shared/independent A/B tempo, free/divided delay time and ping-pong.
- **Wet bypass** stops new excitation through a 10 ms ramp and drains the tail.
  It never passes dry input. **Mute** immediately clears and silences the
  selected engine; press again to resume. **PANIC** does that to both.
- **Routing** edits a detached logical draft. **Apply** validates all inputs
  and returns; errors retain the draft, selection and active rack. **Cancel**
  returns to the overview. The **Ports** button opens physical configuration.
- **More → Ports** selects exact JACK names for input/output slots 1–4.
  Switch between the four input and four output rows with **Inputs/Outputs**.
  Use **Name >** to read every part of a long selected port name.
  Choose **Unassigned** for unused slots. Apply connects only these explicitly
  selected names and saves local settings. It never chooses enumeration order.
- **Sounds** selects one of 16 generated slots with Previous/Next. Save writes
  the current complete rack; replacing a saved slot requires the visible
  Replace action. Load validates before publication. Cancel closes the page.
  Snapshots include algorithms, all parameters, tempo policy and logical
  routing. Physical connections and MIDI mappings stay local. Startup uses
  conservative built-in settings; sound recall is explicit.
- **More → Retry audio** closes only this client's resources and attaches
  again, preparing buffers for the server's current rate. Use it after a server
  restart or sample-rate change. It requires the original `--audio` flag.

Keyboard equivalents: `a`/`b` engine, Tab/Up/Down focus, Enter activate,
Left/Right `-`/`+`, Esc cancel, `t` tap, `w` wet bypass, `m` mute, Space panic,
`r` routing, `s` sounds, `q` Exit. Focus includes visible fields and buttons.
Touch reacts on press; mouse release/drag cannot repeat destructive actions.

## Routing and headroom

Four stable logical inputs (`fx:in_1` … `fx:in_4`) and outputs
(`fx:out_1` … `fx:out_4`) exist independently of physical channel count. A
source is any mono slot, the arithmetic mean of any two distinct slots, or any
ordered stereo pair. Both engines can share a source. Only assigned, exactly
connected physical slots are available during live operation.

| Layout | A return | B return | Physical outputs |
|---|---|---|---|
| Dual mono | assigned mono | separate assigned mono | 2 |
| Shared stereo | contribution to A's assigned L/R | same L/R | 2 |
| Dual stereo | assigned L/R | separate assigned L/R | 4 |
| Mono + stereo | assigned mono | separate assigned L/R | 3 |
| Stereo + mono | assigned L/R | separate assigned mono | 3 |
| A mono / A stereo | mono or L/R | off | 1 / 2 |
| B mono / B stereo | off | mono or L/R | 1 / 2 |

Mono uses `(L + R) / 2`, including phase cancellation for opposite channels.
Each separate return is multiplied by **0.5** (6.02 dB headroom). Shared stereo
multiplies each engine by **0.25**, then adds them. This is deterministic gain
staging, not a limiter. Meters show each engine's contribution after its return
level and routing headroom; shared physical peaks can be higher than either
individual meter. LED thresholds are −48, −30, −18, −6 and −1 dBFS.

Mono slots need not be adjacent. Stereo L/R must be distinct. Duplicate output
ownership is rejected except in Shared stereo. Four outputs do **not** require
four inputs. Switching layouts preserves explicit slot choices, which may need
repair before Apply. Inactive R fields are stored but do not route audio.
Shared stereo explicitly uses A's L/R fields for both engines.

Lost or extra connections mute only dependent engines. shr-fx never removes an
unrelated connection to fix ambiguity. Repair it with the graph owner, or
reapply the intended exact assignments. Physical Apply first checks all names,
directions, duplicates and required slots, then acknowledges silence before
editing this client's connections. A connection or save failure restores the
previous exact connections; failed restoration leaves the client muted with
Retry guidance. Logical Apply never changes physical connections.

## MIDI control

Launch with `--midi`; **More → MIDI** discovers readable ALSA Sequencer ports.
Select an exact client/port name pair and Apply. Missing or duplicate names are
errors. Numeric ALSA addresses are resolved at runtime and not persisted.
The app subscribes only its own `fx:control` input to the selected source and
has no MIDI output port, forwarding, note playback or clock transmission.

Choose a learn target (an action or an A/B parameter), choose its control kind,
press Learn, move the control, then Apply or Cancel. Source setup must be
applied before receiving learn gestures. Existing Cancel/Panic/Exit bindings
continue to work during learn. Relearning replaces the same target or the
same physical control binding in the draft.

Kinds: note button, CC button, absolute CC with pickup, and relative CC using
MIDI two's complement (`1..63` positive, `65..127` negative, `0/64` neutral;
acceleration capped at eight steps). Other relative encodings need a controller
configuration change; they are not auto-guessed. Channels are stored as 0–15.
Buttons trigger only on press; note-off, velocity-zero note-on and CC release
rearm them. CC buttons press at 64 and release below 64. Holding/repeating a
button does not repeatedly invoke an action. Learned mappings remain drafts
until Apply. No controller model or physical note mapping is hardcoded.

Targets include Previous, Next, Confirm, Cancel, Decrease, Increase, Select A/B,
Tap, Bypass, Mute, Panic, Routing, Sounds, MIDI and Exit. These let a configured
controller operate every visible screen without a computer keyboard. Direct
parameter targets are time/predelay, feedback, damping, level and BPM. Absolute
knobs pick up within 2.5% of the current value or when crossing it. Touch edits
and recall reset pickup. MIDI parameter CCs are continuous live controls;
touch parameter fields are drafts.

Clock consumes 24 ticks per quarter note, acquires after 24 plausible intervals,
smooths intervals and bounds tempo to 30–300 BPM. Lost/stopped clock holds the
last valid BPM and tails. Fallback to Internal is explicit; taps/manual BPM do
not seize external tempo. The display's clock estimate uses input-thread
receipt time; USB/controller and scheduler jitter remain a hardware test item.
Queue overflow drops the bounded backlog and requires button releases before
rearming. It does not cut audio tails.

## Timing, DSP and limits

Delay is 1–2000 ms with feedback 0–0.9, low-pass damping and independent L/R or
crossed feedback. Mono ping-pong starts on L and alternates; stereo inputs retain
independent excitation. Divisions are 1/16, 1/8, 1/4, dotted 1/4 and 1/2. Values
beyond the buffer capacity clamp at 2000 ms, which the overview displays.
Room is an original four-comb/two-allpass network per side with 0–200 ms explicit
predelay, damped feedback and distinct stereo lengths. Its Feedback control
sets internal comb gain from 0.45 to 0.9; no RT60/calibrated-space claim is made.

Both algorithms are prepared for both engines before activation. Continuous
controls ramp over approximately 10 ms. Delay and predelay changes crossfade
read taps over 20 ms; rapid changes coalesce into the next fade. Algorithm and
ping-pong changes fade the affected engine out, clear in constant time, then
fade it in. Routing changes fade both returns out, change routing at a callback
boundary, clear affected history, then fade in. There is no overlapping second
rack or unbounded topology. Algorithm changes deliberately retire that engine's
old tail; independent edits preserve the other engine.

A delay adds its chosen echo time; room has its comb-network onset plus the
chosen predelay (minimum one sample in its predelay storage). No extra fixed
5–6 ms is inserted. The owner's initial 5–6 ms allowance is an estimate of the
physical path, not a measurement. Whole-path latency depends on JACK, buffers,
converters and connections. No total-predelay compensation is claimed.

Supported preparation rates: 8–192 kHz; periods up to 8192 frames. Callback
storage and work are fixed by these bounds. Buffer-size changes within the cap
need no allocation; rate changes mute until explicit Retry prepares new state.
Non-finite or runaway values above an absolute amplitude of 16 latch a fault
in the affected engine. Its entire period contribution is removed, including
from shared stereo. Mute then unmute clears the fault. This fault boundary is
not a musical dynamics processor. Denormally small state is flushed. Bypass
returns eventually reach exact zero; an upper bound fades the final second
and clears after 281 seconds even at the longest, highest-feedback setting.

Larger spaces, chorus, additional modulation and combinations are deferred
until matched-level listening and measured dual-engine budgets justify them.
There is no plugin host, dry insert path, master strip or network catalog.

## Validation and shutdown

```sh
cargo fmt --all -- --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
cargo build --release --locked
python3 scripts/smoke_terminal.py target/release/shr-fx
```

The normal suite contains fast production routing, DSP, safety, allocation,
MIDI, strict-storage, recovery, CLI and 40×13 interaction tests. A one-minute
**offline simulation** is opt-in:

```sh
cargo test --release --locked --test soak -- --ignored --nocapture
```

See [the implementation plan](docs/PLAN.md), [architecture and ownership](docs/ARCHITECTURE.md)
and [remaining live acceptance](docs/ACCEPTANCE.md). Offline simulation does not
establish callback scheduling, xruns, listening quality or physical latency.

Visible Exit, `q`, Ctrl-C, SIGINT, SIGTERM and SIGHUP stop only shr-fx. It requests
silence, waits up to 150 ms for callback acknowledgement, deactivates/joins the
owned JACK client, closes its MIDI input, frees buffers off-thread and returns
to the caller. Normal exit and handled errors restore raw mode, cursor, mouse
reporting and the alternate screen. It never stops the JACK server or changes
services. Run directly or let `go` launch/wait for the ordinary `shr-fx` executable.
