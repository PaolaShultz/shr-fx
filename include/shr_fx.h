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

#ifdef __cplusplus
}
#endif
#endif
