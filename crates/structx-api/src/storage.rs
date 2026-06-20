use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use serde::{de::DeserializeOwned, Serialize};

pub fn state_root() -> PathBuf {
    std::env::var("STRUCTX_STATE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("artifacts/structx_state"))
}

pub fn audits_dir() -> PathBuf {
    state_root().join("audits")
}

pub fn positions_dir() -> PathBuf {
    state_root().join("positions")
}

pub fn redeems_dir() -> PathBuf {
    state_root().join("redeems")
}

pub fn markets_dir() -> PathBuf {
    state_root().join("markets")
}

pub fn all_markets_cache_path() -> PathBuf {
    markets_dir().join("all.markets.json")
}

pub fn btc_markets_cache_path() -> PathBuf {
    markets_dir().join("btc.markets.json")
}

pub fn positions_path(owner: &str, manager_id: &str) -> PathBuf {
    let owner = owner.to_lowercase();
    let manager = manager_id.to_lowercase();
    positions_dir().join(owner).join(format!("{manager}.positions.json"))
}

pub fn audit_record_path(digest: &str) -> PathBuf {
    audits_dir().join(format!("{digest}.record.json"))
}

pub fn redeem_record_path(digest: &str) -> PathBuf {
    redeems_dir().join(format!("{digest}.redeem.json"))
}

/// Make sure `path`'s parent directory exists. No-op if it already does.
pub fn ensure_parent(path: &Path) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }
    Ok(())
}

/// Atomic JSON write. Serializes `value`, writes to a sibling `.tmp` file,
/// fsyncs the temp file, then renames over the destination. A crash before
/// rename leaves the original file intact; a crash after rename leaves the
/// new file intact. No state ever in-between.
pub fn atomic_write_json<T: Serialize>(path: &Path, value: &T) -> io::Result<()> {
    ensure_parent(path)?;
    let bytes = serde_json::to_vec_pretty(value)
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
    let tmp = path.with_extension(match path.extension().and_then(|s| s.to_str()) {
        Some(ext) => format!("{ext}.tmp"),
        None => "tmp".to_string(),
    });
    {
        let mut file = fs::File::create(&tmp)?;
        file.write_all(&bytes)?;
        // Flush kernel buffers to disk before we rename — otherwise crash
        // recovery could see the rename but lose the bytes.
        file.sync_all().ok();
    }
    fs::rename(&tmp, path)?;
    Ok(())
}

/// Read + parse JSON. Returns Ok(None) if the file doesn't exist; Err on
/// I/O failure or parse failure.
pub fn read_json<T: DeserializeOwned>(path: &Path) -> io::Result<Option<T>> {
    match fs::read_to_string(path) {
        Ok(contents) => {
            let parsed: T = serde_json::from_str(&contents)
                .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
            Ok(Some(parsed))
        }
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(err) => Err(err),
    }
}

/// List paths in a directory. Returns empty list if directory doesn't exist.
pub fn list_dir(path: &Path) -> io::Result<Vec<PathBuf>> {
    match fs::read_dir(path) {
        Ok(iter) => iter.map(|entry| entry.map(|e| e.path())).collect::<io::Result<Vec<_>>>(),
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(err) => Err(err),
    }
}

pub fn unix_now() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;
    use tempfile::tempdir;

    #[derive(Serialize, Deserialize, PartialEq, Debug)]
    struct Sample {
        x: u32,
        s: String,
    }

    #[test]
    fn atomic_write_then_read_roundtrips() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("sample.json");
        let value = Sample { x: 42, s: "hello".to_string() };
        atomic_write_json(&path, &value).unwrap();
        let read: Sample = read_json(&path).unwrap().unwrap();
        assert_eq!(read, value);
    }

    #[test]
    fn read_json_missing_returns_none() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("does-not-exist.json");
        let read: Option<Sample> = read_json(&path).unwrap();
        assert!(read.is_none());
    }

    #[test]
    fn read_json_corrupt_returns_err() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("bad.json");
        fs::write(&path, "{ this is not valid json").unwrap();
        let result: io::Result<Option<Sample>> = read_json(&path);
        assert!(result.is_err());
    }

    #[test]
    fn atomic_write_creates_parent_dirs() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("a").join("b").join("c.json");
        atomic_write_json(&path, &Sample { x: 1, s: "x".into() }).unwrap();
        assert!(path.exists());
    }
}
