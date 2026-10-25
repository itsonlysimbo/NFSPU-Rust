//! Factory Driver, the second career: 34 test-driver missions for Porsche's test team, in three
//! ranks. Data is `FEData/Data/nfs5.fac` (34 records of 0xf0 bytes) plus, for the timed courses,
//! one scene file per mission, `GameData/Track/<track>_st<id>.scn` (gates and cones).
//!
//! **Verified** (record layout, play order, pass/fail rules) from the data and the executable:
//! see `docs/provenance/career.md`, "Factory Driver". **Guess**: cone hit radius, gate capture
//! rules, the unit of the cone penalty (seconds), the unit of the damage limit (percent of the
//! average zone damage), the exact retail timing of the time-out. Our progress lives in our own
//! file, `settings/factory/<player>.txt`; the install and its saves are never written.

use nfspu_formats::scn;
use std::path::{Path, PathBuf};

/// Bytes per record of `nfs5.fac`.
pub const RECORD: usize = 0xf0;
/// The race clock rate (ticks per second).
pub const TICK_HZ: f32 = 64.0;
/// A cone is knocked over when the car's centre passes this close (**guess**: half a car plus the
/// cone).
pub const CONE_HIT_RADIUS: f32 = 1.3;
/// A gate counts when the car crosses its line, extended this far at both ends (**guess**).
pub const GATE_SLACK: f32 = 1.0;
/// Gates this wide or narrower mark a single cone to knock down; getting this close also counts.
pub const NARROW_GATE: f32 = 4.0;
pub const NARROW_CAPTURE: f32 = 2.5;

fn u16_at(b: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([b[at], b[at + 1]])
}
fn u32_at(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
}

/// What a mission asks of the driver.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// Race the listed drivers; finish no lower than `min_place`.
    Race,
    /// Drive from the start to the one finish gate within the limits.
    Delivery,
    /// Pass the gates in order (slalom, S-curve, spin, rally, cone hunt) within the limits.
    Course,
}

/// One record of `nfs5.fac`.
#[derive(Debug, Clone, PartialEq)]
pub struct Mission {
    /// Position in the file; also the play order (the first record not yet done is next).
    pub index: usize,
    /// String id of the briefing (`u32` at 0).
    pub brief: u32,
    /// Rank 1-3: Junior Test Driver, Test Driver, Chief Test Driver (`u16` at 8).
    pub level: u16,
    /// String id of the promotion text shown when this rank's last mission is passed (at 0xc).
    pub promotion: u32,
    /// Who talks: 0 Billy, 1 Stephanie, 2 Dieter, 3 Frank, 4 Klaus, 5 Rolf (`u16` at 0x1e; the
    /// name is string 2500 + this).
    pub narrator: u16,
    /// Track id in `nfs5.trk` (`i16` at 0x20).
    pub track: i16,
    /// Route code of the race missions (`u16` at 0x22; 0 for the scene missions).
    pub route: u16,
    /// Scene number (`u16` at 0x24): the file is `<track>_st<scene>.scn`. 0 for race missions.
    pub scene: u16,
    /// Picture / script stem (`0M01`, `1M01`...) at 0x3f.
    pub stem: String,
    /// Strings said on a pass (up to three, 0 = unused) and on a fail (at 0x58 and 0x64).
    pub pass_text: [u32; 3],
    pub fail_text: [u32; 3],
    /// Car (catalogue id, `u16` at 0x70) and its part ids (count at 0x72, list at 0x74).
    pub car: u16,
    pub parts: Vec<u16>,
    /// Paint colours (`u32` at 0xa0 and 0xb0, as stored).
    pub colours: [u32; 2],
    /// Seconds allowed (byte 0xc1 enables, `u16` at 0xc2).
    pub time_limit: Option<u16>,
    /// Average zone damage allowed (byte 0xc4 enables, `u16` at 0xc6; **guess**: percent).
    pub damage_limit: Option<u16>,
    /// Most cones that may fall (`u16` at 0xcc when byte 0xc8 is set and `u16` at 0xca is 0).
    pub cone_limit: Option<u16>,
    /// Seconds added per fallen cone (`u16` at 0xce when byte 0xc8 is set and `u16` at 0xca is 1).
    pub cone_penalty: Option<u16>,
    /// Lowest place that passes a race (byte 0xd0 enables, `u16` at 0xd2).
    pub min_place: Option<u16>,
    /// Opponents (`u16` driver id at 0xd4 + 4 n, car id after it; five slots).
    pub opponents: Vec<(u16, u16)>,
}

impl Mission {
    fn parse(index: usize, r: &[u8]) -> Mission {
        let flag = |at: usize| r[at] != 0;
        let parts = (0..u16_at(r, 0x72).min(40) as usize).map(|i| u16_at(r, 0x74 + 2 * i)).collect();
        let (cone_on, cone_kind) = (flag(0xc8), u16_at(r, 0xca));
        let stem: String = r[0x3f..0x50].iter().take_while(|b| **b != 0).map(|b| *b as char).collect();
        Mission {
            index,
            brief: u32_at(r, 0),
            level: u16_at(r, 8),
            promotion: u32_at(r, 0xc),
            narrator: u16_at(r, 0x1e),
            track: u16_at(r, 0x20) as i16,
            route: u16_at(r, 0x22),
            scene: u16_at(r, 0x24),
            stem,
            pass_text: [u32_at(r, 0x58), u32_at(r, 0x5c), u32_at(r, 0x60)],
            fail_text: [u32_at(r, 0x64), u32_at(r, 0x68), u32_at(r, 0x6c)],
            car: u16_at(r, 0x70),
            parts,
            colours: [u32_at(r, 0xa0), u32_at(r, 0xb0)],
            time_limit: flag(0xc1).then(|| u16_at(r, 0xc2)),
            damage_limit: flag(0xc4).then(|| u16_at(r, 0xc6)),
            cone_limit: (cone_on && cone_kind == 0).then(|| u16_at(r, 0xcc)),
            cone_penalty: (cone_on && cone_kind == 1).then(|| u16_at(r, 0xce)),
            min_place: flag(0xd0).then(|| u16_at(r, 0xd2)),
            opponents: (0..5).map(|i| (u16_at(r, 0xd4 + 4 * i), u16_at(r, 0xd6 + 4 * i))).filter(|(d, _)| *d != 0).collect(),
        }
    }

    /// Race, point-to-point delivery or gate course (see [`Kind`]).
    pub fn kind(&self, scene: Option<&Scene>) -> Kind {
        if self.min_place.is_some() || self.scene == 0 {
            Kind::Race
        } else if scene.is_some_and(|s| s.gates.len() <= 1) {
            Kind::Delivery
        } else {
            Kind::Course
        }
    }

    /// The narrator's name string id.
    pub fn narrator_string(&self) -> u32 {
        2500 + self.narrator as u32
    }
}

/// Reads every record of `nfs5.fac`.
pub fn parse_missions(data: &[u8]) -> Vec<Mission> {
    data.chunks_exact(RECORD).enumerate().map(|(i, r)| Mission::parse(i, r)).collect()
}

/// The install's mission table, `FEData/Data/nfs5.fac`.
pub fn load_missions(game_dir: &Path) -> Vec<Mission> {
    std::fs::read(game_dir.join("FEData").join("Data").join("nfs5.fac")).map(|d| parse_missions(&d)).unwrap_or_default()
}

/// One gate of a mission scene: a line the car crosses, in the order of `seq`.
#[derive(Debug, Clone, PartialEq)]
pub struct Gate {
    pub seq: u32,
    /// 0 waypoint, 1 the finish.
    pub kind: i32,
    pub ends: [[f32; 3]; 2],
}

impl Gate {
    pub fn centre(&self) -> [f32; 3] {
        let [a, b] = self.ends;
        [(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0, (a[2] + b[2]) / 2.0]
    }
    pub fn width(&self) -> f32 {
        dist2d(self.ends[0], self.ends[1])
    }
}

/// The gates and cones of one mission (file coordinates; flip x with `world_mirror_x` to place
/// them in the world).
#[derive(Debug, Clone, PartialEq)]
pub struct Scene {
    pub start: [f32; 3],
    /// Unit heading (x, z) the car starts facing.
    pub heading: [f32; 2],
    /// Waypoints then the finish, in the order they must be passed.
    pub gates: Vec<Gate>,
    pub cones: Vec<[f32; 3]>,
}

fn dist2d(a: [f32; 3], b: [f32; 3]) -> f32 {
    ((a[0] - b[0]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

/// Reads a mission scene. `None` when it has no start or no finish.
pub fn parse_scene(text: &str) -> Option<Scene> {
    let elements = scn::parse(text);
    let mut start = None;
    let mut gates = Vec::new();
    let mut cones = Vec::new();
    for e in &elements {
        if e.kind == "TRIGGER" {
            // versions 3 (race scenes) and 5 share the first lines
            let params: Vec<&str> = e.lines.first().map(|l| l.split_whitespace().collect()).unwrap_or_default();
            let ends: Vec<f32> = e.lines.get(1).map(|l| l.split_whitespace().filter_map(|t| t.parse().ok()).collect()).unwrap_or_default();
            let (Some(kind), true) = (params.first().and_then(|k| k.parse::<i32>().ok()), ends.len() == 6) else { continue };
            let t = scn::Trigger { kind, width: 0.0, pos: e.pos, ends: [[ends[0], ends[1], ends[2]], [ends[3], ends[4], ends[5]]] };
            let seq = params.get(7).and_then(|s| s.parse().ok()).unwrap_or(0);
            if t.kind == scn::trigger_kind::START {
                let tail: Vec<f32> = e.lines.get(2).map(|l| l.split_whitespace().filter_map(|t| t.parse().ok()).collect()).unwrap_or_default();
                let h = [tail.get(5).copied().unwrap_or(0.0), tail.get(7).copied().unwrap_or(1.0)];
                let n = (h[0] * h[0] + h[1] * h[1]).sqrt().max(1e-6);
                start = Some((t.pos, [h[0] / n, h[1] / n]));
            } else {
                gates.push(Gate { seq, kind: t.kind, ends: t.ends });
            }
        } else if e.geom_tag() == Some(*b"CONE") {
            cones.push(e.pos);
        }
    }
    gates.sort_by_key(|g| g.seq);
    gates.last()?;
    let (start, heading) = start?;
    Some(Scene { start, heading, gates, cones })
}

/// The scene file of a mission: `<track file>_st<scene>.scn` in `GameData/Track`.
pub fn scene_path(game_dir: &Path, track_file: &str, scene: u16) -> PathBuf {
    game_dir.join("GameData").join("Track").join(format!("{track_file}_st{scene}.scn"))
}

/// Loads the scene of `m` (`None` for race missions or a missing file).
pub fn load_scene(game_dir: &Path, tracks: &[crate::catalog::Track], m: &Mission) -> Option<Scene> {
    if m.scene == 0 {
        return None;
    }
    let track = tracks.iter().find(|t| t.id == m.track)?;
    parse_scene(&std::fs::read_to_string(scene_path(game_dir, &track.file, m.scene)).ok()?)
}

/// Why a mission was failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fail {
    Time,
    Damage,
    Cones,
    Place,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Running,
    Passed,
    Failed(Fail),
}

/// What a finished mission is saved as: retail keeps three shorts per record (place, average
/// damage, time ticks; at 0xea, 0xec, 0xee).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Outcome {
    pub place: u16,
    pub damage_pct: u16,
    pub ticks: u16,
}

/// One car sample for the mission rules (file coordinates for the position).
#[derive(Debug, Clone, Copy)]
pub struct Sample {
    pub pos: [f32; 3],
    /// Average of the car's zone damage, 0-100.
    pub damage_pct: f32,
    /// The player's place in a race mission (1 = first); ignored on courses.
    pub place: u16,
}

/// A mission in progress.
#[derive(Debug, Clone)]
pub struct Run {
    next: usize,
    ticks: u32,
    hit: Vec<bool>,
    last: Option<[f32; 3]>,
    status: Status,
    damage_pct: f32,
    place: u16,
}

fn cross2(a: [f32; 2], b: [f32; 2]) -> f32 {
    a[0] * b[1] - a[1] * b[0]
}

/// Whether the move `p -> q` crosses the segment `a -> b` (x/z plane).
fn segments_cross(p: [f32; 3], q: [f32; 3], a: [f32; 3], b: [f32; 3]) -> bool {
    let r = [q[0] - p[0], q[2] - p[2]];
    let s = [b[0] - a[0], b[2] - a[2]];
    let d = cross2(r, s);
    if d.abs() < 1e-9 {
        return false;
    }
    let ap = [a[0] - p[0], a[2] - p[2]];
    let t = cross2(ap, s) / d;
    let u = cross2(ap, r) / d;
    (0.0..=1.0).contains(&t) && (0.0..=1.0).contains(&u)
}

/// Distance from `c` to the segment `a -> b` (x/z plane).
fn dist_to_segment(c: [f32; 3], a: [f32; 3], b: [f32; 3]) -> f32 {
    let ab = [b[0] - a[0], b[2] - a[2]];
    let l2 = (ab[0] * ab[0] + ab[1] * ab[1]).max(1e-9);
    let t = (((c[0] - a[0]) * ab[0] + (c[2] - a[2]) * ab[1]) / l2).clamp(0.0, 1.0);
    ((c[0] - a[0] - t * ab[0]).powi(2) + (c[2] - a[2] - t * ab[1]).powi(2)).sqrt()
}

impl Run {
    pub fn new(scene: Option<&Scene>) -> Run {
        Run { next: 0, ticks: 0, hit: vec![false; scene.map_or(0, |s| s.cones.len())], last: None, status: Status::Running, damage_pct: 0.0, place: 0 }
    }

    pub fn status(&self) -> Status {
        self.status
    }
    pub fn ticks(&self) -> u32 {
        self.ticks
    }
    pub fn gates_passed(&self) -> usize {
        self.next
    }
    pub fn cone_is_hit(&self, i: usize) -> bool {
        self.hit.get(i).copied().unwrap_or(false)
    }
    pub fn cones_hit(&self) -> usize {
        self.hit.iter().filter(|h| **h).count()
    }

    /// Driving time plus the cone penalty, in seconds.
    pub fn score_time(&self, m: &Mission) -> f32 {
        self.ticks as f32 / TICK_HZ + self.cones_hit() as f32 * m.cone_penalty.unwrap_or(0) as f32
    }

    /// The three numbers retail saves for a finished mission.
    pub fn outcome(&self) -> Outcome {
        Outcome { place: self.place, damage_pct: self.damage_pct.round().clamp(0.0, 100.0) as u16, ticks: self.ticks.min(u16::MAX as u32) as u16 }
    }

    fn judge(&self, m: &Mission) -> Status {
        if m.time_limit.is_some_and(|l| self.score_time(m) > l as f32) {
            return Status::Failed(Fail::Time);
        }
        if m.damage_limit.is_some_and(|l| self.damage_pct > l as f32) {
            return Status::Failed(Fail::Damage);
        }
        if m.cone_limit.is_some_and(|l| self.cones_hit() > l as usize) {
            return Status::Failed(Fail::Cones);
        }
        if m.min_place.is_some_and(|p| self.place > p) {
            return Status::Failed(Fail::Place);
        }
        Status::Passed
    }

    /// One race tick of a gate mission: counts time, knocks cones, passes gates in order. A mission
    /// fails the moment the time limit runs out (**guess**: retail may let the clock run on) and is
    /// judged when the finish gate is crossed.
    pub fn tick(&mut self, m: &Mission, scene: &Scene, s: &Sample) -> Status {
        if self.status != Status::Running {
            return self.status;
        }
        self.ticks += 1;
        self.damage_pct = s.damage_pct;
        self.place = s.place;
        for (cone, hit) in scene.cones.iter().zip(self.hit.iter_mut()) {
            if !*hit && dist2d(*cone, s.pos) <= CONE_HIT_RADIUS {
                *hit = true;
            }
        }
        if let (Some(prev), Some(gate)) = (self.last, scene.gates.get(self.next)) {
            let [a, b] = gate.ends;
            let w = gate.width().max(1e-3);
            let dir = [(b[0] - a[0]) / w * GATE_SLACK, (b[1] - a[1]) / w * GATE_SLACK, (b[2] - a[2]) / w * GATE_SLACK];
            let (a2, b2) = ([a[0] - dir[0], a[1] - dir[1], a[2] - dir[2]], [b[0] + dir[0], b[1] + dir[1], b[2] + dir[2]]);
            let narrow = w <= NARROW_GATE && dist_to_segment(s.pos, a, b) <= NARROW_CAPTURE;
            if narrow || segments_cross(prev, s.pos, a2, b2) {
                self.next += 1;
                if self.next >= scene.gates.len() {
                    self.status = self.judge(m);
                    self.last = Some(s.pos);
                    return self.status;
                }
            }
        }
        self.last = Some(s.pos);
        if m.time_limit.is_some_and(|l| self.score_time(m) > l as f32) {
            self.status = Status::Failed(Fail::Time);
        }
        self.status
    }

    /// Ends a race mission: the place, time and damage the race produced.
    pub fn finish_race(&mut self, m: &Mission, place: u16, ticks: u32, damage_pct: f32) -> Status {
        self.place = place;
        self.ticks = ticks;
        self.damage_pct = damage_pct;
        self.status = self.judge(m);
        self.status
    }
}

/// Where a mission scene car starts (file coordinates) and which way it faces.
pub fn start_pose(scene: &Scene) -> ([f32; 3], [f32; 2]) {
    (scene.start, scene.heading)
}

/// Rank names by `Mission::level` (the promotion text names them too).
pub const RANKS: [&str; 4] = ["Junior Test Driver", "Test Driver", "Chief Test Driver", "Ace Test Driver"];

/// The player's mission progress: one slot per record; `Some` once passed.
#[derive(Debug, Clone, PartialEq)]
pub struct Progress {
    pub player: String,
    pub done: Vec<Option<Outcome>>,
}

impl Progress {
    pub fn new(player: &str, missions: usize) -> Progress {
        Progress { player: player.to_string(), done: vec![None; missions] }
    }

    /// The next mission to play: the first record not passed (retail's rule). `None` when all are.
    pub fn current(&self) -> Option<usize> {
        self.done.iter().position(Option::is_none)
    }

    pub fn all_done(&self) -> bool {
        self.current().is_none()
    }

    /// The rank now held (0 Junior ... 3 Ace when every mission is done).
    pub fn rank(&self, missions: &[Mission]) -> usize {
        self.current().and_then(|i| missions.get(i)).map_or(3, |m| m.level.saturating_sub(1) as usize)
    }

    /// Records a finished mission. Only the current mission counts and only a pass is kept; returns
    /// whether the rank changed (the promotion message should be shown, the mission's own
    /// `promotion` string).
    pub fn record(&mut self, missions: &[Mission], index: usize, status: Status, outcome: Outcome) -> bool {
        if status != Status::Passed || self.current() != Some(index) {
            return false;
        }
        self.done[index] = Some(outcome);
        let next_level = self.current().and_then(|i| missions.get(i)).map(|m| m.level);
        next_level != missions.get(index).map(|m| m.level)
    }

    /// Retail's mission cheat: marks every mission before number `n` (1-based) passed and the rest
    /// not.
    pub fn jump_to(&mut self, n: usize) {
        let n = n.clamp(1, self.done.len().max(1)) - 1;
        for (i, d) in self.done.iter_mut().enumerate() {
            *d = if i < n { Some(Outcome { place: 0, damage_pct: 0, ticks: 0 }) } else { None };
        }
    }

    /// Our folder for factory-driver files: `NFSPU_SETTINGS_DIR/factory`, else `settings/factory`
    /// in the project (never the install).
    pub fn dir() -> PathBuf {
        std::env::var_os("NFSPU_SETTINGS_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..").join("..").join("settings"))
            .join("factory")
    }

    pub fn path_in(dir: &Path, player: &str) -> PathBuf {
        let safe: String = player.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' }).collect();
        dir.join(format!("{safe}.txt"))
    }

    pub fn to_text(&self) -> String {
        let mut s = format!("version = 1\nplayer = {}\nmissions = {}\n", self.player, self.done.len());
        for (i, d) in self.done.iter().enumerate() {
            if let Some(o) = d {
                s.push_str(&format!("done = {i},{},{},{}\n", o.place, o.damage_pct, o.ticks));
            }
        }
        s
    }

    pub fn parse(text: &str) -> Option<Progress> {
        let mut player = None;
        let mut n = 0usize;
        let mut done = Vec::new();
        for line in text.lines() {
            let Some((k, v)) = line.split_once('=') else { continue };
            let v = v.trim();
            match k.trim() {
                "player" => player = Some(v.to_string()),
                "missions" => n = v.parse().ok()?,
                "done" => {
                    let f: Vec<u16> = v.split(',').filter_map(|x| x.trim().parse().ok()).collect();
                    if f.len() == 4 {
                        done.push((f[0] as usize, Outcome { place: f[1], damage_pct: f[2], ticks: f[3] }));
                    }
                }
                _ => {}
            }
        }
        let mut p = Progress::new(&player?, n);
        for (i, o) in done {
            *p.done.get_mut(i)? = Some(o);
        }
        Some(p)
    }

    pub fn load_from(dir: &Path, player: &str) -> Option<Progress> {
        Self::parse(&std::fs::read_to_string(Self::path_in(dir, player)).ok()?)
    }

    pub fn save_to(&self, dir: &Path) -> std::io::Result<()> {
        std::fs::create_dir_all(dir)?;
        std::fs::write(Self::path_in(dir, &self.player), self.to_text())
    }
}

/// A scripted driver for the headless runs: follows the gate centres at `speed` m/s, optionally
/// swinging `swerve` metres off the gate centres so it knocks cones, with `damage` fixed.
pub struct Driver {
    pub speed: f32,
    pub damage_pct: f32,
    /// Drives straight at the cone nearest each gate instead of its centre (fails cone rules).
    pub hit_cones: bool,
}

/// The result of a headless mission run.
#[derive(Debug, Clone, PartialEq)]
pub struct DriveReport {
    pub status: Status,
    pub gates: usize,
    pub gates_total: usize,
    pub cones_hit: usize,
    pub seconds: f32,
    pub score: f32,
}

/// Runs a gate mission headlessly: the scripted driver starts at the scene's start and heads for
/// each gate's centre in turn (steering is not simulated: a straight line between gate centres).
/// Gives up after ten minutes of mission time.
pub fn drive(m: &Mission, scene: &Scene, d: &Driver) -> DriveReport {
    let mut run = Run::new(Some(scene));
    let mut pos = scene.start;
    let dt = 1.0 / TICK_HZ;
    let limit = (600.0 * TICK_HZ) as u32;
    for _ in 0..limit {
        let Some(gate) = scene.gates.get(run.gates_passed()) else { break };
        let mut target = gate.centre();
        if d.hit_cones {
            if let Some(c) = scene.cones.iter().enumerate().filter(|(i, _)| !run.cone_is_hit(*i)).map(|(_, c)| c).min_by(|a, b| dist2d(**a, pos).total_cmp(&dist2d(**b, pos))) {
                target = *c;
            }
        }
        let to = [target[0] - pos[0], target[2] - pos[2]];
        let len = (to[0] * to[0] + to[1] * to[1]).sqrt().max(1e-6);
        let step = (d.speed * dt).min(len + 2.0 * GATE_SLACK + 1.0);
        let next = [pos[0] + to[0] / len * step, target[1], pos[2] + to[1] / len * step];
        let s = Sample { pos: next, damage_pct: d.damage_pct, place: 1 };
        pos = next;
        if run.tick(m, scene, &s) != Status::Running {
            break;
        }
    }
    DriveReport { status: run.status(), gates: run.gates_passed(), gates_total: scene.gates.len(), cones_hit: run.cones_hit(), seconds: run.ticks() as f32 / TICK_HZ, score: run.score_time(m) }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(level: u16, scene: u16, time: Option<u16>, penalty: Option<u16>, damage: Option<u16>) -> Vec<u8> {
        let mut r = vec![0u8; RECORD];
        r[8..10].copy_from_slice(&level.to_le_bytes());
        r[0x24..0x26].copy_from_slice(&scene.to_le_bytes());
        if let Some(t) = time {
            r[0xc1] = 1;
            r[0xc2..0xc4].copy_from_slice(&t.to_le_bytes());
        }
        if let Some(p) = penalty {
            r[0xc8] = 1;
            r[0xca] = 1;
            r[0xce..0xd0].copy_from_slice(&p.to_le_bytes());
        }
        if let Some(d) = damage {
            r[0xc4] = 1;
            r[0xc6..0xc8].copy_from_slice(&d.to_le_bytes());
        }
        r
    }

    /// Two gates 40 m apart, a start and a cone beside the way.
    fn scene() -> Scene {
        let text = "TRIGGER_ELEMENT 5\n0 0 0\n1 0 0\t0 1 0\t0 0 1\n2 0 1 3 0.25 0 200 0 -1 -1 0 -1 1 0 0 0 0\n0 0 -1\t0 0 1\n0 0 0 0\t2 0 0 1\nStart\nTRIGGER_ELEMENT 5\n0 0 20\n1 0 0\t0 1 0\t0 0 1\n0 1 1 20 0.25 0 200 1 -1 -1 1 -1 1 0 0 0 0\n-10 0 20\t10 0 20\n0 0 0 0\t0 0 0 0\nGate\nTRIGGER_ELEMENT 5\n0 0 40\n1 0 0\t0 1 0\t0 0 1\n1 1 1 20 0.25 0 200 2 -1 -1 2 -1 -1 0 1 0 0\n-10 0 40\t10 0 40\n0 0 0 0\t0 0 0 0\nEnd\nGEOM_ELEMENT 4\n5 0 10\n1 0 0\t0 1 0\t0 0 1\n0 1162760003 0 0 -1 1 1 100\nGeometry Element\n";
        parse_scene(text).expect("scene")
    }

    #[test]
    fn records_decode() {
        let mut data = record(1, 11, Some(26), Some(1), Some(5));
        data.extend(record(2, 21, None, None, None));
        let m = parse_missions(&data);
        assert_eq!(m.len(), 2);
        assert_eq!((m[0].level, m[0].scene, m[0].time_limit, m[0].cone_penalty, m[0].damage_limit), (1, 11, Some(26), Some(1), Some(5)));
        assert_eq!((m[1].time_limit, m[1].cone_penalty, m[1].damage_limit, m[1].min_place), (None, None, None, None));
    }

    #[test]
    fn scene_reads_gates_cones_and_start() {
        let s = scene();
        assert_eq!((s.gates.len(), s.cones.len(), s.start, s.heading), (2, 1, [0.0, 0.0, 0.0], [0.0, 1.0]));
        assert_eq!(s.gates.last().unwrap().kind, 1);
    }

    #[test]
    fn a_clean_run_inside_the_limit_passes() {
        let m = Mission::parse(0, &record(1, 11, Some(10), Some(2), Some(5)));
        let s = scene();
        let r = drive(&m, &s, &Driver { speed: 10.0, damage_pct: 0.0, hit_cones: false });
        assert_eq!((r.status, r.gates, r.cones_hit), (Status::Passed, 2, 0), "{r:?}");
    }

    #[test]
    fn too_slow_fails_on_time() {
        let m = Mission::parse(0, &record(1, 11, Some(3), None, None));
        let r = drive(&m, &scene(), &Driver { speed: 10.0, damage_pct: 0.0, hit_cones: false });
        assert_eq!(r.status, Status::Failed(Fail::Time), "{r:?}");
    }

    #[test]
    fn a_fallen_cone_adds_its_penalty_and_can_fail_the_run() {
        // 40 m at 10 m/s is 4.0 s; one cone at 2 s each is 6.0 s against a 5 s limit
        let m = Mission::parse(0, &record(1, 11, Some(5), Some(2), None));
        let s = scene();
        let clean = drive(&m, &s, &Driver { speed: 10.0, damage_pct: 0.0, hit_cones: false });
        let messy = drive(&m, &s, &Driver { speed: 10.0, damage_pct: 0.0, hit_cones: true });
        assert_eq!(clean.status, Status::Passed, "{clean:?}");
        assert_eq!((messy.status, messy.cones_hit), (Status::Failed(Fail::Time), 1), "{messy:?}");
        assert!(messy.score >= 2.0 + 2.9, "the penalty is included: {messy:?}");
    }

    #[test]
    fn damage_over_the_limit_fails() {
        let m = Mission::parse(0, &record(1, 11, Some(60), None, Some(5)));
        let r = drive(&m, &scene(), &Driver { speed: 10.0, damage_pct: 6.0, hit_cones: false });
        assert_eq!(r.status, Status::Failed(Fail::Damage), "{r:?}");
        let ok = drive(&m, &scene(), &Driver { speed: 10.0, damage_pct: 4.0, hit_cones: false });
        assert_eq!(ok.status, Status::Passed);
    }

    #[test]
    fn gates_must_be_passed_in_order() {
        let m = Mission::parse(0, &record(1, 11, Some(60), None, None));
        let s = scene();
        let mut run = Run::new(Some(&s));
        // jump straight to the end gate: the first gate is skipped, so nothing counts
        for z in [30.0, 38.0, 42.0, 45.0] {
            run.tick(&m, &s, &Sample { pos: [0.0, 0.0, z], damage_pct: 0.0, place: 1 });
        }
        assert_eq!((run.gates_passed(), run.status()), (0, Status::Running));
    }

    #[test]
    fn races_judge_the_place() {
        let mut r = record(3, 0, None, None, None);
        r[0xd0] = 1;
        r[0xd2] = 1;
        let m = Mission::parse(0, &r);
        assert_eq!(m.kind(None), Kind::Race);
        assert_eq!(Run::new(None).finish_race(&m, 1, 6400, 0.0), Status::Passed);
        assert_eq!(Run::new(None).finish_race(&m, 2, 6400, 0.0), Status::Failed(Fail::Place));
    }

    #[test]
    fn progress_follows_the_file_order_and_saves() {
        let mut data = record(1, 11, None, None, None);
        data.extend(record(1, 12, None, None, None));
        data.extend(record(2, 21, None, None, None));
        let m = parse_missions(&data);
        let mut p = Progress::new("Greg", 3);
        let o = Outcome { place: 1, damage_pct: 2, ticks: 900 };
        assert_eq!(p.current(), Some(0));
        assert!(!p.record(&m, 1, Status::Passed, o), "only the current mission counts");
        assert!(!p.record(&m, 0, Status::Failed(Fail::Time), o), "a fail is not kept");
        assert!(!p.record(&m, 0, Status::Passed, o));
        assert!(p.record(&m, 1, Status::Passed, o), "finishing rank 1 is a promotion");
        assert_eq!((p.current(), p.rank(&m)), (Some(2), 1));
        assert_eq!(Progress::parse(&p.to_text()).unwrap(), p);
        p.jump_to(1);
        assert_eq!(p.current(), Some(0));
        p.record(&m, 0, Status::Passed, o);
        p.record(&m, 1, Status::Passed, o);
        p.record(&m, 2, Status::Passed, o);
        assert!(p.all_done());
    }

    #[test]
    fn progress_files_round_trip_on_disk() {
        let dir = std::env::temp_dir().join(format!("nfspu_factory_{}", std::process::id()));
        let mut p = Progress::new("Test Player", 34);
        p.done[0] = Some(Outcome { place: 1, damage_pct: 0, ticks: 1234 });
        p.save_to(&dir).unwrap();
        assert_eq!(Progress::load_from(&dir, "Test Player"), Some(p));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Against the real install: the table, every scene file and the verified facts.
    #[test]
    fn the_install_has_34_missions_and_30_scenes() {
        let Ok(dir) = std::env::var("NFSPU_GAME_DIR") else { return };
        let dir = Path::new(&dir);
        let missions = load_missions(dir);
        if missions.is_empty() {
            return;
        }
        let tracks = std::fs::read(dir.join("FEData").join("Data").join("nfs5.trk")).map(|d| crate::catalog::parse_tracks(&d)).unwrap();
        assert_eq!(missions.len(), 34);
        assert_eq!(missions.iter().map(|m| m.level).fold([0; 4], |mut a, l| { a[l as usize] += 1; a }), [0, 13, 10, 11], "missions per rank");
        let mut scenes = 0;
        for m in &missions {
            if let Some(s) = load_scene(dir, &tracks, m) {
                scenes += 1;
                assert!(s.gates.last().is_some_and(|g| g.kind == 1), "{}: ends at a finish gate", m.stem);
            } else {
                assert!(m.scene == 0, "mission {} ({}) lost its scene", m.index, m.stem);
            }
        }
        assert_eq!(scenes, 30);
    }

    /// A clean scripted run of every course mission passes when it is fast enough, and the same run
    /// fails when slowed down. Writes `logs/factory_drive.txt`.
    #[test]
    fn every_course_mission_runs_headless() {
        let Ok(dir) = std::env::var("NFSPU_GAME_DIR") else { return };
        let dir = Path::new(&dir);
        let missions = load_missions(dir);
        if missions.is_empty() {
            return;
        }
        let tracks = std::fs::read(dir.join("FEData").join("Data").join("nfs5.trk")).map(|d| crate::catalog::parse_tracks(&d)).unwrap();
        let report = drive_all(dir, &missions, &tracks);
        let _ = std::fs::create_dir_all("logs");
        let _ = std::fs::write("logs/factory_drive.txt", &report.text);
        assert!(report.ok, "{}", report.text);
    }
}

/// The outcome of [`drive_all`].
pub struct AllReport {
    pub ok: bool,
    pub text: String,
}

/// Headless check of every mission that has a scene: a fast clean driver must pass, a crawling one
/// must fail on time (when the mission has a time limit), a driver that rams the cones must pick
/// up the penalty, and progress saved after passing the first mission reads back. This is what
/// `race_drive` runs with `RACE_DRIVE_FACTORY=1`.
pub fn drive_all(game_dir: &Path, missions: &[Mission], tracks: &[crate::catalog::Track]) -> AllReport {
    use std::fmt::Write as _;
    let mut text = String::new();
    let mut ok = true;
    let mut progress = Progress::new("factory-check", missions.len());
    for m in missions {
        let Some(scene) = load_scene(game_dir, tracks, m) else {
            let _ = writeln!(text, "mission {:2} {:5} race mission (no scene): judged by place only", m.index, m.stem);
            continue;
        };
        let length: f32 = std::iter::once(scene.start).chain(scene.gates.iter().map(|g| g.centre())).collect::<Vec<_>>().windows(2).map(|w| dist2d(w[0], w[1])).sum();
        // fast enough to beat the limit with room to spare; no limit: a brisk 15 m/s
        let fast = m.time_limit.map_or(15.0, |l| (length / (l as f32 * 0.5)).max(5.0));
        let clean = drive(m, &scene, &Driver { speed: fast, damage_pct: 0.0, hit_cones: false });
        let pass_ok = clean.status == Status::Passed || m.cone_limit.is_some() || clean.cones_hit > 0;
        let slow = drive(m, &scene, &Driver { speed: 1.0, damage_pct: 0.0, hit_cones: false });
        let slow_ok = m.time_limit.is_none() || slow.status == Status::Failed(Fail::Time);
        let _ = writeln!(
            text,
            "mission {:2} {:5} rank {} {:?} len {:6.0} m, limit {:?} s, penalty {:?}: clean {:?} gates {}/{} cones {} ({:.1}s), crawl {:?}{}",
            m.index,
            m.stem,
            m.level,
            m.kind(Some(&scene)),
            length,
            m.time_limit,
            m.cone_penalty,
            clean.status,
            clean.gates,
            clean.gates_total,
            clean.cones_hit,
            clean.score,
            slow.status,
            if pass_ok && slow_ok { "" } else { "  <-- WRONG" }
        );
        ok &= pass_ok && slow_ok;
        if clean.status == Status::Passed && progress.current() == Some(m.index) {
            progress.record(missions, m.index, clean.status, Outcome { place: 1, damage_pct: 0, ticks: (clean.seconds * TICK_HZ) as u16 });
        }
    }
    let reread = Progress::parse(&progress.to_text());
    let saved = reread.as_ref() == Some(&progress);
    let _ = writeln!(text, "progress after the scripted run: current {:?}, rank {}, saved text round-trips: {saved}", progress.current(), progress.rank(missions));
    ok &= saved;
    let _ = writeln!(text, "factory drive: {}", if ok { "ALL OK" } else { "SOME FAILED" });
    AllReport { ok, text }
}
