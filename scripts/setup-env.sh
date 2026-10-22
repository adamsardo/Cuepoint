#!/usr/bin/env bash
# Cloud environment setup for reverse engineering rekordbox (macOS).
# Installs Ghidra for REA and downloads + unpacks the rekordbox installer.
# Safe to re-run: each step skips if its output already exists.
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
log() { echo "[setup $(date +%H:%M:%S)] $*"; }

GHIDRA_VERSION="12.1.4"
GHIDRA_ZIP="ghidra_${GHIDRA_VERSION}_PUBLIC_20260921.zip"
GHIDRA_URL="https://sourceforge.net/projects/ghidra.mirror/files/Ghidra_${GHIDRA_VERSION}_build/${GHIDRA_ZIP}/download"
GHIDRA_DIR="/opt/ghidra"

RB_VERSION="7.2.19"
RB_URL="https://cdn.rekordbox.com/files/20260914095016/Install_rekordbox_7_2_19.pkg_.zip"
RB_DIR="/opt/rekordbox/${RB_VERSION}"

# Ghidra
if [ ! -x "${GHIDRA_DIR}/support/analyzeHeadless" ]; then
  tmp="$(mktemp -d)"
  log "downloading Ghidra ${GHIDRA_VERSION} (~570 MB)"
  curl -fsSL --retry 3 -o "${tmp}/ghidra.zip" "${GHIDRA_URL}"
  log "unzipping Ghidra"
  unzip -q "${tmp}/ghidra.zip" -d "${tmp}"
  rm -rf "${GHIDRA_DIR}"
  mv "${tmp}"/ghidra_${GHIDRA_VERSION}_PUBLIC "${GHIDRA_DIR}"
  rm -rf "${tmp}"
fi
log "Ghidra ready: ${GHIDRA_DIR}"

# rekordbox: zip -> .pkg (xar) -> gzip'd cpio Payload -> rekordbox.app
if [ ! -d "${RB_DIR}/payload" ]; then
  mkdir -p "${RB_DIR}"
  log "downloading rekordbox ${RB_VERSION} installer (~940 MB)"
  curl -fsSL --retry 3 -o "${RB_DIR}/installer.zip" "${RB_URL}"
  log "unzipping installer"
  unzip -q -o "${RB_DIR}/installer.zip" -d "${RB_DIR}/zip"
  rm -f "${RB_DIR}/installer.zip"
  pkg="$(find "${RB_DIR}/zip" -name '*.pkg' -print -quit)"
  log "unpacking ${pkg##*/}"
  python3 "${HERE}/unpack_pkg.py" "${pkg}" "${RB_DIR}/payload.tmp"
  mv "${RB_DIR}/payload.tmp" "${RB_DIR}/payload"
fi
log "rekordbox ready: ${RB_DIR}/payload"

# Split the universal main binary into per-arch slices (no lipo on Linux)
RB_BIN="${RB_DIR}/payload/rekordbox/rekordbox.app/Contents/MacOS/rekordbox"
if [ ! -d "${RB_DIR}/slices" ]; then
  log "splitting universal binary"
  mkdir -p "${RB_DIR}/slices.tmp"
  python3 - "${RB_BIN}" "${RB_DIR}/slices.tmp" <<'PY'
import struct, sys
src, out = sys.argv[1], sys.argv[2]
names = {0x01000007: "x86_64", 0x0100000C: "arm64"}
with open(src, "rb") as f:
    magic, n = struct.unpack(">II", f.read(8))
    assert magic == 0xCAFEBABE, "not a fat Mach-O"
    archs = [struct.unpack(">iiIII", f.read(20)) for _ in range(n)]
    for cputype, _, offset, size, _ in archs:
        f.seek(offset)
        path = f"{out}/rekordbox-{names.get(cputype & 0xFFFFFFFF, hex(cputype))}"
        with open(path, "wb") as o:
            o.write(f.read(size))
PY
  mv "${RB_DIR}/slices.tmp" "${RB_DIR}/slices"
fi
log "slices ready: $(ls "${RB_DIR}/slices" | tr '\n' ' ')"
