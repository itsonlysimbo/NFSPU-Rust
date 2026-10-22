//! Which way opponents go at road junctions.
//!
//! Retail behaviour this follows (decomp, `docs/provenance/ai.md`); the code is our own:
//! - The road's slice groups are cut into runs where another run's end meets their middle, in one
//!   pass in file order, the cut-off tails added at the end of the list. Every run has two ends.
//! - Junctions are made in run order: the first end not yet in a junction starts one, and every
//!   other free end within 16 m of it joins (at most 8). Each end is a route of that junction.
//! - A route's weight defaults to 1 / (number of routes of its kind, run starts or run ends) and
//!   is replaced by the track's `.jnc` weights when the file has one record per junction.
//! - When the junction at the end of a driver's run comes within its look-ahead (the `.ais`
//!   lookahead slices), and it is not the junction the driver last chose at, the driver draws a
//!   random number r in 0..1 and picks one of the routes leaving that way (run starts going
//!   forward, run ends going backward):
//!   - beginner and advanced, one route: that one. Two routes: with probability 0.55 / 0.70 the
//!     route weighted 0.5 or more, else the other. Three: r up to 0.4 / 0.5 takes the route
//!     weighted 0.4 or more, up to 0.7 / 0.8 the one between 0.3 and 0.4, above that the one under
//!     0.3. Four or more: the first.
//!   - expert: by weight (the first route whose running total of weights reaches r).
//! - Looking ahead along the road (steering points, bends), the chosen route is taken at that
//!   junction; at junctions further on, the last route leaving that way.
//!
//! Ours: when no route matches the rule (e.g. two routes both weighted 0.5 and r above the
//! threshold), the last route leaving that way is taken; retail keeps its previous choice there.

use crate::race_path::Direction;
use nfspu_formats::aidata::JNC_ROUTES;
use nfspu_formats::sim::SimData;

/// Squared distance (m) within which run ends meet (retail 256).
const JOIN_DIST2: f32 = 256.0;
/// A run is only cut this many slices or more from its own ends (retail: more than 5).
const SPLIT_MARGIN: usize = 6;
/// Beginner, advanced: chance of taking the main route (weight >= 0.5) out of two.
const TWO_MAIN: [f32; 2] = [0.55, 0.70];
/// Beginner, advanced: r limits for the heavy / middle route out of three.
const THREE_LIMITS: [(f32, f32); 2] = [(0.4, 0.7), (0.5, 0.8)];
/// Weight classes for three routes.
const HEAVY: f32 = 0.4;
const LIGHT: f32 = 0.3;

/// One route out of a junction: the end of a run.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Route {
    pub run: usize,
    /// The run's last slice (true) or first slice (false) is at the junction.
    pub at_last: bool,
    pub weight: f32,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Junction {
    pub routes: Vec<Route>,
}

/// The junction table of a track.
#[derive(Debug, Clone, Default)]
pub struct Routes {
    /// (first, last) slice of every run, in retail order.
    pub runs: Vec<(usize, usize)>,
    pub junctions: Vec<Junction>,
    /// True when the weights came from the track's `.jnc`.
    pub from_file: bool,
    /// Junction at each run's (first, last) end.
    junction_of: Vec<[Option<usize>; 2]>,
    run_of: Vec<usize>,
}

/// A driver's pick at a junction (for the log).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Choice {
    pub junction: usize,
    /// Index into the junction's routes.
    pub route: usize,
    /// How many routes left that way.
    pub options: usize,
    pub r: f32,
}

impl Routes {
    /// Builds the table from the track's slices and groups, with `.jnc` weights if given.
    pub fn build(sim: &SimData, jnc: Option<&[[f32; JNC_ROUTES]]>) -> Self {
        let n = sim.slices.len();
        let d2 = |a: usize, b: usize| {
            let (p, q) = (sim.slices[a].pos, sim.slices[b].pos);
            (p[0] - q[0]).powi(2) + (p[2] - q[2]).powi(2)
        };
        let mut runs: Vec<(usize, usize)> = sim.groups.iter().filter(|g| g.1 > 0).map(|g| (g.0, g.0 + g.1 - 1)).collect();
        if runs.is_empty() && n > 0 {
            runs.push((0, n - 1));
        }

        // cut runs where another run's end meets them (one pass over the original runs)
        for a in 0..runs.len() {
            for at_last in [false, true] {
                let end = if at_last { runs[a].1 } else { runs[a].0 };
                let mut b = 0;
                while b < runs.len() {
                    let (f, l) = runs[b];
                    if b != a && f < l {
                        let (k, d) = (f..l).map(|k| (k, d2(end, k))).fold((f, f32::MAX), |m, x| if x.1 < m.1 { x } else { m });
                        if d < JOIN_DIST2 && k - f >= SPLIT_MARGIN && l - k >= SPLIT_MARGIN {
                            runs[b].1 = k;
                            runs.push((k + 1, l));
                        }
                    }
                    b += 1;
                }
            }
        }

        // junctions, in order of the first end that is not yet in one
        let ends = runs.len() * 2;
        let slice_of = |e: usize| if e % 2 == 1 { runs[e / 2].1 } else { runs[e / 2].0 };
        let mut junction_of = vec![[None; 2]; runs.len()];
        let mut junctions: Vec<Junction> = Vec::new();
        for e in 0..ends {
            if junction_of[e / 2][e % 2].is_some() {
                continue;
            }
            let j = junctions.len();
            let mut members = vec![e];
            junction_of[e / 2][e % 2] = Some(j);
            for o in 0..ends {
                if members.len() < JNC_ROUTES && o != e && junction_of[o / 2][o % 2].is_none() && d2(slice_of(e), slice_of(o)) < JOIN_DIST2 {
                    members.push(o);
                    junction_of[o / 2][o % 2] = Some(j);
                }
            }
            let lasts = members.iter().filter(|&&m| m % 2 == 1).count();
            let routes = members
                .iter()
                .map(|&m| {
                    let at_last = m % 2 == 1;
                    let same = if at_last { lasts } else { members.len() - lasts };
                    Route { run: m / 2, at_last, weight: 1.0 / same as f32 }
                })
                .collect();
            junctions.push(Junction { routes });
        }
        let from_file = jnc.is_some_and(|w| w.len() == junctions.len());
        if let (true, Some(w)) = (from_file, jnc) {
            for (j, w) in junctions.iter_mut().zip(w) {
                for (r, w) in j.routes.iter_mut().zip(w) {
                    r.weight = *w;
                }
            }
        }
        let mut run_of = vec![0usize; n];
        for (i, &(f, l)) in runs.iter().enumerate() {
            for s in run_of.iter_mut().take(l + 1).skip(f) {
                *s = i;
            }
        }
        Routes { runs, junctions, from_file, junction_of, run_of }
    }

    /// The run a slice is on.
    pub fn run_of(&self, slice: usize) -> usize {
        self.run_of.get(slice).copied().unwrap_or(0)
    }

    /// The junction a car on `slice` reaches within `ahead` slices going `dir`.
    pub fn junction_ahead(&self, slice: usize, ahead: usize, dir: Direction) -> Option<usize> {
        let r = self.run_of(slice);
        let (f, l) = *self.runs.get(r)?;
        match dir {
            Direction::Forward if slice + ahead > l => self.junction_of[r][1],
            Direction::Backward if slice < f + ahead => self.junction_of[r][0],
            _ => None,
        }
    }

    /// Indices of the routes leaving junction `j` when moving up the slice numbers (`up`, they
    /// start runs) or down them (they end runs).
    pub fn leaving(&self, j: usize, up: bool) -> Vec<usize> {
        self.junctions[j].routes.iter().enumerate().filter(|(_, r)| r.at_last != up).map(|(i, _)| i).collect()
    }

    /// The route a driver of `difficulty` takes out of junction `j` for the random number `r`.
    pub fn choose(&self, j: usize, difficulty: u8, r: f32, dir: Direction) -> Option<Choice> {
        let leaving = self.leaving(j, dir == Direction::Forward);
        let w = |i: usize| self.junctions[j].routes[i].weight;
        let pick = if difficulty < 2 {
            let d = difficulty as usize;
            match leaving.len() {
                2 => leaving.iter().copied().find(|&i| (w(i) >= 0.5) == (r <= TWO_MAIN[d])),
                3 => {
                    let (a, b) = THREE_LIMITS[d];
                    leaving.iter().copied().find(|&i| {
                        let w = w(i);
                        (r <= a && w >= HEAVY) || (r > a && r <= b && (LIGHT..HEAVY).contains(&w)) || (r > b && w < LIGHT)
                    })
                }
                _ => leaving.first().copied(),
            }
        } else {
            let mut sum = 0.0;
            leaving.iter().copied().find(|&i| {
                sum += w(i);
                r <= sum
            })
        };
        let route = pick.or(leaving.last().copied())?;
        Some(Choice { junction: j, route, options: leaving.len(), r })
    }

    /// Walks `steps` slices from `slice` in `dir` (negative walks the other way). At the junction
    /// of `choice` it takes the chosen route, at others the last route leaving that way. Stops at
    /// a dead end.
    pub fn walk(&self, slice: usize, steps: i32, dir: Direction, choice: Option<Choice>) -> usize {
        if self.runs.is_empty() {
            return slice;
        }
        let mut run = self.run_of(slice);
        let mut target = slice as i64 + if dir == Direction::Forward { steps as i64 } else { -(steps as i64) };
        for _ in 0..64 {
            let (f, l) = (self.runs[run].0 as i64, self.runs[run].1 as i64);
            if (f..=l).contains(&target) {
                return target as usize;
            }
            let up = target > l;
            let edge = if up { l } else { f };
            let Some(j) = self.junction_of[run][up as usize] else { return edge as usize };
            let leaving = self.leaving(j, up);
            let route = match choice {
                Some(c) if c.junction == j && leaving.contains(&c.route) => c.route,
                _ => match leaving.last() {
                    Some(&i) => i,
                    None => return edge as usize,
                },
            };
            let over = (target - edge).abs();
            run = self.junctions[j].routes[route].run;
            let (nf, nl) = (self.runs[run].0 as i64, self.runs[run].1 as i64);
            target = if up { nf + over - 1 } else { nl - (over - 1) };
        }
        slice
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nfspu_formats::sim::Slice;

    fn slice(x: f32, z: f32) -> Slice {
        Slice { pos: [x, 0.0, z], up: [0.0, 1.0, 0.0], forward: [0.0, 0.0, -1.0], right: [-1.0, 0.0, 0.0], kind: 0, lanes: [1, 1], extra_widths: [5.0; 2], left_width: 4.0, right_width: 6.0 }
    }

    /// A road of 40 slices north, splitting at slice 20 into a branch east (group 2) that ends
    /// back on the road at slice 30.
    fn fork() -> SimData {
        let mut s: Vec<Slice> = (0..40).map(|i| slice(0.0, -6.0 * i as f32)).collect();
        // branch: from near slice 20 out east and back in near slice 30
        for i in 0..12 {
            let t = i as f32 / 11.0;
            s.push(slice(4.0 + 30.0 * (t * std::f32::consts::PI).sin(), -120.0 - 60.0 * t));
        }
        SimData { slices: s, groups: vec![(0, 40), (40, 12)] }
    }

    #[test]
    fn builds_junctions_and_picks_routes() {
        let r = Routes::build(&fork(), None);
        // the main road is cut where the branch leaves (slice 20) and where it comes back (30)
        assert_eq!(r.runs.len(), 4, "{:?}", r.runs);
        let j = r.junction_ahead(15, 10, Direction::Forward).expect("junction ahead");
        let out = r.leaving(j, true);
        assert_eq!(out.len(), 2);
        assert!(out.iter().all(|&i| r.junctions[j].routes[i].weight == 0.5));
        // expert, by weight: r 0.3 takes the first, r 0.8 the second
        let a = r.choose(j, 2, 0.3, Direction::Forward).unwrap();
        let b = r.choose(j, 2, 0.8, Direction::Forward).unwrap();
        assert_ne!(a.route, b.route);
        let ra = r.junctions[j].routes[a.route].run;
        let rb = r.junctions[j].routes[b.route].run;
        // walking past the junction follows the chosen run
        let end = r.runs[r.run_of(15)].1;
        assert_eq!(r.run_of(r.walk(end, 2, Direction::Forward, Some(a))), ra);
        assert_eq!(r.run_of(r.walk(end, 2, Direction::Forward, Some(b))), rb);
        // .jnc weights replace the defaults when the count matches
        let w = vec![[0.9, 0.1, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]; r.junctions.len()];
        assert!(Routes::build(&fork(), Some(&w)).from_file);
        assert!(!Routes::build(&fork(), Some(&w[1..])).from_file);
    }

    #[test]
    fn beginner_rules() {
        let mut r = Routes::default();
        r.runs = vec![(0, 9), (10, 19), (20, 29), (30, 39)];
        r.junction_of = vec![[None, Some(0)], [Some(0), None], [Some(0), None], [Some(0), None]];
        r.run_of = (0..40).map(|s| s / 10).collect();
        let route = |run, weight| Route { run, at_last: false, weight };
        r.junctions = vec![Junction { routes: vec![Route { run: 0, at_last: true, weight: 1.0 }, route(1, 0.8), route(2, 0.2)] }];
        // two routes, beginner: main (0.8) up to r = 0.55, the other above
        assert_eq!(r.choose(0, 0, 0.5, Direction::Forward).unwrap().route, 1);
        assert_eq!(r.choose(0, 0, 0.6, Direction::Forward).unwrap().route, 2);
        assert_eq!(r.choose(0, 1, 0.6, Direction::Forward).unwrap().route, 1);
        // three routes: heavy / middle / light by r
        r.junctions[0].routes.push(route(3, 0.35));
        r.junctions[0].routes[2].weight = 0.1;
        assert_eq!(r.choose(0, 0, 0.3, Direction::Forward).unwrap().route, 1);
        assert_eq!(r.choose(0, 0, 0.6, Direction::Forward).unwrap().route, 3);
        assert_eq!(r.choose(0, 0, 0.9, Direction::Forward).unwrap().route, 2);
    }
}
