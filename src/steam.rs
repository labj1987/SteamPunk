use std::path::{Path, PathBuf};

/// Locate the Steam client installation directory: the first of the usual
/// install locations that actually has a steamapps/ subdirectory.
pub fn steam_client_dir() -> Option<PathBuf> {
    let home = std::env::var("HOME").ok()?;
    for candidate in [
        format!("{home}/.local/share/Steam"),
        format!("{home}/.steam/steam"),
    ] {
        let dir = PathBuf::from(candidate);
        if dir.join("steamapps").is_dir() {
            return Some(dir);
        }
    }
    None
}

/// All Steam library roots: the client dir itself (always has its own
/// steamapps) plus every "path" value in libraryfolders.vdf. Parsed as
/// quoted key/value pairs rather than split on whitespace, since library
/// paths routinely contain spaces (e.g. external drives).
pub fn library_folders(client_dir: &Path) -> Vec<PathBuf> {
    let mut libs = vec![client_dir.to_path_buf()];

    let vdf_path = client_dir.join("steamapps/libraryfolders.vdf");
    let Ok(contents) = std::fs::read_to_string(&vdf_path) else {
        return libs;
    };

    for line in contents.lines() {
        let fields = quoted_fields(line);
        if fields.first() == Some(&"path") {
            if let Some(path) = fields.get(1) {
                let path = PathBuf::from(path);
                if !libs.contains(&path) {
                    libs.push(path);
                }
            }
        }
    }

    libs
}

/// The quoted fields on a VDF line, e.g. `"path"    "/mnt/Storage/Steam"`
/// -> ["path", "/mnt/Storage/Steam"].
fn quoted_fields(line: &str) -> Vec<&str> {
    line.split('"').skip(1).step_by(2).collect()
}

/// The game's display name from its appmanifest, used to tell several running
/// games apart. Returns None if no library has a manifest for this AppId, in
/// which case callers fall back to showing the bare number.
pub fn game_name(libraries: &[PathBuf], appid: &str) -> Option<String> {
    for lib in libraries {
        let manifest = lib
            .join("steamapps")
            .join(format!("appmanifest_{appid}.acf"));
        let Ok(contents) = std::fs::read_to_string(&manifest) else {
            continue;
        };
        for line in contents.lines() {
            let fields = quoted_fields(line);
            if fields.first() == Some(&"name") {
                if let Some(name) = fields.get(1) {
                    return Some((*name).to_string());
                }
            }
        }
    }
    None
}

/// The first library whose steamapps/compatdata contains this AppId.
pub fn compatdata_dir(libraries: &[PathBuf], appid: &str) -> Option<PathBuf> {
    libraries
        .iter()
        .map(|lib| lib.join("steamapps/compatdata").join(appid))
        .find(|p| p.is_dir())
}
