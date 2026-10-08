use shr_fx::{
    c_api::{CAPACITY, INVALID_ARGUMENT, INVALID_SAMPLE, OK},
    c_api_v2::*,
};
use std::{ffi::c_void, ptr};

struct Fx(*mut c_void);
impl Fx {
    fn new(rate: u32, block: u32) -> Self {
        let raw = shr_fx_v2_create(rate, block);
        assert!(!raw.is_null());
        Self(raw)
    }
    fn status(&self) -> StatusV2 {
        let mut s = StatusV2::default();
        assert_eq!(
            unsafe { shr_fx_v2_status(self.0, &mut s, 2, size_of::<StatusV2>() as u32) },
            OK
        );
        s
    }
    fn config(&self, generation: u64) -> ConfigV2 {
        let s = self.status();
        ConfigV2 {
            version: 2,
            size: size_of::<ConfigV2>() as u32,
            expected_generation: s.applied_generation,
            generation,
            channel: s.target,
        }
    }
    fn apply(&self, config: &ConfigV2) -> i32 {
        let token = unsafe {
            shr_fx_v2_prepare(
                config,
                self.status().sample_rate,
                2,
                size_of::<ConfigV2>() as u32,
            )
        };
        assert!(!token.is_null());
        let result = unsafe { shr_fx_v2_commit(self.0, token, self.status().next_source_frame) };
        unsafe { shr_fx_v2_retire(token) };
        result
    }
    fn process(&self, input: &[f64]) -> Vec<f64> {
        assert_eq!(input.len() % 2, 0);
        let mut output = vec![99.0; input.len()];
        let block = self.status().max_block_frames as usize * 2;
        for (i, o) in input.chunks(block).zip(output.chunks_mut(block)) {
            assert_eq!(
                unsafe {
                    shr_fx_v2_process(
                        self.0,
                        i.as_ptr(),
                        o.as_mut_ptr(),
                        (i.len() / 2) as u32,
                        self.status().next_source_frame,
                    )
                },
                OK
            );
        }
        output
    }
    fn reset(&self, frame: u64) {
        assert_eq!(unsafe { shr_fx_v2_reset(self.0, frame) }, OK);
    }
}
impl Drop for Fx {
    fn drop(&mut self) {
        unsafe { shr_fx_v2_destroy(self.0) };
    }
}

#[test]
fn layout_identity_bounds_and_no_change_generation_are_truthful() {
    assert_eq!(size_of::<ChannelV2>(), 40);
    assert_eq!(size_of::<ConfigV2>(), 104);
    assert_eq!(size_of::<CapabilitiesV2>(), 128);
    assert_eq!(size_of::<StatusV2>(), 168);
    let mut caps = CapabilitiesV2::default();
    assert_eq!(unsafe { shr_fx_v2_capabilities(&mut caps, 2, 128) }, OK);
    assert_eq!(&caps.identity[..23], b"fx-a/prepared-delay-v2\0");
    assert_eq!(
        (
            caps.channels,
            caps.sample_bits,
            caps.rack_available,
            caps.adapter_buffer_frames
        ),
        (2, 64, 0, 0)
    );
    assert_eq!(
        (
            caps.transition_ms,
            caps.max_read_heads_per_channel,
            caps.max_delay_storage_bytes
        ),
        (20, 2, 1536064)
    );
    for (r, b) in [(7999, 1), (192001, 1), (48000, 0), (48000, 8193)] {
        assert!(shr_fx_v2_create(r, b).is_null());
    }
    let fx = Fx::new(48000, 8192);
    let before = fx.status();
    assert_eq!(before.target, [ChannelV2::default(); 2]);
    assert_eq!(fx.apply(&fx.config(1)), OK);
    let after = fx.status();
    assert_eq!(
        (
            after.applied_generation,
            after.settled_generation,
            after.transitioning_mask
        ),
        (1, 1, 0)
    );
    assert_eq!(after.settled_source_frame, 0);
}

#[test]
fn independent_fractional_onset_and_native_precision_at_rate_bounds() {
    for rate in [8000, 8001, 44100, 48000, 96000, 192000] {
        let fx = Fx::new(rate, 8192);
        let mut c = fx.config(1);
        c.channel = [
            ChannelV2 {
                delay_ms: 1.0,
                feedback: 0.0,
                damping: 0.0,
                wet_gain: 1.0,
                ..Default::default()
            },
            ChannelV2 {
                delay_ms: 2.0,
                feedback: 0.0,
                damping: 0.0,
                wet_gain: 0.5,
                ..Default::default()
            },
        ];
        assert_eq!(fx.apply(&c), OK);
        fx.reset(0);
        let mut input = vec![0.0; (rate as usize / 100 + 2) * 2];
        input[0] = 0.5 + 2f64.powi(-40);
        input[1] = -0.25 - 2f64.powi(-42);
        let out = fx.process(&input);
        for ch in 0..2 {
            let delay = c.channel[ch].delay_ms * rate as f64 / 1000.0;
            let n = delay as usize;
            let fraction = delay.fract();
            assert!(out.chunks_exact(2).take(n).all(|x| x[ch] == 0.0));
            assert_eq!(
                out[n * 2 + ch],
                input[ch] * (1.0 - fraction) * c.channel[ch].wet_gain
            );
            assert_eq!(
                out[(n + 1) * 2 + ch],
                input[ch] * fraction * c.channel[ch].wet_gain
            );
        }
        if rate == 48000 {
            assert_ne!(out[96], (input[0] as f32) as f64);
        }
    }
}

#[test]
fn maximum_delay_and_independent_feedback_are_not_dry_or_crossfed() {
    for rate in [8000, 8001, 48000, 192000] {
        let fx = Fx::new(rate, 8192);
        let mut c = fx.config(1);
        c.channel[0] = ChannelV2 {
            delay_ms: 500.0,
            feedback: 0.5,
            damping: 0.0,
            wet_gain: 1.0,
            ..Default::default()
        };
        c.channel[1] = ChannelV2 {
            delay_ms: 1.0,
            feedback: 0.85,
            damping: 0.99,
            wet_gain: 1.0,
            ..Default::default()
        };
        assert_eq!(fx.apply(&c), OK);
        fx.reset(0);
        let mut input = vec![0.0; (rate as usize + 4) * 2];
        input[0] = 0.5 + 2f64.powi(-40);
        let out = fx.process(&input);
        let delay = rate as f64 / 2.0;
        let n = delay as usize;
        assert!(out.chunks_exact(2).all(|x| x[1] == 0.0));
        assert!(out[..n * 2].iter().all(|x| *x == 0.0));
        assert_eq!(out[n * 2], input[0] * (1.0 - delay.fract()));
        if rate % 2 == 0 {
            assert_eq!(out[rate as usize * 2], input[0] * 0.5);
        }
        assert!(out.iter().all(|x| x.is_finite()));
    }
}

#[test]
fn changed_channel_crossfade_and_panic_preserve_other_history_bit_exactly() {
    let changed = Fx::new(48000, 8192);
    let reference = Fx::new(48000, 8192);
    let input: Vec<_> = (0..2048)
        .flat_map(|n| {
            [
                if n % 61 == 0 { 0.7 } else { 0.0 },
                if n % 47 == 0 { -0.4 } else { 0.0 },
            ]
        })
        .collect();
    assert_eq!(changed.process(&input), reference.process(&input));
    let mut c = changed.config(1);
    c.channel[0].delay_ms = 3.75;
    c.channel[0].feedback = 0.85;
    c.channel[0].wet_gain = 0.8;
    assert_eq!(changed.apply(&c), OK);
    let s = changed.status();
    assert_eq!(
        (
            s.applied_generation,
            s.settled_generation,
            s.applied_source_frame,
            s.transitioning_mask,
            s.remaining_frames
        ),
        (1, 0, 2048, 1, [960, 0])
    );
    let zero = vec![0.0; 959 * 2];
    let a = changed.process(&zero);
    let b = reference.process(&zero);
    assert!(
        a.chunks_exact(2)
            .zip(b.chunks_exact(2))
            .all(|(a, b)| a[1].to_bits() == b[1].to_bits())
    );
    assert_eq!(changed.status().remaining_frames, [1, 0]);
    let a = changed.process(&[0.0; 2]);
    let b = reference.process(&[0.0; 2]);
    assert_eq!(a[1].to_bits(), b[1].to_bits());
    let s = changed.status();
    assert_eq!(
        (
            s.settled_generation,
            s.settled_source_frame,
            s.transitioning_mask
        ),
        (1, 3008, 0)
    );
    assert_eq!(unsafe { shr_fx_v2_panic(changed.0, 1) }, OK);
    let a = changed.process(&vec![0.0; 4096]);
    let b = reference.process(&vec![0.0; 4096]);
    assert!(a.chunks_exact(2).all(|x| x[0] == 0.0));
    assert!(
        a.chunks_exact(2)
            .zip(b.chunks_exact(2))
            .all(|(a, b)| a[1].to_bits() == b[1].to_bits())
    );
    assert!(a.chunks_exact(2).any(|x| x[1] != 0.0));
}

#[test]
fn delay_edit_uses_exact_bounded_two_head_weights_and_retains_tail() {
    let fx = Fx::new(8000, 8192);
    let mut c = fx.config(1);
    c.channel[0] = ChannelV2 {
        delay_ms: 1.0,
        feedback: 0.0,
        damping: 0.0,
        wet_gain: 1.0,
        ..Default::default()
    };
    assert_eq!(fx.apply(&c), OK);
    fx.reset(0);
    let mut impulse = vec![0.0; 8 * 2];
    impulse[0] = 1.0;
    fx.process(&impulse);
    c = fx.config(2);
    c.channel[0].delay_ms = 2.0;
    assert_eq!(fx.apply(&c), OK);
    let out = fx.process(&vec![0.0; 160 * 2]);
    assert_eq!(out[0], 1.0 - 1.0 / 160.0);
    assert_eq!(out[16], 9.0 / 160.0);
    assert!(
        out.chunks_exact(2)
            .enumerate()
            .all(|(n, x)| n == 0 || n == 8 || x[0] == 0.0)
    );
    assert_eq!(fx.status().settled_source_frame, 168);
}

#[test]
fn bypass_fades_excitation_then_drains_without_dry_and_resume_is_bounded() {
    let fx = Fx::new(8000, 8192);
    let mut c = fx.config(1);
    c.channel[0] = ChannelV2 {
        delay_ms: 1.0,
        feedback: 0.0,
        damping: 0.0,
        wet_gain: 1.0,
        ..Default::default()
    };
    assert_eq!(fx.apply(&c), OK);
    fx.reset(0);
    fx.process(&vec![1.0; 200 * 2]);
    c = fx.config(2);
    c.channel[0].bypass = 1;
    assert_eq!(fx.apply(&c), OK);
    let out = fx.process(&vec![1.0; 160 * 2]);
    assert_eq!(out[8 * 2], 159.0 / 160.0);
    assert_eq!(fx.status().settled_source_frame, 360);
    let out = fx.process(&vec![16.0; 160 * 2]);
    assert!(out.chunks_exact(2).take(7).any(|x| x[0] != 0.0));
    assert!(out.chunks_exact(2).skip(8).all(|x| x[0] == 0.0));
    c = fx.config(3);
    c.channel[0].bypass = 0;
    assert_eq!(fx.apply(&c), OK);
    let out = fx.process(&vec![1.0; 168 * 2]);
    assert!(out.chunks_exact(2).take(8).all(|x| x[0] == 0.0));
    assert_eq!(out[16], 1.0 / 160.0);
    assert_eq!(out[167 * 2], 1.0);
}

#[test]
fn bypass_settlement_is_not_tail_silence_and_panic_is_not_permanent_mute() {
    let fx = Fx::new(8000, 8192);
    let mut c = fx.config(1);
    c.channel[0] = ChannelV2 {
        delay_ms: 50.0,
        feedback: 0.85,
        damping: 0.35,
        wet_gain: 1.0,
        ..Default::default()
    };
    assert_eq!(fx.apply(&c), OK);
    fx.reset(0);
    let mut input = vec![0.0; 10 * 2];
    input[0] = 1.0;
    fx.process(&input);
    c = fx.config(2);
    c.channel[0].bypass = 1;
    assert_eq!(fx.apply(&c), OK);
    fx.process(&vec![0.0; 160 * 2]);
    assert_eq!(fx.status().settled_generation, 2);
    let out = fx.process(&vec![16.0; 500 * 2]);
    assert_eq!(out[(400 - 170) * 2], 1.0);
    assert_eq!(unsafe { shr_fx_v2_panic(fx.0, 1) }, OK);
    assert!(
        fx.process(&vec![16.0; 1200 * 2])
            .chunks_exact(2)
            .all(|x| x[0] == 0.0)
    );
    c = fx.config(3);
    c.channel[0].bypass = 0;
    assert_eq!(fx.apply(&c), OK);
    fx.reset(10000);
    input.fill(0.0);
    input[0] = 1.0;
    fx.process(&input);
    let out = fx.process(&vec![0.0; 500 * 2]);
    assert_eq!(out[(400 - 10) * 2], 1.0);
}

#[test]
fn prepared_lifetime_busy_stale_rate_and_source_refusals_leave_state_exact() {
    let fx = Fx::new(48000, 8192);
    let mut c = fx.config(1);
    c.channel[0].delay_ms = 1.0;
    assert_eq!(fx.apply(&c), OK);
    let before = fx.status();
    c = fx.config(2);
    c.channel[1].delay_ms = 2.0;
    assert_eq!(fx.apply(&c), BUSY);
    assert_eq!(fx.status(), before);
    unsafe {
        let p = shr_fx_v2_prepare(&c, 8000, 2, 104);
        assert!(!p.is_null());
        assert_eq!(shr_fx_v2_commit(fx.0, p, 0), INVALID_ARGUMENT);
        shr_fx_v2_retire(p);
        let p = shr_fx_v2_prepare(&c, 48000, 2, 104);
        assert!(!p.is_null());
        assert_eq!(shr_fx_v2_commit(fx.0, p, 99), TIMELINE);
        shr_fx_v2_retire(p);
    }
    assert_eq!(fx.status(), before);
    fx.process(&vec![0.0; 960 * 2]);
    c.expected_generation = 0;
    assert_eq!(fx.apply(&c), STALE);
    c.expected_generation = 1;
    assert_eq!(fx.apply(&c), OK); // fresh token, retired before render
    fx.process(&vec![0.0; 960 * 2]);
    assert_eq!(fx.status().settled_generation, 2);
    fx.reset(50000);
    let before = fx.status();
    let mut output = [99.0; 4];
    assert_eq!(
        unsafe { shr_fx_v2_process(fx.0, [0.0; 4].as_ptr(), output.as_mut_ptr(), 2, 1) },
        TIMELINE
    );
    assert_eq!(output, [99.0; 4]);
    assert_eq!(fx.status(), before);
    fx.reset(u64::MAX - 1);
    let before = fx.status();
    assert_eq!(
        unsafe {
            shr_fx_v2_process(
                fx.0,
                [0.0; 4].as_ptr(),
                output.as_mut_ptr(),
                2,
                u64::MAX - 1,
            )
        },
        TIMELINE
    );
    assert_eq!(fx.status(), before);
    assert_eq!(output, [99.0; 4]);
    assert_eq!(
        unsafe { shr_fx_v2_process(fx.0, ptr::null(), ptr::null_mut(), 0, u64::MAX - 1) },
        OK
    );
    assert_eq!(fx.status().next_source_frame, u64::MAX - 1);
}

#[test]
fn whole_configuration_validation_and_invalid_shapes_never_half_apply() {
    let fx = Fx::new(48000, 8192);
    let base = fx.config(1);
    let before = fx.status();
    let mut bads = vec![];
    for bad in [f64::NAN, f64::INFINITY, -1.0, 500.01] {
        let mut c = base;
        c.channel[1].delay_ms = bad;
        bads.push(c);
    }
    for (field, bad) in [(0, 0.851), (1, 0.991), (2, 1.001), (0, f64::NEG_INFINITY)] {
        let mut c = base;
        match field {
            0 => c.channel[1].feedback = bad,
            1 => c.channel[1].damping = bad,
            _ => c.channel[1].wet_gain = bad,
        }
        bads.push(c);
    }
    let mut c = base;
    c.channel[1].reserved = 1;
    bads.push(c);
    c = base;
    c.channel[1].bypass = 2;
    bads.push(c);
    c = base;
    c.generation = 0;
    bads.push(c);
    c = base;
    c.version = 1;
    bads.push(c);
    c = base;
    c.size = 103;
    bads.push(c);
    for mut c in bads {
        c.channel[0].delay_ms = 2.0;
        assert!(unsafe { shr_fx_v2_prepare(&c, 48000, 2, 104) }.is_null());
        assert_eq!(fx.status(), before);
    }
    unsafe {
        assert!(shr_fx_v2_prepare(ptr::null(), 48000, 2, 104).is_null());
        assert!(shr_fx_v2_prepare(ptr::dangling::<ConfigV2>(), 48000, 1, 104).is_null());
        assert!(shr_fx_v2_prepare((usize::MAX - 7) as *const ConfigV2, 48000, 2, 104).is_null());
        let mut s = before;
        assert_eq!(
            shr_fx_v2_status(8usize as *mut c_void, &mut s, 1, 168),
            INVALID_ARGUMENT
        );
        for (v, n) in [(1, 168), (2, 167), (2, 169)] {
            assert_eq!(shr_fx_v2_status(fx.0, &mut s, v, n), INVALID_ARGUMENT);
            assert_eq!(s, before);
        }
        assert_eq!(shr_fx_v2_commit(fx.0, fx.0, 0), INVALID_ARGUMENT);
        assert_eq!(
            shr_fx_v2_status(fx.0, fx.0.cast(), 2, 168),
            INVALID_ARGUMENT
        );
        assert_eq!(shr_fx_v2_panic(fx.0, 0), INVALID_ARGUMENT);
        assert_eq!(shr_fx_v2_panic(fx.0, 4), INVALID_ARGUMENT);
        shr_fx_v2_destroy(ptr::null_mut());
        shr_fx_v2_retire(ptr::null_mut());
    }
    assert_eq!(fx.status(), before);
}

#[test]
fn pointer_capacity_sample_faults_clear_and_recover_without_cursor_advance() {
    let fx = Fx::new(8000, 512);
    fx.process(&[0.5; 1024]);
    let mut out = [99.0; 1026];
    let cursor = fx.status().next_source_frame;
    unsafe {
        assert_eq!(
            shr_fx_v2_process(fx.0, ptr::null(), out.as_mut_ptr(), 513, cursor),
            CAPACITY
        );
        assert_eq!(out, [99.0; 1026]);
        assert_eq!(
            shr_fx_v2_process(fx.0, out.as_ptr(), out.as_mut_ptr().add(1), 512, cursor),
            INVALID_ARGUMENT
        );
        assert_eq!(out, [99.0; 1026]);
        assert_eq!(
            shr_fx_v2_process(fx.0, fx.0.cast(), out.as_mut_ptr(), 1, cursor),
            INVALID_ARGUMENT
        );
        assert_eq!(out, [99.0; 1026]);
        assert_eq!(
            shr_fx_v2_process(fx.0, ptr::null(), ptr::null_mut(), 0, cursor),
            OK
        );
    }
    for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, 16.01, -16.01] {
        let mut input = [0.0; 1024];
        input[1001] = bad;
        out.fill(99.0);
        assert_eq!(
            unsafe { shr_fx_v2_process(fx.0, input.as_ptr(), out.as_mut_ptr(), 512, cursor) },
            INVALID_SAMPLE
        );
        assert!(out[..1024].iter().all(|x| *x == 0.0));
        assert_eq!(&out[1024..], &[99.0; 2]);
        assert_eq!(fx.status().next_source_frame, cursor);
    }
    assert!(fx.process(&[0.0; 1024]).iter().all(|x| *x == 0.0));
    assert_eq!(fx.status().reset_reason, 4);
    assert_eq!(fx.status().last_process_result, OK);
}

#[test]
fn reset_snaps_accepted_targets_and_in_place_blocking_is_deterministic() {
    let a = Fx::new(48000, 8192);
    let b = Fx::new(48000, 37);
    for fx in [&a, &b] {
        let mut c = fx.config(1);
        c.channel[0].delay_ms = 2.75;
        c.channel[1].bypass = 1;
        assert_eq!(fx.apply(&c), OK);
        fx.reset(123);
        let s = fx.status();
        assert_eq!(
            (
                s.applied_generation,
                s.settled_generation,
                s.settled_source_frame,
                s.transitioning_mask
            ),
            (1, 1, 123, 0)
        );
    }
    let mut input: Vec<_> = (0..4000)
        .flat_map(|n| [if n % 41 == 0 { 0.2 } else { 0.0 }, -0.3])
        .collect();
    let expected = a.process(&input);
    for block in input.chunks_mut(74) {
        assert_eq!(
            unsafe {
                shr_fx_v2_process(
                    b.0,
                    block.as_ptr(),
                    block.as_mut_ptr(),
                    (block.len() / 2) as u32,
                    b.status().next_source_frame,
                )
            },
            OK
        );
    }
    assert_eq!(input, expected);
    assert!(input.chunks_exact(2).all(|x| x[1] == 0.0));
}

#[test]
fn f64_coefficients_and_linear_return_ramp_reach_exact_targets() {
    let fx = Fx::new(8000, 8192);
    let mut c = fx.config(1);
    c.channel[0] = ChannelV2 {
        delay_ms: 1.0,
        feedback: 0.25 + 2f64.powi(-40),
        damping: 0.35 + 2f64.powi(-42),
        wet_gain: 0.5 + 2f64.powi(-44),
        ..Default::default()
    };
    assert_eq!(fx.apply(&c), OK);
    fx.reset(0);
    let mut input = vec![0.0; 24 * 2];
    input[0] = 0.5 + 2f64.powi(-40);
    let out = fx.process(&input);
    assert_eq!(out[16], input[0] * c.channel[0].wet_gain);
    let repeated =
        input[0] * (1.0 - c.channel[0].damping) * c.channel[0].feedback * c.channel[0].wet_gain;
    assert_eq!(out[32], repeated);
    let rounded = input[0]
        * (1.0 - (c.channel[0].damping as f32) as f64)
        * (c.channel[0].feedback as f32) as f64
        * (c.channel[0].wet_gain as f32) as f64;
    assert_ne!(out[32], rounded);
    c = fx.config(2);
    c.channel[0].feedback = 0.0;
    c.channel[0].damping = 0.0;
    c.channel[0].wet_gain = 1.0;
    assert_eq!(fx.apply(&c), OK);
    fx.reset(0);
    fx.process(&[1.0; 400]);
    c = fx.config(3);
    c.channel[0].wet_gain = 0.0;
    assert_eq!(fx.apply(&c), OK);
    let out = fx.process(&[1.0; 320]);
    assert_eq!(out[0], 1.0 - 1.0 / 160.0);
    assert_eq!(out[158 * 2], 1.0 - 159.0 / 160.0);
    assert_eq!(out[159 * 2], 0.0);
    assert_eq!(fx.status().remaining_frames, [0, 0]);
}

#[test]
fn fractional_rate_transition_finishes_exactly_and_split_blocks_match() {
    let whole = Fx::new(8001, 8192);
    let split = Fx::new(8001, 7);
    for fx in [&whole, &split] {
        let mut c = fx.config(1);
        c.channel[0].delay_ms = 500.0;
        c.channel[1].bypass = 1;
        assert_eq!(fx.apply(&c), OK);
        assert_eq!(fx.status().remaining_frames, [161, 161]);
    }
    let input = vec![0.25; 161 * 2];
    assert_eq!(whole.process(&input), split.process(&input));
    assert_eq!(whole.status().settled_source_frame, 161);
    assert_eq!(split.status().settled_source_frame, 161);
    let mut output = [99.0; 2];
    whole.reset(u64::MAX - 1);
    assert_eq!(
        unsafe {
            shr_fx_v2_process(
                whole.0,
                [0.0; 2].as_ptr(),
                output.as_mut_ptr(),
                1,
                u64::MAX - 1,
            )
        },
        OK
    );
    assert_eq!(whole.status().next_source_frame, u64::MAX);
    let before = whole.status();
    output.fill(99.0);
    assert_eq!(
        unsafe { shr_fx_v2_process(whole.0, [0.0; 2].as_ptr(), output.as_mut_ptr(), 1, u64::MAX) },
        TIMELINE
    );
    assert_eq!(whole.status(), before);
    assert_eq!(output, [99.0; 2]);
}

#[test]
fn changed_commit_refuses_unsettleable_timeline_but_no_change_is_immediate() {
    let fx = Fx::new(48000, 8192);
    fx.reset(u64::MAX - 959);
    let mut c = fx.config(1);
    c.channel[0].delay_ms = 1.0;
    let before = fx.status();
    assert_eq!(fx.apply(&c), TIMELINE);
    assert_eq!(fx.status(), before);
    let no_change = fx.config(1);
    assert_eq!(fx.apply(&no_change), OK);
    assert_eq!(fx.status().settled_generation, 1);
    assert_eq!(fx.status().settled_source_frame, u64::MAX - 959);
    fx.reset(u64::MAX - 960);
    c = fx.config(2);
    c.channel[0].delay_ms = 1.0;
    assert_eq!(fx.apply(&c), OK);
    fx.process(&vec![0.0; 960 * 2]);
    assert_eq!(fx.status().settled_source_frame, u64::MAX);
    assert_eq!(fx.status().next_source_frame, u64::MAX);
}
