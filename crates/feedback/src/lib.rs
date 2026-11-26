//! Feedback reports: what they hold, how they are cleaned, kept and sent. No window, no game state.

pub mod crash;
pub mod log;
pub mod report;
pub mod scrub;
pub mod shot;

#[cfg(test)]
fn temp(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("alaz-feedback-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}
