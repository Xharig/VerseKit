# -*- coding: utf-8 -*-
#
# VerseKit — companion for Star Citizen.
# Copyright (C) 2026 Xharig
#
# SPDX-License-Identifier: GPL-3.0-only
#
# This program is free software: you can redistribute it and/or modify
# it under the terms of the GNU General Public License as published by
# the Free Software Foundation, version 3.
#
# This program is distributed in the hope that it will be useful,
# but WITHOUT ANY WARRANTY; without even the implied warranty of
# MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
# GNU General Public License for more details.
#
# You should have received a copy of the GNU General Public License
# along with this program.  If not, see <https://www.gnu.org/licenses/>.
"""
Guard before a push to the public repository: searches the working tree
and the history that would be pushed for traces of things that must not
be public.

The script holds the mechanism only. What counts as a trace is read from a
file of patterns that lies outside the repository (see `pattern_file`);
without it the guard refuses to run, because an empty list would say
"clean" about everything.

Run:
    python -I tools/privat_spuren_pruefen.py [--ref <rev>]... [--nur-baum] [--flach]
    python -I tools/privat_spuren_pruefen.py --selbsttest

Without `--ref` the history of `HEAD` and of every tag is searched. All
file contents of that history are searched (not only the changed lines), so
a binary file that was added and removed again is found too, as is a file
whose bytes match a listed SHA-256 under any name. `--flach` skips the
search through all file contents of the history (faster, less sure).
Exit code: 0 clean, 1 traces found, 2 the search itself failed or could not
look at everything (a file it could not read counts).

The file of patterns is JSON: {"muster": [{"regex": "...", "probe": "..."}],
"sha256": ["..."]}. `probe` is a text the regex must match. The names in the
report are only numbers, so the report says nothing about what is searched.

`--selbsttest` is the counter-check: it builds throw-away repositories that
hold made-up traces in every place (file text, binary file, UTF-16 file,
file name, picture under another name, removed again in a later commit,
commit message, author, tag) and demands that the search finds each of them;
a clean repository with harmless words must stay clean. With a file of
patterns present, its real patterns are planted the same way.
"""

import argparse
import hashlib
import json
import os
import re
import subprocess
import sys
import tempfile
import threading
import unicodedata
import urllib.parse
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent

# A file bigger than this is looked at by its name and its hash only.
MAX_BYTES = 16 * 1024 * 1024
COMMIT_MARK = "@@COMMIT@@"
ENV_PATTERNS = "VERSEKIT_SPUREN"


class Failed(Exception):
    pass


def pattern_file():
    """Where the patterns lie: the variable, else next to the settings of the private tools."""
    if os.environ.get(ENV_PATTERNS):
        return Path(os.environ[ENV_PATTERNS])
    if os.name == "nt":
        base = Path(os.environ.get("LOCALAPPDATA") or Path.home())
    else:
        base = Path(os.environ.get("XDG_CONFIG_HOME") or Path.home() / ".config")
    return base / "VerseKit-Waechter" / "spuren.json"


class Patterns:
    def __init__(self, entries, hashes):
        flags = re.IGNORECASE | re.MULTILINE
        self.items = [(f"Muster {i + 1}", re.compile(e["regex"], flags), e.get("probe", "")) for i, e in enumerate(entries)]
        # Patterns that make no sense in the bytes of a binary file ("nur_text").
        self.text_only = {f"Muster {i + 1}" for i, e in enumerate(entries) if e.get("nur_text")}
        self.hashes = {h.lower() for h in hashes}

    def traces_in(self, text, binary=False):
        return [name for name, rx, _ in self.items if rx.search(text) and not (binary and name in self.text_only)]

    def whole(self, text, binary=False):
        """The patterns that match somewhere in `text`: one pass each, so a long file without a hit is quick."""
        return [(name, rx) for name, rx, _ in self.items if rx.search(text) and not (binary and name in self.text_only)]

    def names(self):
        return {name for name, _, _ in self.items}


def load_patterns(path):
    try:
        data = json.loads(Path(path).read_text(encoding="utf-8"))
        patterns = Patterns(data["muster"], data.get("sha256", []))
    except (OSError, ValueError, KeyError, TypeError, re.error) as error:
        raise Failed(f"Die Musterdatei {path} fehlt oder ist unlesbar ({error}). Ohne sie prüft der Wächter nichts.")
    if not patterns.items:
        raise Failed(f"Die Musterdatei {path} enthält kein Muster.")
    return patterns


def git(repo, *args):
    done = subprocess.run(["git", "-C", str(repo), *args], capture_output=True, check=False)
    if done.returncode != 0:
        raise Failed(f"git {' '.join(args)}: {done.stderr.decode('utf-8', 'replace').strip()}")
    return done.stdout


class Findings:
    def __init__(self):
        self.by_trace = {}
        # Files that could not be read at all: the result is not "clean" then.
        self.skipped = []
        # Files too big for the search of their text, looked at by hash only.
        self.big = []

    def add(self, name, where):
        self.by_trace.setdefault(name, []).append(where)

    def names(self):
        return set(self.by_trace)

    def count(self):
        return sum(len(places) for places in self.by_trace.values())

    def report(self):
        for name in sorted(self.by_trace):
            places = self.by_trace[name]
            print(f"  {name}: {len(places)} Stelle(n)")
            for place in places[:4]:
                print(f"      {place}")
            if len(places) > 4:
                print(f"      … und {len(places) - 4} weitere")


def views(data):
    """The readings of some bytes a trace may hide in: plain text, text without zero bytes
    (UTF-16 and similar), the text with %XX written out, and the normalised text."""
    plain = data.decode("utf-8", "replace")
    out = [("", plain)]
    if b"\0" in data:
        if data[:2] in (b"\xff\xfe", b"\xfe\xff"):
            out.append(("UTF-16", data.decode("utf-16", "replace")))
        out.append(("ohne Nullbytes", data.replace(b"\0", b"").decode("utf-8", "replace")))
    unquoted = urllib.parse.unquote(plain) if "%" in plain else plain
    normal = unicodedata.normalize("NFKC", unquoted)
    if normal != plain:
        out.append(("umgeschrieben", normal))
    return out


def scan_bytes(patterns, findings, label, data):
    if hashlib.sha256(data).hexdigest() in patterns.hashes:
        findings.add("Bekannte Datei (SHA-256)", label)
    binary = b"\0" in data and data[:2] not in (b"\xff\xfe", b"\xfe\xff")
    for tag, text in views(data):
        hit = patterns.whole(text, binary)
        if not hit:
            continue
        for number, line in enumerate(text.split("\n"), 1):
            for name, rx in hit:
                if rx.search(line):
                    findings.add(name, f"{label}:{number}" + (f" ({tag})" if tag else ""))


def scan_tree(repo, patterns, findings):
    """File names and file contents of everything tracked or not yet ignored."""
    listing = git(repo, "ls-files", "-z", "--cached", "--others", "--exclude-standard")
    for raw in listing.split(b"\0"):
        if not raw:
            continue
        rel = raw.decode("utf-8", "replace")
        for name in patterns.traces_in(rel):
            findings.add(name, f"Dateiname: {rel}")
        path = Path(repo) / rel
        try:
            if path.is_symlink() or not path.is_file():
                continue
            if path.stat().st_size > MAX_BYTES:
                data = path.read_bytes()
                if hashlib.sha256(data).hexdigest() in patterns.hashes:
                    findings.add("Bekannte Datei (SHA-256)", rel)
                findings.big.append(rel)
                continue
            data = path.read_bytes()
        except OSError as error:
            findings.skipped.append(f"{rel} ({error})")
            continue
        scan_bytes(patterns, findings, rel, data)


def scan_history(repo, patterns, refs, findings):
    """Messages, authors, committers, file names and changed lines of every commit reachable from `refs` (each once)."""
    command = [
        "git", "-C", str(repo), "log", *refs, "--no-color", "--no-renames", "--no-ext-diff", "--cc", "-U0", "-p",
        f"--format={COMMIT_MARK}%H%n%an <%ae>%n%cn <%ce>%n%s%n%b",
    ]
    process = subprocess.Popen(command, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    assert process.stdout is not None and process.stderr is not None
    commit = "?"
    state = "none"
    header_line = 0
    for raw in process.stdout:
        line = raw.decode("utf-8", "replace").rstrip("\r\n")
        if line.startswith(COMMIT_MARK):
            commit = line[len(COMMIT_MARK):][:10]
            state, header_line = "message", 0
            continue
        if state == "message":
            if line.startswith("diff --git ") or line.startswith("diff --cc "):
                state = "file-head"
            else:
                header_line += 1
                for name in patterns.traces_in(line):
                    findings.add(name, f"Commit {commit}: {'Autor/Committer' if header_line <= 2 else 'Text'}")
                continue
        if line.startswith("diff --git ") or line.startswith("diff --cc "):
            state = "file-head"
            for name in patterns.traces_in(line):
                findings.add(name, f"Commit {commit}: {line[11:]}")
        elif state == "file-head":
            # The lines of a file's head (index, mode, ---, +++) end at the first hunk.
            if line.startswith("@@"):
                state = "hunk"
        elif state == "hunk" and line[:1] in ("+", "-", " "):
            for name in patterns.traces_in(line[1:]):
                findings.add(name, f"Commit {commit}: {line[1:80]}")
    error = process.stderr.read().decode("utf-8", "replace").strip()
    process.wait()
    if process.returncode != 0:
        raise Failed(f"git log: {error}")


def scan_objects(repo, patterns, refs, findings):
    """Every file of every commit reachable from `refs`, whole: by name, by hash and by content."""
    listing = git(repo, "rev-list", "--objects", *refs).decode("utf-8", "replace")
    wanted = {}
    for line in listing.splitlines():
        sha, _, path = line.partition(" ")
        if path:
            wanted.setdefault(sha, path)
    process = subprocess.Popen(
        ["git", "-C", str(repo), "cat-file", "--batch"], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE
    )
    assert process.stdin is not None and process.stdout is not None
    # The names go in from a thread of their own while the answers are read here, in the same order.
    stdin = process.stdin

    def feed():
        try:
            for sha in wanted:
                stdin.write(sha.encode() + b"\n")
            stdin.close()
        except OSError:
            pass

    feeder = threading.Thread(target=feed, daemon=True)
    feeder.start()
    for sha, path in wanted.items():
        head = process.stdout.readline().decode("utf-8", "replace").split()
        if len(head) != 3:
            findings.skipped.append(f"{path} (Objekt {sha[:10]} nicht lesbar)")
            continue
        kind, size = head[1], int(head[2])
        data = process.stdout.read(size)
        process.stdout.read(1)
        if kind != "blob":
            continue
        if size > MAX_BYTES:
            if hashlib.sha256(data).hexdigest() in patterns.hashes:
                findings.add("Bekannte Datei (SHA-256)", f"Verlauf {sha[:10]} {path}")
            findings.big.append(f"{path} (Verlauf)")
            continue
        scan_bytes(patterns, findings, f"Verlauf {sha[:10]} {path}", data)
    feeder.join()
    process.wait()


def scan_tags(repo, patterns, findings):
    out = git(
        repo, "for-each-ref", "refs/tags",
        "--format=%(refname)%0a%(taggername) %(taggeremail)%0a%(contents)%0a@@END@@",
    )
    for block in out.decode("utf-8", "replace").split("@@END@@"):
        lines = block.strip().splitlines()
        for name in patterns.traces_in(block):
            findings.add(name, f"Tag: {lines[0]}" if lines else "Tag")


def tag_names(repo):
    return [t for t in git(repo, "tag", "--list").decode("utf-8", "replace").split() if t]


def search(repo, patterns, refs, tree_only, flat=False):
    findings = Findings()
    scan_tree(repo, patterns, findings)
    if not tree_only:
        # What a push sends: the named history and every tag with its history.
        every = list(dict.fromkeys([*refs, *tag_names(repo)]))
        scan_history(repo, patterns, every, findings)
        if not flat:
            scan_objects(repo, patterns, every, findings)
        scan_tags(repo, patterns, findings)
    return findings


# --- Self-test -------------------------------------------------------------------

DUMMY = Patterns(
    [
        {"regex": r"zzq[-_ ]?alpha", "probe": "ZZQ-Alpha"},
        {"regex": r"zzq\.beta\d+", "probe": "zzq.beta7"},
        {"regex": r"gam+a-zzq", "probe": "gamma-zzq"},
    ],
    [hashlib.sha256(b"a picture nobody may see").hexdigest()],
)
PICTURE = b"a picture nobody may see"


def make_repo(folder):
    folder.mkdir(parents=True, exist_ok=True)
    git(folder, "init", "-q", "-b", "main")
    git(folder, "config", "user.email", "test@example.invalid")
    git(folder, "config", "user.name", "Test")
    git(folder, "config", "commit.gpgsign", "false")
    return folder


def commit_all(repo, message="x", author=None):
    git(repo, "add", "-A")
    extra = ["--author", author] if author else []
    git(repo, "commit", "-q", "--allow-empty", "-m", message, *extra)


def safe_file_name(sample):
    return re.sub(r"[^A-Za-z0-9._ -]", "_", sample)


def self_test_with(patterns, label, problems):
    every = patterns.names()
    samples = [(name, sample) for name, _, sample in patterns.items]

    def expect(where, repo, wanted, refs=("HEAD",), tree_only=False, flat=False):
        found = search(repo, patterns, refs, tree_only, flat).names()
        for name in sorted(wanted - found):
            problems.append(f"{label}/{where}: nicht gefunden: {name}")
        for name in sorted(found - wanted):
            problems.append(f"{label}/{where}: zu viel gefunden: {name}")

    for name, rx, sample in patterns.items:
        if not rx.search(sample):
            problems.append(f"{label}: {name} passt nicht auf sein Beispiel")

    with tempfile.TemporaryDirectory(prefix="spuren-selbsttest-") as tmp:
        base = Path(tmp)

        clean = make_repo(base / "sauber")
        (clean / "a.txt").write_text(
            "Entwickler dürfen das. version 5.0.0-dev. feature = \"gui\". Spieldaten, Log, Update, Test.\n"
            "192 168 is not an address. git@github.com is not a private server.\n"  # privacy-ok: help text
            "++x and -- comment lines are no trouble.\n",
            encoding="utf-8",
        )
        commit_all(clean, "Add harmless text")
        expect("sauber", clean, set())

        text_repo = make_repo(base / "text")
        for index, (_, sample) in enumerate(samples):
            (text_repo / f"t{index}.txt").write_text(f"first line\nthis holds {sample} here\n", encoding="utf-8")
            (text_repo / f"u{index}.txt").write_text(f"x {sample}\n", encoding="utf-16")
            escaped = "".join(f"%{byte:02X}" for byte in sample.encode("utf-8"))
            (text_repo / f"p{index}.txt").write_text(f"x {escaped} y", encoding="utf-8")
        commit_all(text_repo, "Add files")
        expect("Text, UTF-16, %XX", text_repo, every, tree_only=True)

        binary_repo = make_repo(base / "binaer")
        for index, (_, sample) in enumerate(samples):
            (binary_repo / f"b{index}.bin").write_bytes(b"\x00\x01" + sample.encode("utf-8") + b"\x00\xff")
        commit_all(binary_repo, "Add binaries")
        expect("Binärdatei", binary_repo, every - patterns.text_only, tree_only=True)

        name_repo = make_repo(base / "dateiname")
        named = set()
        for name, rx, sample in patterns.items:
            candidate = safe_file_name(sample)
            if rx.search(candidate):
                (name_repo / f"{candidate}.dat").write_text("nothing\n", encoding="utf-8")
                named.add(name)
        commit_all(name_repo, "Add named files")
        expect("Dateiname", name_repo, named, tree_only=True)

        # In the history only: added, then removed again, text and binary. The tree is clean.
        history_repo = make_repo(base / "verlauf")
        (history_repo / "keep.txt").write_text("keep\n", encoding="utf-8")
        commit_all(history_repo, "Add keep")
        for index, (_, sample) in enumerate(samples):
            (history_repo / f"h{index}.txt").write_text(f"{sample}\n", encoding="utf-8")
            (history_repo / f"h{index}.bin").write_bytes(b"\x00\x01" + sample.encode("utf-8") + b"\x00\xff")
        commit_all(history_repo, "Add things")
        for index in range(len(samples)):
            (history_repo / f"h{index}.txt").unlink()
            (history_repo / f"h{index}.bin").unlink()
        commit_all(history_repo, "Remove things")
        expect("Verlauf: Baum", history_repo, set(), tree_only=True)
        expect("Verlauf: Verlauf", history_repo, every)

        # Only the objects of the history show a binary file that was added and removed again.
        only_binary = make_repo(base / "nur-binaer")
        (only_binary / "keep.txt").write_text("keep\n", encoding="utf-8")
        commit_all(only_binary, "Add keep")
        (only_binary / "blob.bin").write_bytes(b"\x00\x01" + samples[0][1].encode("utf-8") + b"\x00\xff")
        commit_all(only_binary, "Add blob")
        (only_binary / "blob.bin").unlink()
        commit_all(only_binary, "Remove blob")
        expect("Verlauf: Binärdatei nur über die Objekte", only_binary, {samples[0][0]})
        found_flat = search(only_binary, patterns, ("HEAD",), False, flat=True).names()
        if samples[0][0] in found_flat:
            problems.append(f"{label}/flach: die Binärdatei im Verlauf wurde ohne Objektsuche gefunden (die Probe prüft nichts)")

        # A picture under an innocent name is found by its hash, and after it is gone, too.
        if patterns is DUMMY:
            picture_repo = make_repo(base / "bild")
            (picture_repo / "harmlos.dat").write_bytes(PICTURE)
            commit_all(picture_repo, "Add a file")
            found = search(picture_repo, patterns, ("HEAD",), True).names()
            if found != {"Bekannte Datei (SHA-256)"}:
                problems.append(f"{label}/Bild im Baum: {sorted(found)}")
            (picture_repo / "harmlos.dat").unlink()
            commit_all(picture_repo, "Remove it")
            if search(picture_repo, patterns, ("HEAD",), True).names():
                problems.append(f"{label}/Bild: nach dem Entfernen im Baum gefunden")
            if search(picture_repo, patterns, ("HEAD",), False).names() != {"Bekannte Datei (SHA-256)"}:
                problems.append(f"{label}/Bild: im Verlauf nicht über den Hash gefunden")

        message_repo = make_repo(base / "nachricht")
        for index, (_, sample) in enumerate(samples):
            (message_repo / "f.txt").write_text(str(index), encoding="utf-8")
            commit_all(message_repo, f"Change {sample}")
        expect("Nachricht", message_repo, every)

        author_repo = make_repo(base / "autor")
        for index, (_, sample) in enumerate(samples):
            (author_repo / "f.txt").write_text(str(index), encoding="utf-8")
            commit_all(author_repo, "Change", author=f"{safe_file_name(sample)} <a@example.invalid>")
        expect("Autor", author_repo, {name for name, rx, sample in patterns.items if rx.search(safe_file_name(sample))})

        # Lines that start like the head of a file in a diff are content, not head.
        dash_repo = make_repo(base / "striche")
        (dash_repo / "f.txt").write_text("a\n", encoding="utf-8")
        commit_all(dash_repo, "Add f")
        (dash_repo / "f.txt").write_text(f"-- {samples[0][1]}\n", encoding="utf-8")
        commit_all(dash_repo, "Change f")
        (dash_repo / "f.txt").write_text(f"a\n++{samples[1 % len(samples)][1]}\n", encoding="utf-8")
        commit_all(dash_repo, "Change f again")
        git(dash_repo, "mv", "f.txt", "g.txt")
        commit_all(dash_repo, "Rename")
        expect("Zeilen mit -- und ++", dash_repo, {samples[0][0], samples[1 % len(samples)][0]})

        tag_repo = make_repo(base / "tag")
        (tag_repo / "f.txt").write_text("f\n", encoding="utf-8")
        commit_all(tag_repo, "Add f")
        for index, (_, sample) in enumerate(samples):
            git(tag_repo, "tag", "-a", f"v{index}", "-m", f"release {sample}")
        expect("Tag", tag_repo, every)

        # The history behind a tag is searched even when the named history is clean.
        behind_repo = make_repo(base / "hinter-tag")
        (behind_repo / "f.txt").write_text("f\n", encoding="utf-8")
        commit_all(behind_repo, "Add f")
        git(behind_repo, "switch", "-q", "-c", "other")
        (behind_repo / "g.txt").write_text(samples[0][1] + "\n", encoding="utf-8")
        commit_all(behind_repo, "Add g")
        git(behind_repo, "tag", "-a", "v1", "-m", "release")
        git(behind_repo, "switch", "-q", "main")
        expect("Verlauf hinter einem Tag", behind_repo, {samples[0][0]})

        # Only the named history is searched: a side branch without a tag stays out.
        side_repo = make_repo(base / "seitenzweig")
        (side_repo / "f.txt").write_text("f\n", encoding="utf-8")
        commit_all(side_repo, "Add f")
        git(side_repo, "switch", "-q", "-c", "other")
        (side_repo / "g.txt").write_text(samples[0][1] + "\n", encoding="utf-8")
        commit_all(side_repo, "Add g")
        git(side_repo, "switch", "-q", "main")
        expect("Seitenzweig nicht geprüft", side_repo, set())
        expect("Seitenzweig geprüft", side_repo, {samples[0][0]}, refs=("other",))

        # A file the guard cannot read is reported, not passed over.
        if os.name != "nt":
            unreadable = make_repo(base / "unlesbar")
            (unreadable / "f.txt").write_text("f\n", encoding="utf-8")
            commit_all(unreadable, "Add f")
            os.chmod(unreadable / "f.txt", 0)
            if not search(unreadable, patterns, ("HEAD",), True).skipped:
                problems.append(f"{label}/unlesbar: die Datei wurde nicht gemeldet")
            os.chmod(unreadable / "f.txt", 0o644)


def self_test():
    problems = []
    self_test_with(DUMMY, "Probe", problems)
    real = pattern_file()
    if real.is_file():
        try:
            self_test_with(load_patterns(real), "Musterdatei", problems)
        except Failed as error:
            problems.append(str(error))
        note = "mit den Mustern der Musterdatei"
    else:
        note = "nur mit erfundenen Mustern (keine Musterdatei gefunden)"
    if problems:
        print("SELBSTTEST ROT:")
        for problem in problems:
            print("  " + problem)
        return 1
    print(f"SELBSTTEST GRÜN ({note}; alle Orte gefunden, sauberes Repository bleibt sauber)")
    return 0


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--ref", action="append", default=None, help="history to search (default HEAD); may repeat")
    parser.add_argument("--nur-baum", action="store_true", help="search the working tree only")
    parser.add_argument("--flach", action="store_true", help="skip the search through all files of the history")
    parser.add_argument("--repo", default=str(REPO), help="repository to search (default: this one)")
    parser.add_argument("--selbsttest", action="store_true", help="counter-check of the search itself")
    args = parser.parse_args()
    if args.selbsttest:
        return self_test()
    try:
        patterns = load_patterns(pattern_file())
        findings = search(Path(args.repo), patterns, args.ref or ["HEAD"], args.nur_baum, args.flach)
    except Failed as error:
        print(f"FEHLER: {error}")
        return 2
    if findings.count() > 0:
        print(f"ERGEBNIS Spuren gefunden ({findings.count()} Stellen, {len(findings.by_trace)} Muster):")
        findings.report()
        return 1
    if findings.skipped:
        print(f"FEHLER: {len(findings.skipped)} Datei(en) nicht vollständig geprüft:")
        for entry in findings.skipped[:10]:
            print("   " + entry)
        return 2
    where = "Baum" if args.nur_baum else "Baum, Verlauf (mit allen Dateien) und Tags"
    print(f"ERGEBNIS sauber ({where}, {len(patterns.items)} Muster)")
    if findings.big:
        print(f"Hinweis: {len(findings.big)} große Datei(en) nur über den Hash geprüft, nicht über den Text.")
    return 0


if __name__ == "__main__":
    os.environ.setdefault("GIT_OPTIONAL_LOCKS", "0")
    sys.exit(main())
