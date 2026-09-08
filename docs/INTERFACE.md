# Rack, effect screen and controller

Eight pads/buttons, sixteen rotaries in two rows of eight, and clicks on
rotaries 1 and 9. **Rack → open effect → adjust → Back** is the main motion.
Opening an effect gives it a dedicated screen; all other effects keep sounding.
The external mixer retains dry audio. shr-fx always returns wet only.

## Design decision and evidence

Fructal mode: **Implement**. Actors are the musician using touch/controller/
keyboard, MIDI input worker, UI thread, audio callback and private storage.

**Provided:** the owner's corrected hardware description supersedes the first
brief's extra button mappings. The 40×13 geometry, existing 480×320 terminal and
Uni2-TerminusBold24x12 font, eight parallel slots per engine, full routing
matrix, wet-only safety and offline authorization remain binding constraints.
Click messages and rotary encoding are unknown; setup learns those identities.

**Observed:** the first implementation put a slot rail beside a parameter panel
and relied on eight additional button roles. The owner rejected its appearance
and navigation. It did not use the two actual rotary clicks. The starting
pre-redesign implementation also paged three slots, made scalar edits drafts
and required explicit engine/slot/parameter learning. Highest-priority friction
was controls that did not match the hardware, followed by cramped effect detail
and lost editing context. Existing slot bypass and configured-count headroom
already provided independent toggles; no DSP redesign was needed.

Two distinct 40×13 concepts were compared. The first keeps the rack beside the
sound; the second replaces the rack with the opened effect. These are sketches;
[the gallery](screens/README.md) is generated from actual renderer cells.

**A — persistent rail and split parameter panel (superseded):**

```text
shr-fx / OFFLINE       PANIC      Exit
 [A] B     2 Room / ON
 1Dl+ 32% | SHAPE 1/2       page >
>2Rv+ 68% |>09 Pre 0ms      13 --
 3Ch+ 24% | 10 Dcy 45%      14 --
 4Ex+ 18% | 11 Dmp 50%      15 --
 5 EMPTY  | 12 --          16 --
 6 EMPTY  | K2 Wet 68%     level
 7 EMPTY  | I ooooo O ooooo
 8 EMPTY  | 120 Int Mono 1>L1 R2
    -         +       Page >     TAP
 Slot OFF    Mute      Menu    Effects
A2 ON / K1-8 wet; K9-16 params
```

It shows all slots while editing, but the narrow panel scatters rotary order
and gives each slot only one 24-pixel-high touch row. It does not satisfy the
owner's request that opening a slot become that effect's screen.

**B — card browser, then one effect (implemented):**

```text
shr-fx / OFFLINE       PANIC      Exit
[A] Mono 1 > L1 R2      120 Int
>1 Digital           2 Room
   32% ON              68% ON
 3 Chorus            4 Warm exciter
   24% ON              18% ON
 5 Empty             6 Empty
   --                  --
 7 Empty             8 Empty
   --                  --
 Engine A  Engine B    TAP       Menu
   Open   Configure  Routing    Sounds
Offline: configure and save sounds
```

Open Room and the same space becomes:

```text
shr-fx / OFFLINE       PANIC      Exit
A / 2 Room ON
>Wet 68%     Wet level 68%
I ooooo  O ooooo  120 Int / L1 R2
[01] 02   03   04   05   06   07   08
Pick Wet2 Wet3 Wet4 --   --   --   --
Open 68%  24%  18%
[09] 10   11   12   13   14   15   16
Edit Pre  Dcy  Dmp  BPM  Rtn  --   --
Back 0ms  45%  50%  120  70%
    -         +         TAP    Slot OFF
   Rack      A / B      Menu  Configure
A2 ON / 1: Pick/Open 9: Value/Back
```

This design follows the physical rows, gives rack selection 240×48-pixel cards,
and gives an opened effect the full width. Returning costs one click, with
selection preserved. Choosing this interaction is an **inference** about ease
of live use; the owner has not yet accepted the revised screens in performance.
Amber/white/gray ANSI styling, bold focus and restrained spacing provide visual
identity. `o`/`O` are font-supported circular LEDs. `>`, brackets, labels and
pickup arrows carry meaning without relying on color. The renderer changes no
console palette, font or operating-system settings.

## The physical map

Numbers are positions, not MIDI CC or note numbers. The first position in each
physical row is clickable. No other click, extra button bank, controllable LED
or motorized knob is required.

| Top row | Rotation | Click |
|---|---|---|
| 1 | Browse visible cards, fields and actions | Open / Confirm |
| 2 | Slot 2 wet, 0–100% | — |
| 3 | Slot 3 wet, 0–100% | — |
| 4 | Slot 4 wet, 0–100% | — |
| 5 | Slot 5 wet, 0–100% | — |
| 6 | Slot 6 wet, 0–100% | — |
| 7 | Slot 7 wet, 0–100% | — |
| 8 | Slot 8 wet, 0–100% | — |

| Bottom row | Rotation | Click |
|---|---|---|
| 9 | Adjust selected value; in menus, selected draft field | Back / Cancel |
| 10 | Time / Predelay / Rate / Tune | — |
| 11 | Feedback / Decay / Depth / Drive | — |
| 12 | Damping / Damping / Base delay / Tone | — |
| 13 | Engine BPM 30–300, Internal only | — |
| 14 | Engine return 0–100% | — |
| 15 | Delay free/sync; otherwise unassigned | — |
| 16 | Delay division; otherwise unassigned | — |

**Pads/buttons 1–8:** wet bypass for the corresponding slot. They never select,
open or resize an effect, move focus, change other mixing gain or clear other
tails. The same meanings hold in menus. An empty slot reports empty and remains
empty. Buttons and pads are the same eight controls.

Rotary 1 is dedicated navigation, so there is no dedicated slot-1 wet rotary.
Select Wet on the effect screen and turn 9; this works for every slot. On the
rack screen, 9 adjusts the focused configured card's wet level without opening
it. This is the deliberate tradeoff for a consistent browse/open and value/back
pair. All screen actions, including A/B, TAP, Menu, PANIC and Exit, are reachable
with rotary 1 and its click. They also have direct touch targets. No long press,
double tap or modifier chord is needed.

| Family | Rotary 10 | Rotary 11 | Rotary 12 |
|---|---|---|---|
| Reverb | Predelay 0–200 ms | Decay 0–90%, relative | Damping 0–95% |
| Delay | Time 1–2000 ms | Feedback 0–90% | Damping 0–95% |
| Chorus | Rate 0.05–5 Hz | Depth 0–8 ms | Base delay 10–30 ms |
| Exciter | Tune 600–6000 Hz | Drive 0–100% | Tone 0–100% |

Delay divisions are 1/16, 1/8, 1/4, dotted 1/4 and 1/2. Effective time clamps
to the prepared 2000 ms bound. Moving Time deliberately selects free time.
Sync and tempo changes use existing DSP smoothing/crossfades. No fixed path
predelay or new DSP control was added. Algorithm/type, ping-pong and rack count
remain structural draft controls. The current continuous controls fit on one
effect screen; no parameter page is required for this hardware map. Legacy
saved parameter-page bindings still work without moving the new sound roles.
When the selected slot is empty, 10–16 are unassigned; rotaries 2–8 still
address their own configured slots.

## Performing and returning

Touch a rack card, or browse and click 1, to open the effect. Browse a parameter
with 1, then turn 9 to adjust; dedicated sound rotaries act directly. Touch a
parameter and use `-` / `+` for the same immediate smoothed edit. The selected
value detail gives its longer name and unit; status feedback gives its range.
Unassigned positions show `--`. Returning from Menu restores the effect, value
and focus. Click 9 from the effect returns to its card. A/B retains each engine's
selected slot; an open structural draft must be applied or cancelled first.

ON means excitation is enabled in configuration; OFFLINE explicitly says when
no audio is running. OFF means bypass: excitation stops through the existing
ramp and the tail may drain. No measured tail completion is claimed. MUTED
means the engine was cleared and silenced; FAULT means a latched DSP fault.
BLOCK/NO OUT identifies unavailable buffers or returns. EMPTY is outside the
configured slot count. Pads under engine bypass/mute still change only their
own flags; they cannot resume the engine.

Menu exposes engine Wet bypass and Mute/Resume. PANIC clears/mutes both engines;
Resume each desired engine explicitly. Mute then Resume recovers a DSP fault.
PANIC and Exit remain at the top of every normal screen. Exit safely stops only
shr-fx and restores raw mode, cursor, mouse capture and alternate screen.

Rows 11–12 hold contextual actions, row 13 one shared status/fault message.
Faults, missing ports, clock loss and MIDI failure outrank routine feedback.
`^`/`v` beside moved absolute sound controls shows waiting pickup direction.
Input/output circular LEDs use peak thresholds −48, −30, −18, −6 and −1 dBFS;
they are not tail detectors or callback-cost measurements.

Keyboard equivalents: Tab/Up/Down focus, Enter activate, Left/Right adjust,
`[`/`]` previous/next effect (menu focus when open), `e` switch engine,
`a`/`b` select engine, `g` Menu/Confirm, `x` slot bypass, `t` TAP, `w` engine
bypass, `m` Mute/Resume, Space PANIC, Esc Back/Cancel, `r` Routing, `s` Sounds,
`q` Exit. `p` retains the legacy mapping page. Touch acts once on press.

## Guided setup and pickup

1. In a separately authorized MIDI input session (`--midi`), use Menu → MIDI
   to select the exact source name and Apply. Name > shows the full identity.
   Missing/ambiguous sources fail visibly. There is no MIDI transmission.
2. Open Guided or Menu → Controller. Learn rotations 1–16, pads 1–8, then
   clicks 1 and 9: **26 inputs**. Choose Kind, Learn, move/press, release, then
   Role >. Role < revisits; skipped roles remain unassigned. Touch operates
   setup before navigation clicks have been learned.
3. Choose absolute CC/pickup or relative CC/two's complement per rotary;
   choose Note or CC button for pads and clicks. Encoding is never guessed.
   Relative 1–63 increases, 65–127 decreases, 0/64 is neutral; acceleration is
   bounded to eight steps. Other encoder formats need compatible configuration
   or future support. Absolute navigation selects a proportional visible
   position; it changes focus only. Relative navigation steps and wraps.
4. Apply saves the complete mapping draft privately. Duplicate roles/messages
   are rejected. Clear removes only the current draft role. Existing explicit
   bindings remain available through Source / explicit; Use role visibly
   adopts an explicit physical control without deleting unrelated mappings.
   Cancel discards unsaved mappings. Learn never performs its capture gesture.

Sound rotations are suppressed throughout guided setup. Elsewhere 2–8 and
10–16 keep acting on the saved performance context while 9 edits the visible
draft. The draft banner identifies that live context. Click 9 cancels capture
first, then leaves setup; PANIC and Exit remain visible during learn.

Absolute sound pickup acquires within 2.5% of the target or by crossing it.
Feedback reports UP/DOWN and target; affected controls re-arm on engine/effect
changes, recall, touch or edits through another binding. Unchanged slot levels
stay acquired across effect changes. Rotary 9 follows the selected value and
re-arms whenever that target changes, including menu/draft changes. The first
event after a context change cannot cross using a stale prior position. Draft
absolute editing also waits for pickup before altering the draft.

Note-off, zero-velocity note-on and CC below 64 release buttons. Repeated presses
without release do nothing, including a click held across a screen transition.
MIDI loss/overflow discards backlog, resets pickup and requires button release.
Clock loss/stop holds the last BPM and leaves tails alone; Internal fallback is
explicit. Menu → MIDI → Apply reconnects input. Source replacement is prepared
before retiring the old subscription; failed open/save keeps active mappings
and the editable draft. Hardware release behavior and reconnect timing remain
untested on the physical controller.

## Drafts and recovery

Effects, slot configuration and Timing share one detached engine draft. Apply
merges fields changed since entry into current live state. Cancel keeps the
active rack. Unedited live MIDI/clock fields, other engine state and Panic/mute
survive Apply. A field changed both live and in the draft causes a visible
**Live conflict: Keep live, or Cancel**. Keep live copies changed live fields
into the draft while preserving other draft edits; review and Apply again.
No conflict silently overwrites either side. Engine switching and unrelated
shortcut menus cannot discard an open effect/tempo draft.

Routing is a validated draft; visiting physical Ports and returning retains
logical edits. Invalid applies, failed loads and failed saves preserve active
state and the relevant editor for repair/retry. Sounds use generated names,
so keyboard text entry is optional; overwriting has a visible Replace action.

Rack snapshots remain v3 with read-only v1/v2 migration. Private local v2
stores mappings separately; v1 explicit bindings migrate in memory and only
Apply writes v2. Legacy surface Knob/Button roles remain accepted for saved
configurations; the wizard no longer creates an additional eight-button bank.
Unknown/missing fields, duplicate physical identities or roles, invalid indices,
control-kind mismatches and more than 64 bindings are rejected. Only rotary
click indices 0 and 8 are valid. Sounds contain no physical mappings.

## Verification and remaining effort

Observed normal tests cover the actual 26-input layout from four-effect setup
through editing, routing, save/recall, menu return, all eight slots, PANIC and
Exit. Further tests cover pickup/context, held/released controls, input loss,
draft conflicts, failed operations, strict persistence, unchanged tails/gain
and every native touch target. The renderer gallery uses the installed font;
release PTY smokes run offline. See [verification results](ACCEPTANCE.md).

The six Fructal checks hold in those software workflows: opening has one clear
result; browse/open/value/back is consistent; wet-only and draft constraints
stay active; selection and entered work survive return; feedback and retained
drafts enable retry; setup learns physical roles once. The supported tradeoffs
are returning to Rack to choose another card and using selected-value rotary 9
for slot 1 wet. Effect rotary touch targets are 60×72 pixels, rack cards 240×48,
and footer targets generally 120×24 on the unchanged display.

Software checks do not establish musician acceptance, physical reach/touch
accuracy, controller CC/note identities, encoder encoding, MIDI timing, live
JACK deadlines, xruns, thermals, latency or dry-plus-wet listening quality.
No live audio, hardware MIDI, transmission, recording, installation, service,
font or operating-system changes were performed.
