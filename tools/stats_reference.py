#!/usr/bin/env python3
"""The statistics numbers of 4.12 on a copy of a data folder.

Reference for `crates/versekit-core/tests/stats_parity.rs`: the Rust sums must
equal these. Never point it at live data; it works on a copy of the files it
needs in a throw-away folder.

    python3 -I tools/stats_reference.py <repo> <out.json> <flat folder with a copy of
        statistik.json spielzeit.json auftragslog.json namen-original.json orte-original.json>
"""
import json
import os
import shutil
import sys
import tempfile

FILES = ("statistik.json", "spielzeit.json", "auftragslog.json", "namen-original.json", "orte-original.json")
OWN = "Xharig"


def main(repo, target, source):
    home = tempfile.mkdtemp(prefix="stats-reference-")
    try:
        for name in FILES:
            shutil.copy(os.path.join(source, name), os.path.join(home, name))
        os.environ["SC_BP_HOME"] = home
        sys.path.insert(0, repo)
        from scbp import play_stats, playtime

        # The session running now would read the real installation of this computer.
        playtime._running_span = lambda: None

        out = {
            "summary": play_stats.summary(OWN),
            "ships": play_stats.ships(OWN),
            "missions": play_stats.missions(OWN),
            "quantum": play_stats.quantum(OWN),
            "stability": play_stats.stability(OWN),
            "state": play_stats.state(),
        }
        names = play_stats.place_names()
        out["places"] = {key: list(play_stats.place_parts(key, names)) for key, _ in out["quantum"]["ziele"]}
        out["display"] = {name: play_stats.display_name(name, vehicle=True) for name, _ in out["ships"]["genutzt"]}
        play_stats.export(os.path.join(home, "export.json"), OWN, "x")
        with open(os.path.join(home, "export.json"), encoding="utf-8") as f:
            out["export"] = json.load(f)
        with open(target, "w", encoding="utf-8") as f:
            json.dump(out, f, ensure_ascii=False, indent=1, default=list)
    finally:
        shutil.rmtree(home, ignore_errors=True)


if __name__ == "__main__":
    main(*sys.argv[1:4])
