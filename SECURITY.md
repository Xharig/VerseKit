# Sicherheit

**Deutsch** · [English](SECURITY.en.md)

## Eine Sicherheitslücke melden

Bitte melde Sicherheitsprobleme **nicht** als öffentliches Issue, sondern über
das [Formular für Sicherheitshinweise](https://github.com/Xharig/VerseKit/security/advisories/new)
von GitHub. Du bekommst innerhalb weniger Tage eine Antwort.

## Zugangsdaten zum KRT Profit Basetool

Verbindest du VerseKit mit dem KRT Profit Basetool, hält es ein Zugangs-Token
und einen eigenen Schlüssel. Beides liegt **nur** im Schlüsselspeicher deines
Systems: unter Windows mit DPAPI verschlüsselt und als nicht exportierbarer
Schlüssel (im TPM, wo es eins gibt), unter Linux im Secret Service. Nur wo es
unter Linux keinen Secret Service gibt oder `secret-tool` fehlt bzw. nicht
antwortet, liegen sie in einer Datei, die allein du lesen kannst, und VerseKit
zeigt dir das an. Lehnt der Secret Service ab, weicht VerseKit nicht still auf
die Datei aus. Sie stehen nie in deinen VerseKit-Daten, in einer Sicherung, im
Fehlerprotokoll oder in einem Fehlerbericht; was wie ein Token aussieht, wird
dort geschwärzt.

Eine gemeldete Lücke im Umgang mit diesen Zugangsdaten wird **innerhalb von
sieben Tagen** mit einer veröffentlichten Fassung behoben.

## Wie die Programmdateien entstehen

Jede veröffentlichte Datei wird von einem **öffentlichen GitHub-Actions-Ablauf**
gebaut ([`.github/workflows/release.yml`](.github/workflows/release.yml)),
ausgelöst von einem Git-Tag. Was auf einem Rechner von Hand gebaut wurde, wird
nie veröffentlicht. Damit lässt sich jede ausgelieferte Datei auf genau einen
Commit und genau einen Bau-Lauf zurückführen, und das Bau-Protokoll ist
öffentlich. Jedes Release trägt eine `SHA256SUMS.txt`; VerseKit nimmt sie nur
von GitHub aus diesem Repository an und spielt kein Update ein, dessen
Prüfsumme nicht stimmt: Die Datei wird dann gelöscht und nichts installiert.
Kurz vor dem Einspielen prüft VerseKit die Summe noch einmal.

Ausgeliefert werden:

| Datei | System | Gebaut mit |
|---|---|---|
| `VerseKit-Setup.exe` | Windows | Rust (`cargo build --release`) + Inno Setup |
| `VerseKit-x86_64.AppImage` | Linux | Rust (`cargo build --release`) + AppImage |

### Herkunftsnachweis selbst prüfen

Zu den beiden Dateien und zur `SHA256SUMS.txt` erstellt der Bau-Lauf einen
**Herkunftsnachweis** (GitHub Artifact Attestation, signiert über Sigstore und
im öffentlichen Transparenzlog eingetragen). Er belegt, dass genau diese Datei
von `.github/workflows/release.yml` in diesem Repository für den Tag des
Releases gebaut wurde. Das gilt für Releases, die durch das Hochladen eines
Tags entstehen, den üblichen Weg. Startet ein anderer Ablauf den Bau
(`geplant.yml`), nennt der Nachweis dessen Zweig statt des Tags, und VerseKit
erkennt ihn nicht an. GitHub hebt ihn getrennt von den Release-Dateien auf:
Wer nur die Anhänge eines Releases austauscht, kann ihn nicht mitfälschen.

Mit der [GitHub-Kommandozeile](https://cli.github.com) prüfst du eine
heruntergeladene Datei so:

```bash
gh attestation verify VerseKit-Setup.exe --repo Xharig/VerseKit
```

VerseKit fragt den Nachweis nach jedem Update-Download selbst bei GitHub ab
(dafür geht die Prüfsumme der Datei an api.github.com, das Nachweis-Paket
kommt aus GitHubs Speicher bei Microsoft Azure) und prüft Signatur,
Zertifikat, den Eintrag im Transparenzlog, Repository, Workflow und Tag. Das
Ergebnis steht im Update-Protokoll `update-helfer.txt`; der Fehlerbericht
zeigt unter „Letzter Update-Versuch“ dessen letzte acht Zeilen. Ist ein
Nachweis da, hält aber der Prüfung nicht stand, kommt das zusätzlich ins
Fehlerprotokoll.

Noch hält der Nachweis kein Update auf, weder ein fehlender noch ein falscher
oder einer, den GitHub gerade nicht liefert, denn ältere Releases haben keinen.
Die Prüfsumme dagegen muss immer stimmen.

## Fremde Bestandteile

Das Programm ist in Rust geschrieben. Jede Abhängigkeit steht mit fester Version
und Prüfsumme in `Cargo.lock`; der Bau läuft mit `--locked`, und `deny.toml`
legt fest, welche Lizenzen und Quellen erlaubt sind.

Mitgeliefert werden drei fremde Bestandteile, jeweils mit Lizenz:

| Was | Lizenz | Wo |
|---|---|---|
| Symbole aus [Lucide](https://lucide.dev) | ISC | `tools/symbol-vorlagen/` |
| Flaggen aus [flag-icons](https://flagicons.lipis.dev) | MIT | `tools/flaggen-vorlagen/` |
| DPoP-Referenz des [KRT Profit Basetools](https://krt-profit.github.io/basetool/) | MIT | `crates/versekit-core/licenses/` |

## Was das Programm verschickt

Siehe die [Datenschutzerklärung](PRIVACY.md).

## Fehlalarme von Virenscannern

Selbst gebaute, nicht signierte Programme werden gelegentlich von lernenden
Erkennungen gemeldet, etwa als `Trojan:Win32/Wacatac.C!ml`. Das sind Fehlalarme; der
vollständige Quellcode liegt in diesem Repository, und der Bau ist öffentlich.
Wenn dir so etwas begegnet, melde es gern deinem Hersteller — und mach ruhig
ein Issue auf, damit andere die Information finden.
