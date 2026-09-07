# Ownership and bounded work

`model.rs` owns versioned scalar rack state, independent input selection and
finite output layouts. `storage.rs` owns strict JSON decoding, file size limits,
private physical identities and atomic file replacement. `dsp.rs` has no JACK,
ALSA, UI, filesystem, network or clock dependency. The implementation is original;
no code or private state was copied from sibling repositories. The reference
architecture documents listed in AGENTS.md informed the separation of wet
returns, publication, fault ownership and measurement from acceptance.

`audio.rs` owns the exact JACK client, four registered inputs and four outputs,
connection edits, the SPSC command producer, atomics and shutdown. Its callback
owns the `Processor` exclusively. The complete core is shared with hardware-free
allocation/recovery tests. JACK adapter code checks missing pointers before
forming slices; each registered port owns a distinct buffer. A missing buffer
removes dependent engines, preserving a healthy separate/shared contribution.
An unsupported block size clears all supplied outputs without processing.

Each engine preallocates a maximum two-second stereo delay, a 200 ms stereo room
predelay, four combs and two allpasses per side. Maximum-rate DSP preparation is tested against an 8 MiB allocation budget.
Both algorithms stay prepared;
there is one active algorithm per engine, no second rack and no structural
allocation/retirement during performance. Only scalars are published. All DSP
buffers are freed by the owner after deactivation joins the callback. Logical
clears reset ring validity and positions; they do not walk or free delay memory.

The callback accepts at most eight Copy commands from an eight-entry SPSC ring
at a block boundary. A full queue refuses ordinary edits before UI state changes.
Mute and Panic use a separate atomic bit mask, applied after bounded queue
intake, so an old command cannot defeat a simultaneous panic. Parameter drafts,
JSON, source discovery, graph audits and formatting live on the control thread.
No DSP is borrowed by that thread. Per-period stereo scratch storage allows a
late engine fault to remove its entire contribution while keeping the healthy
engine even on shared physical outputs. Maximum period is 8192 frames.

Meters publish nonnegative peak bits through atomic max; the UI consumes peak
holds. Fault, missing-port, rate, xrun, callback count and command-consumption
status use atomics. The applied sequence reports command consumption; individual
fade completion can follow by up to a few blocks. JACK CPU load is queried and
labelled as whole-server load off-thread; it is not a measurement of this
callback's maximum cost. No callback timings or temperatures are claimed by
this initial UI.

A port audit every 250 ms checks exact counterpart names and connection counts.
Notifications immediately invalidate the affected owned slot. An atomic graph
generation prevents an in-flight audit from restoring availability from stale
observations. Unrelated clients' graph edits do not invalidate shr-fx. Unexpected
extra links are visible ambiguity, never a reason to disconnect another owner's
links. Physical Apply preflights before requesting callback silence, retains
exact old connection pairs, and rolls back only pairs this operation changed.
A failed rollback keeps silence until explicit audio Retry.

JACK sample-rate changes publish the new rate, which gates processing against
the prepared rate. Retry creates a new off-thread Processor; JACK is neither
reconfigured nor restarted. A server shutdown callback only stores atomic
state. The terminal and sound storage remain usable without an audio server.

`midi.rs` has a dedicated nonblocking Sequencer input worker, one explicit
source, a 512-packet SPSC queue, and a maximum 256 input events per worker pass.
The UI consumes at most 128 packets per 10 ms pass. Clock smoothing and action
mapping happen off the audio callback. Input source identity is rechecked every
500 ms; exact sender addresses filter foreign traffic. Overflow drops the
bounded queue and requires button release before rearming; no transport stop
or audio-tail cut is inferred from missing MIDI. There is no transmit path.

`ui.rs` defines visible hit targets and dispatch for touch and configured MIDI
navigation. `main.rs` owns CLI parsing, signal flags and terminal restoration.
The interface starts without writing user state. Snapshot reads cap at 64 KiB,
reject unknown/missing schema fields and unsupported versions, and validate the
whole rack before publication. Load errors cannot partially update an engine.
Writes use private temporary files, fsync and atomic rename. Directory fsync is
best effort after commit; once rename succeeds the app reports committed state.
