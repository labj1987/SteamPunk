use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub struct Trainer {
    /// File stem, used as the display name (e.g. "CrimsonDesert_1_13").
    pub name: String,
    pub path: PathBuf,
}

/// `~/.local/share/proton-trainer/trainers/` — where imported trainers live,
/// flat, no per-game folders or association.
pub fn trainers_dir() -> Result<PathBuf> {
    let home = std::env::var("HOME").context("HOME not set")?;
    Ok(PathBuf::from(home).join(".local/share/proton-trainer/trainers"))
}

pub fn list_trainers() -> Result<Vec<Trainer>> {
    let dir = trainers_dir()?;
    if !dir.is_dir() {
        return Ok(Vec::new());
    }

    let mut trainers: Vec<Trainer> = std::fs::read_dir(&dir)?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_file() && p.extension().and_then(|e| e.to_str()) == Some("exe"))
        .map(|path| Trainer {
            name: path
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| path.to_string_lossy().into_owned()),
            path,
        })
        .collect();

    trainers.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(trainers)
}

/// Copy a dropped/picked .exe into the managed trainers dir, flat, keeping
/// its filename. Overwrites an existing trainer of the same name.
pub fn import_trainer(src: &Path) -> Result<PathBuf> {
    let dir = trainers_dir()?;
    std::fs::create_dir_all(&dir)?;

    let filename = src
        .file_name()
        .context("dropped file has no filename")?;
    let dest = dir.join(filename);
    std::fs::copy(src, &dest)
        .with_context(|| format!("copying {} to {}", src.display(), dest.display()))?;
    Ok(dest)
}

pub fn remove_trainer(path: &Path) -> Result<()> {
    std::fs::remove_file(path)
        .with_context(|| format!("removing {}", path.display()))
}
