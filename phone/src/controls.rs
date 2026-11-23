//! Convert browser input into hardware switch edges and encoder steps.
use std::collections::VecDeque;
use std::f64::consts::{PI, TAU};
const MIN_EDGE_GAP: u64 = 16_000_000;
pub const ENCODER_DEGREES_PER_STEP: i32 = 15;

pub struct EncoderTurn {
    center: (f64, f64),
    dead_radius_squared: f64,
    angle: Option<f64>,
    remainder: f64,
}

impl EncoderTurn {
    pub fn new(center: (f64, f64), dead_radius: f64, start: (f64, f64)) -> Self {
        let mut turn = Self {
            center,
            dead_radius_squared: dead_radius * dead_radius,
            angle: None,
            remainder: 0.,
        };
        turn.move_to(start);
        turn
    }

    pub fn move_to(&mut self, position: (f64, f64)) -> i32 {
        let x = position.0 - self.center.0;
        let y = position.1 - self.center.1;
        let angle = (x * x + y * y > self.dead_radius_squared).then(|| y.atan2(x));
        let previous = std::mem::replace(&mut self.angle, angle);
        let (Some(angle), Some(previous)) = (angle, previous) else {
            self.remainder = 0.;
            return 0;
        };
        // Keep the short arc when the pointer crosses the atan2 seam.
        let delta = (angle - previous + PI).rem_euclid(TAU) - PI;
        let detents = self.remainder + delta.to_degrees() / f64::from(ENCODER_DEGREES_PER_STEP);
        let steps = detents.trunc() as i32;
        self.remainder = detents - f64::from(steps);
        steps
    }
}
#[derive(Default)]
pub struct Switch {
    requested: bool,
    last_edge: Option<u64>,
    edges: VecDeque<(u64, bool)>,
}
impl Switch {
    pub fn set(&mut self, pressed: bool, now: u64) {
        if pressed == self.requested {
            return;
        }
        let at = self
            .last_edge
            .map_or(now, |last| now.max(last.saturating_add(MIN_EDGE_GAP)));
        self.edges.push_back((at, pressed));
        self.last_edge = Some(at);
        self.requested = pressed;
    }
    pub fn due(&mut self, now: u64) -> Option<bool> {
        if self.edges.front().is_some_and(|edge| edge.0 <= now) {
            self.edges.pop_front().map(|edge| edge.1)
        } else {
            None
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn point(degrees: f64) -> (f64, f64) {
        let (y, x) = degrees.to_radians().sin_cos();
        (x * 30., y * 30.)
    }

    #[test]
    fn encoder_tracks_clockwise_and_counterclockwise_across_the_seam() {
        let mut clockwise = EncoderTurn::new((0., 0.), 6., point(170.));
        assert_eq!(clockwise.move_to(point(-170.)), 1);
        let mut counterclockwise = EncoderTurn::new((0., 0.), 6., point(-170.));
        assert_eq!(counterclockwise.move_to(point(170.)), -1);
    }

    #[test]
    fn encoder_accumulates_small_moves_without_jumping_over_the_center() {
        let mut turn = EncoderTurn::new((0., 0.), 6., point(0.));
        assert_eq!(turn.move_to(point(7.)), 0);
        assert_eq!(turn.move_to(point(14.)), 0);
        assert_eq!(turn.move_to(point(19.)), 1);
        assert_eq!(turn.move_to((0., 0.)), 0);
        assert_eq!(turn.move_to(point(180.)), 0);
        assert_eq!(turn.move_to(point(163.)), -1);
    }
    #[test]
    fn a_browser_double_click_keeps_two_distinct_presses() {
        let mut switch = Switch::default();
        for pressed in [true, false, true, false] {
            switch.set(pressed, 0);
        }
        assert_eq!(switch.due(0), Some(true));
        assert_eq!(switch.due(15_999_999), None);
        assert_eq!(switch.due(16_000_000), Some(false));
        assert_eq!(switch.due(32_000_000), Some(true));
        assert_eq!(switch.due(48_000_000), Some(false));
        assert_eq!(switch.due(64_000_000), None);
    }
    #[test]
    fn holding_and_releasing_a_key_keeps_the_performers_duration() {
        let mut switch = Switch::default();
        switch.set(true, 100_000_000);
        switch.set(true, 120_000_000);
        assert_eq!(switch.due(100_000_000), Some(true));
        assert_eq!(switch.due(500_000_000), None);
        switch.set(false, 600_000_000);
        switch.set(false, 600_000_000);
        assert_eq!(switch.due(600_000_000), Some(false));
        assert_eq!(switch.due(600_000_000), None);
    }
}
