//! The race path: road slices joined into a route graph, the lap fraction of every slice, a
//! tracker that follows a car from slice to slice, and the start grid.
//!
//! How the retail game does it (from the decomp, `docs/provenance/race.md`): slice groups are
//! split where another group's end meets them, the group ends are joined into junctions, the lap
//! length is the longest route (in slices) from the start junction, and each junction's lap
//! fraction is its distance along that route divided by the lap length. Inside a run of slices the
//! fraction is interpolated by slice count. The code here is our own implementation of that idea.
//! Positions are in Bevy space (x mirrored on load).

use bevy::math::Vec3;
use nfspu_formats::scn::{trigger_kind, Trigger};
use nfspu_formats::sim::SimData;

/// Squared distance (metres) under which a group end joins another group or junction. Retail uses
/// 256 (16 m) for both tests.
const JOIN_DIST2: f32 = 256.0;
/// A group is only split this many slices or more away from its own ends (retail: more than 5).
const SPLIT_MARGIN: usize = 6;
/// How far either side of its current slice the tracker looks each update.
const SEARCH: usize = 12;
/// Beyond this distance from the best nearby slice, the tracker searches the whole track.
const LOST_DIST: f32 = 40.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PathSlice {
    pub pos: Vec3,
    pub forward: Vec3,
    pub right: Vec3,
    pub up: Vec3,
    pub left_width: f32,
    pub right_width: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Segment {
    pub first: usize,
    pub last: usize,
    pub start_node: usize,
    pub end_node: usize,
    pub start_frac: f32,
    pub end_frac: f32,
}

impl Segment {
    pub fn len(&self) -> usize {
        self.last - self.first + 1
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Forward,
    Backward,
}

impl Direction {
    pub fn sign(self) -> f32 {
        match self {
            Self::Forward => 1.0,
            Self::Backward => -1.0,
        }
    }
}

pub struct RoadPath {
    pub slices: Vec<PathSlice>,
    pub segments: Vec<Segment>,
    /// Segment index of every slice.
    segment_of: Vec<usize>,
    /// Longest route from the start junction, in slices (the lap length).
    pub lap_slices: usize,
    /// True when the main route returns to the start junction (a circuit).
    pub looped: bool,
}

fn v(a: [f32; 3]) -> Vec3 {
    Vec3::new(a[0] * crate::world_mirror_x(), a[1], a[2])
}

fn flat_d2(a: Vec3, b: Vec3) -> f32 {
    let (dx, dz) = (a.x - b.x, a.z - b.z);
    dx * dx + dz * dz
}

impl RoadPath {
    pub fn new(sim: &SimData) -> Self {
        Self::with_mirror(sim, crate::world_mirror_x())
    }

    /// Builds the path with an explicit x mirror (the tests use this instead of the process-wide
    /// [`crate::set_world_mirrored`]).
    pub fn with_mirror(sim: &SimData, mirror: f32) -> Self {
        let v = |a: [f32; 3]| Vec3::new(a[0] * mirror, a[1], a[2]);
        let slices: Vec<PathSlice> = sim
            .slices
            .iter()
            .map(|s| PathSlice {
                pos: v(s.pos),
                forward: v(s.forward),
                right: v(s.right),
                up: v(s.up),
                left_width: s.left_width,
                right_width: s.right_width,
            })
            .collect();
        let mut runs: Vec<(usize, usize)> = sim.groups.iter().filter(|g| g.1 > 0).map(|g| (g.0, g.0 + g.1 - 1)).collect();
        if runs.is_empty() && !slices.is_empty() {
            runs.push((0, slices.len() - 1));
        }

        // 1. Split runs where another run's end touches their middle.
        let mut changed = true;
        while changed {
            changed = false;
            'outer: for a in 0..runs.len() {
                for end in [runs[a].0, runs[a].1] {
                    let p = slices[end].pos;
                    for b in 0..runs.len() {
                        if a == b {
                            continue;
                        }
                        let (f, l) = runs[b];
                        let (best, d2) = (f..=l).map(|k| (k, flat_d2(p, slices[k].pos))).fold((f, f32::MAX), |m, x| if x.1 < m.1 { x } else { m });
                        if d2 < JOIN_DIST2 && best - f >= SPLIT_MARGIN && l - best >= SPLIT_MARGIN {
                            runs[b] = (f, best);
                            runs.push((best + 1, l));
                            changed = true;
                            break 'outer;
                        }
                    }
                }
            }
        }
        runs.sort();

        // 2. Join run ends into junctions (union-find over the 2 * runs end points).
        let n = runs.len();
        let mut parent: Vec<usize> = (0..2 * n).collect();
        fn find(p: &mut Vec<usize>, i: usize) -> usize {
            let mut r = i;
            while p[r] != r {
                r = p[r];
            }
            p[i] = r;
            r
        }
        let end_pos = |e: usize| slices[if e % 2 == 0 { runs[e / 2].0 } else { runs[e / 2].1 }].pos;
        for a in 0..2 * n {
            for b in a + 1..2 * n {
                if flat_d2(end_pos(a), end_pos(b)) < JOIN_DIST2 {
                    let (ra, rb) = (find(&mut parent, a), find(&mut parent, b));
                    parent[ra] = rb;
                }
            }
        }
        let mut node_ids: Vec<usize> = Vec::new();
        let mut node_of = vec![0usize; 2 * n];
        for e in 0..2 * n {
            let r = find(&mut parent, e);
            node_of[e] = match node_ids.iter().position(|&x| x == r) {
                Some(i) => i,
                None => {
                    node_ids.push(r);
                    node_ids.len() - 1
                }
            };
        }
        let mut segments: Vec<Segment> = runs
            .iter()
            .enumerate()
            .map(|(i, r)| Segment { first: r.0, last: r.1, start_node: node_of[2 * i], end_node: node_of[2 * i + 1], start_frac: 0.0, end_frac: 0.0 })
            .collect();

        // 3. Longest route from the start junction (the start of slice 0's run).
        let start = segments.first().map(|s| s.start_node).unwrap_or(0);
        let mut depth = vec![-1i64; node_ids.len()];
        if !segments.is_empty() {
            depth[start] = 0;
        }
        let mut longest = 0i64;
        let mut looped = false;
        let mut on_path = vec![false; node_ids.len()];
        fn relax(node: usize, segs: &[Segment], start: usize, depth: &mut Vec<i64>, on_path: &mut Vec<bool>, longest: &mut i64, looped: &mut bool) {
            on_path[node] = true;
            for s in segs.iter().filter(|s| s.start_node == node) {
                let d = depth[node] + s.len() as i64;
                *longest = (*longest).max(d);
                if s.end_node == start {
                    *looped = true;
                    continue;
                }
                if on_path[s.end_node] || depth[s.end_node] >= d {
                    continue;
                }
                depth[s.end_node] = d;
                relax(s.end_node, segs, start, depth, on_path, longest, looped);
            }
            on_path[node] = false;
        }
        if !segments.is_empty() {
            relax(start, &segments, start, &mut depth, &mut on_path, &mut longest, &mut looped);
        }
        let lap = longest.max(1) as f32;
        let frac = |n: usize| if depth[n] <= 0 { 0.0 } else { depth[n] as f32 / lap };
        for s in &mut segments {
            s.start_frac = frac(s.start_node);
            s.end_frac = frac(s.end_node);
            if s.end_frac <= 0.01 {
                s.end_frac = 1.0; // a run back into the start junction ends the lap
            }
        }
        let mut segment_of = vec![0usize; slices.len()];
        for (i, s) in segments.iter().enumerate() {
            for k in s.first..=s.last {
                segment_of[k] = i;
            }
        }
        RoadPath { slices, segments, segment_of, lap_slices: longest.max(0) as usize, looped }
    }

    pub fn segment_of(&self, slice: usize) -> usize {
        self.segment_of[slice]
    }

    /// Lap fraction (0..1) of a slice, measured in the forward direction.
    pub fn forward_fraction(&self, slice: usize) -> f32 {
        let s = &self.segments[self.segment_of[slice]];
        s.start_frac + (s.end_frac - s.start_frac) * ((slice - s.first) as f32 / s.len() as f32)
    }

    /// Lap fraction for the given travel direction. Backwards it is mirrored, and on a circuit an
    /// exact 1.0 counts as 0.0 (as in retail).
    pub fn fraction(&self, slice: usize, dir: Direction) -> f32 {
        let f = self.forward_fraction(slice);
        match dir {
            Direction::Forward => f,
            Direction::Backward => {
                let b = 1.0 - f;
                if self.looped && b == 1.0 {
                    0.0
                } else {
                    b
                }
            }
        }
    }

    /// The runs that follow (`ahead`) or precede a run. Ahead, the one that leads furthest along
    /// the lap comes first (side roads and dead ends end at a lower fraction); behind, the one
    /// that starts earliest.
    fn neighbours(&self, seg: usize, ahead: bool) -> Vec<usize> {
        let s = &self.segments[seg];
        let mut n: Vec<usize> = (0..self.segments.len())
            .filter(|&i| if ahead { self.segments[i].start_node == s.end_node } else { self.segments[i].end_node == s.start_node })
            .collect();
        if ahead {
            n.sort_by(|&a, &b| self.segments[b].end_frac.total_cmp(&self.segments[a].end_frac).then(a.cmp(&b)));
        } else {
            n.sort_by(|&a, &b| self.segments[a].start_frac.total_cmp(&self.segments[b].start_frac).then(a.cmp(&b)));
        }
        n
    }

    /// Walks `steps` slices from `slice` in `dir` (negative steps walk the other way), taking the
    /// route that leads furthest at junctions (ours; retail's choice was not read). Stops at a
    /// dead end.
    pub fn walk(&self, slice: usize, steps: i32, dir: Direction) -> usize {
        let forward_steps = if dir == Direction::Forward { steps } else { -steps };
        let mut cur = slice;
        for _ in 0..forward_steps.unsigned_abs() {
            let seg = self.segment_of[cur];
            let s = self.segments[seg];
            cur = if forward_steps > 0 {
                if cur < s.last {
                    cur + 1
                } else {
                    match self.neighbours(seg, true).first() {
                        Some(&n) => self.segments[n].first,
                        None => return cur,
                    }
                }
            } else if cur > s.first {
                cur - 1
            } else {
                match self.neighbours(seg, false).first() {
                    Some(&n) => self.segments[n].last,
                    None => return cur,
                }
            };
        }
        cur
    }

    /// Nearest slice to a point over the whole track (3D distance).
    pub fn nearest(&self, p: Vec3) -> usize {
        (0..self.slices.len()).min_by(|&a, &b| self.slices[a].pos.distance_squared(p).total_cmp(&self.slices[b].pos.distance_squared(p))).unwrap_or(0)
    }

    /// The slice a car at `p` is on, searching near its previous slice and across junctions.
    pub fn track(&self, prev: usize, p: Vec3) -> usize {
        let seg = self.segment_of[prev];
        let s = self.segments[seg];
        let mut cands: Vec<usize> = (prev.saturating_sub(SEARCH).max(s.first)..=(prev + SEARCH).min(s.last)).collect();
        if prev + SEARCH > s.last {
            for n in self.neighbours(seg, true) {
                let t = self.segments[n];
                cands.extend(t.first..=(t.first + SEARCH).min(t.last));
            }
        }
        if prev < s.first + SEARCH {
            for n in self.neighbours(seg, false) {
                let t = self.segments[n];
                cands.extend(t.last.saturating_sub(SEARCH).max(t.first)..=t.last);
            }
        }
        let best = cands.into_iter().min_by(|&a, &b| self.slices[a].pos.distance_squared(p).total_cmp(&self.slices[b].pos.distance_squared(p))).unwrap_or(prev);
        if self.slices[best].pos.distance(p) > LOST_DIST {
            self.nearest(p)
        } else {
            best
        }
    }

    /// True when moving with `velocity` on `slice` goes against the race direction.
    pub fn wrong_way(&self, slice: usize, velocity: Vec3, dir: Direction) -> bool {
        // retail: velocity . slice forward < 0 (negated backwards). The 1 m/s guard is ours, so a
        // car standing still with physics jitter is not flagged.
        let along = velocity.dot(self.slices[slice].forward) * dir.sign();
        velocity.length() > 1.0 && along < 0.0
    }
}

/// Where one car starts: position on the road, heading (yaw, Bevy convention as in `car.rs`:
/// facing -Z at yaw 0) and the slice it is on.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GridSpot {
    pub pos: Vec3,
    pub yaw: f32,
    pub slice: usize,
}

/// The slice the start line is on: nearest to the start trigger, or the retail fallback when the
/// track has no start file (slice 1 on circuits; on point-to-point roads slice 50, or 1 if the
/// track has under 100 slices).
pub fn start_slice(path: &RoadPath, triggers: &[Trigger], dir: Direction) -> usize {
    if let Some(t) = triggers.iter().find(|t| t.kind == trigger_kind::START) {
        return path.nearest(v(t.pos));
    }
    let fallback = if !path.looped && path.slices.len() > 99 { 50 } else { 1 };
    match dir {
        Direction::Forward => fallback.min(path.slices.len().saturating_sub(1)),
        Direction::Backward if path.looped => path.walk(0, fallback as i32, Direction::Backward),
        Direction::Backward => path.slices.len().saturating_sub(1 + fallback),
    }
}

/// Grid spot for `grid_pos` (0 = pole) of `cars`. Retail places every car ahead of the start
/// slice: `(cars - grid_pos) * 3 + 1` slices with more than two cars, else 1, alternating left
/// and right of the centre at half of that side's half-width, 0.5 m up.
pub fn grid_spot(path: &RoadPath, start: usize, grid_pos: usize, cars: usize, dir: Direction) -> GridSpot {
    let steps = if cars > 2 { ((cars - grid_pos.min(cars)) * 3 + 1) as i32 } else { 1 };
    let slice = path.walk(start, steps, dir);
    let s = &path.slices[slice];
    // side 0 = left of the slice's right vector, side 1 = right. Backwards, the slice's right is
    // the driver's left, so the sides are swapped to keep pole on the same side of the car (guess).
    let mut side = grid_pos & 1;
    if dir == Direction::Backward {
        side = 1 - side;
    }
    let lateral = if side == 0 { -0.5 * s.left_width } else { 0.5 * s.right_width };
    let heading = s.forward * dir.sign();
    GridSpot { pos: s.pos + s.right * lateral + s.up * 0.5, yaw: (-heading.x).atan2(-heading.z), slice }
}

/// The trigger line the race finishes on, if the track has one (point-to-point roads).
pub fn finish_line(triggers: &[Trigger]) -> Option<(Vec3, Vec3)> {
    triggers.iter().find(|t| t.kind == trigger_kind::FINISH).map(|t| (v(t.ends[0]), v(t.ends[1])))
}

/// True when the move from `a` to `b` crosses the line `l0`-`l1` (seen from above).
pub fn crosses(a: Vec3, b: Vec3, l0: Vec3, l1: Vec3) -> bool {
    let cross = |o: Vec3, p: Vec3, q: Vec3| (p.x - o.x) * (q.z - o.z) - (p.z - o.z) * (q.x - o.x);
    let (d1, d2) = (cross(l0, l1, a), cross(l0, l1, b));
    let (d3, d4) = (cross(a, b, l0), cross(a, b, l1));
    (d1 > 0.0) != (d2 > 0.0) && (d3 > 0.0) != (d4 > 0.0)
}

/// Everything a race needs from a track's files, for one direction.
pub struct TrackRace {
    pub path: RoadPath,
    pub dir: Direction,
    /// Triggers of `<track>_fstart.scn` or `_bstart.scn` (empty if the track has none).
    pub triggers: Vec<Trigger>,
    pub start_slice: usize,
    pub finish: Option<(Vec3, Vec3)>,
}

impl TrackRace {
    /// Point-to-point when the road does not loop back to its start.
    pub fn point_to_point(&self) -> bool {
        !self.path.looped
    }

    /// Length of one lap (or of the whole road) in metres, summed along the main route.
    pub fn lap_metres(&self) -> f32 {
        let mut s = 0usize;
        let mut d = 0.0;
        for _ in 0..self.path.lap_slices.saturating_sub(1) {
            let n = self.path.walk(s, 1, Direction::Forward);
            if n == s {
                break;
            }
            d += self.path.slices[s].pos.distance(self.path.slices[n].pos);
            s = n;
        }
        d
    }
}

/// The racing line of a track and direction, `GameData/Track/racer/work/<track><0|1>.lsp` (0
/// forward, 1 backward; `nfspu_formats::aidata::parse_lsp`), or None without one.
pub fn load_racing_line(track: &str, dir: Direction) -> Option<Vec<[f32; 2]>> {
    let file = crate::track::track_dir().ok()?.join("racer").join("work").join(format!("{track}{}.lsp", if dir == Direction::Forward { 0 } else { 1 }));
    nfspu_formats::aidata::parse_lsp(&std::fs::read(file).ok()?)
}

/// Reads the slices and the start/finish scene of a track from the install.
pub fn load(track: &str, dir: Direction) -> Result<TrackRace, String> {
    use nfspu_formats::{crp, refpack, scn, sim};
    let tdir = crate::track::track_dir()?;
    let raw = std::fs::read(tdir.join(format!("{track}.crp"))).map_err(|e| format!("{track}.crp: {e}"))?;
    let data = refpack::decompress(&raw).map_err(|e| format!("{e}"))?;
    let c = crp::parse(&data).map_err(|e| format!("{e}"))?;
    let path = RoadPath::new(&sim::read(&c).map_err(|e| format!("{e}"))?);
    let suffix = if dir == Direction::Forward { "fstart" } else { "bstart" };
    // file names differ in case between tracks; Windows does not mind, but look it up anyway
    let wanted = format!("{track}_{suffix}.scn").to_ascii_lowercase();
    let file = std::fs::read_dir(&tdir).map_err(|e| format!("{e}"))?.filter_map(|e| e.ok()).find(|e| e.file_name().to_string_lossy().to_ascii_lowercase() == wanted);
    let triggers = match file {
        Some(f) => scn::triggers(&scn::parse(&std::fs::read_to_string(f.path()).map_err(|e| format!("{e}"))?)),
        None => Vec::new(),
    };
    let start_slice = start_slice(&path, &triggers, dir);
    let finish = finish_line(&triggers);
    Ok(TrackRace { path, dir, triggers, start_slice, finish })
}

#[cfg(test)]
mod tests {
    use super::*;
    use nfspu_formats::sim::Slice;

    fn slice(x: f32, z: f32, fwd: [f32; 3]) -> Slice {
        Slice { pos: [x, 0.0, z], up: [0.0, 1.0, 0.0], forward: fwd, right: [fwd[2], 0.0, -fwd[0]], kind: 0, lanes: [1, 1], extra_widths: [5.0; 2], left_width: 4.0, right_width: 6.0 }
    }

    /// A square loop of 4 x 25 slices 6 m apart (one group).
    fn square() -> SimData {
        let mut s = Vec::new();
        for i in 0..25 {
            s.push(slice(0.0, -6.0 * i as f32, [0.0, 0.0, -1.0]));
        }
        for i in 0..25 {
            s.push(slice(6.0 * i as f32, -150.0, [1.0, 0.0, 0.0]));
        }
        for i in 0..25 {
            s.push(slice(150.0, -150.0 + 6.0 * i as f32, [0.0, 0.0, 1.0]));
        }
        for i in 0..25 {
            s.push(slice(150.0 - 6.0 * i as f32, 0.0, [-1.0, 0.0, 0.0]));
        }
        SimData { groups: vec![(0, s.len())], slices: s }
    }

    #[test]
    fn single_loop_is_a_circuit_with_linear_fraction() {
        let p = RoadPath::new(&square());
        assert!(p.looped);
        assert_eq!(p.segments.len(), 1);
        assert_eq!(p.lap_slices, 100);
        assert_eq!(p.forward_fraction(0), 0.0);
        assert!((p.forward_fraction(50) - 0.5).abs() < 1e-6);
        assert!((p.fraction(25, Direction::Backward) - 0.75).abs() < 1e-6);
        assert_eq!(p.fraction(0, Direction::Backward), 0.0);
        assert_eq!(p.walk(98, 3, Direction::Forward), 1);
        assert_eq!(p.walk(1, 3, Direction::Backward), 98);
    }

    #[test]
    fn branch_splits_main_road_and_shares_fractions() {
        // main road 0..60 straight north, a 20-slice bypass from slice 20 to slice 40
        let mut s: Vec<Slice> = (0..61).map(|i| slice(0.0, -6.0 * i as f32, [0.0, 0.0, -1.0])).collect();
        for i in 0..20 {
            s.push(slice(20.0, -120.0 - 6.0 * i as f32, [0.0, 0.0, -1.0]));
        }
        // bypass ends 14 m from slices 20 and 39/40
        s[61].pos = [12.0, 0.0, -126.0];
        s[80].pos = [12.0, 0.0, -234.0];
        let p = RoadPath::new(&SimData { groups: vec![(0, 61), (61, 20)], slices: s });
        assert!(!p.looped, "a road that does not come back is point-to-point");
        assert_eq!(p.segments.len(), 4, "main road split twice, plus the bypass");
        assert_eq!(p.lap_slices, 63, "22 + the longer bypass (20) + 21");
        let f_main = p.forward_fraction(21);
        let f_bypass = p.forward_fraction(61);
        assert!((f_main - f_bypass).abs() < 0.03, "both routes start at the same fraction: {f_main} {f_bypass}");
        assert!(p.forward_fraction(60) > 0.98);
    }

    #[test]
    fn tracker_follows_and_finds_lost_cars() {
        let p = RoadPath::new(&square());
        let mut s = 0;
        for i in 0..100 {
            let target = p.slices[(i + 1) % 100].pos + Vec3::new(1.0, 0.0, 0.0);
            s = p.track(s, target);
        }
        assert_eq!(s, 0);
        assert_eq!(p.track(0, p.slices[60].pos), 60, "far jump falls back to a full search");
        assert!(p.wrong_way(10, Vec3::new(0.0, 0.0, 5.0), Direction::Forward));
        assert!(!p.wrong_way(10, Vec3::new(0.0, 0.0, -5.0), Direction::Forward));
        assert!(p.wrong_way(10, Vec3::new(0.0, 0.0, -5.0), Direction::Backward));
        assert!(!p.wrong_way(10, Vec3::new(0.0, 0.0, 0.3), Direction::Forward));
    }

    #[test]
    fn grid_matches_retail_spacing() {
        let p = RoadPath::new(&square());
        // 8 cars: pole 25 slices ahead, last car 4 slices ahead
        assert_eq!(grid_spot(&p, 0, 0, 8, Direction::Forward).slice, 25);
        assert_eq!(grid_spot(&p, 0, 7, 8, Direction::Forward).slice, 4);
        assert_eq!(grid_spot(&p, 0, 0, 1, Direction::Forward).slice, 1);
        // pole on the left (-2 m along right), second on the right (+3 m)
        let a = grid_spot(&p, 0, 7, 8, Direction::Forward);
        let b = grid_spot(&p, 0, 6, 8, Direction::Forward);
        let right = p.slices[4].right;
        assert!(((a.pos - p.slices[4].pos).dot(right) - 3.0).abs() < 1e-4);
        assert!(((b.pos - p.slices[7].pos).dot(right) + 2.0).abs() < 1e-4);
        // heading along the slice: on the first side of the square that is -Z, yaw 0
        assert!(a.yaw.abs() < 1e-5);
    }

    #[test]
    fn a_mirrored_path_is_the_x_mirror_of_the_usual_one() {
        let sim = square();
        let normal = RoadPath::with_mirror(&sim, -1.0);
        let mirrored = RoadPath::with_mirror(&sim, 1.0);
        for (a, b) in normal.slices.iter().zip(&mirrored.slices) {
            assert!((a.pos.x + b.pos.x).abs() < 1e-5);
            assert_eq!((a.pos.y, a.pos.z), (b.pos.y, b.pos.z));
            assert!((a.forward.x + b.forward.x).abs() < 1e-5);
            assert!((a.right.x + b.right.x).abs() < 1e-5);
        }
        assert_eq!(normal.lap_slices, mirrored.lap_slices);
        assert_eq!(normal.looped, mirrored.looped);
        // The start grid mirrors too: same slice, x mirrored, heading yaw negated (slice 25 heads
        // along x, so its yaw is not zero).
        let a = grid_spot(&normal, 0, 0, 8, Direction::Forward);
        let b = grid_spot(&mirrored, 0, 0, 8, Direction::Forward);
        assert_eq!(a.slice, b.slice);
        assert!((a.pos.x + b.pos.x).abs() < 1e-4);
        assert!((a.pos.z - b.pos.z).abs() < 1e-4);
        assert!((a.yaw + b.yaw).abs() < 1e-4, "yaw {} vs {}", a.yaw, b.yaw);
    }

    #[test]
    fn crossing_test() {
        let (l0, l1) = (Vec3::new(-5.0, 0.0, 0.0), Vec3::new(5.0, 0.0, 0.0));
        assert!(crosses(Vec3::new(0.0, 0.0, 1.0), Vec3::new(0.0, 0.0, -1.0), l0, l1));
        assert!(!crosses(Vec3::new(0.0, 0.0, 1.0), Vec3::new(0.0, 0.0, 0.5), l0, l1));
        assert!(!crosses(Vec3::new(9.0, 0.0, 1.0), Vec3::new(9.0, 0.0, -1.0), l0, l1));
    }
}
