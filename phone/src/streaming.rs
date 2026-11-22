//! Live codec scheduling. Keep enough frames for a bounded CPU slice, without
//! accumulating a full packet of avoidable delay at the firmware input.
pub const PACKET_FRAMES: usize = 128;
pub const INPUT_READY: usize = 64;
pub const GUARD_FRAMES: usize = 16;
pub const PREBUFFER_SECONDS: f64 = 0.006;
pub const MAX_PLAYBACK_SECONDS: f64 = 0.04;
pub const BOOT_NS: u64 = 1_000_000_000;
pub fn slice_ns(pending: usize, rate: f64) -> u64 {
    if pending < INPUT_READY || rate <= 0. || !rate.is_finite() {
        return 0;
    }
    (((pending - GUARD_FRAMES) as f64 * 1e9 / rate).min(2_000_000.)) as u64
}
