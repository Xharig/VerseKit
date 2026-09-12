# -*- coding: utf-8 -*-
#
# SC BP Watcher — zeigt live neue Star-Citizen-Baupläne an.
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
Die Originaltexte des Spiels aus `Data.p4k` holen.

Bis v2.0.0-rc9 gab es diesen Weg nur als Entwicklerwerkzeug
(`tools/extract_global_ini.py`). Im Programm stand „Englisch — Originaltexte aus
dem Spiel" zwar zur Auswahl, tat aber nichts: Es **prüfte nur**, ob schon eine
englische `global.ini` dalag. War keine da — der Normalfall —, kam eine
Fehlermeldung. Damit war die Bauplan-Auszeichnung faktisch an eine der beiden
Fremdquellen gebunden.

Das ist unnötig, denn die Datei liegt auf jedem Rechner mit Star Citizen: im
`Data.p4k`. Es sind **144 GB**, aber nur eine von 1.364.115 Dateien wird
gebraucht — gelesen werden das Inhaltsverzeichnis und genau ein Block.
**0,2 Sekunden.**

Warum eigener Code und kein Fremdwerkzeug:

* `zipfile` bricht ab (`BadZipFile: Corrupt extra field c072`) — CIG legt eigene
  Extra-Felder ab. Das Inhaltsverzeichnis selbst ist stinknormales ZIP64 und
  lässt sich von Hand lesen.
* **7-Zip kann es nicht.** Es listet das Archiv zwar, scheitert aber beim
  Entpacken mit `Headers Error`: CIG komprimiert mit **Verfahren 100 = zstd**.
* `compression.zstd` ist **ab Python 3.14 Standardbibliothek** — damit bleibt die
  Projektregel „keine Zusatzpakete" unangetastet. Auf älterem Python wird
  `zstandard`/`pyzstd` versucht, sonst 7-Zip als letzter Strohhalm.
"""
import io
import os
import struct
import subprocess
import sys
import tempfile

from . import pfade
from .sprache import t

# 7-Zip nur als letzter Strohhalm für altes Python. Es kann CIGs zstd meist
# **nicht** — auf dem Testrechner scheiterte es mit „Headers Error". Der Eintrag
# bleibt für den Fall, dass jemand eine Version mit zstd-Unterstützung hat.
SIEBENZIP = [
    os.environ.get('SEVENZIP', ''),
    '/usr/bin/7z', '/usr/bin/7za', '/usr/local/bin/7z',
    r'C:\Program Files\7-Zip\7z.exe',
    r'C:\Program Files (x86)\7-Zip\7z.exe',
]

# Die Sprachordner im Archiv heißen wie die Ordner im Dateisystem.
def archivpfad(sprache='english'):
    return 'Data/Localization/%s/global.ini' % sprache


def p4k_pfad(spielordner=None):
    """Die Data.p4k der Installation — oder None."""
    wurzel = spielordner or pfade.spiel_ordner()
    if not wurzel:
        return None
    p = os.path.join(wurzel, 'Data.p4k')
    return p if os.path.isfile(p) else None


def lies_verzeichnis(f, groesse):
    """Gibt den rohen Block des zentralen Inhaltsverzeichnisses zurück."""
    f.seek(max(0, groesse - 66000))
    ende = f.read()
    pos = ende.rfind(b'PK\x06\x07')                      # ZIP64-Locator
    if pos < 0:
        raise RuntimeError('Kein ZIP64-Locator gefunden — unerwartetes Archivformat.')
    _, _, z64_off, _ = struct.unpack('<IIQI', ende[pos:pos + 20])
    f.seek(z64_off)
    kopf = f.read(56)
    if kopf[:4] != b'PK\x06\x06':
        raise RuntimeError('EOCD64-Signatur fehlt.')
    *_, anzahl, _, cd_size, cd_off = struct.unpack('<IQHHIIQQQQ', kopf)
    f.seek(cd_off)
    return f.read(cd_size), anzahl


def suche(cd, zielname):
    """Findet den Eintrag und löst die ZIP64-Platzhalter im Extra-Feld auf.
    Gibt (methode, komprimierte_groesse, rohgroesse, lokaler_offset) zurück."""
    ziel = zielname.lower().replace('/', '\\')
    i = 0
    while i < len(cd) - 46:
        if cd[i:i + 4] != b'PK\x01\x02':
            i += 1
            continue
        (_, _, _, _, methode, _, _, _, cs, rs,
         n_len, e_len, k_len, _, _, _, off) = struct.unpack('<IHHHHHHIIIHHHHHII', cd[i:i + 46])
        name = cd[i + 46:i + 46 + n_len].decode('utf-8', 'replace')
        if name.lower().replace('/', '\\') == ziel:
            extra = cd[i + 46 + n_len:i + 46 + n_len + e_len]
            # ZIP64-Feld (0x0001): die überlaufenen Werte stehen dort der Reihe nach
            j = 0
            while j < len(extra) - 4:
                hid, hlen = struct.unpack('<HH', extra[j:j + 4])
                if hid == 0x0001:
                    daten, k = extra[j + 4:j + 4 + hlen], 0
                    for feld in ('rs', 'cs', 'off'):
                        grenze = {'rs': rs, 'cs': cs, 'off': off}[feld]
                        if grenze == 0xFFFFFFFF and k + 8 <= len(daten):
                            wert = struct.unpack('<Q', daten[k:k + 8])[0]
                            k += 8
                            if feld == 'rs':   rs = wert
                            elif feld == 'cs': cs = wert
                            else:              off = wert
                    break
                j += 4 + hlen
            return methode, cs, rs, off
        i += 46 + n_len + e_len + k_len
    return None


def hole_block(f, lokal_off, cs):
    """Liest die komprimierten Bytes. Der lokale Kopf hat eigene Namens- und
    Extra-Längen — die echten Daten beginnen erst dahinter."""
    f.seek(lokal_off)
    kopf = f.read(30)
    if kopf[:4] != b'PK\x03\x04':
        raise RuntimeError('Lokaler Dateikopf fehlt bei Offset %d.' % lokal_off)
    n_len, e_len = struct.unpack('<HH', kopf[26:30])
    f.seek(lokal_off + 30 + n_len + e_len)
    return f.read(cs)


def entpacke_zstd(roh, erwartet):
    """Versucht der Reihe nach: Standardbibliothek, Fremdmodul, 7-Zip."""
    try:
        from compression import zstd as _z          # Python >= 3.14
        return _z.decompress(roh), 'compression.zstd'
    except Exception:
        pass
    for modul in ('zstandard', 'pyzstd'):
        try:
            m = __import__(modul)
            if modul == 'zstandard':
                return m.ZstdDecompressor().decompress(
                    roh, max_output_size=max(erwartet, 1) * 2), modul
            return m.decompress(roh), modul
        except Exception:
            continue
    for exe in SIEBENZIP:
        if exe and os.path.exists(exe):
            tmp = tempfile.mkdtemp(prefix='p4k-')
            quelle = os.path.join(tmp, 'block.zst')
            with io.open(quelle, 'wb') as g:
                g.write(roh)
            # 7-Zip legt die entpackte Datei ohne Endung daneben
            r = subprocess.run([exe, 'e', quelle, '-o' + tmp, '-y'],
                               stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
            ziel = os.path.join(tmp, 'block')
            if r.returncode == 0 and os.path.exists(ziel):
                with io.open(ziel, 'rb') as g:
                    return g.read(), '7-Zip (%s)' % os.path.basename(exe)
            raise RuntimeError('7-Zip scheiterte:\n%s' % r.stdout.decode('utf-8', 'replace')[:400])
    raise RuntimeError(
        'Kein zstd-Entpacker gefunden. Möglichkeiten:\n'
        '  * Python 3.14 oder neuer (bringt compression.zstd mit)\n'
        '  * pip install zstandard\n'
        '  * 7-Zip ab Version 22 installieren (Pfad notfalls über SEVENZIP setzen)')

def _sprache_eintragen(sprache, spielordner):
    """`g_language` setzen — Fehlschlag ist kein Grund, den Rest wegzuwerfen."""
    try:
        from . import translation
        translation.set_user_cfg(sprache, None, spielordner)
    except Exception as ausnahme:
        from . import fehler
        fehler.merken('spieltexte._sprache_eintragen', ausnahme)


def holen(sprache='english', spielordner=None, fortschritt=None,
          sprache_eintragen=True):
    """Die `global.ini` einer Sprache aus dem Archiv holen. (Erfolg, Meldung).

    Geschrieben wird direkt an den Ort, an dem das Spiel sie erwartet. Eine
    **vorhandene Datei wird nicht angetastet** — dort könnte die Übersetzung
    eines anderen Projekts liegen, und die zu überschreiben, weil jemand auf
    „Originaltexte" geklickt hat, wäre ein handfester Verlust.

    ⚠ **Die Datei allein reicht nicht.** Ohne `g_language` in der `user.cfg`
    liest Star Citizen sie gar nicht erst, sondern bleibt bei den Texten aus
    dem Archiv — und dann steht am Traktorstrahl weiter nur der nackte Name.
    Wer englisch spielt, hat diesen Ordner nämlich **gar nicht**; er entsteht
    erst hier. Genau deshalb steht der Eintrag jetzt in dieser Funktion und
    nicht bei den Aufrufern: Dort stand er zweimal, in `assistent.py` und in
    `einstellungsfenster.py`, und ein dritter Weg hätte ihn vergessen.
    (Hinweis von gemeldet, 27.08.2026: „auch da ist dann die user cfg wieder
    mit wichtig, sonst kann man das nie ohne eine übersetzung nutzen.")

    `sprache_eintragen=False` lässt die `user.cfg` in Ruhe — für Werkzeuge,
    die nur an die Datei wollen, ohne die Installation umzustellen."""
    def melde(text):
        if fortschritt:
            fortschritt(text)

    archiv = p4k_pfad(spielordner)
    if not archiv:
        return False, t('m_kein_p4k')
    ziel = None
    try:
        from . import translation
        ziel = translation.target_ini(sprache, spielordner)
    except Exception:
        pass
    if not ziel:
        return False, 'Zielordner unbekannt'
    if os.path.isfile(ziel):
        # ⚠ Auch hier eintragen: Die Datei liegt richtig, wird aber ohne den
        # Eintrag nicht gelesen. Wer sie von Hand hingelegt hat, säße sonst vor
        # einem Ergebnis, das es gar nicht gibt.
        if sprache_eintragen:
            _sprache_eintragen(sprache, spielordner)
        return True, 'vorhandene Datei behalten'

    melde(t('z_originaltexte'))
    try:
        groesse = os.path.getsize(archiv)
        with open(archiv, 'rb') as f:
            cd, _anzahl = lies_verzeichnis(f, groesse)
            treffer = suche(cd, archivpfad(sprache))
            if not treffer:
                return False, t('m_keine_ini_archiv')
            methode, cs, rs, off = treffer
            roh = hole_block(f, off, cs)
        # entpacke_zstd gibt (Daten, benutztes Verfahren) zurück
        if methode == 100:
            daten, weg = entpacke_zstd(roh, rs)
            melde(t('z_entpackt') % weg)
        else:
            daten = roh
        if not daten:
            return False, 'Entpacken fehlgeschlagen'
        os.makedirs(os.path.dirname(ziel), exist_ok=True)
        with open(ziel + '.tmp', 'wb') as f:
            f.write(daten)
        os.replace(ziel + '.tmp', ziel)
        # Frische Grundlage — die gemerkten Originaltexte gehören zur alten
        # Datei. Siehe `injektion.urtext_verwerfen()`.
        from . import injektion
        injektion.urtext_verwerfen()
        if sprache_eintragen:
            _sprache_eintragen(sprache, spielordner)
        return True, '%.1f MB' % (len(daten) / 1048576.0)
    except Exception as e:
        return False, str(e)
