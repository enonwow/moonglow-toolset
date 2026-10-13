#!/usr/bin/env python3
"""Make the inputs of the Aurora UI inventory from the installed game.

    python3 tools/aurora/uiinv/inputs.py [--root GAME] [--out DIR]
    python3 tools/aurora/uiinv/gen.py          # writes docs/parity/aurora-ui-inventory.md

Reads two files of the install, as data (nothing of the game is run):

    bin/win32/nwtoolset.exe     its RCDATA resources that are VCL forms (TPF0)
                                and its .data section (the forms' StrRef tables)
    lang/en/data/dialog.tlk     the English talk table (else data/dialog.tlk)

and writes, into the work directory (see common.py: $MOONGLOW_UIINV, else
~/.local/share/moonglow-oracle/uiinv; game data, never committed):

    forms.json      the forms, parsed by dfmparse.py, by resource name
    exe.data        the .data section's bytes as they lie in the file
    dialog.json     {"entries": [{"id": n, "text": "..."}]}, as `nwn_tlk -k json`
                    has them (entries without text left out)
    locbases.json   matchloc.py's vote for each form's StrRef table
    build.json      the exe read: bin/win32/build.txt, its version resource,
                    link time, size and SHA-256

The game install: --root, else $NWN_ROOT, else Steam's default folder.

What is not made here, because it is not derived: `mapping.py` (the GFF
field each control edits, notes, sections, and LOC_OVERRIDE: the twelve
StrRef tables whose vote was not clear, settled by eye with loccheck.py).
Those offsets are of the exe they were found in; `gen.py` says so when
build.json names another build.
"""
import argparse
import hashlib
import json
import os
import struct
import sys
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import common
import dfmparse
import matchloc

RT_RCDATA, RT_VERSION = 10, 16


class Pe:
    """The little of a PE file this needs: its sections and resources."""

    def __init__(self, b):
        self.b = b
        if b[:2] != b'MZ':
            raise SystemExit('not a PE file (no MZ)')
        pe = struct.unpack_from('<I', b, 0x3C)[0]
        if b[pe:pe + 4] != b'PE\0\0':
            raise SystemExit('not a PE file (no PE signature)')
        nsect, self.link_time = struct.unpack_from('<HI', b, pe + 6)
        optsize = struct.unpack_from('<H', b, pe + 20)[0]
        opt = pe + 24
        magic = struct.unpack_from('<H', b, opt)[0]
        dirs = opt + {0x10B: 96, 0x20B: 112}[magic]
        self.bits = 32 if magic == 0x10B else 64
        self.rsrc_rva = struct.unpack_from('<I', b, dirs + 2 * 8)[0]
        self.sections = []
        for i in range(nsect):
            o = opt + optsize + i * 40
            name = b[o:o + 8].rstrip(b'\0').decode('ascii', 'replace')
            vsize, vaddr, rawsize, rawptr = struct.unpack_from('<4I', b, o + 8)
            self.sections.append((name, vsize, vaddr, rawsize, rawptr))

    def section(self, name):
        """A section's bytes as they lie in the file (its raw size)."""
        for n, _vsize, _vaddr, rawsize, rawptr in self.sections:
            if n == name:
                return self.b[rawptr:rawptr + rawsize]
        raise SystemExit(f'no {name} section')

    def at_rva(self, rva):
        for _n, vsize, vaddr, rawsize, rawptr in self.sections:
            if vaddr <= rva < vaddr + max(vsize, rawsize):
                return rawptr + rva - vaddr
        raise SystemExit(f'RVA {rva:#x} is in no section')

    def _dir(self, base, off):
        named, ids = struct.unpack_from('<HH', self.b, base + off + 12)
        out = []
        for i in range(named + ids):
            name, to = struct.unpack_from('<II', self.b, base + off + 16 + i * 8)
            if name & 0x80000000:
                p = base + (name & 0x7FFFFFFF)
                n = struct.unpack_from('<H', self.b, p)[0]
                name = self.b[p + 2:p + 2 + n * 2].decode('utf-16-le')
            out.append((name, to))
        return out

    def resources(self, kind):
        """{name or id: bytes} of the resources of one type (first language)."""
        base = self.at_rva(self.rsrc_rva)
        out = {}
        for t, to in self._dir(base, 0):
            if t != kind or not to & 0x80000000:
                continue
            for name, to in self._dir(base, to & 0x7FFFFFFF):
                if to & 0x80000000:
                    to = self._dir(base, to & 0x7FFFFFFF)[0][1]
                rva, size = struct.unpack_from('<II', self.b, base + to)
                o = self.at_rva(rva)
                out[name] = self.b[o:o + size]
        return out


def read_tlk(b):
    """dialog.tlk's entries that have text, as nwn_tlk's JSON has them."""
    if b[:8] != b'TLK V3.0':
        raise SystemExit('dialog.tlk: not a TLK V3.0 file')
    _lang, count, strings = struct.unpack_from('<3I', b, 8)
    entries = []
    for i in range(count):
        o = 20 + i * 40
        flags = struct.unpack_from('<I', b, o)[0]
        off, size = struct.unpack_from('<II', b, o + 28)
        if flags & 1 and size:
            text = b[strings + off:strings + off + size].decode('cp1252', 'replace')
            entries.append({'id': i, 'text': text})
    return {'entries': entries}


def file_version(pe):
    """The exe's version resource (VS_FIXEDFILEINFO), "a.b.c.d", if it has one."""
    for data in pe.resources(RT_VERSION).values():
        i = data.find(struct.pack('<I', 0xFEEF04BD))
        if i >= 0:
            ms, ls = struct.unpack_from('<II', data, i + 8)
            return f'{ms >> 16}.{ms & 0xFFFF}.{ls >> 16}.{ls & 0xFFFF}'
    return None


def find_root(arg):
    root = arg or os.environ.get('NWN_ROOT')
    root = Path(root) if root else Path.home() / '.local/share/Steam/steamapps/common/Neverwinter Nights'
    if not (root / 'bin/win32/nwtoolset.exe').is_file():
        raise SystemExit(f'{root}: no bin/win32/nwtoolset.exe (give --root or set NWN_ROOT)')
    return root


def main():
    ap = argparse.ArgumentParser(description=__doc__.split('\n')[0])
    ap.add_argument('--root', help='game install (default: $NWN_ROOT, else the Steam default)')
    ap.add_argument('--out', help='work directory (default: see common.py)')
    a = ap.parse_args()
    root = find_root(a.root)
    out = Path(a.out) if a.out else common.work_dir()
    if common.REPO in out.resolve().parents and 'target' not in out.resolve().relative_to(common.REPO).parts[:1]:
        raise SystemExit(f'{out}: game data does not go into the repository')
    out.mkdir(parents=True, exist_ok=True)

    exe_path = root / 'bin/win32/nwtoolset.exe'
    exe = exe_path.read_bytes()
    pe = Pe(exe)

    forms = {}
    for name, data in sorted(pe.resources(RT_RCDATA).items(), key=lambda kv: str(kv[0])):
        if data[:4] == b'TPF0':
            forms[str(name)] = dfmparse.parse(data)
    json.dump(forms, open(out / 'forms.json', 'w'), indent=1)

    data = pe.section('.data')
    (out / 'exe.data').write_bytes(data)

    tlk_path = next((p for p in (root / 'lang/en/data/dialog.tlk', root / 'data/dialog.tlk') if p.is_file()), None)
    if tlk_path is None:
        raise SystemExit(f'{root}: no lang/en/data/dialog.tlk or data/dialog.tlk')
    tlk = read_tlk(tlk_path.read_bytes())
    json.dump(tlk, open(out / 'dialog.json', 'w'), ensure_ascii=False)

    T = {e['id']: e['text'] for e in tlk['entries']}
    locbases = matchloc.vote(forms, T, data)
    json.dump(locbases, open(out / 'locbases.json', 'w'), indent=1)

    build_txt = root / 'bin/win32/build.txt'
    lines = build_txt.read_text().split('\n') if build_txt.is_file() else []
    build = {
        'exe': 'bin/win32/nwtoolset.exe',
        'size': len(exe),
        'sha256': hashlib.sha256(exe).hexdigest(),
        'bits': pe.bits,
        'linked': time.strftime('%Y-%m-%dT%H:%M:%SZ', time.gmtime(pe.link_time)),
        'file_version': file_version(pe),
        'build_id': lines[0].strip() if lines else None,
        'build_date': lines[1].strip() if len(lines) > 1 else None,
        'talk_table': str(tlk_path.relative_to(root)),
        'talk_table_sha256': hashlib.sha256(tlk_path.read_bytes()).hexdigest(),
    }
    json.dump(build, open(out / 'build.json', 'w'), indent=1)

    found = sum(1 for v in locbases.values() if v)
    print(f'{out}: {len(forms)} forms, .data {len(data)} bytes, {len(tlk["entries"])} strings, '
          f'{found} StrRef tables voted for')
    print(f'nwtoolset.exe: build {build["build_id"]} ({build["build_date"]}), '
          f'version {build["file_version"]}, linked {build["linked"]}')


if __name__ == '__main__':
    main()
