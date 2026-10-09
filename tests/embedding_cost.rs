//! Opt-in numerical timing only; no endpoint access or live-readiness claim.
use shr_fx::c_api_v2::*;
#[test]
#[ignore = "offline cost matrix; cargo test --release --test embedding_cost -- --ignored --nocapture"]
fn aggregate_prepared_effect_cost() {
    unsafe {
        for rate in [48000, 192000] {
            for count in [1, 4, 8] {
                let mut handles = Vec::new();
                for i in 0..count {
                    let mut s = Settings::default();
                    shr_fx_v2_defaults(i % 3 + 1, &mut s, 2, size_of::<Settings>() as u32);
                    if s.algorithm != 3 {
                        s.amount = 0.9;
                    }
                    handles.push(shr_fx_v2_create(rate, 48, &s));
                }
                let input = [0.25; 96];
                let mut out = [0.0; 96];
                let mut costs = Vec::with_capacity(2000);
                for block in 0..2000 {
                    if block % 100 == 0 {
                        for (i, &h) in handles.iter().enumerate() {
                            let mut s = Settings::default();
                            shr_fx_v2_defaults(
                                (i as u32 + block / 100) % 3 + 1,
                                &mut s,
                                2,
                                size_of::<Settings>() as u32,
                            );
                            let mut st = Status::default();
                            shr_fx_v2_status(h, &mut st, 2, size_of::<Status>() as u32);
                            let p = shr_fx_v2_prepare(rate, 48, &s);
                            let r = shr_fx_v2_publish(h, p, block as u64 + 1, st.revision);
                            if r != 0 {
                                shr_fx_v2_cancel(p);
                            }
                        }
                    }
                    let start = std::time::Instant::now();
                    for &h in &handles {
                        assert_eq!(
                            shr_fx_v2_process(h, input.as_ptr(), out.as_mut_ptr(), 48),
                            0
                        );
                    }
                    costs.push(start.elapsed().as_nanos());
                    for &h in &handles {
                        shr_fx_v2_cancel(shr_fx_v2_retire(h));
                    }
                }
                costs.sort_unstable();
                println!(
                    "offline rate={rate} frames=48 instances={count}: ns p50={} p95={} p99={} max={}; NOT hardware admission",
                    costs[1000], costs[1900], costs[1980], costs[1999]
                );
                for h in handles {
                    shr_fx_v2_destroy(h);
                }
            }
        }
    }
}
