use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use animestan_core::AnimeMetadata;
use serde::{Deserialize, Serialize};

fn default_epoch() -> u64 {
    0
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CachedMetadataEntry {
    pub metadata: AnimeMetadata,
    #[serde(default = "default_epoch")]
    pub created_at: u64,
    pub updated_at: u64,
}

#[derive(Debug, Serialize, Deserialize)]
struct MetadataCacheFile {
    #[serde(default)]
    entries: HashMap<String, CachedMetadataEntry>,
}

pub fn load(path: &Path) -> io::Result<HashMap<String, CachedMetadataEntry>> {
    match fs::read_to_string(path) {
        Ok(contents) => {
            if let Ok(cache) = serde_json::from_str::<MetadataCacheFile>(&contents) {
                Ok(cache.entries)
            } else {
                let _ = fs::remove_file(path);
                Ok(HashMap::new())
            }
        }
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(HashMap::new()),
        Err(err) => Err(err),
    }
}

pub fn save(path: &Path, entries: &HashMap<String, CachedMetadataEntry>) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let cache = MetadataCacheFile {
        entries: entries.clone(),
    };
    let payload = serde_json::to_string_pretty(&cache)
        .map_err(|source| io::Error::new(io::ErrorKind::InvalidData, source))?;
    fs::write(path, payload)
}

pub fn now_epoch() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default()
}
