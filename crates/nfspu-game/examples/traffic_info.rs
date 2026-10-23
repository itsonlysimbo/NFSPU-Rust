//! Lists the traffic cars (kind 1) of the install's car catalogue.
//! Run: `cargo run -p nfspu-game --example traffic_info --release`
fn main() {
    let game = std::path::PathBuf::from(std::env::var("NFSPU_GAME_DIR").expect("set NFSPU_GAME_DIR"));
    for c in nfspu_game::car_load::catalogue(&game).iter().filter(|c| c.kind != 0) {
        println!("id {:4} kind {} model {:8} sim {:14} ai {:10} style {} name {}", c.id, c.kind, c.model, c.sim, c.ai_name, c.style, c.name);
    }
}
