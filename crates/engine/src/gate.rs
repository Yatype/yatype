//! Шлюз, который не даёт нажатиям пользователя вклиниться в замену текста.
//!
//! Замена — пачка из десятков событий, отправка занимает десятки
//! миллисекунд. Нажатие пользователя посреди пачки испортило бы текст.
//! Поэтому:
//!
//! - пока пачка не дошла до конца (через перехват не прошёл её маркер
//!   `End`), все нажатия пользователя придерживаются;
//! - потом придержанные отправляются заново (с меткой `Replay`) вместе с
//!   маркером `Barrier`; пока он не прошёл, новые нажатия тоже ждут —
//!   иначе они обогнали бы отпущенные;
//! - если отпущенное нажатие само вызвало исправление, остальные отпущенные
//!   снова ждут конца этой пачки.
//!
//! Сами события и маркеры у каждой системы свои; здесь только порядок.

use std::time::{Duration, Instant};

/// Если маркер почему-то не дошёл, дольше этого нажатия не держим.
pub const MAX_HOLD: Duration = Duration::from_millis(500);

/// Метка события, которое пришло в перехват.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Marker {
    /// Нажатие пользователя.
    User,
    /// Отпущенное (повторно отправленное) нажатие пользователя.
    Replay,
    /// Конец нашей пачки.
    End,
    /// Конец отпущенных нажатий.
    Barrier,
}

/// Придержанные нажатия, которые пора отправить заново (с меткой `Replay`).
/// С `barrier` следом нужно отправить маркер `Barrier`.
#[derive(Debug, PartialEq, Eq)]
pub struct Release<E> {
    pub events: Vec<E>,
    pub barrier: bool,
}

/// Решение по событию.
#[derive(Debug, PartialEq, Eq)]
pub struct Decision<E> {
    /// Обработать событие как обычно; иначе — не пропускать дальше.
    pub handle: bool,
    pub release: Option<Release<E>>,
}

#[derive(Debug)]
pub struct Gate<E> {
    /// Сколько пачек отправлено, но ещё не дошло до конца.
    open_batches: usize,
    /// Отпущенные нажатия ещё в пути.
    barrier: bool,
    /// Копии придержанных событий, по порядку.
    held: Vec<E>,
    /// Когда шлюз закрылся — для страховки от потерянного маркера.
    since: Option<Instant>,
}

impl<E> Default for Gate<E> {
    fn default() -> Self {
        Gate { open_batches: 0, barrier: false, held: Vec::new(), since: None }
    }
}

impl<E> Gate<E> {
    /// Начинается пачка: после неё система должна отправить маркер `End`.
    pub fn begin_batch(&mut self, now: Instant) {
        self.open_batches += 1;
        self.since.get_or_insert(now);
    }

    /// Событие пришло в перехват. `capture` делает копию события, если его
    /// нужно придержать.
    pub fn on_event(&mut self, marker: Marker, now: Instant, capture: impl FnOnce() -> E) -> Decision<E> {
        let mut release = None;
        if self.since.is_some_and(|t| now.duration_since(t) > MAX_HOLD) {
            // Маркер потерялся: открыть шлюз, ничего не держать.
            self.open_batches = 0;
            self.barrier = false;
            self.since = None;
            release = self.flush(false, now);
        }
        match marker {
            Marker::End => {
                self.open_batches = self.open_batches.saturating_sub(1);
                if self.open_batches == 0 {
                    release = merge(release, self.flush(true, now));
                }
                return Decision { handle: false, release };
            }
            Marker::Barrier => {
                self.barrier = false;
                if self.open_batches == 0 {
                    release = merge(release, self.flush(true, now));
                }
                return Decision { handle: false, release };
            }
            Marker::User | Marker::Replay => {}
        }
        // Отпущенные нажатия ждут только новых пачек, обычные — ещё и барьера.
        let hold = match marker {
            Marker::Replay => self.open_batches > 0,
            _ => self.open_batches > 0 || self.barrier,
        };
        if hold {
            self.held.push(capture());
        }
        Decision { handle: !hold, release }
    }

    fn flush(&mut self, barrier: bool, now: Instant) -> Option<Release<E>> {
        let events = std::mem::take(&mut self.held);
        if events.is_empty() {
            if self.open_batches == 0 && !self.barrier {
                self.since = None;
            }
            return None;
        }
        if barrier {
            self.barrier = true;
            self.since = Some(now);
        }
        Some(Release { events, barrier })
    }
}

fn merge<E>(a: Option<Release<E>>, b: Option<Release<E>>) -> Option<Release<E>> {
    match (a, b) {
        (Some(mut a), Some(b)) => {
            a.events.extend(b.events);
            a.barrier = b.barrier;
            Some(a)
        }
        (a, b) => a.or(b),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(t: Instant, n: u64) -> Instant {
        t + Duration::from_millis(n)
    }

    #[test]
    fn idle_gate_passes_everything() {
        let (mut g, t) = (Gate::<u8>::default(), Instant::now());
        assert_eq!(g.on_event(Marker::User, t, || 1), Decision { handle: true, release: None });
    }

    #[test]
    fn keys_during_batch_wait_and_come_back_in_order() {
        let (mut g, t) = (Gate::default(), Instant::now());
        g.begin_batch(t);
        assert!(!g.on_event(Marker::User, ms(t, 1), || 'u').handle);
        assert!(!g.on_event(Marker::User, ms(t, 2), || 'p').handle);
        let d = g.on_event(Marker::End, ms(t, 3), || unreachable!());
        assert_eq!(d.release, Some(Release { events: vec!['u', 'p'], barrier: true }));
    }

    #[test]
    fn keys_between_end_and_barrier_do_not_overtake_replays() {
        // Случай из сквозного теста: «b» пришла после конца пачки, но раньше,
        // чем отпущенные «u p» — она должна подождать их.
        let (mut g, t) = (Gate::default(), Instant::now());
        g.begin_batch(t);
        g.on_event(Marker::User, ms(t, 1), || 'u');
        g.on_event(Marker::End, ms(t, 2), || unreachable!());
        assert!(!g.on_event(Marker::User, ms(t, 3), || 'b').handle);
        assert!(g.on_event(Marker::Replay, ms(t, 4), || unreachable!()).handle);
        let d = g.on_event(Marker::Barrier, ms(t, 5), || unreachable!());
        assert_eq!(d.release, Some(Release { events: vec!['b'], barrier: true }));
        g.on_event(Marker::Replay, ms(t, 6), || unreachable!());
        let d = g.on_event(Marker::Barrier, ms(t, 7), || unreachable!());
        assert_eq!(d.release, None);
        assert!(g.on_event(Marker::User, ms(t, 8), || 'r').handle);
    }

    #[test]
    fn replay_that_starts_a_new_batch_holds_the_rest() {
        let (mut g, t) = (Gate::default(), Instant::now());
        g.begin_batch(t);
        g.on_event(Marker::User, ms(t, 1), || ' ');
        g.on_event(Marker::User, ms(t, 1), || 'b');
        g.on_event(Marker::End, ms(t, 2), || unreachable!());
        // Отпущенный пробел вызвал новое исправление.
        assert!(g.on_event(Marker::Replay, ms(t, 3), || unreachable!()).handle);
        g.begin_batch(ms(t, 3));
        // Отпущенная «b» теперь ждёт конца новой пачки.
        assert!(!g.on_event(Marker::Replay, ms(t, 4), || 'b').handle);
        // Барьер прошёл, но пачка ещё не закончилась — не отпускаем.
        assert_eq!(g.on_event(Marker::Barrier, ms(t, 5), || unreachable!()).release, None);
        let d = g.on_event(Marker::End, ms(t, 6), || unreachable!());
        assert_eq!(d.release, Some(Release { events: vec!['b'], barrier: true }));
    }

    #[test]
    fn lost_marker_releases_after_timeout() {
        let (mut g, t) = (Gate::default(), Instant::now());
        g.begin_batch(t);
        g.on_event(Marker::User, ms(t, 1), || 'x');
        let d = g.on_event(Marker::User, ms(t, 600), || 'y');
        assert!(d.handle);
        assert_eq!(d.release, Some(Release { events: vec!['x'], barrier: false }));
    }
}
