//! Measures the join snapshot. Run: cargo test -p alaz-net --release --test snapshot_size -- --ignored --nocapture
use alaz_sim::{map::MAP_W, World};

#[test]
#[ignore]
fn snapshot_size_report() {
    let mut w = World::new(5);
    let (sx, sy) = w.spawn_mm;
    for i in 0..3 {
        w.spawn_agent(sx + i * 1_500, sy);
    }
    let mut done = 0u64;
    for target in [0u64, 1_000, 20_000] {
        while done < target {
            w.step(&[]);
            done += 1;
        }
        println!("map {MAP_W}x{MAP_W}, tick {target}: snapshot {} bytes", w.to_bytes().len());
    }
}
