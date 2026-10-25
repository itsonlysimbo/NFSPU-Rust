//! A Factory Driver mission inside the race scene: the car starts at the scene's start gate, the
//! cones stand where the scene puts them (and fall over when hit), `nfspu_game::factory::Run`
//! judges the gates, time, cones and damage, and the verdict goes back to the menu
//! (`nfspu_game::factory_live`). Runs only when the menu named a mission before the race.

use crate::flow::{AppState, ReturnToMenu};
use crate::race_mode::RaceHold;
use bevy::prelude::*;
use nfspu_game::factory::{self, Run, Sample, Status};
use nfspu_game::factory_live::{self, Verdict};

/// Seconds the verdict stays up before the race closes.
const VERDICT_SECONDS: f32 = 3.5;

#[derive(Resource)]
pub struct MissionLive {
    active: factory_live::Active,
    run: Run,
    cones: Vec<Entity>,
    fallen: Vec<bool>,
    banner: Entity,
    carry: f32,
    ended_for: Option<f32>,
}

fn world_pos(p: [f32; 3]) -> Vec3 {
    Vec3::new(p[0] * nfspu_game::world_mirror_x(), p[1], p[2])
}

fn log(msg: &str) {
    crate::log_line(&format!("mission: {msg}"));
}

#[allow(clippy::too_many_arguments)]
pub fn mission(
    mut commands: Commands,
    time: Res<Time>,
    live: Option<ResMut<MissionLive>>,
    car: Option<ResMut<crate::CarRes>>,
    hold: Res<RaceHold>,
    back: Res<ReturnToMenu>,
    mut next: ResMut<NextState<AppState>>,
    mut exit: MessageWriter<AppExit>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut transforms: Query<&mut Transform>,
    mut texts: Query<(&mut Text, &mut TextColor)>,
) {
    let Some(mut car) = car else { return };
    let Some(mut live) = live else {
        // first frame of a mission race: place the car, stand the cones up, open the banner
        let Some(active) = factory_live::active() else { return };
        let Some(scene) = active.scene.clone() else { return };
        if !hold.0 {
            return;
        }
        let (start, heading) = factory::start_pose(&scene);
        let pos = world_pos(start) + Vec3::Y * 1.5;
        let yaw = (-heading[0] * nfspu_game::world_mirror_x()).atan2(-heading[1]);
        car.car = car.car.reset(pos, yaw);
        car.start_pos = pos;
        car.start_yaw = yaw;
        let mesh = meshes.add(Cone::new(0.3, 0.7));
        let paint = materials.add(StandardMaterial { base_color: Color::srgb(1.0, 0.45, 0.05), perceptual_roughness: 0.6, ..default() });
        let cones = scene
            .cones
            .iter()
            .map(|c| commands.spawn((Mesh3d(mesh.clone()), MeshMaterial3d(paint.clone()), Transform::from_translation(world_pos(*c) + Vec3::Y * 0.35), DespawnOnExit(AppState::Race))).id())
            .collect();
        let banner = commands
            .spawn((
                Text::new(""),
                TextFont { font_size: FontSize::Px(22.0), ..default() },
                TextColor(Color::WHITE),
                Node { position_type: PositionType::Absolute, top: Val::Px(70.0), left: Val::Px(0.0), right: Val::Px(0.0), justify_self: JustifySelf::Center, ..default() },
                DespawnOnExit(AppState::Race),
            ))
            .id();
        log(&format!("{} ({}) on scene {}: {} gates, {} cones, limit {:?} s", active.mission.index, active.mission.stem, active.mission.scene, scene.gates.len(), scene.cones.len(), active.mission.time_limit));
        let run = Run::new(Some(&scene));
        commands.insert_resource(MissionLive { fallen: vec![false; scene.cones.len()], cones, active, run, banner, carry: 0.0, ended_for: None });
        return;
    };
    let live = &mut *live;
    let Some(scene) = live.active.scene.clone() else { return };
    let m = live.active.mission.clone();
    let dt = time.delta_secs().min(0.25);
    if let Some(t) = live.ended_for.as_mut() {
        *t += dt;
        if *t >= VERDICT_SECONDS {
            crate::flow::leave_race(&back, &mut next, &mut exit);
        }
        return;
    }
    // `NFSPU_FACTORY_AUTO=1`: a scripted driver for headless tests carries the car along the gates
    // (mission rules, cones and the verdict run as in play; the car is moved, not driven).
    if !hold.0 && live.run.status() == Status::Running && std::env::var_os("NFSPU_FACTORY_AUTO").is_some() {
        let speed: f32 = std::env::var("NFSPU_FACTORY_AUTO").ok().and_then(|v| v.parse().ok()).filter(|v: &f32| *v > 1.0).unwrap_or(30.0);
        if let Some(gate) = scene.gates.get(live.run.gates_passed()) {
            let target = world_pos(gate.centre()) + Vec3::Y * 1.0;
            let to = target - car.car.pos;
            let flat = Vec3::new(to.x, 0.0, to.z);
            let step = (speed * dt).min(flat.length() + 3.0);
            let dir = flat.normalize_or_zero();
            let pos = car.car.pos + dir * step;
            let yaw = (-dir.x).atan2(-dir.z);
            car.car = car.car.reset(Vec3::new(pos.x, target.y, pos.z), yaw);
            car.car.set_speed(speed);
        }
    }
    if !hold.0 {
        live.carry += dt;
        let step = 1.0 / factory::TICK_HZ;
        let zones = &car.car.sim.damage.zones;
        let damage = zones.iter().sum::<f32>() / zones.len() as f32;
        // file coordinates: undo the world flip
        let p = car.car.pos;
        let pos = [p.x * nfspu_game::world_mirror_x(), p.y, p.z];
        while live.carry >= step && live.run.status() == Status::Running {
            live.carry -= step;
            if live.run.ticks() % 64 == 0 {
                log(&format!("t={} pos=({:.1},{:.1},{:.1}) gate {} cones {}", live.run.ticks(), pos[0], pos[1], pos[2], live.run.gates_passed(), live.run.cones_hit()));
            }
            live.run.tick(&m, &scene, &Sample { pos, damage_pct: damage, place: 1 });
        }
    }
    for (i, e) in live.cones.iter().enumerate() {
        if live.run.cone_is_hit(i) && !live.fallen[i] {
            live.fallen[i] = true;
            if let Ok(mut t) = transforms.get_mut(*e) {
                t.rotation = Quat::from_rotation_z(1.45) * Quat::from_rotation_y(i as f32);
                t.translation.y -= 0.25;
            }
        }
    }
    let status = live.run.status();
    let limit = m.time_limit.map_or(String::new(), |l| format!(" / {l}"));
    let (line, colour) = match status {
        Status::Running => (format!("TIME {:.1}{limit}   GATES {}/{}   CONES {}", live.run.score_time(&m), live.run.gates_passed(), scene.gates.len(), live.run.cones_hit()), Color::WHITE),
        Status::Passed => (format!("MISSION PASSED   {:.1} s", live.run.score_time(&m)), Color::srgb(0.4, 1.0, 0.4)),
        Status::Failed(why) => (format!("MISSION FAILED ({why:?})   {:.1} s", live.run.score_time(&m)), Color::srgb(1.0, 0.35, 0.3)),
    };
    if let Ok((mut text, mut tc)) = texts.get_mut(live.banner) {
        if text.0 != line {
            text.0 = line;
        }
        tc.0 = colour;
    }
    if status != Status::Running {
        factory_live::set_verdict(Verdict { index: m.index, status, outcome: live.run.outcome() });
        log(&format!("mission {} verdict {status:?}, {} gates, {} cones, {:.2} s", m.index, live.run.gates_passed(), live.run.cones_hit(), live.run.score_time(&m)));
        live.ended_for = Some(0.0);
    }
}
