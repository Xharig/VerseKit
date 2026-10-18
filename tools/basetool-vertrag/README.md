# Basetool-Vertrag (Exchange API v1)

Die Datenformate, über die VerseKit einmal mit dem **KRT Profit Basetool**
Baupläne, Lager und Schiffe abgleicht. Übernommen aus dem Basetool-Repo, damit
der Selbsttest ohne Netz prüfen kann, ob VerseKit genau dieses Format schreibt.

| | |
|---|---|
| Quelle | [github.com/krt-profit/basetool](https://github.com/krt-profit/basetool) |
| Stand | Commit `f799e1e4ffe3f4e0ad8f055cca51f2534c1eac8a` (28.09.2026, Go-live-Stand mit Basetool v1.13.0) |
| Lager nachgezogen | Commit `c519ebbbb62c0d18967ba3c2110b0b14ce57d0da` (17.10.2026, Basetool v1.17.0): Schemas `change-set`, `stock-lot`, `quality`, `material-kind` und die Beispiele `change-set--stockChangeSet`, `stock-lot` — `split` am Posten, Ops `share`/`unshare`. `fehlercodes.md` ist noch der alte Stand; die Gründe der beiden Ops stehen in `docs/exchange/resources/stock.md` |
| Lager der Einheit nachgezogen | Commit `df7aa203ac0544ce924198a900cb4a82d56f3b41` (18.10.2026, krt-profit/basetool#2754): Schemas `org-unit` (neu: `logistician`), `org-unit-list`, `org-stock-lot`, `org-stock-members`, `org-stock-targets`, `component`, `page` (`orgStockPage`) und die Beispiele `page--orgStockPage`, `org-unit`, `org-unit-list`, `org-stock-lot`, `org-stock-members`, `org-stock-targets`, `change-set--orgStockChangeSet`. Die übrigen Dateien bleiben auf dem Stand darüber |
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

Geprüft wird in `tools/selbsttest.py` (Prüfung 267, Lager der Einheit 561) mit `tools/schema_pruefen.py`.
