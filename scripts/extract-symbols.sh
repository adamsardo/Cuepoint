#!/usr/bin/env bash
# Build the symbol/string evidence index for the rekordbox main binary.
# Outputs stay outside the repo (they are derived from proprietary code):
#   $OUT/syms.txt             raw llvm-nm defined symbols
#   $OUT/imports.txt          undefined (imported) symbols
#   $OUT/syms.demangled.txt   demangled C++/ObjC names
#   $OUT/classes.tsv          owner scope <TAB> symbol count <TAB> member names
#   $OUT/objc_classes.txt     Objective-C classes by method count
#   $OUT/strings.txt          printable strings (min length 6)
# Needs llvm-nm, llvm-cxxfilt, strings, python3. Run after setup-env.sh.
set -euo pipefail

RB_VERSION="${RB_VERSION:-7.2.19}"
BIN="${1:-/opt/rekordbox/${RB_VERSION}/slices/rekordbox-arm64}"
OUT="${2:-/opt/rekordbox/${RB_VERSION}/analysis}"
log() { echo "[symbols $(date +%H:%M:%S)] $*"; }

mkdir -p "${OUT}"
log "dumping symbols from ${BIN}"
llvm-nm --defined-only "${BIN}" > "${OUT}/syms.txt"
llvm-nm -u "${BIN}" > "${OUT}/imports.txt"

log "demangling"
# llvm-nm prints "<16-hex addr> <type> <name>"; names can contain spaces (ObjC).
cut -c20- "${OUT}/syms.txt" | sed -E 's/\.cold\.[0-9]+$//' | llvm-cxxfilt -_ > "${OUT}/syms.demangled.txt"

log "building class index"
python3 - "${OUT}" <<'PY'
import collections, re, sys
out = sys.argv[1]
prefix = re.compile(r'^(non-virtual thunk to |virtual thunk to |typeinfo name for |typeinfo for '
                    r'|vtable for |guard variable for |VTT for |construction vtable for )')
third_party = ('std::', 'juce::', 'icu_51', 'TagLib', 'spdlog', 'fmt::', 'nlohmann', 'asio::', 'boost')

def strip_templates(s):
    out, depth = [], 0
    for c in s:
        if c == '<':
            depth += 1
        elif c == '>':
            depth -= 1
        elif depth == 0:
            out.append(c)
    return ''.join(out)

def split_scope(s):
    parts, depth, cur, i = [], 0, '', 0
    while i < len(s) and s[i] != '(':
        if s.startswith('::', i) and depth == 0:
            parts.append(cur); cur = ''; i += 2; continue
        depth += s[i] == '<'
        depth -= s[i] == '>'
        cur += s[i]; i += 1
    parts.append(cur)
    return parts

counts, members = collections.Counter(), collections.defaultdict(set)
objc = collections.Counter()
for line in open(f'{out}/syms.demangled.txt'):
    line = line.rstrip()
    if line.startswith(('-[', '+[')):
        objc[line[2:].split(' ')[0].split('(')[0]] += 1
        continue
    if '::' not in line:
        continue
    line = prefix.sub('', line)
    if line.startswith(third_party):
        continue
    parts = [strip_templates(p).split(' ')[-1] for p in split_scope(line)]
    if len(parts) < 2:
        continue
    owner = '::'.join(parts[:-1])
    counts[owner] += 1
    members[owner].add(parts[-1])

with open(f'{out}/classes.tsv', 'w') as f:
    for owner in sorted(counts):
        f.write(f"{owner}\t{counts[owner]}\t{' '.join(sorted(members[owner]))[:4000]}\n")
with open(f'{out}/objc_classes.txt', 'w') as f:
    for name, n in objc.most_common():
        f.write(f'{n}\t{name}\n')
print(f'{len(counts)} C++ scopes, {len(objc)} ObjC classes')
PY

log "extracting strings"
strings -n 6 "${BIN}" > "${OUT}/strings.txt"
log "done: ${OUT}"
