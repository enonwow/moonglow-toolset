"""Set the owned private server's native JSON charset before it starts."""
import re
import sys
import tomllib
from pathlib import Path


def configured(text, codepage):
    if codepage not in ("cp1250", "cp1251", "cp1252"):
        raise ValueError("Unsupported game codepage")
    parsed = tomllib.loads(text)
    header = re.search(r"(?m)^\s*\[game\.language\][ \t]*(?:#.*)?$", text)
    setting = 'codepage = "' + codepage + '"\n'
    if header:
        start = header.end()
        following = re.search(r"(?m)^\s*\[", text[start:])
        end = start + following.start() if following else len(text)
        section = text[start:end]
        line = re.search(r"(?m)^[ \t]*codepage[ \t]*=.*(?:\n|$)", section)
        if line:
            section = section[:line.start()] + setting + section[line.end():]
        elif "codepage" in parsed.get("game", {}).get("language", {}):
            raise ValueError("Unrecognized codepage key syntax; settings left unchanged")
        else:
            section = "\n" + setting + section.lstrip("\n")
        text = text[:start] + section + text[end:]
    else:
        if "language" in parsed.get("game", {}):
            raise ValueError("Unrecognized language section syntax; settings left unchanged")
        text = text.rstrip() + "\n\n[game.language]\n" + setting
    assert tomllib.loads(text)["game"]["language"]["codepage"] == codepage
    return text


def main():
    path = Path(sys.argv[1])
    expected = {"mglive-home": (".moonglow-live-demo", "moonglow-live-demo"),
                "mglive-events": (".moonglow-live-events", "moonglow-live-events-v1")}
    assert path.name == "settings.tml" and path.parent.parent == Path.home()
    assert path.resolve() == path and not path.is_symlink() and not path.parent.is_symlink()
    marker, owner = expected[path.parent.name]
    marker = path.parent / marker
    assert not marker.is_symlink() and marker.read_text().strip() == owner
    text = path.read_text(encoding="utf-8") if path.exists() else ""
    updated = configured(text, sys.argv[2])
    if updated != text:
        path.write_text(updated, encoding="utf-8")


if __name__ == "__main__":
    main()
