# Basetool-Vertrag (Exchange API v1)

Die Datenformate, über die VerseKit einmal mit dem **KRT Profit Basetool**
Baupläne, Lager und Schiffe abgleicht. Übernommen aus dem Basetool-Repo, damit
der Selbsttest ohne Netz prüfen kann, ob VerseKit genau dieses Format schreibt.

| | |
|---|---|
| Quelle | [github.com/krt-profit/basetool](https://github.com/krt-profit/basetool) |
| Stand | Commit `57184c01bbc1482a8a0a798ba4511799bfceb8b5` (23.11.2026): alle Schemas, alle Beispiele und `fehlercodes.md` — samt Bank, Raffinerie-Aufträgen und der Auswahl der Auftragsziele (`store-choice-job-orders`) |
| Lizenz | GPL-3.0 — wie VerseKit |
| Urheber | greluc (KRT) und die Mitwirkenden am Basetool |

| Ordner | Herkunft im Basetool |
|---|---|
| `v1/schemas/` | `ingest/src/main/resources/exchange/v1/schemas/` |
| `v1/beispiele/` | `docs/exchange/examples/v1/` (je Format `valid/` und `invalid/`) |
| `v1/fehlercodes.md` | `docs/exchange/errors.md` |

Ein Ordner `<format>--<teil>` in `beispiele/` meint den Teil
`<format>.schema.json#/$defs/<teil>`.

**Nicht von Hand ändern.** Ändert sich der Vertrag drüben, werden die Dateien
neu übernommen und der Commit oben nachgetragen. Innerhalb von v1 darf das
Basetool laut eigener Regel nur ergänzen, nie etwas wegnehmen.

Die Python-Fassung prüfte gegen diese Dateien (`tools/selbsttest.py`, `tools/schema_pruefen.py`). In der Rust-Fassung gibt es diese Prüfung noch nicht.
