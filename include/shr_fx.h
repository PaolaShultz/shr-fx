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

/* V2 single-owner prepared wet effect. See Architecture's v2 contract.
 * Never mix v1/v2 handles. All caller storage disjoint from objects/buffers.
 * Success publish consumes prepared; errors retain caller ownership.
 * retire transfers a relinquished object; destroy/cancel only off-thread. */
enum { SHR_FX_V2_OK=0, SHR_FX_V2_ARGUMENT=-1, SHR_FX_V2_CAPACITY=-2,
 SHR_FX_V2_SAMPLE=-3, SHR_FX_V2_STALE=-4, SHR_FX_V2_DUPLICATE=-5,
 SHR_FX_V2_SETTINGS=-6, SHR_FX_V2_PANIC=-7 };
enum { SHR_FX_V2_DELAY=1, SHR_FX_V2_ROOM=2, SHR_FX_V2_CHORUS=3 };
/* IDs time=1, amount=2, damping=3, rate=4, gain=5, bypass=6.
 * Units ratio=0, ms=1, Hz=2, boolean=3. unavailable != legitimate zero.
 * Chorus additionally requires time-amount >=1 and time+amount <=39 ms. */
typedef struct shr_fx_parameter_v2 {
 uint32_t version, size, algorithm, parameter, available, unit;
 double minimum, maximum, default_value;
} shr_fx_parameter_v2;
int32_t shr_fx_v2_parameter(uint32_t algorithm, uint32_t parameter,
 shr_fx_parameter_v2 *out, uint32_t version, uint32_t size);
typedef struct shr_fx_settings_v2 {
 uint32_t version, size, algorithm, bypass;
 double time_ms, amount, damping, rate_hz, gain;
 uint32_t seed, reserved;
} shr_fx_settings_v2;
typedef struct shr_fx_capabilities_v2 {
 uint32_t version, size, algorithm_mask, sample_bits;
 uint32_t min_rate, max_rate, max_block, pending_capacity, retired_capacity;
 uint32_t transition_frames_per_second, max_prepared_bytes, hardware_budget_available;
} shr_fx_capabilities_v2;
typedef struct shr_fx_status_v2 {
 uint32_t version, size, rate, max_block;
 uint64_t revision, accepted_request, applied_request, last_request;
 /* last request: 0 none, 1 accepted, 2 swapped, 3 applied, 4 interrupted,
  * 5 rejected. Revision is the swapped state; applied waits for fade-in. */
 uint32_t request_state;
 int32_t request_result; /* Refusal survives subsequent process success. */
 int32_t last_result;
 uint32_t fault, transition, intentional_delay_frames;
 shr_fx_settings_v2 current, target;
} shr_fx_status_v2;
int32_t shr_fx_v2_defaults(uint32_t algorithm, shr_fx_settings_v2 *out, uint32_t version, uint32_t size);
int32_t shr_fx_v2_validate(const shr_fx_settings_v2 *settings);
int32_t shr_fx_v2_capabilities(shr_fx_capabilities_v2 *out, uint32_t version, uint32_t size);
void *shr_fx_v2_prepare(uint32_t rate, uint32_t max_block, const shr_fx_settings_v2 *settings);
void *shr_fx_v2_create(uint32_t rate, uint32_t max_block, const shr_fx_settings_v2 *settings);
int32_t shr_fx_v2_publish(void *instance, void *prepared, uint64_t request, uint64_t base_revision);
int32_t shr_fx_v2_process(void *instance, const double *input, double *output, uint32_t frames);
int32_t shr_fx_v2_reset(void *instance);
int32_t shr_fx_v2_status(void *instance, shr_fx_status_v2 *out, uint32_t version, uint32_t size);
void *shr_fx_v2_retire(void *instance);
void shr_fx_v2_cancel(void *prepared);
void shr_fx_v2_destroy(void *instance);

#ifdef __cplusplus
}
#endif
#endif
