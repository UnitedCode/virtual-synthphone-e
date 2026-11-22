//! A tighter browser input reserve must not exhaust the codec or change PCM.
#[path = "../../phone/src/streaming.rs"]
#[allow(dead_code)]
mod streaming;
use virtual_synthphone_e_engine::{reference_image, Firmware};
fn boot() -> Firmware {
    let mut f = Firmware::new(reference_image()).unwrap();
    f.handset(true).unwrap();
    while f.virtual_time_ns() < streaming::BOOT_NS {
        for _ in 0..4 {
            f.run_for(100_000, 2_000_000).unwrap();
        }
        f.take_audio();
    }
    assert_eq!(
        f.pending_input(),
        0,
        "live mic must not be captured during boot"
    );
    f
}
fn render(streamed: bool) -> Vec<f32> {
    let mut f = boot();
    let rate = f.sample_rate();
    let input: Vec<f32> = (0..32768)
        .flat_map(|i| {
            [
                0.,
                (i as f64 * std::f64::consts::TAU * 220. / rate).sin() as f32 * 0.15,
            ]
        })
        .collect();
    let mut out = Vec::new();
    let mut at = 0;
    if !streamed {
        f.input_stereo(&input).unwrap();
    }
    while out.len() < 16000 {
        if streamed && f.pending_input() < streaming::INPUT_READY {
            let end = at + streaming::PACKET_FRAMES * 2;
            f.input_stereo(&input[at..end]).unwrap();
            at = end;
        }
        let duration = if streamed {
            streaming::slice_ns(f.pending_input(), rate)
        } else {
            2_000_000
        };
        assert!(duration > 0);
        f.run_for(100_000, duration).unwrap();
        if streamed {
            assert!(f.pending_input() >= 8, "codec slice consumed its guard");
        }
        out.extend(f.take_audio());
    }
    out.truncate(16000);
    out
}
#[test]
fn live_128_frame_packets_preserve_the_real_firmware_pcm() {
    let reference = render(false);
    let actual = render(true);
    assert!(reference.iter().any(|v| v.abs() > 0.02));
    assert_eq!(
        actual, reference,
        "input packet cadence must not change the ARM instrument's output"
    );
}
