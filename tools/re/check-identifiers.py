#!/usr/bin/env python3
"""Check that no identifier of the decompiled aquasuite code enters the repository (design D9).

Scope: the text files of docs/protocol/, docs/api.md, docs/storage.md and crates/.

Token rule: a token is a maximal run of [A-Za-z0-9_] with 6 or more characters that does not start
with a digit and is a compound name: it holds an underscore between two characters, a lowercase
letter followed by an uppercase letter, or a letter followed by a digit. A plain word ("Pressure",
"standby", "LEAKSHIELD") is not checked, because it says nothing about the origin of a name.
The script hashes each token (case-sensitive) with SHA-256 (lowercase hex of the UTF-8 bytes) and
compares it with tools/re/identifier-hashes.txt, minus the tokens of tools/re/identifier-allowlist.txt.
A token that is in the hash file and not in the allowlist is an error (exit 1).

Options:
  --build-hashes DIR  write the hash file from the identifiers of the .cs files under DIR
                      (tokens with the same rule, minus tokens that start with a digit)
  --self-test         plant a hashed token in a temporary tree and expect exit 1 on it
"""

import argparse
import hashlib
import re
import sys
import tempfile
from pathlib import Path

TOKEN = re.compile(r"[A-Za-z0-9_]{6,}")
COMPOUND = re.compile(r"[A-Za-z0-9]_[A-Za-z0-9]|[a-z][A-Z]|[A-Za-z][0-9]")


def checked(token):
    return not token[0].isdigit() and COMPOUND.search(token) is not None
SCOPE = ("docs/protocol", "docs/api.md", "docs/storage.md", "crates")
SKIP_DIRS = {"target", ".git"}
TOOL_DIR = Path(__file__).resolve().parent


def sha(token):
    return hashlib.sha256(token.encode("utf-8")).hexdigest()


def read_text(path):
    data = path.read_bytes()
    if b"\0" in data:
        return None
    try:
        return data.decode("utf-8")
    except UnicodeDecodeError:
        return None


def scope_files(root):
    for entry in SCOPE:
        base = root / entry
        if base.is_file():
            yield base
        elif base.is_dir():
            for path in sorted(base.rglob("*")):
                if path.is_file() and not SKIP_DIRS.intersection(path.relative_to(root).parts):
                    yield path


def read_lines(path):
    if not path.is_file():
        return []
    lines = (line.strip() for line in path.read_text(encoding="utf-8").splitlines())
    return [line for line in lines if line and not line.startswith("#")]


def check(root, hashes_file, allowlist_file):
    hashes = set(read_lines(hashes_file))
    allowed = set(read_lines(allowlist_file))
    errors = []
    for path in scope_files(root):
        text = read_text(path)
        if text is None:
            continue
        for number, line in enumerate(text.splitlines(), 1):
            for match in TOKEN.finditer(line):
                token = match.group()
                if checked(token) and token not in allowed and sha(token) in hashes:
                    errors.append(f"{path.relative_to(root)}:{number}:{match.start() + 1}: {token}")
    return errors


def build_hashes(source, out_file):
    found = set()
    for path in source.rglob("*.cs"):
        text = read_text(path)
        if text is None:
            continue
        for token in TOKEN.findall(text):
            if checked(token):
                found.add(sha(token))
    out_file.write_text("".join(f"{h}\n" for h in sorted(found)), encoding="utf-8")
    return len(found)


def self_test():
    planted = "planted_token_for_self_test"
    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp)
        (root / "docs").mkdir()
        (root / "crates" / "demo").mkdir(parents=True)
        hashes = root / "hashes.txt"
        allow = root / "allow.txt"
        hashes.write_text(sha(planted) + "\n", encoding="utf-8")
        allow.write_text("# none\n", encoding="utf-8")
        clean = root / "crates" / "demo" / "clean.rs"
        clean.write_text("fn clean_function() {}\n", encoding="utf-8")
        assert not check(root, hashes, allow), "clean tree must pass"
        (root / "docs" / "api.md").write_text(f"text {planted} text\n", encoding="utf-8")
        assert len(check(root, hashes, allow)) == 1, "planted token in docs/api.md must fail"
        (root / "docs" / "api.md").unlink()
        planted_rs = root / "crates" / "demo" / "planted.rs"
        planted_rs.write_text(f"fn {planted}() {{}}\n", encoding="utf-8")
        assert len(check(root, hashes, allow)) == 1, "planted token in crates/ must fail"
        allow.write_text(planted + "\n", encoding="utf-8")
        assert not check(root, hashes, allow), "allowlisted token must pass"
        allow.write_text("# none\n", encoding="utf-8")
        planted_rs.write_bytes(b"\0\xff" + planted.encode())
        assert not check(root, hashes, allow), "binary file must be skipped"
        planted_rs.unlink()
        short = root / "crates" / "demo" / "short.rs"
        hashes.write_text(sha("short") + "\n", encoding="utf-8")
        short.write_text("let short = 1;\n", encoding="utf-8")
        assert not check(root, hashes, allow), "token under 6 characters is not checked"
        hashes.write_text(sha("Pressure") + "\n", encoding="utf-8")
        short.write_text("// Pressure in mbar\n", encoding="utf-8")
        assert not check(root, hashes, allow), "a plain word is not checked"
    print("self-test ok")
    return 0


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--build-hashes", metavar="DIR", type=Path)
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("--root", type=Path, default=TOOL_DIR.parent.parent)
    parser.add_argument("--hashes", type=Path, default=TOOL_DIR / "identifier-hashes.txt")
    parser.add_argument("--allowlist", type=Path, default=TOOL_DIR / "identifier-allowlist.txt")
    args = parser.parse_args()

    if args.self_test:
        return self_test()
    if args.build_hashes:
        count = build_hashes(args.build_hashes, args.hashes)
        print(f"{count} hashes written to {args.hashes}")
        return 0
    errors = check(args.root, args.hashes, args.allowlist)
    for error in errors:
        print(error)
    if errors:
        print(f"{len(errors)} identifier(s) of the decompiled code found", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
