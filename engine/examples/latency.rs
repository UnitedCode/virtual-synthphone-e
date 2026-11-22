//! Measures intrinsic ARM firmware delay, without browser or physical devices.
//! This is a component check; use the phone's acoustic test for real round trip.
#[path = "../../phone/src/latency.rs"]
mod latency;
use virtual_synthphone_e_engine::{reference_image, Firmware};
fn advance(f: &mut Firmware, milliseconds: u64) {
    let until = f.virtual_time_ns() + milliseconds * 1_000_000;
    while f.virtual_time_ns() < until {
        f.run_for(100_000, 2_000_000).unwrap();
        f.take_audio();
    }
}
fn main() {
    let profile: usize = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "2".into())
        .parse()
        .unwrap();
    assert!(profile <= 5);
    let mut f = Firmware::new(reference_image()).unwrap();
    f.handset(true).unwrap();
    advance(&mut f, 800);
    for _ in 0..profile {
        f.key(10, true).unwrap();
        advance(&mut f, 40);
        f.key(10, false).unwrap();
        advance(&mut f, 40);
    }
    advance(&mut f, 200);
    let rate = f.sample_rate();
    let reference = latency::probe(rate);
    let mut output = Vec::new();
    let mut cursor = 0;
    while output.len() < reference.len() {
        if f.pending_input() < 256 {
            let samples: Vec<f32> = (cursor..cursor + 256)
                .flat_map(|i| [0., reference.get(i).copied().unwrap_or(0.)])
                .collect();
            cursor += 256;
            f.input_stereo(&samples).unwrap();
        }
        f.run_for(100_000, 2_000_000).unwrap();
        output.extend(f.take_audio());
    }
    output.truncate(reference.len());
    let result = latency::measure(&reference, &reference, &output, rate, 0);
    println!("Intrinsic firmware only; profile={profile}, sample_rate={rate}, result={result:?}");
    if let Ok(result) = result {
        println!(
            "firmware_ms={:.2}, spread_ms={:.2}, confidence={:.3}",
            result.total_ms(),
            result.spread_ms,
            result.confidence
        );
    } else {
        std::process::exit(1);
    }
}
