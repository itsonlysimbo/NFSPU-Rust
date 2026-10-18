//! Finds a car's simulation file in the player's install and turns it into a [`Spec`]: the
//! catalogue (`FEData/Data/nfs5.car`) names the `.sim` file and the AI-data name, and retail falls
//! back to `default.sim` when a car's own file is missing.

use crate::car_sim::{Spec, SpecOptions};
use crate::catalog::{self, Car as CatalogCar};
use nfspu_formats::carsim;
use std::fs;
use std::path::Path;

/// Which car to load.
#[derive(Clone, Debug, PartialEq)]
pub enum Pick<'a> {
    /// A catalogue id (the race setup's `car_id`).
    Id(u16),
    /// The first road car with this model stem (`993`).
    Model(&'a str),
    /// A `.sim` file stem directly (`959coupe29`), with the AI-data name for the per-car rules.
    Sim { sim: &'a str, ai_name: &'a str },
}

/// A loaded spec and where it came from (for the log).
#[derive(Clone, Debug)]
pub struct Loaded {
    pub spec: Spec,
    pub sim_file: String,
    pub note: String,
}

/// The catalogue cars of an install.
pub fn catalogue(game_dir: &Path) -> Vec<CatalogCar> {
    fs::read(game_dir.join("FEData").join("Data").join("nfs5.car")).map(|d| catalog::parse_cars(&d)).unwrap_or_default()
}

/// Reads `<stem>.sim` (any letter case), or `default.sim` when it is missing, as retail does.
pub fn read_sim(game_dir: &Path, stem: &str) -> Result<(carsim::SimFile, String), String> {
    let dir = game_dir.join("Simulation").exists().then(|| game_dir.join("Simulation")).unwrap_or_else(|| game_dir.join("GameData").join("Simulation")).join("CarData");
    let want = format!("{stem}.sim").to_ascii_lowercase();
    let entries: Vec<_> = fs::read_dir(&dir).map_err(|e| format!("{}: {e}", dir.display()))?.filter_map(|e| e.ok()).collect();
    let find = |name: &str| entries.iter().find(|e| e.file_name().to_string_lossy().to_ascii_lowercase() == name).map(|e| e.path());
    let (path, used) = match find(&want) {
        Some(p) => (p, stem.to_string()),
        None => (find("default.sim").ok_or_else(|| format!("no {want} and no default.sim in {}", dir.display()))?, "default".to_string()),
    };
    let data = fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let sim = carsim::parse(&data).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok((sim, used))
}

/// Loads a car's spec. `manual` is the race setup's gearbox choice (retail gives manual 5% more
/// drive force).
pub fn load(game_dir: &Path, pick: Pick, manual: bool) -> Result<Loaded, String> {
    let (sim_stem, ai_name, label) = match pick {
        Pick::Sim { sim, ai_name } => (sim.to_string(), ai_name.to_string(), sim.to_string()),
        Pick::Id(id) => {
            let cars = catalogue(game_dir);
            let c = cars.iter().find(|c| c.id == id).ok_or_else(|| format!("car id {id} is not in nfs5.car"))?;
            (c.sim.clone(), c.ai_name.clone(), c.name.clone())
        }
        Pick::Model(model) => {
            let cars = catalogue(game_dir);
            let c = cars
                .iter()
                .find(|c| c.kind == 0 && c.model.eq_ignore_ascii_case(model))
                .or_else(|| cars.iter().find(|c| c.model.eq_ignore_ascii_case(model)))
                .ok_or_else(|| format!("no catalogue car uses model {model}"))?;
            (c.sim.clone(), c.ai_name.clone(), c.name.clone())
        }
    };
    let (sim, used) = read_sim(game_dir, &sim_stem)?;
    let spec = Spec::from_sim(&sim, SpecOptions::for_car(&ai_name, manual));
    let note = format!(
        "{label}: {used}.sim \"{}\", {:.0} kg, {} gears, redline {:.0}, top {:.1} m/s, grip {:.2}{}",
        spec.name,
        spec.mass,
        spec.forward_gears,
        spec.redline,
        spec.top_speed,
        spec.grip,
        if used != sim_stem { format!(" (its own {sim_stem}.sim is missing)") } else { String::new() }
    );
    Ok(Loaded { spec, sim_file: used, note })
}
