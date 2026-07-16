//! On-disk cache of fetched topologies, keyed by provider + scope. Start-up
//! shows the cached picture instantly instead of re-running the whole CLI
//! inventory; ⟳ Refresh always fetches live and rewrites the cache. Files
//! live under the platform cache directory (`~/.cache/cloudviz` on
//! Linux/macOS, `%LOCALAPPDATA%\cloudviz\cache` on Windows) and contain only
//! inventory JSON — never credentials.

use crate::model::Topology;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Bump whenever the cached shape changes (e.g. a new `Topology` field);
/// files with another version are silently ignored and re-fetched.
const VERSION: u32 = 4;

#[derive(Serialize, Deserialize)]
struct CacheFile {
    version: u32,
    /// Unix seconds at save time.
    saved_at: u64,
    topology: Topology,
}

pub struct CacheHit {
    pub topology: Topology,
    pub age: Duration,
}

pub fn load(provider: &str, scope: &str) -> Option<CacheHit> {
    load_from(&dir()?, provider, scope)
}

/// Best-effort: a failed write only means a live fetch next start-up.
pub fn save(provider: &str, scope: &str, topology: &Topology) {
    if let Some(dir) = dir() {
        let _ = save_in(&dir, provider, scope, topology);
    }
}

fn dir() -> Option<PathBuf> {
    if cfg!(windows) {
        std::env::var_os("LOCALAPPDATA")
            .map(|base| PathBuf::from(base).join("cloudviz").join("cache"))
    } else {
        std::env::var_os("XDG_CACHE_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".cache")))
            .map(|base| base.join("cloudviz"))
    }
}

fn file_path(dir: &Path, provider: &str, scope: &str) -> PathBuf {
    let sanitize = |s: &str| -> String {
        s.chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '-' {
                    c.to_ascii_lowercase()
                } else {
                    '_'
                }
            })
            .collect()
    };
    dir.join(format!("{}--{}.json", sanitize(provider), sanitize(scope)))
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn load_from(dir: &Path, provider: &str, scope: &str) -> Option<CacheHit> {
    let raw = std::fs::read_to_string(file_path(dir, provider, scope)).ok()?;
    let file: CacheFile = serde_json::from_str(&raw).ok()?;
    if file.version != VERSION {
        return None;
    }
    Some(CacheHit {
        topology: file.topology,
        age: Duration::from_secs(now_unix().saturating_sub(file.saved_at)),
    })
}

fn save_in(dir: &Path, provider: &str, scope: &str, topology: &Topology) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let file = CacheFile {
        version: VERSION,
        saved_at: now_unix(),
        topology: topology.clone(),
    };
    let json = serde_json::to_string(&file).map_err(std::io::Error::other)?;
    std::fs::write(file_path(dir, provider, scope), json)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::demo::demo_topology;

    #[test]
    fn roundtrip_miss_and_corruption_handling() {
        let dir = std::env::temp_dir().join(format!("cloudviz-cache-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);

        let topo = demo_topology();
        save_in(&dir, "azure", "sub/One 1", &topo).unwrap();
        let hit = load_from(&dir, "azure", "sub/One 1").expect("cache hit");
        assert_eq!(hit.topology.nodes.len(), topo.nodes.len());
        assert_eq!(hit.topology.edges.len(), topo.edges.len());
        assert!(hit.age.as_secs() < 60);

        // Unknown scope misses instead of erroring.
        assert!(load_from(&dir, "azure", "other").is_none());
        // A corrupt file is ignored, not an error.
        std::fs::write(file_path(&dir, "azure", "bad"), "{not json").unwrap();
        assert!(load_from(&dir, "azure", "bad").is_none());
        // A version mismatch is ignored too.
        let stale = serde_json::json!({"version": 0, "saved_at": 0, "topology": null});
        std::fs::write(file_path(&dir, "azure", "old"), stale.to_string()).unwrap();
        assert!(load_from(&dir, "azure", "old").is_none());

        let _ = std::fs::remove_dir_all(&dir);
    }
}
