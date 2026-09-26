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
Prüft JSON gegen die Formatbeschreibungen des Basetool-Vertrags.

Nur für Prüfläufe — das Programm selbst braucht es nicht. Ein vollständiger
JSON-Schema-Prüfer wäre ein Zusatzpaket, und die Projektregel lautet: nur
Standardbibliothek. Deshalb hier genau die Schlüsselwörter, die der Vertrag in
`tools/basetool-vertrag/v1/schemas/` benutzt (JSON Schema 2020-12), nicht mehr.

⚠⚠ **Der Prüfer muss sich zuerst selbst beweisen.** Ein Prüfer, der alles
durchwinkt, macht jede Prüfung damit wertlos. `check_examples()` läuft deshalb
über gelucs Beispieldateien: Jede Datei in `valid/` muss durchgehen, jede in
`invalid/` muss hängen bleiben. Erst danach taugt er, um VerseKits eigene
Ausgabe zu prüfen. Kommt ein Schlüsselwort dazu, das hier fehlt, meldet der
Prüfer es als Fehler, statt es still zu übergehen.

Aufruf von Hand: `python tools/schema_pruefen.py` — prüft alle Beispiele.
"""
import json
import os
import re
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
CONTRACT = os.path.join(HERE, 'basetool-vertrag', 'v1')
SCHEMAS = os.path.join(CONTRACT, 'schemas')
EXAMPLES = os.path.join(CONTRACT, 'beispiele')

# Schlüsselwörter, die nur beschreiben und nichts verlangen.
_ANNOTATIONS = {'$schema', '$id', '$defs', 'title', 'description', 'default',
                'readOnly'}
_HANDLED = {'$ref', 'type', 'properties', 'required', 'additionalProperties',
            'allOf', 'anyOf', 'oneOf', 'const', 'enum', 'minLength',
            'maxLength', 'pattern', 'minimum', 'maximum', 'exclusiveMinimum',
            'minItems', 'maxItems', 'items', 'uniqueItems',
            'dependentRequired', 'propertyNames', 'maxProperties', 'if',
            'then', 'format'}

_DATE_TIME = re.compile(r'^\d{4}-\d{2}-\d{2}[Tt]\d{2}:\d{2}:\d{2}(\.\d+)?'
                        r'([Zz]|[+-]\d{2}:\d{2})$')
_UUID = re.compile(r'^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-'
                   r'[0-9a-fA-F]{4}-[0-9a-fA-F]{12}$')
_URI = re.compile(r'^[A-Za-z][A-Za-z0-9+.-]*:\S+$')

_cache = {}


def load_schema(name):
    """Ein Schema aus dem Vertrag, z. B. `change-set.schema.json`."""
    if name not in _cache:
        with open(os.path.join(SCHEMAS, name), encoding='utf-8') as f:
            _cache[name] = json.load(f)
    return _cache[name]


def _python_pattern(pattern):
    """ECMA-Regex → Python `re`.

    ⚠ Python kennt `\\p{L}` und `\\p{N}` nicht. Im Vertrag stehen sie nur
    gemeinsam in einer Zeichenklasse (Buchstaben und Ziffern), und das ist
    genau `\\w` ohne den Unterstrich — der steht in derselben Klasse ohnehin
    daneben. Andere Formen werden abgelehnt statt geraten."""
    fixed = pattern.replace(r'\p{L}\p{N}', r'\w')
    if r'\p{' in fixed:
        raise ValueError('unbekannte Unicode-Klasse in %r' % pattern)
    return fixed


def _type_ok(value, kind):
    if kind == 'object':
        return isinstance(value, dict)
    if kind == 'array':
        return isinstance(value, list)
    if kind == 'string':
        return isinstance(value, str)
    if kind == 'boolean':
        return isinstance(value, bool)
    if kind == 'null':
        return value is None
    if kind == 'integer':
        return (isinstance(value, int) and not isinstance(value, bool)) or (
            isinstance(value, float) and value.is_integer())
    if kind == 'number':
        return isinstance(value, (int, float)) and not isinstance(value, bool)
    raise ValueError('unbekannter Typ %r' % kind)


def _resolve(ref, doc_name):
    """`$ref` → (Teilschema, Dokumentname)."""
    file_part, _, pointer = ref.partition('#')
    name = file_part or doc_name
    node = load_schema(name)
    for piece in [p for p in pointer.split('/') if p]:
        node = node[piece]
    return node, name


def validate(value, schema, doc_name, where=''):
    """Fehlerliste (leer = gültig)."""
    errors = []
    if schema is True:
        return errors
    if schema is False:
        return ['%s: nichts erlaubt' % (where or '/')]
    unknown = set(schema) - _HANDLED - _ANNOTATIONS
    if unknown:
        # Lieber laut scheitern als ein Schlüsselwort still übergehen.
        return ['%s: Schlüsselwort nicht umgesetzt: %s'
                % (where or '/', ', '.join(sorted(unknown)))]
    here = where or '/'

    if '$ref' in schema:
        sub, name = _resolve(schema['$ref'], doc_name)
        errors += validate(value, sub, name, where)
    if 'type' in schema:
        kinds = schema['type'] if isinstance(schema['type'], list) \
            else [schema['type']]
        if not any(_type_ok(value, k) for k in kinds):
            return errors + ['%s: Typ %s erwartet' % (here, '/'.join(kinds))]
    if 'const' in schema and value != schema['const']:
        errors.append('%s: muss %r sein' % (here, schema['const']))
    if 'enum' in schema and value not in schema['enum']:
        errors.append('%s: %r nicht erlaubt' % (here, value))

    if isinstance(value, str):
        if len(value) < schema.get('minLength', 0):
            errors.append('%s: zu kurz' % here)
        if 'maxLength' in schema and len(value) > schema['maxLength']:
            errors.append('%s: zu lang' % here)
        if 'pattern' in schema and not re.search(
                _python_pattern(schema['pattern']), value):
            errors.append('%s: passt nicht zu %s' % (here, schema['pattern']))
        fmt = schema.get('format')
        if fmt == 'date-time' and not _DATE_TIME.match(value):
            errors.append('%s: kein Zeitpunkt' % here)
        elif fmt == 'uuid' and not _UUID.match(value):
            errors.append('%s: keine UUID' % here)
        elif fmt == 'uri' and not _URI.match(value):
            errors.append('%s: keine Adresse' % here)
    if isinstance(value, (int, float)) and not isinstance(value, bool):
        if 'minimum' in schema and value < schema['minimum']:
            errors.append('%s: kleiner als %s' % (here, schema['minimum']))
        if 'maximum' in schema and value > schema['maximum']:
            errors.append('%s: größer als %s' % (here, schema['maximum']))
        if 'exclusiveMinimum' in schema and value <= schema['exclusiveMinimum']:
            errors.append('%s: nicht größer als %s'
                          % (here, schema['exclusiveMinimum']))
    if isinstance(value, list):
        if len(value) < schema.get('minItems', 0):
            errors.append('%s: zu wenige Einträge' % here)
        if 'maxItems' in schema and len(value) > schema['maxItems']:
            errors.append('%s: zu viele Einträge' % here)
        if schema.get('uniqueItems'):
            seen = [json.dumps(v, sort_keys=True) for v in value]
            if len(seen) != len(set(seen)):
                errors.append('%s: doppelte Einträge' % here)
        if 'items' in schema:
            for i, item in enumerate(value):
                errors += validate(item, schema['items'], doc_name,
                                   '%s/%d' % (where, i))
    if isinstance(value, dict):
        for key in schema.get('required', ()):
            if key not in value:
                errors.append('%s: %s fehlt' % (here, key))
        props = schema.get('properties', {})
        for key, sub in props.items():
            if key in value:
                errors += validate(value[key], sub, doc_name,
                                   '%s/%s' % (where, key))
        extra = schema.get('additionalProperties', True)
        for key in value:
            if key not in props:
                errors += validate(value[key], extra, doc_name,
                                   '%s/%s' % (where, key))
        for key, needed in schema.get('dependentRequired', {}).items():
            if key in value:
                for n in needed:
                    if n not in value:
                        errors.append('%s: %s verlangt %s' % (here, key, n))
        if 'propertyNames' in schema:
            for key in value:
                errors += validate(key, schema['propertyNames'], doc_name,
                                   '%s/<%s>' % (where, key))
        if 'maxProperties' in schema and len(value) > schema['maxProperties']:
            errors.append('%s: zu viele Felder' % here)

    for sub in schema.get('allOf', ()):
        errors += validate(value, sub, doc_name, where)
    if 'anyOf' in schema and not any(
            not validate(value, sub, doc_name, where)
            for sub in schema['anyOf']):
        errors.append('%s: passt zu keiner Variante (anyOf)' % here)
    if 'oneOf' in schema:
        hits = sum(1 for sub in schema['oneOf']
                   if not validate(value, sub, doc_name, where))
        if hits != 1:
            errors.append('%s: passt zu %d statt genau einer Variante (oneOf)'
                          % (here, hits))
    if 'if' in schema and not validate(value, schema['if'], doc_name, where):
        if 'then' in schema:
            errors += validate(value, schema['then'], doc_name, where)
    return errors


def check(value, target):
    """`target` wie ein Beispielordner: `blueprint` oder
    `change-set--blueprintChangeSet`. Gibt die Fehlerliste zurück."""
    name, _, part = target.partition('--')
    doc = name + '.schema.json'
    schema = load_schema(doc)
    if part:
        schema = schema['$defs'][part]
    return validate(value, schema, doc)


def check_examples():
    """Alle Beispieldateien → Liste von (Datei, erwartet, Fehlerliste).

    `erwartet` ist 'valid' oder 'invalid'. Ein Eintrag ist ein Treffer, wenn
    eine gültige Datei keine Fehler hat oder eine ungültige mindestens einen."""
    results = []
    for target in sorted(os.listdir(EXAMPLES)):
        folder = os.path.join(EXAMPLES, target)
        if not os.path.isdir(folder):
            continue
        for expected in ('valid', 'invalid'):
            sub = os.path.join(folder, expected)
            if not os.path.isdir(sub):
                continue
            for file_name in sorted(os.listdir(sub)):
                with open(os.path.join(sub, file_name), encoding='utf-8') as f:
                    value = json.load(f)
                results.append(('%s/%s/%s' % (target, expected, file_name),
                                expected, check(value, target)))
    return results


def main():
    # ⛔ Vor der ersten Ausgabe: Die Windows-Konsole kann nicht jedes Zeichen
    # der Fehlermeldungen — siehe ausgabe.py. Nur hier, nicht beim Import:
    # Der Selbsttest lädt dieses Modul und hat seine Ausgabe schon gesetzt.
    sys.path.insert(0, HERE)
    import ausgabe
    ausgabe.utf8()
    wrong = []
    results = check_examples()
    for path, expected, errors in results:
        if (expected == 'valid') == bool(errors):
            wrong.append((path, errors))
    for path, errors in wrong:
        print('FALSCH:', path, errors[:3])
    print('%d Beispiele, %d falsch beurteilt' % (len(results), len(wrong)))
    return 1 if wrong else 0


if __name__ == '__main__':
    sys.exit(main())
