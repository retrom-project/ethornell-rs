//! Presentation pacing shared by the window and hosted compositors.
//! The extended 91:00 forced idle path must not consume a scheduled frame.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Presentation {
    None,
    Scheduled,
    Forced,
}

#[derive(Debug, Clone)]
pub(crate) struct GraphPresentation {
    enabled: bool,
    interval_ms: u32,
    next_ms: u32,
    force_present: i32,
}

impl Default for GraphPresentation {
    fn default() -> Self {
        Self {
            enabled: true,
            interval_ms: 16,
            next_ms: 0,
            force_present: 0,
        }
    }
}

impl GraphPresentation {
    #[cfg(test)]
    pub(crate) fn enabled(&self) -> bool {
        self.enabled
    }
    #[cfg(test)]
    pub(crate) fn interval_ms(&self) -> u32 {
        self.interval_ms
    }
    pub(crate) fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }
    pub(crate) fn set_rate(&mut self, rate: u32) {
        self.interval_ms = (1000 / rate).max(1);
        self.next_ms = 0;
    }
    pub(crate) fn set_force_present(&mut self, value: i32) {
        self.force_present = value;
    }

    pub(crate) fn take_due(&mut self, now: u32) -> Presentation {
        if !self.enabled || self.next_ms > now {
            // The native idle pump also presents when scheduling is disabled
            // or its deadline has not elapsed. It leaves that deadline intact.
            return if self.force_present != 0 {
                Presentation::Forced
            } else {
                Presentation::None
            };
        }
        let interval = self.interval_ms.max(1);
        self.next_ms = self
            .next_ms
            .wrapping_add(interval.wrapping_mul((now - self.next_ms) / interval + 1));
        Presentation::Scheduled
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disabled_presentation_stays_disabled_without_a_force_request() {
        let mut schedule = GraphPresentation::default();
        schedule.set_enabled(false);
        assert_eq!(schedule.take_due(20), Presentation::None);
        assert_eq!(schedule.next_ms, 0);
    }

    #[test]
    fn any_nonzero_force_value_presents_while_disabled_without_changing_the_deadline() {
        for value in [1, 2, -1, i32::MIN] {
            let mut schedule = GraphPresentation::default();
            schedule.set_enabled(false);
            schedule.set_force_present(value);
            assert_eq!(schedule.take_due(20), Presentation::Forced);
            assert_eq!(schedule.next_ms, 0);
            schedule.set_force_present(0);
            assert_eq!(schedule.take_due(21), Presentation::None);
        }
    }

    #[test]
    fn forced_early_frames_do_not_postpone_the_next_timed_frame() {
        let mut schedule = GraphPresentation::default();
        schedule.set_rate(1);
        schedule.set_force_present(1);
        assert_eq!(schedule.take_due(0), Presentation::Scheduled);
        assert_eq!(schedule.next_ms, 1000);
        assert_eq!(schedule.take_due(16), Presentation::Forced);
        assert_eq!(schedule.take_due(32), Presentation::Forced);
        assert_eq!(schedule.next_ms, 1000);
        schedule.set_force_present(0);
        assert_eq!(schedule.take_due(999), Presentation::None);
        assert_eq!(schedule.take_due(1000), Presentation::Scheduled);
        assert_eq!(schedule.next_ms, 2000);
    }

    #[test]
    fn timed_frames_advance_by_complete_intervals_without_drift() {
        let mut schedule = GraphPresentation::default();
        schedule.set_rate(20);
        assert!(schedule.enabled());
        assert_eq!(schedule.interval_ms(), 50);
        assert_eq!(schedule.take_due(120), Presentation::Scheduled);
        assert_eq!(schedule.next_ms, 150);
        assert_eq!(schedule.take_due(149), Presentation::None);
        assert_eq!(schedule.take_due(150), Presentation::Scheduled);
        assert_eq!(schedule.next_ms, 200);
    }
}
