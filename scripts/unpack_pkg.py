#!/usr/bin/env python3
"""Unpack a macOS flat .pkg (xar) and its gzip'd cpio Payloads.

Pure stdlib, so the setup script doesn't need apt for 7z/cpio.
Usage: unpack_pkg.py INSTALLER.pkg OUT_DIR
Each component's Payload lands in OUT_DIR/<component name without .pkg>/.
"""
import gzip
import os
import stat
import struct
import sys
import xml.etree.ElementTree as ET
import zlib


class Slice:
    """Read-only file view over [offset, offset+length)."""

    def __init__(self, f, offset, length):
        self.f, self.pos, self.end = f, offset, offset + length

    def read(self, n=-1):
        if n < 0 or n > self.end - self.pos:
            n = self.end - self.pos
        self.f.seek(self.pos)
        data = self.f.read(n)
        self.pos += len(data)
        return data


def xar_payloads(f):
    magic, hsize, _ver, toc_len, _toc_raw, _alg = struct.unpack(">4sHHQQI", f.read(28))
    assert magic == b"xar!", "not a xar archive"
    f.seek(hsize)
    toc = ET.fromstring(zlib.decompress(f.read(toc_len)))
    heap = hsize + toc_len

    def walk(node, path):
        for child in node.findall("file"):
            name = os.path.join(path, child.findtext("name"))
            data = child.find("data")
            if data is not None and child.findtext("name") == "Payload":
                style = data.find("encoding").get("style")
                yield name, style, heap + int(data.findtext("offset")), int(data.findtext("length"))
            yield from walk(child, name)

    yield from walk(toc.find("toc"), "")


def read_exact(stream, n):
    buf = b""
    while len(buf) < n:
        chunk = stream.read(n - len(buf))
        if not chunk:
            raise EOFError("truncated cpio")
        buf += chunk
    return buf


def extract_cpio(stream, out):
    """Extract an odc ("070707") cpio stream, as written by macOS pkgbuild."""
    count = 0
    while True:
        hdr = read_exact(stream, 76)
        assert hdr[:6] == b"070707", f"unsupported cpio header {hdr[:6]!r}"
        mode = int(hdr[18:24], 8)
        namesize = int(hdr[59:65], 8)
        filesize = int(hdr[65:76], 8)
        name = read_exact(stream, namesize)[:-1].decode()
        if name == "TRAILER!!!":
            return count
        dest = os.path.normpath(os.path.join(out, name))
        assert dest == out or dest.startswith(out + os.sep), f"unsafe path {name}"
        kind = stat.S_IFMT(mode)
        if kind == stat.S_IFDIR:
            os.makedirs(dest, exist_ok=True)
        elif kind == stat.S_IFLNK:
            target = read_exact(stream, filesize).decode()
            os.makedirs(os.path.dirname(dest), exist_ok=True)
            if os.path.lexists(dest):
                os.remove(dest)
            os.symlink(target, dest)
            filesize = 0
        else:
            os.makedirs(os.path.dirname(dest), exist_ok=True)
            with open(dest, "wb") as o:
                left = filesize
                while left:
                    chunk = stream.read(min(left, 1 << 20))
                    if not chunk:
                        raise EOFError("truncated cpio")
                    o.write(chunk)
                    left -= len(chunk)
            os.chmod(dest, stat.S_IMODE(mode))
            filesize = 0
        if filesize:
            read_exact(stream, filesize)
        count += 1


def main(pkg, out):
    out = os.path.abspath(out)
    with open(pkg, "rb") as f:
        for name, style, offset, length in list(xar_payloads(f)):
            component = os.path.basename(os.path.dirname(name)).removesuffix(".pkg") or "root"
            dest = os.path.join(out, component)
            os.makedirs(dest, exist_ok=True)
            if style != "application/octet-stream":
                raise SystemExit(f"unsupported xar encoding {style} for {name}")
            n = extract_cpio(gzip.GzipFile(fileobj=Slice(f, offset, length)), dest)
            print(f"{name}: {n} entries -> {dest}")


if __name__ == "__main__":
    main(*sys.argv[1:3])
