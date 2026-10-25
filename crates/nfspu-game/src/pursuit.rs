//! Pursuit rules: who wins and loses. Findings and guesses: `docs/provenance/pursuit.md`.
//!
//! You drive the pursued car. Police (`cop.rs`) chase you. A cop that stays close while you are
//! slow busts you: that is a ticket, and the cops back off for a few seconds. Three tickets, or
//! the clock running out before the finish, lose; crossing the finish in time wins.
//! All numbers here are guesses (the retail rule values were not decoded).

/// Tickets that end the pursuit.
pub const MAX_TICKETS: u32 = 3;
/// A cop this near the player counts towards a bust (m).
pub const BUST_DIST: f32 = 8.0;
/// The player must be slower than this for a bust to count (m/s).
pub const BUST_SPEED: f32 = 9.0;
/// Seconds of cop-close-and-slow that make one ticket.
pub const BUST_SECONDS: f32 = 2.5;
/// Seconds the cops hold off after a ticket.
pub const HOLD_SECONDS: f32 = 6.0;
/// The time limit is the race length divided by this speed (m/s), plus the slack below.
pub const LIMIT_SPEED: f32 = 14.0;
pub const LIMIT_SLACK: f32 = 30.0;

const HZ: f32 = 64.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// Crossed the finish in time.
    Escaped,
    /// Busted `MAX_TICKETS` times.
    Busted,
    /// The clock ran out.
    TimeUp,
}

impl Outcome {
    pub fn won(self) -> bool {
        self == Outcome::Escaped
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Event {
    Ticket { n: u32 },
    Ended(Outcome),
}

#[derive(Debug, Clone)]
pub struct Pursuit {
    pub limit_ticks: u32,
    pub tickets: u32,
    pub outcome: Option<Outcome>,
    bust: u32,
    hold_until: u32,
}

/// Time allowed for `laps` laps of `lap_metres` (seconds).
pub fn time_limit(lap_metres: f32, laps: u32) -> f32 {
    lap_metres * laps as f32 / LIMIT_SPEED + LIMIT_SLACK
}

impl Pursuit {
    pub fn new(limit_secs: f32) -> Self {
        Pursuit { limit_ticks: (limit_secs * HZ) as u32, tickets: 0, outcome: None, bust: 0, hold_until: 0 }
    }

    /// True while the cops are backing off after a ticket.
    pub fn holding(&self, tick: u32) -> bool {
        tick < self.hold_until
    }

    pub fn secs_left(&self, tick: u32) -> f32 {
        self.limit_ticks.saturating_sub(tick) as f32 / HZ
    }

    /// One race tick after the start. `nearest_cop` is the distance to the nearest chasing cop.
    pub fn tick(&mut self, tick: u32, nearest_cop: Option<f32>, player_speed: f32) -> Vec<Event> {
        let mut ev = Vec::new();
        if self.outcome.is_some() {
            return ev;
        }
        let close = nearest_cop.is_some_and(|d| d < BUST_DIST) && player_speed.abs() < BUST_SPEED && !self.holding(tick);
        self.bust = if close { self.bust + 1 } else { 0 };
        if self.bust as f32 >= BUST_SECONDS * HZ {
            self.bust = 0;
            self.tickets += 1;
            self.hold_until = tick + (HOLD_SECONDS * HZ) as u32;
            ev.push(Event::Ticket { n: self.tickets });
            if self.tickets >= MAX_TICKETS {
                self.end(Outcome::Busted, &mut ev);
            }
        }
        if self.outcome.is_none() && tick >= self.limit_ticks {
            self.end(Outcome::TimeUp, &mut ev);
        }
        ev
    }

    /// The player crossed the finish line.
    pub fn finished(&mut self) -> Vec<Event> {
        let mut ev = Vec::new();
        if self.outcome.is_none() {
            self.end(Outcome::Escaped, &mut ev);
        }
        ev
    }

    fn end(&mut self, o: Outcome, ev: &mut Vec<Event>) {
        self.outcome = Some(o);
        ev.push(Event::Ended(o));
    }
}

/// Siren light phase at a tick: 0 off, 1 red, 2 blue (alternating about 3 times a second; guess).
pub fn siren_light(tick: u32) -> u8 {
    match (tick / 10) % 4 {
        0 => 1,
        2 => 2,
        _ => 0,
    }
}

/// The pursuit HUD text: time left and tickets (layout is ours; retail's was not read).
pub fn hud_line(p: &Pursuit, tick: u32, cops: usize) -> String {
    let s = p.secs_left(tick) as u32;
    format!("TIME {}:{:02}   TICKETS {}/{}   COPS {}", s / 60, s % 60, p.tickets, MAX_TICKETS, cops)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tickets_end_the_pursuit() {
        let mut p = Pursuit::new(1000.0);
        let mut t = 0;
        let mut ended = None;
        while ended.is_none() && t < 64 * 100 {
            t += 1;
            for e in p.tick(t, Some(3.0), 0.0) {
                if let Event::Ended(o) = e {
                    ended = Some(o);
                }
            }
        }
        assert_eq!(ended, Some(Outcome::Busted));
        assert_eq!(p.tickets, MAX_TICKETS);
    }

    #[test]
    fn fast_player_is_not_busted_and_wins() {
        let mut p = Pursuit::new(100.0);
        for t in 1..64 * 20 {
            assert!(p.tick(t, Some(3.0), 30.0).is_empty());
        }
        assert_eq!(p.finished(), vec![Event::Ended(Outcome::Escaped)]);
        assert!(p.outcome.unwrap().won());
    }

    #[test]
    fn clock_runs_out() {
        let mut p = Pursuit::new(2.0);
        let ev: Vec<_> = (1..=200).flat_map(|t| p.tick(t, None, 20.0)).collect();
        assert_eq!(ev, vec![Event::Ended(Outcome::TimeUp)]);
        assert!(!Outcome::TimeUp.won());
    }

    #[test]
    fn cops_hold_after_a_ticket() {
        let mut p = Pursuit::new(1000.0);
        let mut t = 0;
        while p.tickets == 0 {
            t += 1;
            p.tick(t, Some(2.0), 0.0);
        }
        assert!(p.holding(t + 1));
        assert!(!p.holding(t + 64 * 7));
    }
}
