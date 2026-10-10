use shr_fx::c_api_v2::*;
use std::{ffi::c_void, ptr};
fn defaults(a: u32) -> Settings {
    let mut s = Settings::default();
    assert_eq!(
        unsafe { shr_fx_v2_defaults(a, &mut s, 2, size_of::<Settings>() as u32) },
        0
    );
    s
}
fn status(h: *mut c_void) -> Status {
    let mut s = Status::default();
    assert_eq!(
        unsafe { shr_fx_v2_status(h, &mut s, 2, size_of::<Status>() as u32) },
        0
    );
    s
}
fn render(h: *mut c_void, input: &[f64]) -> Vec<f64> {
    let mut out = vec![99.0; input.len()];
    assert_eq!(
        unsafe {
            shr_fx_v2_process(
                h,
                input.as_ptr(),
                out.as_mut_ptr(),
                (input.len() / 2) as u32,
            )
        },
        0
    );
    out
}
#[test]
fn delay_independent_samples_partitions_precision_and_feedback() {
    unsafe {
        let mut s = defaults(1);
        s.time_ms = 10.0;
        s.damping = 0.0;
        s.gain = 1.0;
        s.amount = 0.5;
        let h = shr_fx_v2_create(8000, 8192, &s);
        let split = shr_fx_v2_create(8000, 8192, &s);
        let mut input = vec![0.0; 1600];
        input[0] = 0.5 + 2f64.powi(-40);
        input[3] = -0.25;
        let out = render(h, &input);
        for n in 0..800 {
            for c in 0..2 {
                let expected = if c == 0 && n >= 80 && n % 80 == 0 {
                    input[0] * 0.5f64.powi((n / 80 - 1) as i32)
                } else if c == 1 && n >= 81 && (n - 1) % 80 == 0 {
                    -0.25 * 0.5f64.powi(((n - 1) / 80 - 1) as i32)
                } else {
                    0.0
                };
                assert_eq!(out[n * 2 + c], expected)
            }
        }
        let actual: Vec<_> = input.chunks(14).flat_map(|x| render(split, x)).collect();
        assert_eq!(out, actual);
        shr_fx_v2_destroy(h);
        shr_fx_v2_destroy(split);
    }
}
#[test]
fn requests_backpressure_cancel_transition_reset_and_fault_isolation() {
    unsafe {
        let s = defaults(1);
        let h = shr_fx_v2_create(8000, 512, &s);
        let healthy = shr_fx_v2_create(8000, 512, &s);
        let mut t = s;
        t.time_ms = 1.0;
        let p = shr_fx_v2_prepare(8000, 512, &t);
        assert_eq!(shr_fx_v2_publish(h, p, 1, 1), -4);
        assert_eq!(status(h).current, s);
        assert_eq!(shr_fx_v2_publish(h, p, 1, 0), 0);
        assert_eq!(status(h).target, t);
        assert_eq!(status(h).revision, 0);
        let other = shr_fx_v2_prepare(8000, 512, &s);
        assert_eq!(shr_fx_v2_publish(h, other, 2, 0), -2);
        render(h, &[0.0; 320]);
        assert_eq!(status(h).revision, 1);
        assert_eq!(status(h).applied_request, 1);
        assert_eq!(status(h).last_request, 2);
        assert_eq!(status(h).request_result, -2);
        assert_eq!(status(h).request_state, 5);
        assert_eq!(shr_fx_v2_publish(h, other, 2, 1), -2);
        shr_fx_v2_cancel(shr_fx_v2_retire(h));
        assert_eq!(shr_fx_v2_publish(h, other, 1, 1), -5);
        assert_eq!(shr_fx_v2_publish(h, other, 2, 1), 0);
        assert_eq!(shr_fx_v2_reset(h), 0);
        shr_fx_v2_cancel(shr_fx_v2_retire(h));
        assert_eq!(status(h).current, t);
        let mut x = [0.0; 400];
        x[0] = 1.0;
        assert!(render(h, &x).iter().any(|x| *x != 0.0));
        assert!(render(healthy, &x).iter().any(|x| *x != 0.0));
        let mut out = [9.0; 2];
        assert_eq!(
            shr_fx_v2_process(h, [f64::NAN, 0.0].as_ptr(), out.as_mut_ptr(), 1),
            -3
        );
        assert_eq!(out, [0.0; 2]);
        assert_eq!(shr_fx_v2_process(h, ptr::null(), ptr::null_mut(), 0), 0);
        assert_eq!(status(h).fault, 1);
        assert!(render(healthy, &[0.0; 400]).iter().any(|x| *x != 0.0));
        shr_fx_v2_reset(h);
        assert!(render(h, &[0.0; 400]).iter().all(|x| *x == 0.0));
        shr_fx_v2_destroy(h);
        shr_fx_v2_destroy(healthy);
    }
}
#[test]
fn room_and_chorus_native_precision_determinism_bypass_tails() {
    unsafe {
        for a in [2, 3] {
            let mut s = defaults(a);
            s.gain = 1.0;
            let h = shr_fx_v2_create(8000, 8192, &s);
            let twin = shr_fx_v2_create(8000, 8192, &s);
            let mut input = vec![0.0; 8192];
            input[0] = 0.5 + 2f64.powi(-40);
            let out = render(h, &input);
            assert_eq!(out, render(twin, &input));
            assert!(out.iter().all(|x| x.is_finite()));
            assert!(out.chunks_exact(2).all(|x| x[1] == 0.0));
            assert!(out.iter().any(|x| *x != 0.0 && *x != (*x as f32) as f64));
            shr_fx_v2_reset(h);
            shr_fx_v2_reset(twin);
            render(h, &[1.0, 0.0]);
            render(twin, &[1.0, 0.0]);
            s.bypass = 1;
            let p = shr_fx_v2_prepare(8000, 8192, &s);
            assert_eq!(shr_fx_v2_publish(h, p, 1, 0), 0);
            let tail = render(h, &[0.0; 8192]);
            assert_eq!(tail, render(twin, &[0.0; 8192]));
            assert!(tail.iter().any(|x| *x != 0.0));
            shr_fx_v2_cancel(shr_fx_v2_retire(h));
            shr_fx_v2_destroy(h);
            shr_fx_v2_destroy(twin);
        }
    }
}
#[test]
fn malformed_shapes_settings_and_in_place() {
    unsafe {
        let mut s = defaults(3);
        s.amount = 20.0;
        assert_eq!(shr_fx_v2_validate(&s), -6);
        assert!(shr_fx_v2_prepare(8000, 10, &s).is_null());
        s = defaults(1);
        let h = shr_fx_v2_create(8000, 10, &s);
        let mut out = [4.0; 22];
        assert_eq!(shr_fx_v2_process(h, ptr::null(), out.as_mut_ptr(), 11), -2);
        assert_eq!(out, [4.0; 22]);
        assert_eq!(
            shr_fx_v2_process(h, out.as_ptr(), out.as_mut_ptr().add(1), 10),
            -1
        );
        assert_eq!(out, [4.0; 22]);
        assert_eq!(
            shr_fx_v2_status(h, h.cast(), 2, size_of::<Status>() as u32),
            -1
        );
        assert_eq!(shr_fx_v2_process(h, out.as_ptr(), out.as_mut_ptr(), 10), 0);
        assert!(out[..20].iter().all(|x| *x == 0.0));
        shr_fx_v2_destroy(h);
    }
}

#[test]
fn modulation_matches_independent_parabolic_fractional_tap_reference() {
    unsafe {
        let s = defaults(3);
        let h = shr_fx_v2_create(8000, 8192, &s);
        let mut input = vec![0.0; 4000];
        input[0] = 0.5 + 2f64.powi(-40);
        let out = render(h, &input);
        let mut phase = s.seed as f64 / 4294967296.0;
        for n in 0..2000 {
            phase += s.rate_hz / 8000.0;
            if phase >= 1.0 {
                phase -= 1.0
            }
            let x = phase * 2.0 - 1.0;
            let osc = 4.0 * x * (1.0 - x.abs());
            let delay = (s.time_ms + s.amount * osc) * 8.0;
            let integral = delay as usize;
            let tap = |d: usize| {
                if d > 0 && d <= n {
                    input[(n - d) * 2]
                } else {
                    0.0
                }
            };
            let expected = (tap(integral)
                + (tap(integral + 1) - tap(integral)) * (delay - integral as f64))
                * s.gain;
            assert!((out[n * 2] - expected).abs() < 1e-17);
        }
        shr_fx_v2_destroy(h);
    }
}

#[test]
fn transitions_use_source_frames_and_interruption_reports_current_exactly() {
    unsafe {
        let s = defaults(1);
        let h = shr_fx_v2_create(8000, 512, &s);
        let mut target = s;
        target.time_ms = 1.0;
        let p = shr_fx_v2_prepare(8000, 512, &target);
        assert_eq!(shr_fx_v2_publish(h, p, 10, 0), 0);
        render(h, &[0.0; 158]);
        assert_eq!(status(h).current, s);
        assert_eq!(status(h).request_state, 1);
        render(h, &[0.0; 2]);
        assert_eq!(status(h).current, target);
        assert_eq!(status(h).request_state, 2);
        assert_eq!(status(h).applied_request, 0);
        assert_eq!(shr_fx_v2_reset(h), 0);
        assert_eq!(status(h).request_state, 4);
        assert_eq!(status(h).current, target);
        shr_fx_v2_cancel(shr_fx_v2_retire(h));
        let p = shr_fx_v2_prepare(44100, 512, &s);
        assert_eq!(shr_fx_v2_publish(h, p, 11, 10), -6);
        assert_eq!(status(h).last_request, 11);
        assert_eq!(status(h).request_state, 5);
        shr_fx_v2_cancel(p);
        shr_fx_v2_destroy(h);
    }
}

#[test]
fn finite_above_unity_input_and_extreme_settings_stay_supported() {
    unsafe {
        for algorithm in [1, 2, 3] {
            let mut s = defaults(algorithm);
            s.gain = 1.0;
            if algorithm == 1 {
                s.amount = 0.9;
                s.time_ms = 1.0;
            }
            if algorithm == 2 {
                s.amount = 0.9;
                s.damping = 1.0;
            }
            if algorithm == 3 {
                s.time_ms = 30.0;
                s.amount = 9.0;
                s.rate_hz = 10.0;
            }
            for rate in [8000, 48000, 192000] {
                let h = shr_fx_v2_create(rate, 8192, &s);
                assert!(!h.is_null());
                let input = [16.0; 16384];
                let out = render(h, &input);
                assert!(out.iter().all(|x| x.is_finite() && x.abs() <= 256.0));
                assert_eq!(status(h).fault, 0);
                shr_fx_v2_destroy(h);
            }
        }
    }
}
