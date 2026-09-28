# Sicherheit

**Deutsch** · [English](SECURITY.en.md)

## Eine Sicherheitslücke melden

Bitte melde Sicherheitsprobleme **nicht** als öffentliches Issue, sondern über
das [Formular für Sicherheitshinweise](https://github.com/Xharig/VerseKit/security/advisories/new)
von GitHub. Du bekommst innerhalb weniger Tage eine Antwort.

## Zugangsdaten zum KRT Profit Basetool

Verbindest du Verse-Kit mit dem KRT Profit Basetool, hält es ein Zugangs-Token
und einen eigenen Schlüssel. Beides liegt **nur** im Schlüsselspeicher deines
Systems: unter Windows mit DPAPI verschlüsselt und als nicht exportierbarer
Schlüssel (im TPM, wo es eins gibt), unter Linux im Secret Service. Nur wo es
unter Linux keinen Secret Service gibt, liegen sie in einer Datei, die allein
du lesen kannst — und Verse-Kit zeigt dir das an. Sie stehen nie in deinen
Verse-Kit-Daten, in einer Sicherung, im Fehlerprotokoll oder in einem
Fehlerbericht; was wie ein Token aussieht, wird dort geschwärzt.

Eine gemeldete Lücke im Umgang mit diesen Zugangsdaten wird **innerhalb von
sieben Tagen** mit einer veröffentlichten Fassung behoben.

## Wie die Programmdateien entstehen

Jede veröffentlichte Datei wird von einem **öffentlichen GitHub-Actions-Ablauf**
gebaut ([`.github/workflows/release.yml`](.github/workflows/release.yml)),
ausgelöst von einem Git-Tag. Was auf einem Rechner von Hand gebaut wurde, wird
nie veröffentlicht. Damit lässt sich jede ausgelieferte Datei auf genau einen
Commit und genau einen Bau-Lauf zurückführen, und das Bau-Protokoll ist
öffentlich. Jedes Release trägt eine `SHA256SUMS.txt`; Verse-Kit spielt kein
Update ein, dessen Prüfsumme nicht stimmt.

Ausgeliefert werden:

| Datei | System | Gebaut mit |
|---|---|---|
| `VerseKit-Setup.exe` | Windows | PyInstaller + Inno Setup |
| `VerseKit-x86_64.AppImage` | Linux | PyInstaller + AppImage |

## Fremde Bestandteile

Das Programm benutzt ausschließlich die Python-Standardbibliothek — keine
Zusatzpakete, keine Netzbibliothek außer `urllib`, nichts Proprietäres. Das ist
eine bewusste Projektregel, kein Zufall.

Mitgeliefert werden nur drei fremde Bestandteile, jeweils mit Lizenz:

| Was | Lizenz | Wo |
|---|---|---|
| Symbole aus [Lucide](https://lucide.dev) | ISC | `tools/symbol-vorlagen/` |
| Flaggen aus [flag-icons](https://flagicons.lipis.dev) | MIT | `tools/flaggen-vorlagen/` |
| DPoP-Referenz des [KRT Profit Basetools](https://krt-profit.github.io/basetool/) | MIT | `scbp/dpop_reference/` |

## Was das Programm verschickt

Siehe die [Datenschutzerklärung](PRIVACY.md).

## Fehlalarme von Virenscannern

Mit PyInstaller gebaute Programme werden regelmäßig von lernenden Erkennungen
gemeldet, etwa als `Trojan:Win32/Wacatac.C!ml`. Das sind Fehlalarme; der
vollständige Quellcode liegt in diesem Repository, und der Bau ist öffentlich.
Wenn dir so etwas begegnet, melde es gern deinem Hersteller — und mach ruhig
ein Issue auf, damit andere die Information finden.
