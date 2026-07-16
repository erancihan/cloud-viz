//! Persisted user preferences (not cloud data — that's `cache.rs`). Lives in
//! the platform config directory (`~/.config/cloudviz/config.json` on
//! Linux/macOS, `%APPDATA%\cloudviz\config.json` on Windows). Best-effort on
//! both ends: a missing or corrupt file just means defaults.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Config {
    /// egui's screen zoom factor (Ctrl +/- / Ctrl 0), restored at start-up
    /// so the window comes back at the zoom the user left it at.
    #[serde(default)]
    pub zoom_factor: Option<f32>,
}

pub fn load() -> Config {
    dir().map(|d| load_from(&d)).unwrap_or_default()
}

pub fn save(config: &Config) {
    if let Some(d) = dir() {
        let _ = save_in(&d, config);
    }
}

fn dir() -> Option<PathBuf> {
    let base = if cfg!(windows) {
        PathBuf::from(std::env::var_os("APPDATA")?)
    } else {
        match std::env::var_os("XDG_CONFIG_HOME") {
            Some(base) => PathBuf::from(base),
            None => PathBuf::from(std::env::var_os("HOME")?).join(".config"),
        }
    };
    Some(base.join("cloudviz"))
}

fn load_from(dir: &Path) -> Config {
    std::fs::read_to_string(dir.join("config.json"))
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

fn save_in(dir: &Path, config: &Config) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let json = serde_json::to_string_pretty(config).map_err(std::io::Error::other)?;
    std::fs::write(dir.join("config.json"), json)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_and_corruption_handling() {
        let dir = std::env::temp_dir().join(format!("cloudviz-config-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);

        // Missing file → defaults.
        assert!(load_from(&dir).zoom_factor.is_none());

        save_in(
            &dir,
            &Config {
                zoom_factor: Some(1.25),
            },
        )
        .unwrap();
        assert_eq!(load_from(&dir).zoom_factor, Some(1.25));

        // Corrupt file → defaults, not an error.
        std::fs::write(dir.join("config.json"), "{not json").unwrap();
        assert!(load_from(&dir).zoom_factor.is_none());

        let _ = std::fs::remove_dir_all(&dir);
    }
}
