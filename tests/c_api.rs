use shr_fx::c_api::*;
use std::{ffi::c_void, ptr};

struct Fx(*mut c_void);
impl Fx {
    fn new(rate: u32, max_block: u32) -> Self {
        let ptr = shr_fx_v1_create(rate, max_block);
        assert!(!ptr.is_null());
        Self(ptr)
    }
    fn process(&mut self, input: &[f64], output: &mut [f64]) -> i32 {
        assert_eq!(input.len(), output.len());
        assert_eq!(input.len() % 2, 0);
        unsafe {
            shr_fx_v1_process(
                self.0,
                input.as_ptr(),
                output.as_mut_ptr(),
                (input.len() / 2) as u32,
            )
        }
    }
}
impl Drop for Fx {
    fn drop(&mut self) {
        unsafe { shr_fx_v1_destroy(self.0) };
    }
}

#[test]
fn f64_sample_detail_and_feedback_survive_delay_storage_and_reset() {
    let mut fx = Fx::new(48000, 2048);
    let mut input = vec![0.0; 4096];
    let mut output = vec![0.0; 4096];
    input[0] = 0.5 + 2f64.powi(-40);
    input[1] = -0.25 - 2f64.powi(-42);
    for _ in 0..2 {
        assert_eq!(fx.process(&input, &mut output), OK);
        for ch in 0..2 {
            // These low bits are lost by any f32 input, ring or output cast.
            assert_eq!(output[960 * 2 + ch], input[ch] * 0.5);
            assert_ne!(output[960 * 2 + ch], (input[ch] as f32 * 0.5) as f64);
            // First repeat also requires native f64 filter/feedback coefficients.
            let repeated = (input[ch] * (1.0 - 0.35)) * 0.25 * 0.5;
            assert!((output[1920 * 2 + ch] - repeated).abs() < 1e-17);
        }
        unsafe { shr_fx_v1_reset(fx.0) };
    }
}

#[test]
fn wet_only_first_tap_is_exact_at_supported_and_nonintegral_rates() {
    for rate in [8000, 8001, 44_100, 48_000, 96_000, 192_000] {
        let mut fx = Fx::new(rate, 8192);
        let delay = unsafe { shr_fx_v1_delay_frames(fx.0) } as usize;
        assert_eq!(delay, (rate as usize + 25) / 50);
        let frames = delay * 2 + 2;
        let mut input = vec![0.0; frames * 2];
        let mut output = vec![f64::NAN; frames * 2];
        input[0] = 0.5;
        input[3] = -0.25;
        assert_eq!(fx.process(&input, &mut output), OK);
        assert!(output[..delay * 2].iter().all(|x| *x == 0.0));
        assert_eq!(&output[delay * 2..delay * 2 + 4], &[0.25, 0.0, 0.0, -0.125]);
        assert!((output[delay * 4] - 0.040625).abs() < 1e-8);
        assert!(output.iter().all(|x| x.is_finite()));
    }
}

#[test]
fn blocks_in_place_and_reset_preserve_determinism_and_stereo_isolation() {
    let mut whole = Fx::new(48_000, 8192);
    let mut split = Fx::new(48_000, 8192);
    let mut input = vec![0.0; 8000];
    input[0] = 1.0;
    input[246] = -0.25;
    let mut expected = vec![0.0; input.len()];
    assert_eq!(whole.process(&input, &mut expected), OK);
    assert!(expected.chunks_exact(2).all(|frame| frame[1] == 0.0));
    let mut actual = input.clone();
    for block in actual.chunks_mut(142) {
        assert_eq!(
            unsafe {
                shr_fx_v1_process(
                    split.0,
                    block.as_ptr(),
                    block.as_mut_ptr(),
                    (block.len() / 2) as u32,
                )
            },
            OK
        );
    }
    assert_eq!(actual, expected);
    unsafe { shr_fx_v1_reset(split.0) };
    let silence = vec![0.0; input.len()];
    assert_eq!(split.process(&silence, &mut actual), OK);
    assert!(actual.iter().all(|sample| *sample == 0.0));
    unsafe { shr_fx_v1_reset(split.0) };
    assert_eq!(split.process(&input, &mut actual), OK);
    assert_eq!(actual, expected);
    // Resetting one handle does not reset a different handle's existing tail.
    assert_eq!(whole.process(&silence, &mut actual), OK);
    assert!(actual.iter().any(|sample| *sample != 0.0));
}

#[test]
fn invalid_samples_silence_the_entire_block_and_clear_pending_echoes() {
    let mut fx = Fx::new(8000, 512);
    let mut input = vec![0.0; 1024];
    let mut output = vec![0.0; 1024];
    for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, 16.01, -16.01] {
        input.fill(0.5);
        assert_eq!(fx.process(&input, &mut output), OK);
        input[1001] = bad;
        output.fill(99.0);
        assert_eq!(fx.process(&input, &mut output), INVALID_SAMPLE);
        assert!(output.iter().all(|sample| *sample == 0.0));
        input.fill(0.0);
        assert_eq!(fx.process(&input, &mut output), OK);
        assert!(output.iter().all(|sample| *sample == 0.0));
    }
}

#[test]
fn invalid_capacity_and_pointer_shape_do_not_write_outside_prepared_bound() {
    assert!(shr_fx_v1_create(7999, 512).is_null());
    assert!(shr_fx_v1_create(192001, 512).is_null());
    assert!(shr_fx_v1_create(48000, 0).is_null());
    assert!(shr_fx_v1_create(48000, 8193).is_null());
    let mut fx = Fx::new(8000, 512);
    let input = [0.5; 1024];
    let mut output = [77.0; 1026];
    assert_eq!(fx.process(&input, &mut output[..1024]), OK);
    output.fill(77.0);
    unsafe {
        // Out-of-capacity calls are rejected before audio-pointer access.
        assert_eq!(
            shr_fx_v1_process(fx.0, ptr::null(), output.as_mut_ptr(), u32::MAX),
            CAPACITY
        );
        assert_eq!(output, [77.0; 1026]);
        assert_eq!(
            shr_fx_v1_process(fx.0, ptr::null(), output.as_mut_ptr(), 512),
            INVALID_ARGUMENT
        );
        assert_eq!(output, [77.0; 1026]);
        assert_eq!(
            shr_fx_v1_process(fx.0, output.as_ptr(), output.as_mut_ptr().add(1), 512),
            INVALID_ARGUMENT
        );
        assert_eq!(output, [77.0; 1026]);
        assert_eq!(
            shr_fx_v1_process(fx.0, input.as_ptr(), ptr::null_mut(), 512),
            INVALID_ARGUMENT
        );
        assert_eq!(shr_fx_v1_process(fx.0, ptr::null(), ptr::null_mut(), 0), OK);
        assert_eq!(
            shr_fx_v1_process(ptr::null_mut(), ptr::null(), ptr::null_mut(), 0),
            INVALID_ARGUMENT
        );
        assert_eq!(shr_fx_v1_delay_frames(ptr::null_mut()), 0);
        shr_fx_v1_reset(ptr::null_mut());
        shr_fx_v1_destroy(ptr::null_mut());
    }
    assert_eq!(fx.process(&[0.0; 1024], &mut output[..1024]), OK);
    assert!(output[..1024].iter().all(|sample| *sample == 0.0));
    assert_eq!(output[1024..], [77.0; 2]);
}

fn status(fx: &Fx) -> StatusV1 {
    let mut value = StatusV1::default();
    assert_eq!(
        unsafe { shr_fx_v1_status(fx.0, &mut value, 1, size_of::<StatusV1>() as u32) },
        OK
    );
    value
}

#[test]
fn queries_report_fixed_capabilities_and_actual_error_reset_history() {
    let mut caps = CapabilitiesV1::default();
    assert_eq!(
        unsafe { shr_fx_v1_capabilities(&mut caps, 1, size_of::<CapabilitiesV1>() as u32) },
        OK
    );
    assert_eq!(size_of::<CapabilitiesV1>(), 112);
    assert_eq!(size_of::<StatusV1>(), 40);
    assert_eq!(&caps.identity[..20], b"fx-a/fixed-delay-v1\0");
    assert_eq!((caps.min_sample_rate, caps.max_sample_rate), (8000, 192000));
    assert_eq!((caps.min_block_frames, caps.max_block_frames), (1, 8192));
    assert_eq!(
        (caps.channels, caps.sample_bits, caps.reset_supported),
        (2, 64, 1)
    );
    assert_eq!(
        (
            caps.writable_parameters,
            caps.rack_available,
            caps.adapter_buffer_frames
        ),
        (0, 0, 0)
    );
    assert_eq!(
        (caps.delay_ms, caps.feedback, caps.damping, caps.wet_gain),
        (20.0, 0.25, 0.35, 0.5)
    );
    let mut fx = Fx::new(48000, 1024);
    let initial = status(&fx);
    assert_eq!(
        (
            initial.sample_rate,
            initial.max_block_frames,
            initial.intentional_delay_frames
        ),
        (48000, 1024, 960)
    );
    assert_eq!(
        (
            initial.last_process_result,
            initial.reset_reason,
            initial.reset_count
        ),
        (0, 0, 0)
    );
    unsafe { shr_fx_v1_reset(fx.0) };
    assert_eq!((status(&fx).reset_reason, status(&fx).reset_count), (1, 1));
    assert_eq!(
        unsafe { shr_fx_v1_process(fx.0, ptr::null(), ptr::null_mut(), 1025) },
        CAPACITY
    );
    assert_eq!(
        (
            status(&fx).last_process_result,
            status(&fx).reset_reason,
            status(&fx).reset_count
        ),
        (-2, 2, 2)
    );
    assert_eq!(
        unsafe { shr_fx_v1_process(fx.0, ptr::null(), ptr::null_mut(), 1) },
        INVALID_ARGUMENT
    );
    assert_eq!(
        (
            status(&fx).last_process_result,
            status(&fx).reset_reason,
            status(&fx).reset_count
        ),
        (-1, 3, 3)
    );
    assert_eq!(fx.process(&[f64::NAN, 0.0], &mut [99.0; 2]), INVALID_SAMPLE);
    unsafe { shr_fx_v1_reset(fx.0) };
    assert_eq!(
        (
            status(&fx).last_process_result,
            status(&fx).reset_reason,
            status(&fx).reset_count
        ),
        (-3, 1, 5)
    );
    assert_eq!(fx.process(&[0.0; 2], &mut [99.0; 2]), OK);
    assert_eq!(
        (
            status(&fx).last_process_result,
            status(&fx).reset_reason,
            status(&fx).reset_count
        ),
        (0, 1, 5)
    );
}

#[test]
fn invalid_queries_leave_storage_and_dsp_exact() {
    let fx = Fx::new(48000, 1024);
    let before = status(&fx);
    let mut output = before;
    let mut caps = CapabilitiesV1::default();
    let cap_before = caps;
    unsafe {
        for (version, size) in [(0, 40), (2, 40), (1, 39), (1, 41), (1, u32::MAX)] {
            assert_eq!(
                shr_fx_v1_status(fx.0, &mut output, version, size),
                INVALID_ARGUMENT
            );
            assert_eq!(output, before);
        }
        assert_eq!(
            shr_fx_v1_status(8usize as *mut c_void, &mut output, 2, 40),
            INVALID_ARGUMENT
        );
        assert_eq!(
            shr_fx_v1_status(ptr::null_mut(), &mut output, 1, 40),
            INVALID_ARGUMENT
        );
        assert_eq!(
            shr_fx_v1_status(fx.0, ptr::null_mut(), 1, 40),
            INVALID_ARGUMENT
        );
        assert_eq!(shr_fx_v1_status(fx.0, fx.0.cast(), 1, 40), INVALID_ARGUMENT);
        let mut bytes = [0u64; 16];
        let bad = bytes.as_mut_ptr().cast::<u8>().add(1);
        assert_eq!(shr_fx_v1_status(fx.0, bad.cast(), 1, 40), INVALID_ARGUMENT);
        assert_eq!(shr_fx_v1_capabilities(bad.cast(), 1, 112), INVALID_ARGUMENT);
        assert_eq!(bytes, [0; 16]);
        assert_eq!(
            shr_fx_v1_status(fx.0, (usize::MAX - 7) as *mut StatusV1, 1, 40),
            INVALID_ARGUMENT
        );
        assert_eq!(shr_fx_v1_capabilities(&mut caps, 2, 112), INVALID_ARGUMENT);
        assert_eq!(shr_fx_v1_capabilities(&mut caps, 1, 111), INVALID_ARGUMENT);
        assert_eq!(caps, cap_before);
    }
    assert_eq!(status(&fx), before);
}
