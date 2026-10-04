/* E07 owner-generated device-free C consumer and numerical corpus. */
#include "shr_fx.h"
#include <assert.h>
#include <math.h>
#include <stdio.h>
#include <string.h>
_Static_assert(sizeof(shr_fx_capabilities_v1) == 112, "capability layout");
_Static_assert(sizeof(shr_fx_status_v1) == 40, "status layout");
int main(void) {
    shr_fx_capabilities_v1 c = {0};
    assert(shr_fx_v1_capabilities(&c, 1, sizeof c) == 0);
    assert(strcmp(c.identity, "fx-a/fixed-delay-v1") == 0);
    assert(c.delay_ms == 20 && c.feedback == .25 && c.damping == .35 && c.wet_gain == .5);
    assert(c.reset_supported == 1 && c.writable_parameters == 0 && c.rack_available == 0);
    const unsigned rates[] = {8000, 8001, 44100, 48000, 96000, 192000};
    for (unsigned r = 0; r < sizeof rates / sizeof rates[0]; ++r) {
        double input[16384] = {0}, output[16384] = {0};
        void *h = shr_fx_v1_create(rates[r], 8192);
        assert(h);
        shr_fx_status_v1 s = {0};
        assert(shr_fx_v1_status(h, &s, 1, sizeof s) == 0);
        unsigned onset = (rates[r] + 25) / 50;
        assert(s.intentional_delay_frames == onset && s.adapter_buffer_frames == 0);
        input[0] = .5 + 0x1p-40; input[1] = -.25 - 0x1p-42;
        assert(shr_fx_v1_process(h, input, output, 8192) == 0);
        for (unsigned i = 0; i < onset * 2; ++i) assert(output[i] == 0);
        assert(output[onset * 2] == input[0] * .5);
        assert(output[onset * 2 + 1] == input[1] * .5);
        assert(fabs(output[onset * 4] - input[0] * .65 * .25 * .5) < 1e-17);
        shr_fx_v1_reset(h);
        memset(input, 0, sizeof input);
        assert(shr_fx_v1_process(h, input, output, 8192) == 0);
        for (unsigned i = 0; i < 16384; ++i) assert(output[i] == 0);
        input[0] = NAN;
        assert(shr_fx_v1_process(h, input, output, 1) == -3);
        assert(shr_fx_v1_status(h, &s, 1, sizeof s) == 0);
        assert(s.last_process_result == -3 && s.reset_reason == 4 && s.reset_count == 2);
        printf("%u %u %.17g %.17g %d %u %llu\n", rates[r], onset,
               (.5 + 0x1p-40) * .5, (-.25 - 0x1p-42) * .5,
               s.last_process_result, s.reset_reason, (unsigned long long)s.reset_count);
        shr_fx_v1_destroy(h);
    }
    return 0;
}
