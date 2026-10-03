# SHR FX operating guide

[Project overview and quick start](../README.md). Run commands from the repository root.

## Performance controls

The 40×13 **Rack** screen shows eight large effect cards. Touch a card, or
browse with rotary 1 and click it, to open that effect's own screen. **Rack** or
clicking rotary 9 returns to the same slot. This is a visual focus change;
the other effects keep sounding. Engine A/B selection is always visible.

The effect screen follows the hardware: **1–8 across the top, 9–16 across the
bottom**, with clickable positions **[01]** and **[09]**. Turn 1 to choose a
field or action; click 1 to open/confirm. Turn 9 to adjust the selected value;
click 9 for Back/Cancel. Select Wet to adjust any slot's wet level, including
slot 1. Rotaries 2–8 retain wet levels for slots 2–8; 10–16 hold sound controls,
BPM and return level. Unused controls show `--`. The current sound controls fit
on one screen, so normal operation needs no parameter bank switch.

There are **eight pads/buttons total**. Pad 1–8 toggles its corresponding
slot without moving selection. OFF stops new excitation and allows wet tails
to drain. Mute clears the selected engine; PANIC clears/mutes both; Resume
releases a mute. Empty slots stay empty until configured. Touch a parameter,
then use `-` / `+`, for immediate smoothed adjustment. Absolute sound controls show pickup direction and re-arm when their target
identity or value changes; navigation rotation changes focus only. Faults take priority
in the shared status row. Rows 11–12 hold contextual controls, row 13 status;
PANIC and Exit stay visible and reachable with rotary 1/click 1.

**Menu** reaches Effects, Tempo, MIDI source, guided Controller setup, physical
Ports, Retry audio, Routing and Sounds. The same browse/confirm and value/back
pair operates every menu. Effects and routing use Apply/Cancel drafts. Rotary 9
edits the visible draft; the dedicated sound rotaries remain live on the saved
performance context. Apply merges unedited live fields. A same-field conflict
retains the draft and offers **Keep live**, then review/Apply. Cancel retains
the active rack. Returning restores the effect screen, selected slot, value and
focus. Sounds use sixteen generated names; replacing a saved sound requires the
visible Replace action.

From the default rack, **Configure → Mode → + → Apply** selects the
four-slot MultiFX setup (Delay, Room, Chorus, Exciter). In
**Menu → Routing**, choose **A stereo / B off**, A's source and its explicit
L/R output slots, then Apply. Physical assignments remain separate under
Ports. Nothing opens audio or MIDI in the default offline invocation.

Keyboard: Tab/Up/Down focus, Enter activate, Left/Right adjust, `[`/`]` previous/
next effect, `p` legacy mapping page, `e` engine switch, `a`/`b` select engine,
`g` Menu/Confirm, `x` selected slot off/on, `t` TAP, `w` engine wet bypass,
`m` mute/resume, Space PANIC, Esc Cancel, `r` routing, `s` sounds, `q` Exit.
Touch reacts on press; releases and dragging do not repeat actions.

See the [complete interaction model, two design concepts and controller map](INTERFACE.md)
and [actual renderer previews](screens/README.md). The gallery uses the
installed terminal font; no font, tty geometry or OS settings are changed.

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

For one mixer send and one stereo output pair: select **A stereo / B off**,
choose A's input and assign A's L/R to the two output slots. A's entire MultiFX
mix returns there; effects do not each require physical outputs. Shared stereo
also allows both engines on that same pair when you need two independent sends.

Lost or extra connections mute only dependent engines. shr-fx never removes an
unrelated connection to fix ambiguity. Repair it with the graph owner, or
reapply the intended exact assignments. Physical Apply first checks all names,
directions, duplicates and required slots, then acknowledges silence before
editing this client's connections. A connection or save failure restores the
previous exact connections; failed restoration leaves the client muted with
Retry guidance. Logical Apply never changes physical connections.

## Parallel MultiFX and sound palette

Every active slot receives the engine's selected input directly. Only its wet
output goes to the engine mix. There is no serial routing or feedback between
slots or engines. For a vocal, choose Delay, Reverb, Chorus and Exciter in
four slots and set their individual wet levels. Up to eight slots are available.
Two delays with different times or multiple reverbs are also supported.

```text
             +-- Slot 1 effect -- wet level --+
Engine send -+-- Slot 2 effect -- wet level --+-- engine return level
             +-- Slot 3 effect -- wet level --+
             +-- Slot 4 exciter - wet level --+
             +-- Slots 5–8 (when enabled) ----+
```

MultiFX multiplies each slot by **1/max(3, configured slot count)**, then sums them:
1/3 for two or three slots, 1/4 for four, through 1/8 for eight. This preserves
old two/three-slot sounds. Bypass, draining tails and zero wet levels do not
change that gain; turning down one effect never boosts another. Changing the
configured count deliberately changes headroom during the engine fade/swap;
the Effects page displays the divisor. Single mode uses slot 1 without this
extra gain. The existing 0.5 separate-return / 0.25 shared-return gain
then applies after the engine return level. Slot and engine levels are 0–100%.

| Family | Choices | Character |
|---|---|---|
| Reverb | Room, Small room | Independent stereo networks; the original Room is retained |
| Reverb | Chamber | Longer comb lengths, input diffusion and gentle stereo crossfeed |
| Reverb | Plate | Shorter dense network, more diffusion; plate-like rather than physical emulation |
| Reverb | Hall | Longer economical network with dense diffusion and stereo crossfeed |
| Delay | Digital | Clear repeats with adjustable feedback damping |
| Delay | Tape echo | Filtered, gently saturated repeats with deterministic wow/flutter |
| Delay | Multi-tap | Three weighted taps per channel at fractions of the main delay time |
| Delay | Diffused | Delayed repeats pass through short allpass diffusers |
| Chorus | Chorus, Ensemble | One or three modulated wet taps per channel, with stereo phase offsets |
| Exciter | Warm, Bright | Filtered harmonic enhancement; even/odd colour or stronger odd harmonics |

All delay types support independent L/R or crossed ping-pong feedback. Multi-tap
uses 1/2, 3/4 and full time on L, and 5/8, 3/4 and full time on R, weighted
1/4, 1/4 and 1/2. Feedback comes from the full-time tap. Tape echo is a character
effect, with up to approximately 0.43 ms of wow/flutter around the selected
time, clamped to storage bounds; it does not emulate a particular machine.

Chorus controls are rate **0.05–5 Hz**, depth **0–8 ms** and base delay
**10–30 ms**. The wet path adds 2–38 ms of intentional modulated delay before
physical I/O latency. It contains no unmodulated dry branch. Listen with the
external dry vocal: relative phase and the amount of return affect the result,
and wet-only tests cannot establish the quality of that combined sound.

Exciter controls are **Tune 600–6000 Hz**, **Drive 0–100%**, **Tone 0–100%**
and the usual slot wet level. Tune focuses its input with two high-pass poles;
Tone rolls off the generated top end. Warm emphasizes even harmonics, Bright
adds stronger odd harmonics. Four-times oversampling surrounds an original
smooth nonlinear residual; no full-band dry signal is added. Drive zero is
exact silence after the control ramp. The return can still contain fundamental
and intermodulation content: it is not a mathematically isolated harmonics bus.
Listen with the external dry vocal because path delay affects their combined
tone. The oversampling FIRs add approximately 11.25 base-rate samples of group
delay (0.234 ms at 48 kHz), plus frequency-dependent filter phase. There is no
added fixed 5–6 ms. At low sample rates Tune caps internally at 20% of the rate
and the tone cutoff at 40%, keeping filters below Nyquist. Oversampling reduces
aliasing; it is not an alias-free or exact hardware-emulation claim.

These expanded sounds are software-verified candidates. Live listening and
measured JACK/thermal headroom remain required before performance acceptance.

## MIDI control

For an authorized hardware session, launch with `--midi`, then **Menu → MIDI**
selects one exact readable ALSA source. Apply opens only an input subscription;
there is no MIDI transmit port, forwarding or downstream note traffic.
Missing or ambiguous names fail visibly. Source replacement is prepared before
retiring the working subscription; failed open/save keeps the old mappings.
Name > reveals long source names in full.

**Guided** learns **26 physical inputs**: sixteen rotations, eight pads and the
two clicks on rotaries 1 and 9. No additional button bank is required. Learn
physical identities once; the app reuses their roles across A/B and all slots.
Rotary 1 browses/opens; rotary 9 edits/goes Back. The other rotations control
sound, and all eight pads retain slot bypass in menus. Missing roles can be
skipped; touch remains available during initial setup.

Select absolute CC with pickup, relative CC using two's complement, Note, or
CC button according to the physical control. The app does not guess encoding.
Relative 1–63 increases, 65–127 decreases, 0/64 is neutral, with eight-step
maximum acceleration. Absolute pickup acquires within 2.5% or on crossing.
Only affected controls re-arm on touch/context changes; recall re-arms all.
Note-off, velocity-zero note-on and CC values below 64 release buttons. Held
buttons cannot retrigger through context changes. After loss/overflow/reconnect,
release controls; **Menu → MIDI → Apply** retries input. No tail is cut by
MIDI loss or clock stop. External clock holds its last valid BPM; Internal
fallback is explicit. Clock consumes 24 PPQN, smooths and bounds 30–300 BPM.

Existing explicit engine/slot parameter and action bindings remain compatible,
with the original learn editor available. Guided adoption of an already mapped
explicit control offers **Use role** before replacing that binding in the draft;
other mappings survive. Duplicate surface roles/physical identities are rejected.
Private local schema v1 migrates read-only to v2; explicit Apply writes v2.
Rack sounds stay schema v3, with original read-only v1/v2 sound migration.
Physical mappings and device names never enter sound snapshots.

The [full guide](INTERFACE.md) specifies every rotary/button role, ranges,
setup, pickup, draft conflicts and recovery. The two known rotary clicks are
used. Controller model, message numbers and encoder encoding remain unknown; motorized knobs and controllable LEDs are not
assumed.

## Timing, DSP and limits

Delay is 1–2000 ms with feedback 0–0.9, low-pass damping and independent L/R or
crossed feedback. Mono ping-pong starts on L and alternates; stereo inputs retain
independent excitation. Divisions are 1/16, 1/8, 1/4, dotted 1/4 and 1/2. Values
beyond the buffer capacity clamp at 2000 ms, which the effect screen displays.
Reverbs use four combs per side and bounded allpass diffusion, with 0–200 ms
explicit predelay. Decay is a relative control, not an RT60 or calibrated-space
claim. The original Room's internal comb gain remains 0.45–0.9; other profiles
use their own bounded decay ranges (all below or equal to 0.9).

Every slot's algorithm storage is prepared before activation. At most sixteen
effects run across A/B, with no second rack during transitions. Continuous
levels, feedback, damping, chorus rate/depth and exciter controls ramp over
approximately 10 ms.
Delay and predelay changes crossfade read taps over 20 ms; rapid changes
coalesce into the next fade. Chorus base delay glides at at most 100 ms/second.

Changing a slot's family, type or ping-pong fades that slot out, logically
clears its history, then fades it in; other slots and the other engine retain
their tails. Changing Single/MultiFX or the active slot count fades and clears
that engine. Physical return-routing changes retain the existing bounded
fade/swap behavior. Slot bypass drains that slot; engine bypass drains every
active slot. Mute/Panic clear all slots of the affected engine immediately.

A delay adds its chosen echo time; room has its comb-network onset plus the
chosen predelay (minimum one sample in its predelay storage). No extra fixed
5–6 ms is inserted. The owner's initial 5–6 ms allowance is an estimate of the
physical path, not a measurement. Whole-path latency depends on JACK, buffers,
converters and connections. No total-predelay compensation is claimed.

Supported preparation rates: 8–192 kHz; periods up to 8192 frames. Callback
storage and work are fixed by these bounds. Sixteen prepared slots and stereo
scratch buffers allocate **70,131,200 bytes (66.88 MiB)** at 192 kHz, under
an **80 MiB DSP allocation budget**.
Exciter filter histories are small fixed arrays inside the owning slots. All
slots are prepared for live changes, including currently inactive ones.
Buffer-size changes within the cap need no allocation; rate changes mute until explicit Retry prepares new state.
Non-finite or runaway values above an absolute amplitude of 16 latch a fault
in the affected engine. Its entire period contribution is removed, including
from shared stereo. Mute then unmute clears the fault. This fault boundary is
not a musical dynamics processor. Denormally small state is flushed. Bypass
returns eventually reach exact zero; an upper bound fades the final second
and clears after 281 seconds even at the longest, highest-feedback setting.

Serial combinations, additional modulation families and larger networks are
outside this version. There is no plugin host, dry insert path, master strip
or network catalog.

## Validation and shutdown

```sh
cargo fmt --all -- --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
cargo build --release --locked
python3 scripts/smoke_terminal.py target/release/shr-fx
```

The normal suite contains fast production routing, DSP, safety, allocation,
MIDI, strict-storage, recovery, CLI and 40×13 interaction tests. An opt-in
**offline cost matrix** simulates one minute per case: one four-effect vocal
engine with a stereo return, two four-effect vocal engines, sixteen halls,
plates, tape echoes, multi-tap delays, diffused delays, exciters, ensemble
choruses, and sixteen slots under rapid edits:

```sh
cargo test --release --locked --test soak -- --ignored --nocapture
```

See the [current handoff](PLAN.md), [architecture and ownership](ARCHITECTURE.md),
[verification and remaining live acceptance](ACCEPTANCE.md), and
[historical milestones](HISTORY.md). Offline simulation does not
establish callback scheduling, xruns, listening quality or physical latency.

Visible Exit, `q`, Ctrl-C, SIGINT, SIGTERM and SIGHUP stop only shr-fx. It requests
silence, waits up to 150 ms for callback acknowledgement, deactivates/joins the
owned JACK client, closes its MIDI input, frees buffers off-thread and returns
to the caller. Normal exit and handled errors restore raw mode, cursor, mouse
reporting and the alternate screen. It never stops the JACK server or changes
services. Run directly or let `go` launch/wait for the ordinary `shr-fx` executable.
