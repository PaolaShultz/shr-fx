#ifndef SHR_FX_H
#define SHR_FX_H

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/* Single-owner handle. Creation/destruction are control-thread operations.
 * Stereo samples are interleaved L,R doubles; DSP and retained state are f64.
 * No process/reset allocation, locks, I/O or host-clock dependence.
 * See docs/ARCHITECTURE.md for sample, pointer and discontinuity contracts. */
void *shr_fx_v1_create(uint32_t sample_rate, uint32_t max_block);
/* 0 success, -1 invalid pointer shape, -2 block capacity, -3 sample fault.
 * Host must mute any failed block. Exact in-place processing is supported.
 * Source bounds: 8..192 kHz, 1..8192 prepared frames, finite |input| <= 16. */
int32_t shr_fx_v1_process(void *handle, const double *input, double *output,
                        uint32_t frames);
void shr_fx_v1_reset(void *handle);
void shr_fx_v1_destroy(void *handle);
/* Intentional echo onset: round(sample_rate * 0.020), 960 at 48 kHz.
 * No adapter block latency. Excludes host/device/network latency. */
uint32_t shr_fx_v1_delay_frames(void *handle);

/* Additive read-only queries. Use version=1 and EXACT sizeof(output struct).
 * Invalid version/size/pointer/alignment/span/handle overlap returns -1 without
 * writing output. Caller guarantees live allocations and exclusive ownership.
 * Keep the library loaded until all handles are destroyed.
 * Queries must be serialized with process/reset/destroy; no thread safety.
 * No allocator ownership crosses this ABI. No hardware health is reported. */
typedef struct shr_fx_capabilities_v1 {
    uint32_t version, size;
    char identity[32]; /* NUL-terminated fx-a/fixed-delay-v1 */
    uint32_t min_sample_rate, max_sample_rate;
    uint32_t min_block_frames, max_block_frames;
    uint32_t channels, sample_bits, reset_supported, writable_parameters;
    uint32_t rack_available, adapter_buffer_frames;
    double delay_ms, feedback, damping, wet_gain;
} shr_fx_capabilities_v1;
typedef struct shr_fx_status_v1 {
    uint32_t version, size, sample_rate, max_block_frames;
    uint32_t intentional_delay_frames, adapter_buffer_frames;
    int32_t last_process_result; /* last valid-handle process call; initially 0 */
    uint32_t reset_reason; /* 0 none, 1 explicit, 2 capacity, 3 pointer, 4 sample */
    uint64_t reset_count; /* saturating history-clear count; initially 0 */
} shr_fx_status_v1;
/* Fixed parameters are read-only; embedded rack/writable controls unavailable. */
int32_t shr_fx_v1_capabilities(shr_fx_capabilities_v1 *output,
                             uint32_t version, uint32_t size);
/* Reset preserves last_process_result; successful process replaces it with 0.
 * reset_reason persists until the next clear. This is history, not a health bit.
 * Output must not overlap handle storage (including owned delay allocations). */
int32_t shr_fx_v1_status(void *handle, shr_fx_status_v1 *output,
                       uint32_t version, uint32_t size);

/* Additive v2: fx-a/prepared-delay-v2, independent stereo digital wet delay.
 * Single serialized owner. V1 handles/tokens cannot be used with v2 functions.
 * All operations require live correctly typed allocations, exclusive ownership,
 * aligned non-overflowing spans; never concurrently retire/access a token.
 * Keep this library loaded until every handle AND prepared token is destroyed.
 * No host allocator ownership crosses the ABI. No rack or hardware capability.
 * See docs/ARCHITECTURE.md for full timeline/error/transition contract.
 * Channel controls: finite delay_ms 1..500, feedback 0..0.85, damping 0..0.99
 * (per-sample coefficient), wet_gain 0..1, bypass 0/1, reserved MUST be zero.
 * Bypass fades excitation to zero over ceil(rate/50) frames, then drains tails;
 * resume fades excitation to one. Settlement never means measured tail silence.
 * Changed controls ramp/read-head crossfade for the same 20ms frame bound.
 * Unchanged channels retain exact state; at most two read heads per channel.
 * Rates 8000..192000 Hz, blocks 1..8192, finite |input| <= 16, f64 throughout.
 * Initial controls 20ms/.25/.35/.5/bypass0; generation/frame cursor zero.
 * Create/destroy/prepare/retire are CONTROL-thread allocation/free operations.
 * Process/commit/panic/reset/queries have bounded work and no allocation/free.
 * Prepare validates complete config: version=2, size=sizeof(config), both
 * embedded version/size identical, generation > expected_generation.
 * NULL means invalid config/rate/pointer; allocation failure aborts as in v1.
 * Commit BORROWS a token (rate must match), copies scalars, never frees it.
 * Retire accepted/refused tokens off render; may retire immediately after commit.
 * Commit source_frame MUST equal next_source_frame and expected_generation
 * MUST equal applied_generation; no commit while either channel transitions.
 * Applied frame is commit boundary; settled frame is exclusive source frame
 * after last ramp sample (immediate for no-change config). Target is readback.
 * Errors: 0 OK, -1 invalid argument/span, -2 capacity, -3 sample fault,
 * -4 stale generation, -5 transition busy, -6 source timeline/overflow.
 * Refused commit/query/timeline calls leave state/output exact.
 * Process uses interleaved L/R doubles; exact in-place allowed, partial overlap
 * and overlap with inline/owned handle storage rejected. Zero frames allows
 * NULL audio pointers. Capacity/pointer errors clear history/output untouched;
 * sample errors clear history/zero valid whole block. HOST MUST MUTE failed
 * blocks. Cursor advances only on successful frames; timeline errors do not
 * replace last_process_result. Other valid-handle process calls do.
 * Reset clears both histories, snaps accepted controls to targets, settles
 * generation at supplied frame and replaces cursor; host mutes discontinuity.
 * Panic mask1..3 clears selected history/filter immediately, preserving controls,
 * ramps and timeline. It is not permanent mute; new excitation can return.
 * reset_reason: 0 initial,1 reset,2 capacity,3 pointer,4 sample,5 panic.
 * reset_count saturates. Queries are serialized snapshots, not atomic observers.
 * Old libraries: resolve ALL v2 symbols; if absent, use complete v1 fixed/read-
 * only API and disable writable controls. Never fake configurable capability. */
typedef struct shr_fx_channel_v2 {
    double delay_ms, feedback, damping, wet_gain;
    uint32_t bypass, reserved;
} shr_fx_channel_v2;
typedef struct shr_fx_config_v2 {
    uint32_t version, size;
    uint64_t expected_generation, generation;
    shr_fx_channel_v2 channel[2];
} shr_fx_config_v2;
typedef struct shr_fx_capabilities_v2 {
    uint32_t version, size;
    char identity[32];
    uint32_t min_sample_rate, max_sample_rate, min_block_frames, max_block_frames;
    uint32_t channels, sample_bits, rack_available, adapter_buffer_frames;
    double min_delay_ms, max_delay_ms, max_feedback, max_damping, max_wet_gain;
    uint32_t transition_ms, max_read_heads_per_channel;
    uint64_t max_delay_storage_bytes;
} shr_fx_capabilities_v2;
typedef struct shr_fx_status_v2 {
    uint32_t version, size, sample_rate, max_block_frames;
    uint32_t transition_frames, transitioning_mask;
    int32_t last_process_result;
    uint32_t reset_reason;
    uint64_t applied_generation, settled_generation;
    uint64_t applied_source_frame, settled_source_frame, next_source_frame, reset_count;
    uint32_t remaining_frames[2];
    shr_fx_channel_v2 target[2];
} shr_fx_status_v2;
void *shr_fx_v2_create(uint32_t sample_rate, uint32_t max_block);
void *shr_fx_v2_prepare(const shr_fx_config_v2 *config, uint32_t sample_rate,
                        uint32_t version, uint32_t size);
int32_t shr_fx_v2_commit(void *handle, const void *prepared, uint64_t source_frame);
void shr_fx_v2_retire(void *prepared);
int32_t shr_fx_v2_process(void *handle, const double *input, double *output,
                         uint32_t frames, uint64_t source_frame);
int32_t shr_fx_v2_panic(void *handle, uint32_t channel_mask);
int32_t shr_fx_v2_reset(void *handle, uint64_t next_source_frame);
int32_t shr_fx_v2_status(void *handle, shr_fx_status_v2 *output,
                        uint32_t version, uint32_t size);
int32_t shr_fx_v2_capabilities(shr_fx_capabilities_v2 *output,
                              uint32_t version, uint32_t size);
void shr_fx_v2_destroy(void *handle);

#ifdef __cplusplus
}
#endif
#endif
