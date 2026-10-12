"""Where the UI inventory's inputs are kept, and how they are read.

The inputs are made from the installed game by `inputs.py` and are game
data: they live outside the repository and are never committed.

    $MOONGLOW_UIINV, else $MOONGLOW_ORACLE/uiinv,
    else ~/.local/share/moonglow-oracle/uiinv

    forms.json     every VCL form of nwtoolset.exe, parsed (dfmparse.py)
    dialog.json    dialog.tlk (English): {"entries": [{"id", "text"}]}
    exe.data       the exe's .data section, as it lies in the file
    locbases.json  each form's StrRef table: its offset in exe.data (matchloc.py)
    build.json     which nwtoolset.exe this was read from
"""
import json
import os
from pathlib import Path

REPO = Path(__file__).resolve().parents[3]
INVENTORY = REPO / 'docs/parity/aurora-ui-inventory.md'


def work_dir():
    d = os.environ.get('MOONGLOW_UIINV')
    if d:
        return Path(d)
    oracle = os.environ.get('MOONGLOW_ORACLE')
    base = Path(oracle) if oracle else Path.home() / '.local/share/moonglow-oracle'
    return base / 'uiinv'


def need(name):
    p = work_dir() / name
    if not p.exists():
        raise SystemExit(f'{p} is missing: run tools/aurora/uiinv/inputs.py first')
    return p


def forms():
    return json.load(open(need('forms.json')))


def tlk():
    return {e['id']: e.get('text', '') for e in json.load(open(need('dialog.json')))['entries']}


def exe_data():
    return need('exe.data').read_bytes()


def locbases():
    return json.load(open(need('locbases.json')))


def build():
    return json.load(open(need('build.json')))
