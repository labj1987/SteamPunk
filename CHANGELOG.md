# Changelog

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
