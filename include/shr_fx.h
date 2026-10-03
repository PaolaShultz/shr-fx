#ifndef SHR_FX_H
#define SHR_FX_H

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/* Single-owner handle. Creation/destruction are control-thread operations.
 * Stereo samples are interleaved L,R doubles; internal DSP precision is f32.
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

#ifdef __cplusplus
}
#endif
#endif
