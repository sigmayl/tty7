use std::time::{Duration, Instant};

const MOVE_DURATION: Duration = Duration::from_millis(85);

#[derive(Default)]
pub(super) struct CursorMotion {
    travel: Option<Travel>,
}

struct Travel {
    from: [f32; 2],
    to: [f32; 2],
    started: Instant,
}

impl Travel {
    fn sample(&self, now: Instant) -> ([f32; 2], bool) {
        let t = (now.duration_since(self.started).as_secs_f32() / MOVE_DURATION.as_secs_f32())
            .clamp(0., 1.);
        let eased = 1. - (1. - t).powi(3);
        (
            std::array::from_fn(|i| self.from[i] + (self.to[i] - self.from[i]) * eased),
            t < 1. && self.from != self.to,
        )
    }
}

impl CursorMotion {
    pub(super) fn reset(&mut self) {
        self.travel = None;
    }

    pub(super) fn sample(
        &mut self,
        target: [f32; 2],
        smooth: bool,
        now: Instant,
    ) -> ([f32; 2], bool) {
        let Some(travel) = self.travel.as_mut().filter(|_| smooth) else {
            self.travel = Some(Travel {
                from: target,
                to: target,
                started: now,
            });
            return (target, false);
        };
        if travel.to != target {
            // Retarget from the point currently on screen, even when typing
            // outruns the animation. Never queue old cursor positions.
            *travel = Travel {
                from: travel.sample(now).0,
                to: target,
                started: now,
            };
        }
        travel.sample(now)
    }
}

pub(super) fn breathe_opacity(elapsed: Duration) -> f32 {
    let phase = elapsed.as_secs_f32() % 1.4 / 1.4 * std::f32::consts::TAU;
    0.15 + 0.85 * (0.5 + 0.5 * phase.cos())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn movement_retargets_continuously_and_finishes() {
        let now = Instant::now();
        let mut motion = CursorMotion::default();
        assert_eq!(motion.sample([0., 0.], true, now), ([0., 0.], false));
        assert_eq!(motion.sample([10., 2.], true, now), ([0., 0.], true));
        let halfway = now + MOVE_DURATION / 2;
        let before = motion.sample([10., 2.], true, halfway).0;
        assert!(before[0] > 0. && before[0] < 10.);
        assert_eq!(motion.sample([20., 4.], true, halfway), (before, true));
        assert_eq!(
            motion.sample([20., 4.], true, halfway + MOVE_DURATION),
            ([20., 4.], false)
        );
    }

    #[test]
    fn disabling_or_resetting_snaps_to_the_target() {
        let now = Instant::now();
        let mut motion = CursorMotion::default();
        motion.sample([0., 0.], true, now);
        motion.sample([10., 0.], true, now);
        assert_eq!(motion.sample([10., 0.], false, now), ([10., 0.], false));
        motion.reset();
        assert_eq!(motion.sample([30., 8.], true, now), ([30., 8.], false));
    }

    #[test]
    fn breathing_is_continuous_periodic_and_never_disappears() {
        assert_eq!(breathe_opacity(Duration::ZERO), 1.);
        assert!((breathe_opacity(Duration::from_millis(700)) - 0.15).abs() < 0.001);
        assert!((breathe_opacity(Duration::from_millis(1400)) - 1.).abs() < 0.001);
        for millis in 0..1400 {
            let a = breathe_opacity(Duration::from_millis(millis));
            let b = breathe_opacity(Duration::from_millis(millis + 1));
            assert!((0.15..=1.).contains(&a));
            assert!((a - b).abs() < 0.003);
        }
    }
}
