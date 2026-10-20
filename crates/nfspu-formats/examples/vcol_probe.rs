//! Exploration: average raw vertex-colour bytes of track articles near given road slices.
//! Run: cargo run -p nfspu-formats --example vcol_probe -- alps 0,800,1600
use nfspu_formats::{crp, refpack, sim};
fn main() {
    let track = std::env::args().nth(1).unwrap_or("alps".into());
    let slices: Vec<usize> = std::env::args().nth(2).unwrap_or("0".into()).split(',').filter_map(|s| s.parse().ok()).collect();
    let dir = std::path::PathBuf::from(std::env::var("NFSPU_GAME_DIR").unwrap()).join("GameData").join("Track");
    let data = refpack::decompress(&std::fs::read(dir.join(format!("{track}.crp"))).unwrap()).unwrap();
    let c = crp::parse(&data).unwrap();
    let s = sim::read(&c).unwrap();
    let mut arts = Vec::new();
    for a in 0..c.articles.len() {
        let Some(m) = c.article_mesh(a, 0) else { continue };
        let t = c.article_transform(a, 0);
        let (mut sum, mut n, mut p) = ([0f64; 3], 0f64, [0f64; 3]);
        for b in &m.batches {
            for (k, col) in b.colors.iter().enumerate() {
                for i in 0..3 { sum[i] += col[i] as f64; }
                let v = m.positions[b.vertex_idx[k] as usize];
                let v = match t { Some(t) => [v[0]*t[0]+v[1]*t[4]+v[2]*t[8]+t[12], v[0]*t[1]+v[1]*t[5]+v[2]*t[9]+t[13], v[0]*t[2]+v[1]*t[6]+v[2]*t[10]+t[14]], None => v };
                for i in 0..3 { p[i] += v[i] as f64; }
                n += 1.0;
            }
        }
        if n > 0.0 { arts.push(([p[0]/n, p[1]/n, p[2]/n], [sum[0]/n, sum[1]/n, sum[2]/n], n, c.article_name(a).unwrap_or_default())); }
    }
    for sl in slices {
        let q = s.slices[sl.min(s.slices.len()-1)].pos;
        let near: Vec<_> = arts.iter().filter(|a| ((a.0[0]-q[0] as f64).powi(2)+(a.0[2]-q[2] as f64).powi(2)).sqrt() < 60.0).collect();
        let tot: f64 = near.iter().map(|a| a.2).sum();
        let avg = [0,1,2].map(|i| near.iter().map(|a| a.1[i]*a.2).sum::<f64>() / tot.max(1.0));
        println!("slice {sl} at {:?}: {} articles, mean bytes [0]={:.0} [1]={:.0} [2]={:.0}; e.g. {:?}", q, near.len(), avg[0], avg[1], avg[2], near.iter().take(3).map(|a| (&a.3, a.1.map(|x| x as i32))).collect::<Vec<_>>());
    }
}
