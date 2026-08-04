# Changelog

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
