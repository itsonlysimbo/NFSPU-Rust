//! Knockout and quick knockout: who is still in, and who goes out after each round.
//!
//! The rules are the retail game's (`docs/provenance/race.md`, "Knockout"): the player and `N`
//! opponents start; each round (a whole race in Knockout, a lap in Quick Knockout) the car in last
//! place is out, until one is left after round `N`. The player finishing last is out too, which
//! ends the run. Nothing here knows about tracks or the 3D scene.

/// Which round structure a run uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// "Quick knockout": one circuit race, one round per lap.
    Quick,
    /// "Knockout": a series of races on linear tracks, one round per race.
    Series,
}

/// One run: `opponents` is the number the player chose (1..=7); rounds run until one car is left.
#[derive(Debug, Clone, PartialEq)]
pub struct Knockout {
    pub mode: Mode,
    /// Opponents at the start (`Game.NumKOOpponents`); also the number of rounds.
    pub total: u32,
    /// Rounds finished so far.
    pub round: u32,
    /// Entrant `0` is the player, `1..=total` the opponents in grid order.
    pub alive: Vec<bool>,
}

/// What one round did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outcome {
    /// The entrant sent out this round.
    pub eliminated: usize,
    pub player_out: bool,
    /// No more rounds: the player is out or only one car is left.
    pub over: bool,
}

impl Knockout {
    pub fn new(mode: Mode, opponents: u32) -> Knockout {
        let total = opponents.clamp(1, 7);
        Knockout { mode, total, round: 0, alive: vec![true; total as usize + 1] }
    }

    /// Opponents the next round starts with: the ones still in (the total minus the rounds done).
    pub fn opponents_left(&self) -> u32 {
        self.total - self.round
    }

    /// Entrants still in, the player first when in.
    pub fn alive_ids(&self) -> Vec<usize> {
        (0..self.alive.len()).filter(|&i| self.alive[i]).collect()
    }

    pub fn player_in(&self) -> bool {
        self.alive[0]
    }

    /// True once the run has ended: every round played or the player out.
    pub fn over(&self) -> bool {
        self.round >= self.total || !self.player_in()
    }

    /// The round just ended. `order` lists the entrants still in, in finishing order (first to
    /// last). The last one goes out. Retail drops every car whose place is past the number of
    /// opponents left, which is the same single car.
    pub fn record(&mut self, order: &[usize]) -> Outcome {
        let alive = self.alive_ids();
        assert_eq!(order.len(), alive.len(), "the order must name every entrant still in");
        let left = self.opponents_left() as usize;
        let mut eliminated = usize::MAX;
        for (place, &who) in order.iter().enumerate() {
            if place + 1 > left {
                self.alive[who] = false;
                eliminated = who;
            }
        }
        self.round += 1;
        Outcome { eliminated, player_out: !self.player_in(), over: self.over() }
    }
}

/// True for the six circuit tracks (the others are one-way roads): quick knockout needs laps, so
/// it only runs on these. From `race_drive`'s track kinds (monaco1-5 and the skid pad).
pub fn is_circuit(stem: &str) -> bool {
    let s = stem.to_ascii_lowercase();
    s == "skidpad" || (s.len() == 7 && s.starts_with("monaco") && matches!(s.as_bytes()[6], b'1'..=b'5'))
}

/// What the menu asked for, handed to the flow when RACE is pressed on a knockout screen.
#[derive(Debug, Clone, PartialEq)]
pub struct Pending {
    pub mode: Mode,
    pub opponents: u32,
    /// Series: the track of each race (first the chosen one). Quick: unused.
    pub tracks: Vec<String>,
}

static PENDING: std::sync::Mutex<Option<Pending>> = std::sync::Mutex::new(None);

/// Sets (or clears, with `None`) the knockout the next RACE press starts.
pub fn set_pending(p: Option<Pending>) {
    *PENDING.lock().unwrap() = p;
}

/// Takes the pending knockout, if any.
pub fn take_pending() -> Option<Pending> {
    PENDING.lock().unwrap().take()
}

/// A knockout being played: the rounds so far plus what the next race needs.
#[derive(Debug, Clone)]
pub struct Run {
    pub ko: Knockout,
    /// Series: the track of each race.
    pub tracks: Vec<String>,
    /// Each opponent's car (entrant `i + 1`), kept for the whole run.
    pub cars: Vec<String>,
}

/// What follows a race.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    /// Another race: the survivors line up again.
    Next,
    /// The run is over; the player won (last car standing / first over the last line).
    Won,
    /// The run is over; the player was knocked out in round `n` (1-based).
    PlayerOut(u32),
}

impl Run {
    pub fn new(mode: Mode, opponents: u32, tracks: Vec<String>) -> Run {
        Run { ko: Knockout::new(mode, opponents), tracks, cars: Vec::new() }
    }

    /// The race's laps: a quick knockout runs one lap per opponent (a guess that makes the last
    /// lap the 2-car final); a series race uses the screen's laps.
    pub fn quick_laps(&self) -> u32 {
        self.ko.total
    }

    /// The track of the race about to be run (series), clamped to the last one.
    pub fn track(&self) -> Option<&str> {
        self.tracks.get((self.ko.round as usize).min(self.tracks.len().saturating_sub(1))).map(String::as_str)
    }

    /// Entrants on the next grid; position `g` is race car `g` (0 is the player).
    pub fn grid(&self) -> Vec<usize> {
        self.ko.alive_ids()
    }

    /// The surviving opponents' cars in grid order.
    pub fn opponent_cars(&self) -> Vec<String> {
        self.grid().into_iter().skip(1).filter_map(|e| self.cars.get(e - 1).cloned()).collect()
    }

    /// A race ended: `order` lists the race cars (grid positions) first to last, and `player_in`
    /// is whether the player stayed in it (finished or was still driving). Series: the last car
    /// goes out. Quick: the race itself did the eliminating, so the winner decides.
    pub fn after_race(&mut self, order: &[usize]) -> Step {
        match self.ko.mode {
            Mode::Quick => {
                let won = order.first() == Some(&0);
                self.ko.round = self.ko.total;
                if won { Step::Won } else { Step::PlayerOut(order.iter().position(|&c| c == 0).map_or(self.ko.total, |p| (order.len() - p) as u32)) }
            }
            Mode::Series => {
                let grid = self.grid();
                let ids: Vec<usize> = order.iter().filter_map(|&c| grid.get(c).copied()).collect();
                let o = self.ko.record(&ids);
                if o.player_out {
                    Step::PlayerOut(self.ko.round)
                } else if o.over {
                    Step::Won
                } else {
                    Step::Next
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn last_place_goes_out_each_round_until_one_is_left() {
        let mut k = Knockout::new(Mode::Series, 3);
        assert_eq!(k.alive_ids(), vec![0, 1, 2, 3]);
        assert_eq!(k.opponents_left(), 3);
        let o = k.record(&[0, 1, 2, 3]);
        assert_eq!(o, Outcome { eliminated: 3, player_out: false, over: false });
        assert_eq!(k.opponents_left(), 2);
        let o = k.record(&[2, 0, 1]);
        assert_eq!(o.eliminated, 1);
        let o = k.record(&[0, 2]);
        assert_eq!(o, Outcome { eliminated: 2, player_out: false, over: true });
        assert_eq!(k.alive_ids(), vec![0]);
    }

    #[test]
    fn the_player_finishing_last_ends_the_run() {
        let mut k = Knockout::new(Mode::Quick, 4);
        let o = k.record(&[1, 2, 3, 4, 0]);
        assert!(o.player_out && o.over);
        assert_eq!(o.eliminated, 0);
        assert!(k.over());
        assert_eq!(k.round, 1);
    }

    #[test]
    fn a_series_run_follows_the_survivors() {
        let mut run = Run::new(Mode::Series, 3, vec!["a".into(), "b".into(), "c".into()]);
        run.cars = vec!["x".into(), "y".into(), "z".into()];
        assert_eq!((run.track(), run.grid(), run.opponent_cars()), (Some("a"), vec![0, 1, 2, 3], vec!["x".into(), "y".into(), "z".into()]));
        // race cars 0..4 finish 0, 2, 1, 3: entrant 3 (race car 3) is out
        assert_eq!(run.after_race(&[0, 2, 1, 3]), Step::Next);
        assert_eq!((run.track(), run.grid(), run.opponent_cars()), (Some("b"), vec![0, 1, 2], vec!["x".into(), "y".into()]));
        // grid now [0,1,2]: race car 1 (entrant 1) comes last
        assert_eq!(run.after_race(&[0, 2, 1]), Step::Next);
        assert_eq!(run.opponent_cars(), vec!["y".to_string()]);
        assert_eq!(run.after_race(&[1, 0]), Step::PlayerOut(3));
    }

    #[test]
    fn circuits_are_monaco_and_the_skid_pad() {
        assert!(is_circuit("Monaco3") && is_circuit("skidpad"));
        assert!(!is_circuit("canyon") && !is_circuit("monaco6") && !is_circuit("alps"));
    }

    #[test]
    fn the_opponent_count_is_kept_in_range() {
        assert_eq!(Knockout::new(Mode::Quick, 0).total, 1);
        assert_eq!(Knockout::new(Mode::Quick, 12).total, 7);
    }
}
