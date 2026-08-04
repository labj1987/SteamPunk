#!/usr/bin/env bash
# build-appimage.sh — build the Proton Trainer AppImage.
# Run from the repo root on Ubuntu (CI uses ubuntu-latest). Run as root in CI.
set -euo pipefail

APP="proton-trainer"
# Single source of truth: the version in Cargo.toml
VERSION="$(grep -m1 '^version' Cargo.toml | cut -d'"' -f2)"
ARCH="x86_64"
BUILD_DIR="build-appimage"
APPDIR="$BUILD_DIR/AppDir"

echo "==> Building $APP $VERSION AppImage"

# ── Build dependencies ────────────────────────────────────────────────
# zsync is installed unconditionally: on CI a prior workflow step already
# installs cargo, so a `command -v cargo` guard around this evaluates false
# and anything gated behind it — zsync included — gets silently skipped.
apt-get install -y -qq zsync

if ! command -v cargo >/dev/null 2>&1 || ! pkg-config --exists gtk4 2>/dev/null; then
    echo "==> Installing build dependencies"
    apt-get update -qq
    apt-get install -y -qq cargo rustc libgtk-4-dev libadwaita-1-dev \
        pkg-config libssl-dev wget file desktop-file-utils zsync
fi

# ── Release build ─────────────────────────────────────────────────────
echo "==> cargo build --release"
cargo build --release

# ── AppDir layout ─────────────────────────────────────────────────────
rm -rf "$BUILD_DIR"
mkdir -p "$APPDIR/usr/bin" \
         "$APPDIR/usr/share/applications" \
         "$APPDIR/usr/share/icons/hicolor/256x256/apps" \
         "$APPDIR/usr/share/metainfo"

cp "target/release/$APP"                              "$APPDIR/usr/bin/"
cp data/$APP.desktop                                  "$APPDIR/usr/share/applications/"
cp data/$APP.png                                       "$APPDIR/usr/share/icons/hicolor/256x256/apps/"
cp data/io.github.labj1987.ProtonTrainer.appdata.xml   "$APPDIR/usr/share/metainfo/"

# Top-level AppImage requirements
cp data/$APP.desktop "$APPDIR/"
cp data/$APP.png     "$APPDIR/"

# ── AppRun ────────────────────────────────────────────────────────────
cat > "$APPDIR/AppRun" << 'APPRUN'
#!/usr/bin/env bash
HERE="$(dirname "$(readlink -f "$0")")"
export PATH="$HERE/usr/bin:$PATH"
exec "$HERE/usr/bin/proton-trainer" "$@"
APPRUN
chmod 755 "$APPDIR/AppRun"

# ── appimagetool ──────────────────────────────────────────────────────
TOOL="$BUILD_DIR/appimagetool"
if [[ ! -f "$TOOL" ]]; then
    echo "==> Downloading appimagetool"
    wget -q -O "$TOOL" \
        "https://github.com/AppImage/appimagetool/releases/download/continuous/appimagetool-x86_64.AppImage"
    chmod +x "$TOOL"
fi

echo "==> Packing AppImage"
OUT="$APP-$VERSION-$ARCH.AppImage"

UPDATE_INFORMATION="gh-releases-zsync|labj1987|proton-trainer|latest|proton-trainer-*-x86_64.AppImage.zsync"
VERSION="$VERSION" ARCH="$ARCH" "$TOOL" --appimage-extract-and-run \
    -u "$UPDATE_INFORMATION" "$APPDIR" "$OUT"

echo "==> Done: $OUT"
ls -lh "$OUT"

# appimagetool's built-in zsync generation silently no-ops on GitHub Actions
# runners even when zsyncmake is installed and working (see MKI's CLAUDE.md
# for the diagnosis) — build the .zsync sidecar directly instead. Non-fatal:
# the AppImage itself is already valid without it.
echo "==> Generating .zsync sidecar"
if zsyncmake "$OUT"; then
    echo "==> .zsync generated: $OUT.zsync"
else
    echo "==> WARNING: zsyncmake failed — continuing without .zsync"
fi
