# Changelog

## 0.1.8 — 2026-08-04

- Fixed trainers failing to launch against a running game.

## 0.1.7 — 2026-08-04

- The app now sets up the required Windows compatibility component
  automatically — no more copying commands into a terminal. Clicking
  Launch on a game that needs it shows a "Set Up Automatically" dialog
  instead of raw shell commands; confirming installs the one-time system
  packages (a single password prompt, via `pkexec` — same pattern already
  used for other privileged operations in this app family) and then runs
  `winetricks dotnet48 win10` against the game's prefix, launching the
  trainer automatically once done. A collapsed "Show manual commands
  instead" disclosure keeps the exact commands available for advanced
  users or troubleshooting.
- Switched the installed runtime from `dotnet40` to `dotnet48`:
  `dotnet40` reliably fails to install a working `clr.dll` on this
  Ubuntu/wine combination (winetricks reports success, but the actual
  runtime binary silently doesn't land), while `dotnet48` is
  backward-compatible for FLiNG trainers and installs correctly. The
  `clr.dll` presence check (`has_dotnet40`) is unaffected — both versions
  populate the same path.

## 0.1.6 — 2026-08-04

- Root cause of the persistent phantom taskbar entry confirmed with
  `WAYLAND_DEBUG=1 <appimage> 2>&1 | grep set_app_id`: on Wayland, GTK4
  announces the GApplication ID (`io.github.labj1987.ProtonTrainer`) as
  the toplevel's `app_id`, not `prgname`. The `.desktop` file's
  `StartupWMClass` was set to the old prgname (`proton-trainer`), so
  GNOME Shell couldn't match the running window to the desktop
  launcher — one process, two dock entries. Fixed by setting both
  `prgname` (in `main.rs`) and `StartupWMClass` (in the `.desktop`
  file) to the application ID, so the match works regardless of
  backend. The AboutDialog/Dialog conversions and `StartupNotify=true`
  from 0.1.1–0.1.5 were reasonable but orthogonal — this is the actual
  fix.

- The 0.1.1/0.1.3/0.1.4 fixes addressed real GTK-window-subclass dialogs
  but the phantom dock icon persisted immediately at launch, before any
  dialog could fire. Root cause: `proton-trainer.desktop` was missing
  `StartupNotify=true`. Without it, GNOME Shell has no way to associate
  the launch sequence with the eventual mapped window, so it leaves an
  orphaned placeholder entry in the dock (generic icon, tooltip showing
  only the raw `io.github.labj1987.ProtonTrainer` app ID) alongside the
  real, correctly-iconed window. NVI already had `StartupNotify=true` and
  never exhibited this. Added the line to match.

## 0.1.4 — 2026-08-04

- The About dialog used `gtk4::AboutDialog`, and the Troubleshoot dialog
  used `libadwaita::Window` — both are real `Gtk.Window` subclasses and
  create separate top-level Wayland surfaces, each showing as a second,
  unnamed window in the dock (the same bug class as the `MessageDialog`
  fix in 0.1.1). Switched to `libadwaita::AboutDialog` and
  `libadwaita::Dialog` respectively, both `Adw.Dialog` subclasses that
  render as sheets inside the main window's own surface.

## 0.1.3 — 2026-08-04

- 0.1.2 corrected the `UPDATE_INFORMATION` string but the fix never took
  effect: `build-appimage.sh` passed it to `appimagetool` as an
  environment variable, and this appimagetool build (continuous, git
  8c8c91f) silently ignores that env var — it only reads update info via
  the `-u`/`--updateinformation` CLI flag. Confirmed by inspecting the
  0.1.2 release AppImage's `.upd_info` ELF section: empty. Switched to
  passing `-u "$UPDATE_INFORMATION"` as an argument.

## 0.1.2 — 2026-08-04

- Fixed `UPDATE_INFORMATION` in `build-appimage.sh` to reference the
  `.zsync` sidecar filename instead of the AppImage filename, per the
  AppImage update spec. This was breaking update detection in tools like
  Gear Lever even though the `.zsync` sidecar was already being generated
  and published correctly. Packaging-only fix, no application behavior
  changes.

## 0.1.1 — 2026-08-04

- Fixed a phantom, unnamed taskbar/dock window appearing whenever the
  one-time .NET 4.0 setup dialog was shown. It used
  `libadwaita::MessageDialog`, a `Gtk.Window` subclass that creates a real
  separate top-level Wayland surface without the app's icon/app_id.
  Switched to `libadwaita::AlertDialog`, which renders as a sheet inside
  the main window's own surface instead.

## 0.1.0 — 2026-07-17

Initial release.

- Detects the running Steam game from `/proc` and resolves its Proton build
  from the actual running wineserver process (not from potentially stale
  `config_info` metadata), falling back to `config_info` only if no
  wineserver is found yet.
- Launches a trainer `.exe` directly on the host via `<proton>/proton run`
  with `STEAM_COMPAT_CLIENT_INSTALL_PATH` and `STEAM_COMPAT_DATA_PATH` set,
  so it joins the game's existing wineserver session.
- Trainer library: drag-and-drop or file-picker import into
  `~/.local/share/proton-trainer/trainers/`, launch, remove, stop all.
- Detects when a game's prefix lacks real .NET Framework 4.0 (required by
  FLiNG's WPF trainers, absent under Proton's bundled wine-mono) and shows
  the exact one-time `winetricks` setup commands instead of failing silently.
- Stale-instance recovery: lists FLiNG trainer-log folders in the prefix so
  a stuck `info.ini` lock can be cleared without leaving the app.
