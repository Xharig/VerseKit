# VerseKit für Stream Deck: Profile

English: README.md

### Was du wissen musst
Profile für VerseKit 5.7.0.

- **Spieltasten gehen sofort.** Jede Taste schickt die Tastenkombination, die Star Citizen ab Werk vorsieht. Du musst dafür nichts einstellen.
- **Einige Kombinationen musst du im Spiel selbst eintragen**, weil das Spiel ab Werk keine Taste dafür hat (Liste unten). Ohne Eintrag passiert beim Druck nichts.
- **VerseKit-Tasten F13 bis F24** (Overlay, Raffinerie lesen, Handle-Suche, Spielbild, Hauptfenster, Bauplan erfassen, Mein Lager scannen, Staffellager scannen, Anti-AFK, Mining Scanner, Overlay leeren, Signatur anlernen) gehen, sobald du die Kürzel in VerseKit einstellst (Tabelle unten). Overlay leeren (F23) und Signatur anlernen (F24) sind ab Werk ohne Belegung: in VerseKit unter Einstellungen › Tasten & Stream Deck eintragen. Signatur anlernen gibt es nur unter Windows (VerseKit muss dafür den Bildschirm lesen).
- **Seitentasten in der Mitte der Startseite** (Mein Hangar, Mein Lager, Bergbau, Bauplan-Liste, Contested Zones und weitere) öffnen die Seiten in VerseKit. Sie brauchen das **VerseKit-Plugin**, das VerseKit selbst installiert (Knopf „Stream Deck einrichten“ auf „Tasten & Stream Deck“). Contested Zones zeigt dazu den Zyklus des Executive Hangars. „aUEC“ bleibt gedimmt und kommt mit 5.8.
- **Overlay an/aus, Hauptfenster an/aus, Anti-AFK an/aus und Mining Scanner** (Startseite, Mining Scanner auch auf Bergbau) sind ebenfalls Plugin-Tasten und zeigen den Zustand (AN/AUS, OFFEN/ZU). Ohne Plugin oder wenn VerseKit nicht läuft, zeigen sie ein Stecker-Symbol und tun nichts.
- Auf der Seite Info öffnen „VerseKit Webseite“ und „GitHub“ die Webseite und das GitHub-Projekt im Browser, „Anleitung öffnen“ die Stream-Deck-Seite der VerseKit-Webseite mit dieser Anleitung und „FAQ“ deren Abschnitt mit den häufigen Fragen.

### Welche Datei für welches Gerät?
Jede Datei gibt es auf Deutsch (deutsche Tastaturlage, deutsche Spieltexte) und Englisch (US-Tastaturlage, englische Spieltexte). Nimm die Sprache deines Spiels.

| Gerät | Datei |
|---|---|
| Stream Deck XL (8 × 4 Tasten) | `VerseKit-StreamDeck-XL-DE.streamDeckProfile` / `VerseKit-StreamDeck-XL-EN.streamDeckProfile` |
| Stream Deck Mobile auf iPad oder iPhone (8 × 4 Tasten) | `VerseKit-StreamDeck-Mobile-DE.streamDeckProfile` / `VerseKit-StreamDeck-Mobile-EN.streamDeckProfile` |
| Stream Deck MK.2 und Standard (15 Tasten) | `VerseKit-StreamDeck-MK2-DE.streamDeckProfile` / `VerseKit-StreamDeck-MK2-EN.streamDeckProfile` |
| Stream Deck Neo (8 Tasten) | `VerseKit-StreamDeck-Neo-DE.streamDeckProfile` / `VerseKit-StreamDeck-Neo-EN.streamDeckProfile` |
| Stream Deck Mini (6 Tasten, nur die wichtigsten Tasten) | `VerseKit-StreamDeck-Mini-DE.streamDeckProfile` / `VerseKit-StreamDeck-Mini-EN.streamDeckProfile` |
| Stream Deck + XL (9 × 4 Tasten und 6 Drehregler) | `VerseKit-StreamDeck-PlusXL-DE.streamDeckProfile` / `VerseKit-StreamDeck-PlusXL-EN.streamDeckProfile` |

Das Stream Deck + XL ist ein anderes Gerät als das Stream Deck XL. Sein Profil hat in den Spalten 1 bis 8 genau die Tasten des XL, dazu eine neunte Spalte und die sechs Drehregler (Abschnitt „Stream Deck + XL“ unten). Noch nicht am Gerät erprobt.

**Was liegt wo (im ZIP)?**
- `Profile/Deutsch/<Gerät>/` und `Profile/English/<Gerät>/`: die Profildatei für dein Gerät.
- `LIZENZEN.md`: Herkunft und Lizenz der Symbole und der Schrift.

### Einrichten
1. **Profil importieren:** Doppelklick auf die Datei öffnet die Stream-Deck-Software und legt das Profil an. Zeigt sich nichts, in der Stream-Deck-Software auf das Zahnrad › Reiter „Profile“ gehen und dort importieren.
2. **Plugin installieren:** VerseKit installiert es selbst (Knopf „Stream Deck einrichten“ auf „Tasten & Stream Deck“). Danach gehen die Seitentasten, Contested Zones und die Zustandsanzeige von Overlay, Hauptfenster und Anti-AFK.
3. **Kürzel in VerseKit einstellen:** VerseKit › Einstellungen › Overlay › Tastenkürzel, ohne Strg, Alt oder Umschalt. Overlay (F13), Hauptfenster (F17), Anti-AFK (F21) und Mining Scanner (F22) liegen im Profil als Plugin-Tasten; ihre Kürzel brauchst du nur für die Drehregler des Stream Deck + XL oder für eigene Tasten:

| Funktion | Kürzel |
|---|---|
| Overlay ein- und ausblenden | F13 |
| Raffinerie lesen | F14 |
| Handle-Suche | F15 |
| Spielbild | F16 |
| Hauptfenster an/aus | F17 |
| Bauplan erfassen | F18 |
| Mein Lager scannen | F19 |
| Staffellager scannen | F20 |
| Anti-AFK ein- und ausschalten | F21 |
| Mining-Scanner an/aus | F22 |
| Overlay leeren | F23 |
| Signatur anlernen | F24 |

4. **Tasten im Spiel eintragen**, die ab Werk keine Taste haben (nächster Abschnitt).
5. **„Zurück“ einstellen (nur XL, + XL und Mobile):** „Zurück“ links unten auf den sieben Hauptseiten soll das VerseKit-Profil verlassen und in dein eigenes Profil wechseln. Das Ziel ist leer, weil wir dein Profil nicht kennen: In der Stream-Deck-Software die Taste anklicken und bei „Profil“ dein eigenes Profil wählen.
6. **Mit dem Spiel öffnen:** Die deutschen Profile für XL, + XL und Mobile sind mit dem Standard-Installationsort `C:\Program Files\Roberts Space Industries\StarCitizen\LIVE\Bin64\StarCitizen.exe` verknüpft und öffnen sich mit dem Spiel. Alle anderen sind nicht verknüpft. Liegt dein Spiel woanders, oder willst du ein anderes Profil, stellst du das in der Stream-Deck-Software unter Zahnrad › „Profile“ › Dropdown „Anwendung“ ein. Nur ein Profil soll mit `StarCitizen.exe` verknüpft sein.
   Springt das Deck außerhalb des Spiels bei jedem Fensterwechsel auf ein anderes Profil, ist dort ein eigenes Profil als „Standard“ für alle Programme gesetzt. Im Spiel bleibt VerseKit (das deutsche 8 × 4-Profil ist mit `StarCitizen.exe` verknüpft). Abhilfe: beim eigenen Profil „Standard“ entfernen, oder VerseKit von Hand wählen.

### Tasten, die du im Spiel selbst eintragen musst
Unter Optionen › Tastenbelegung die Funktion suchen und die Kombination eintragen:

| Spielname | Eintragen |
|---|---|
| Bergbaulaser wechseln (Umschalten) | Alt links+U |
| Waffen-Operator-Modus umschalten | AltGr+F |
| Raketen-Operator-Modus umschalten | AltGr+M |
| Gewähltes Ziel sperren | AltGr+T |
| Türen öffnen/schließen (Umschalten) | Alt links+O |
| Türen verriegeln/entriegeln (Umschalten) | Alt links+V |
| Quantum-Operator-Modus umschalten | AltGr+Q |
| Sprungantrieb - Sprung anfordern | AltGr+J |
| Versetztes Feuer - Ein / Aus umschalten | AltGr+B |
| Geschützturm-E.S.P. Ein / Aus | AltGr+E |
| Geschützturm - Geschwindigkeitsbegrenzer - Ein/Aus (Halten/Umschalten) | AltGr+V |
| Zielhilfen unterdrücken (Halten) | AltGr+X |

**Emotes** (Optionen › Tastenbelegung › Sozial - Emotes): Nur Ja, Nein und Stopp haben ab Werk eine Taste (Ziffernblock 4, 6, 2). Alle anderen trägst du selbst ein:

| Emote | Eintragen |
|---|---|
| Winken / Salutieren / greet / Verbeugen | Strg links+F1 / F2 / F3 / F4 |
| Achtung / Rührt euch | Strg links+F5 / F6 |
| Kommen / wait | Strg links+F7 / F8 |
| Zustimmen / Ablehnen / Zeigen | Strg links+F9 / F10 / F11 |
| Hinsetzen | Strg links+F12 |
| sleep / Lachen / Jubeln / Klatschen | Umschalt links+F1 / F2 / F3 / F4 |
| Tanzen / Weinen / Wütend | Umschalt links+F5 / F6 / F7 |
| Nach Luft schnappen / Gelangweilt / Pfeifen | Umschalt links+F10 / F11 / F12 |
| flex (Muskeln zeigen) | Alt links+F12 |

Gelangweilt und flex gibt es nur auf MK.2 und Neo; der Mini hat acht Emotes.

### Stream Deck + XL
Spalten 1 bis 8 sind dieselben Tasten wie auf dem XL und dem iPad. Neu sind die neunte Spalte und die sechs Drehregler (links drehen, rechts drehen, drücken; was jeder Regler tut, steht auf seinem Streifen).

Neunte Spalte: **Bergbau** Bergbaulaser-Leistung erhöhen / verringern, Scannen-Winkel erhöhen / verringern; **Bergung** Strahlabstand erhöhen / verringern, Desintegrationswerkzeug fokussieren, Fokussierte Bergungsmodifikatoren durchschalten; **Dogfight** Waffen, Schilde, Triebwerke - Verringern, Zuweisungen zurücksetzen; **Zu Fuß** Sekundärfeuer nachladen, FPS Unterlauf-Aufsatz Aktion; **Fahrzeug** Overlay, Spielbild, Hauptfenster. Freie Plätze tragen einen Platzhalter.

Diese Funktionen haben ab Werk keine Taste, für Regler und neunte Spalte im Spiel eintragen:

| Spielname | Eintragen |
|---|---|
| Bergbaulaser-Leistung erhöhen / verringern | Alt links+5 / Alt links+6 |
| Scannen-Winkel erhöhen / verringern | Alt links+7 / Alt links+8 |
| Strahlabstand erhöhen / verringern | AltGr+1 / AltGr+2 |
| Desintegrationswerkzeug fokussieren | AltGr+3 |
| Fokussierte Bergungsmodifikatoren durchschalten | AltGr+4 |
| Zielwechsel - Feindliche - Zurück | AltGr+5 |

Noch nicht am Gerät erprobt: ob die Regler links und rechts richtig herum senden und ob der Druck auf den Regler ankommt. Bitte melden, wenn etwas vertauscht ist.

### Wo bin ich?
In der Leiste unten trägt die Taste der aktuellen Seite einen gefüllten Balken. Auf Unterseiten (Emotes, Geschütz, Info) heißt „Zurück“ nach seinem Ziel („‹ Zu Fuß“, „‹ Flug“, „‹ Start“). Auf Mini und Neo zeigt die Seiten-Taste Seitennamen und Teil, auf der MK.2 das Namensschild. Die weiße Taste „Info“ auf der Startseite öffnet die Seite Info mit der Farblegende und dem Profilstand.

### Häufige Fragen

#### Eine Taste tut im Spiel nichts
Das Stream Deck schickt nur die Tastenkombination. Prüfe der Reihe nach: Läuft die Stream-Deck-Software? Hat die Funktion im Spiel unter Optionen › Tastenbelegung dieselbe Kombination wie die Taste? Steht sie in der Liste der Tasten, die du im Spiel selbst eintragen musst? Hast du im Spiel eine Kombination geändert, ohne die Taste im Stream Deck anzupassen?

#### Auf einer Taste ist ein Stecker-Symbol
Die Taste braucht das VerseKit-Plugin und ein laufendes VerseKit. Starte VerseKit. Bleibt der Stecker, in VerseKit auf „Tasten & Stream Deck“ noch einmal „Stream Deck einrichten“ klicken.

#### „Sitz verlassen“ drückt Y statt Z
Star Citizen nennt die Tasten nach der US-Tastatur. Das deutsche Profil sendet Z für die Taste, die das Spiel Y nennt, weil sie auf der deutschen Tastatur dort liegt. Spielst du mit US-Tastatur, nimm das englische Profil.

#### Das iPad zeigt alte Tastenbilder
Einmal das Profil wechseln und zurück. Hilft das nicht, die Stream-Deck-App auf dem iPad schließen und neu öffnen und prüfen, dass der Rechner im selben Netz ist.

#### Warum feuern keine Waffen über das Deck?
Eine Stream-Deck-Taste schickt Tastaturkombinationen, keine Mausklicks. Feuern liegt im Spiel auf der Maus und fehlt deshalb im Profil.

#### Kann ich Tasten selbst ändern?
Ja, in der Stream-Deck-Software am Rechner. Ändere eine Kombination immer im Stream Deck und im Spiel gleich, sonst tut die Taste nichts. Exportiere dein geändertes Profil (Zahnrad › Profile › Rechtsklick › Exportieren), dann holst du es nach einem neuen Import jederzeit zurück.

#### Geht das auch auf Mac oder Linux?
Die Profile und das Plugin sind für Windows gebaut und dort erprobt.

### Update-Verlauf

#### 5.7.0
- Erste Fassung: Profile für Stream Deck Mini, Neo, MK.2, XL, + XL und Mobile, je auf Deutsch und Englisch.
- Das VerseKit-Plugin öffnet Seiten in VerseKit, zeigt Contested Zones und den Zustand von Overlay, Hauptfenster, Anti-AFK und Mining Scanner.

### Rückmeldung
Fehler, fehlende Tasten oder Wünsche bitte im VerseKit-Discord melden, am besten mit Gerät, Sprache des Profils und einem Bild.
