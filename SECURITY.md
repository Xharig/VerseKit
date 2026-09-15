# Sicherheit

**Deutsch** · [English](SECURITY.en.md)

## Eine Sicherheitslücke melden

Bitte melde Sicherheitsprobleme **nicht** als öffentliches Issue, sondern über
das [Formular für Sicherheitshinweise](https://github.com/Xharig/VerseKit/security/advisories/new)
von GitHub. Du bekommst innerhalb weniger Tage eine Antwort.

## Wie die Programmdateien entstehen

Jede veröffentlichte Datei wird von einem **öffentlichen GitHub-Actions-Ablauf**
gebaut ([`.github/workflows/release.yml`](.github/workflows/release.yml)),
ausgelöst von einem Git-Tag. Was auf einem Rechner von Hand gebaut wurde, wird
nie veröffentlicht. Damit lässt sich jede ausgelieferte Datei auf genau einen
Commit und genau einen Bau-Lauf zurückführen, und das Bau-Protokoll ist
öffentlich.

Ausgeliefert werden:

| Datei | System | Gebaut mit |
|---|---|---|
| `SC-BP-Watcher-Setup.exe` | Windows | PyInstaller + Inno Setup |
| `SC-BP-Watcher-x86_64.AppImage` | Linux | PyInstaller + AppImage |

## Fremde Bestandteile

Es gibt keine. Das Programm benutzt ausschließlich die
Python-Standardbibliothek — keine Zusatzpakete, keine Netzbibliothek außer
`urllib`, nichts Proprietäres. Das ist eine bewusste Projektregel, kein Zufall.

## Was das Programm verschickt

Nichts über dich. Es liest die Protokolldatei des Spiels auf deinem Rechner und
holt öffentliche Herstellungsdaten von `scmdb.net` sowie Versionsangaben von der
GitHub-Schnittstelle. Der eingebaute Fehlerbericht wird in eine Datei
geschrieben, die **du selbst** einfügst — er wird nie von allein verschickt, und
Benutzernamen und Pfade werden ersetzt, bevor er überhaupt angezeigt wird.

## Fehlalarme von Virenscannern

Mit PyInstaller gebaute Programme werden regelmäßig von lernenden Erkennungen
gemeldet, etwa als `Trojan:Win32/Wacatac.C!ml`. Das sind Fehlalarme; der
vollständige Quellcode liegt in diesem Repository, und der Bau ist öffentlich.
Wenn dir so etwas begegnet, melde es gern deinem Hersteller — und mach ruhig
ein Issue auf, damit andere die Information finden.
