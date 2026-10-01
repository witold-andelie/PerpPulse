"""Reject local-only progress data and CJK text from Git publication surfaces."""

from __future__ import annotations

import argparse
import re
import subprocess
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
LOCAL_ONLY_NAMES = {"progress.md"}
CJK_PATTERN = re.compile(
    "["
    "\u3000-\u303f"  # CJK punctuation
    "\u3040-\u30ff"  # Hiragana and Katakana
    "\u3100-\u312f"  # Bopomofo
    "\u3400-\u4dbf"  # CJK Extension A
    "\u4e00-\u9fff"  # CJK Unified Ideographs
    "\uac00-\ud7af"  # Hangul syllables
    "\uf900-\ufaff"  # CJK compatibility ideographs
    "]"
)


def git_output(*args: str) -> bytes:
    result = subprocess.run(
        ["git", *args],
        cwd=ROOT,
        check=False,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    if result.returncode != 0:
        message = result.stderr.decode("utf-8", errors="replace").strip()
        raise RuntimeError(message or f"git {' '.join(args)} failed")
    return result.stdout


def split_paths(raw: bytes) -> list[str]:
    return [part.decode("utf-8", errors="surrogateescape") for part in raw.split(b"\0") if part]


def candidate_paths(staged: bool, working_tree: bool = False) -> list[str]:
    if staged:
        raw = git_output("diff", "--cached", "--name-only", "--diff-filter=ACMR", "-z")
    else:
        raw = git_output("ls-files", "-z")
    paths = split_paths(raw)
    if working_tree:
        paths += split_paths(git_output("ls-files", "--others", "--exclude-standard", "-z"))
    return sorted(set(paths))


def staged_bytes(path: str) -> bytes:
    return git_output("show", f":{path}")


def working_bytes(path: str) -> bytes:
    return (ROOT / path).read_bytes()


def decode_text(raw: bytes) -> str | None:
    if raw.startswith((b"\xff\xfe", b"\xfe\xff")):
        return raw.decode("utf-16", errors="replace")
    if b"\0" in raw:
        return None
    return raw.decode("utf-8", errors="replace")


def check(staged: bool, working_tree: bool = False) -> int:
    violations: list[str] = []
    for path in candidate_paths(staged, working_tree):
        normalized = path.replace("\\", "/")
        if Path(normalized).name.lower() in LOCAL_ONLY_NAMES:
            violations.append(f"local-only file is included: {path}")
            continue
        if CJK_PATTERN.search(normalized):
            violations.append(f"CJK characters appear in the filename: {path}")
            continue
        try:
            raw = staged_bytes(path) if staged else working_bytes(path)
        except (OSError, RuntimeError) as exc:
            violations.append(f"cannot inspect {path}: {exc}")
            continue
        text = decode_text(raw)
        if text is not None and CJK_PATTERN.search(text):
            violations.append(f"CJK characters appear in file content: {path}")

    if violations:
        print("Repository publication policy failed:", file=sys.stderr)
        for violation in violations:
            print(f"- {violation}", file=sys.stderr)
        return 1

    scope = "staged" if staged else "working-tree" if working_tree else "tracked"
    print(f"Repository publication policy passed for {scope} files.")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser()
    group = parser.add_mutually_exclusive_group(required=True)
    group.add_argument("--staged", action="store_true")
    group.add_argument("--tracked", action="store_true")
    group.add_argument("--working-tree", action="store_true", help="Include new, non-ignored files before staging")
    args = parser.parse_args()
    return check(staged=args.staged, working_tree=args.working_tree)


if __name__ == "__main__":
    raise SystemExit(main())
