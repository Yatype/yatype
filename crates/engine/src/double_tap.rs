//! Распознаёт двойное нажатие модификатора (у нас — Control): два коротких
//! нажатия подряд без других клавиш между ними. Не зависит от платформы: время передаётся снаружи.

use std::time::{Duration, Instant};

/// Дольше этого держать клавишу — уже не «нажатие», а удержание.
const MAX_HOLD: Duration = Duration::from_millis(300);
/// Максимальная пауза между двумя нажатиями.
const MAX_GAP: Duration = Duration::from_millis(400);

#[derive(Default)]
pub struct DoubleTap {
    /// Когда клавишу нажали, если сейчас она зажата «чисто» (без других клавиш).
    down_at: Option<Instant>,
    /// Когда закончилось предыдущее короткое нажатие.
    last_tap: Option<Instant>,
}

impl DoubleTap {
    /// Изменились модификаторы. `key` — зажата ли сейчас наша клавиша,
    /// `others` — зажаты ли другие модификаторы. Возвращает `true`, когда
    /// распознано двойное нажатие.
    pub fn modifiers(&mut self, key: bool, others: bool, now: Instant) -> bool {
        if others {
            self.clear();
            return false;
        }
        if key {
            if self.down_at.is_none() {
                self.down_at = Some(now);
            }
            return false;
        }
        // Клавишу отпустили.
        let Some(down) = self.down_at.take() else { return false };
        if now.duration_since(down) > MAX_HOLD {
            self.last_tap = None;
            return false;
        }
        match self.last_tap {
            Some(prev) if now.duration_since(prev) <= MAX_GAP => {
                self.last_tap = None;
                true
            }
            _ => {
                self.last_tap = Some(now);
                false
            }
        }
    }

    /// Нажата обычная клавиша: серия прервана.
    pub fn other_key(&mut self) {
        self.clear();
    }

    fn clear(&mut self) {
        self.down_at = None;
        self.last_tap = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(base: Instant, n: u64) -> Instant {
        base + Duration::from_millis(n)
    }

    fn tap(d: &mut DoubleTap, t: Instant, down: u64, up: u64) -> bool {
        d.modifiers(true, false, ms(t, down));
        d.modifiers(false, false, ms(t, up))
    }

    #[test]
    fn two_quick_taps() {
        let (mut d, t) = (DoubleTap::default(), Instant::now());
        assert!(!tap(&mut d, t, 0, 80));
        assert!(tap(&mut d, t, 200, 280));
    }

    #[test]
    fn third_tap_starts_a_new_series() {
        let (mut d, t) = (DoubleTap::default(), Instant::now());
        tap(&mut d, t, 0, 80);
        assert!(tap(&mut d, t, 200, 280));
        assert!(!tap(&mut d, t, 400, 480));
    }

    #[test]
    fn too_slow_between_taps() {
        let (mut d, t) = (DoubleTap::default(), Instant::now());
        tap(&mut d, t, 0, 80);
        assert!(!tap(&mut d, t, 700, 780));
    }

    #[test]
    fn holding_the_key_is_not_a_tap() {
        let (mut d, t) = (DoubleTap::default(), Instant::now());
        tap(&mut d, t, 0, 80);
        assert!(!tap(&mut d, t, 150, 600));
    }

    #[test]
    fn key_in_between_breaks_the_series() {
        let (mut d, t) = (DoubleTap::default(), Instant::now());
        tap(&mut d, t, 0, 80);
        d.other_key();
        assert!(!tap(&mut d, t, 200, 280));
    }

    #[test]
    fn key_with_other_modifier_does_not_count() {
        let (mut d, t) = (DoubleTap::default(), Instant::now());
        tap(&mut d, t, 0, 80);
        d.modifiers(true, true, ms(t, 200));
        assert!(!d.modifiers(false, false, ms(t, 260)));
    }
}
