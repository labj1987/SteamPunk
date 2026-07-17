use crate::steam;
use anyhow::{anyhow, Context, Result};
use std::path::{Path, PathBuf};

/// Everything needed to launch a trainer against the currently running game.
pub struct LaunchTarget {
    pub appid: u32,
    pub client_dir: PathBuf,
    pub compatdata_dir: PathBuf,
    pub proton_dir: PathBuf,
}

impl LaunchTarget {
    pub fn prefix_dir(&self) -> PathBuf {
        self.compatdata_dir.join("pfx")
    }
}

/// Scan /proc for a `SteamLaunch AppId=<N>` cmdline — whatever Proton game
/// is running is the target, there is no picker.
pub fn find_running_appid() -> Option<u32> {
    let self_pid = std::process::id();
    let entries = std::fs::read_dir("/proc").ok()?;

    for entry in entries.flatten() {
        let Ok(pid) = entry.file_name().to_string_lossy().parse::<u32>() else {
            continue;
        };
        if pid == self_pid {
            continue;
        }
        let Ok(bytes) = std::fs::read(entry.path().join("cmdline")) else {
            continue;
        };
        let args: Vec<&str> = bytes
            .split(|&b| b == 0)
            .filter(|s| !s.is_empty())
            .filter_map(|s| std::str::from_utf8(s).ok())
            .collect();

        if !args.iter().any(|a| *a == "SteamLaunch") {
            continue;
        }
        for a in &args {
            if let Some(id) = a.strip_prefix("AppId=") {
                if let Ok(appid) = id.parse::<u32>() {
                    return Some(appid);
                }
            }
        }
    }
    None
}

/// Resolve `<proton_dir>` from the currently running wineserver whose
/// STEAM_COMPAT_DATA_PATH matches this game's compatdata dir. This is the
/// source of truth — config_info can be stale (observed: pointing at a
/// different Proton build than the one actually running).
fn find_proton_dir_from_wineserver(compatdata_dir: &Path) -> Option<PathBuf> {
    let target = normalize_lexical(compatdata_dir);
    let entries = std::fs::read_dir("/proc").ok()?;

    for entry in entries.flatten() {
        if entry.file_name().to_string_lossy().parse::<u32>().is_err() {
            continue;
        }
        let exe_link = entry.path().join("exe");
        let Ok(exe_target) = std::fs::read_link(&exe_link) else {
            continue;
        };
        let normalized_exe = normalize_lexical(&exe_target);
        if !normalized_exe.to_string_lossy().ends_with("/bin/wineserver") {
            continue;
        }

        let Ok(environ) = std::fs::read(entry.path().join("environ")) else {
            continue;
        };
        let matches_prefix = environ
            .split(|&b| b == 0)
            .filter_map(|s| std::str::from_utf8(s).ok())
            .filter_map(|kv| kv.strip_prefix("STEAM_COMPAT_DATA_PATH="))
            .any(|v| normalize_lexical(Path::new(v)) == target);

        if matches_prefix {
            if let Some(proton_dir) = proton_dir_from_wineserver_exe(&normalized_exe) {
                return Some(proton_dir);
            }
        }
    }
    None
}

/// `<proton_dir>/files/bin/wineserver` -> `<proton_dir>`, after the path has
/// already been lexically normalized (Proton's own libwine loader constructs
/// this path with `../..` components, e.g. `files/lib/wine/../../bin/wineserver`).
fn proton_dir_from_wineserver_exe(normalized_exe: &Path) -> Option<PathBuf> {
    let s = normalized_exe.to_string_lossy();
    s.strip_suffix("/files/bin/wineserver").map(PathBuf::from)
}

/// Resolve `..`/`.` components without touching the filesystem (no
/// canonicalize — components may not all exist, and we don't want symlink
/// resolution changing the answer).
fn normalize_lexical(path: &Path) -> PathBuf {
    let mut out: Vec<std::ffi::OsString> = Vec::new();
    for comp in path.components() {
        match comp {
            std::path::Component::ParentDir => {
                out.pop();
            }
            std::path::Component::CurDir => {}
            other => out.push(other.as_os_str().to_os_string()),
        }
    }
    out.into_iter().collect()
}

/// Fallback when no wineserver is found yet: line 2 of config_info, split on
/// '/' (never on whitespace — Proton directory names contain spaces, e.g.
/// "Proton - Experimental"), up to and including the first segment whose
/// lowercase contains "proton".
fn find_proton_dir_from_config_info(compatdata_dir: &Path) -> Option<PathBuf> {
    let contents = std::fs::read_to_string(compatdata_dir.join("config_info")).ok()?;
    let line2 = contents.lines().nth(1)?;
    let segments: Vec<&str> = line2.split('/').collect();
    let idx = segments
        .iter()
        .position(|s| s.to_lowercase().contains("proton"))?;
    let joined = segments[..=idx].join("/");
    if joined.is_empty() {
        None
    } else {
        Some(PathBuf::from(joined))
    }
}

fn find_proton_dir(compatdata_dir: &Path) -> Option<PathBuf> {
    find_proton_dir_from_wineserver(compatdata_dir)
        .or_else(|| find_proton_dir_from_config_info(compatdata_dir))
}

/// Resolve everything needed to launch: the running game, its compatdata,
/// and the Proton build whose wineserver it's actually using.
pub fn resolve_launch_target() -> Result<LaunchTarget> {
    let appid = find_running_appid().ok_or_else(|| {
        anyhow!("Start the game first, load past the menus, then launch the trainer.")
    })?;

    let client_dir =
        steam::steam_client_dir().ok_or_else(|| anyhow!("Could not find a Steam installation."))?;
    let libraries = steam::library_folders(&client_dir);
    let compatdata_dir = steam::compatdata_dir(&libraries, &appid.to_string())
        .ok_or_else(|| anyhow!("Could not find compatdata for AppId {appid}."))?;
    let proton_dir = find_proton_dir(&compatdata_dir).ok_or_else(|| {
        anyhow!("Could not determine which Proton build the game is currently running.")
    })?;

    Ok(LaunchTarget {
        appid,
        client_dir,
        compatdata_dir,
        proton_dir,
    })
}

/// Real .NET 4.0 exists in the prefix iff clr.dll is present — wine-mono
/// never creates this file, so its absence means dotnet40 hasn't been
/// installed via winetricks yet.
pub fn has_dotnet40(target: &LaunchTarget) -> bool {
    target
        .prefix_dir()
        .join("drive_c/windows/Microsoft.NET/Framework64/v4.0.30319/clr.dll")
        .is_file()
}

/// Launch a trainer against the resolved target, detached: the FLiNG exe
/// unpacks itself to a TrainerCacheData folder and relaunches, so this
/// initial process exiting quickly is expected, not a failure.
pub fn launch_trainer(target: &LaunchTarget, trainer_path: &Path, log_path: &Path) -> Result<()> {
    use std::os::unix::process::CommandExt;
    use std::process::{Command, Stdio};

    if let Some(parent) = log_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let log_out = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path)
        .with_context(|| format!("opening log file {}", log_path.display()))?;
    let log_err = log_out.try_clone()?;

    let proton = target.proton_dir.join("proton");
    Command::new(&proton)
        .arg("run")
        .arg(trainer_path)
        .env("STEAM_COMPAT_CLIENT_INSTALL_PATH", &target.client_dir)
        .env("STEAM_COMPAT_DATA_PATH", &target.compatdata_dir)
        .stdin(Stdio::null())
        .stdout(log_out)
        .stderr(log_err)
        .process_group(0)
        .spawn()
        .with_context(|| format!("spawning {}", proton.display()))?;

    Ok(())
}

/// Kill every running trainer: their unpacked TrainerCacheData helper
/// process, plus anything still referencing the managed trainers dir.
pub fn stop_all(trainers_dir: &Path) {
    let _ = std::process::Command::new("pkill")
        .arg("-f")
        .arg("TrainerCacheData")
        .status();
    let _ = std::process::Command::new("pkill")
        .arg("-f")
        .arg(trainers_dir)
        .status();
}

/// Per-game trainer-logs directories FLiNG trainers leave behind. Each may
/// contain a stale info.ini lock file that makes a trainer refuse to start
/// because it thinks a previous instance is still running; deleting it is
/// the documented fix. There's no reliable way to map the running game to
/// exactly one of these (the folder name is the trainer's internal title,
/// not necessarily Steam's), so recovery lists all of them and lets the
/// user pick.
pub fn trainer_log_dirs(target: &LaunchTarget) -> Vec<PathBuf> {
    let base = target
        .prefix_dir()
        .join("drive_c/users/steamuser/AppData/Local/FLiNGTrainer/trainer-logs");
    let Ok(entries) = std::fs::read_dir(&base) else {
        return Vec::new();
    };
    entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect()
}
