#!/usr/bin/env bash
# Cloud environment setup for reverse engineering rekordbox (macOS).
# Installs Ghidra for REA and downloads + unpacks the rekordbox installer.
# Safe to re-run: each step skips if its output already exists.
set -euo pipefail

GHIDRA_VERSION="12.1.4"
GHIDRA_ZIP="ghidra_${GHIDRA_VERSION}_PUBLIC_20260921.zip"
GHIDRA_URL="https://sourceforge.net/projects/ghidra.mirror/files/Ghidra_${GHIDRA_VERSION}_build/${GHIDRA_ZIP}/download"
GHIDRA_DIR="/opt/ghidra"

RB_VERSION="7.2.19"
RB_URL="https://cdn.rekordbox.com/files/20260914095016/Install_rekordbox_7_2_19.pkg_.zip"
RB_DIR="/opt/rekordbox/${RB_VERSION}"

# Extraction tools for .pkg (xar archive) and its gzip/cpio payloads
if ! command -v 7z >/dev/null || ! command -v cpio >/dev/null; then
  apt-get update -qq
  DEBIAN_FRONTEND=noninteractive apt-get install -y -qq p7zip-full cpio >/dev/null
fi

# Ghidra
if [ ! -x "${GHIDRA_DIR}/support/analyzeHeadless" ]; then
  tmp="$(mktemp -d)"
  curl -fsSL --retry 3 -o "${tmp}/ghidra.zip" "${GHIDRA_URL}"
  unzip -q "${tmp}/ghidra.zip" -d "${tmp}"
  rm -rf "${GHIDRA_DIR}"
  mv "${tmp}"/ghidra_${GHIDRA_VERSION}_PUBLIC "${GHIDRA_DIR}"
  rm -rf "${tmp}"
fi

# rekordbox: zip -> .pkg -> component pkgs -> Payload -> rekordbox.app
if [ ! -d "${RB_DIR}/payload" ]; then
  mkdir -p "${RB_DIR}"
  curl -fsSL --retry 3 -o "${RB_DIR}/installer.zip" "${RB_URL}"
  unzip -q -o "${RB_DIR}/installer.zip" -d "${RB_DIR}/zip"
  pkg="$(find "${RB_DIR}/zip" -name '*.pkg' -print -quit)"
  # -txar: unpack the xar layer only (otherwise 7z also gunzips Payload)
  7z x -txar -y -bd -o"${RB_DIR}/pkg" "${pkg}" >/dev/null
  mkdir -p "${RB_DIR}/payload"
  # Each component package has its own Payload (gzip'd cpio)
  find "${RB_DIR}/pkg" -name Payload -type f | while read -r payload; do
    name="$(basename "$(dirname "${payload}")" .pkg)"
    mkdir -p "${RB_DIR}/payload/${name}"
    (cd "${RB_DIR}/payload/${name}" && gzip -dc "${payload}" | cpio -idm --quiet)
  done
  rm -f "${RB_DIR}/installer.zip"
fi

# Split the universal main binary into per-arch slices (no lipo on Linux)
RB_BIN="${RB_DIR}/payload/rekordbox/rekordbox.app/Contents/MacOS/rekordbox"
if [ ! -d "${RB_DIR}/slices" ]; then
  mkdir -p "${RB_DIR}/slices"
  python3 - "${RB_BIN}" "${RB_DIR}/slices" <<'PY'
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
        print(path)
PY
fi

echo "Ghidra: ${GHIDRA_DIR}"
echo "rekordbox: ${RB_DIR}/payload"
