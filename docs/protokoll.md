# Das Lichtprotokoll: PC zu Tastatur

Dieses Dokument ist der Vertrag zwischen der Firmware und jedem Programm auf dem PC, das die
Tastatur beleuchten will. Wer sich daran hält, braucht die Firmware nicht zu lesen.

Der Kanal ist Roh-HID (QMK `RAW_ENABLE`), ein eigenes HID-Gerät neben der Tastatur. Es braucht
keine Treiber und keine Verwaltungsrechte: unter Windows findet man es über die HID-Schnittstelle,
unter Linux über `hidraw` (dort ist eine udev-Regel nötig, siehe `docs/bauen-und-flashen.md`).

| Feld | Wert |
|---|---|
| USB-Kennung | `0x3233:0x5311` |
| Verwendungsseite / Verwendung | `0xFF60` / `0x61` (QMK-Standard für Roh-HID) |
| Berichtsgröße | 32 Byte in jede Richtung, immer voll gesendet |
| Protokollversion | 1 |

## Befehle

Byte 0 ist der Befehl. Die Antwort trägt denselben Wert mit gesetztem höchsten Bit (`0x01` → `0x81`)
und in Byte 1 den Status.

| Byte 0 | Befehl | Nutzlast | Antwort |
|---|---|---|---|
| `0x01` | GET_INFO | — | `0x81`, Status, Protokollversion, `rows=5`, `cols=15`, LED-Zahl, LEDs je Teilbild, Warteschlangenplätze, Kennung `SF-RGB` |
| `0x02` | FRAME_CHUNK | `[1]` Sequenz, `[2]` Teilbildnummer, `[3]` Teilbildzahl, `[4]` erster LED-Index, `[5]` LED-Zahl (≤ 8), `[6…]` RGB-Tripel | keine |
| `0x03` | FRAME_COMMIT | `[1]` Sequenz | `0x83`, Status, Sequenz, freie Warteschlangenplätze |
| `0x04` | SET_MODE | `[1]` 0 = Tastatur, 1 = PC | `0x84`, Status |
| `0x05` | SET_EFFECT | `[1]` Effekt, `[2]` Geschwindigkeit, `[3]` Helligkeit | `0x85`, Status |
| `0x06` | SAVE_STATE | — | `0x86`, Status |
| `0x07` | SET_KEY | `[1]` LED-Index, `[2…4]` RGB | `0x87`, Status |
| `0x08` | GET_STATE | — | `0x88`, Status, Betriebsart, Effekt, Geschwindigkeit, Helligkeit, Ebene |
| `0x09` | SET_LAYER | `[1]` 0 = Effekte, 1 = CM1, 2 = CM2 | `0x89`, Status |

Ein unbekannter Befehl wird beantwortet, nicht verschluckt: Status 1. Ein Programm, das mit der
falschen Firmware redet, merkt das sofort statt auf eine Antwort zu warten, die nie kommt.

### Status

| Wert | Bedeutung | Was der PC tun sollte |
|---|---|---|
| 0 | in Ordnung | weiter |
| 1 | ungültiges Argument | das Bild ist falsch gebaut; nicht wiederholen, sondern den Fehler beheben |
| 2 | beschäftigt | das vorige Bild ist noch nicht gezeigt; der gesammelte Stand **bleibt** erhalten, es genügt, die Übergabe allein zu wiederholen |
| 3 | falsche Sequenz | Teilbilder und Übergabe passen nicht zusammen, oder die Nummer ist nicht neuer als die letzte; mit der nächsten Nummer von vorn |

## Warum Teilbilder

Ein Roh-HID-Bericht trägt 32 Byte. 68 LEDs brauchen 204 Byte Farbe. Ein Bild kommt also in
Stücken, und erst `FRAME_COMMIT` zeigt es. Das ist der Grund für die Sequenznummer: ohne sie
würde ein verlorener Bericht ein halb altes, halb neues Bild erzeugen, und niemand erfährt davon.

Die Sequenznummer muss in der Übergabe dieselbe sein wie in den Teilbildern und neuer als die
letzte angenommene. Eine übersprungene Nummer ist erlaubt — ein Programm darf Bilder verwerfen —,
eine wiederholte oder rückwärts laufende nicht (Status 3). Eine Übergabe, die nicht zu den
gesammelten Teilbildern passt, wird abgelehnt statt stillschweigend geflickt: ein Programm, dessen
Animation stockt, soll den Grund erfahren. `FRAME_COMMIT` nennt
in der Antwort die freien Warteschlangenplätze, damit der PC seine Bildrate selbst bremsen kann,
statt gegen eine Wand aus Status 2 zu laufen.

### Ein Vollbild, durchgerechnet

| | |
|---|---|
| LEDs | 68 |
| LEDs je Teilbild | 8 |
| Teilbilder | 9 (das letzte trägt 4 LEDs) |
| Berichte je Bild | 9 Teilbilder + 1 Übergabe = 10 |
| gesendete Bytes | 10 × 32 = 320, davon 204 Byte Farbe |
| bei 30 Bildern/s | 300 Berichte/s |

Der Interrupt-Endpunkt wird jede Millisekunde abgefragt, trägt also rund 1000 Berichte je Sekunde.
30 Bilder je Sekunde kosten damit etwa ein Drittel des Kanals; 60 wären möglich, aber die
Tastatur zeichnet ihre LED-Reihen mit 1 kHz neu, sodass darüber ohnehin nichts mehr sichtbar wird.

## Das Zeitfenster

Bleibt **2000 ms** ohne `FRAME_COMMIT`, kehrt die Tastatur von selbst zum gespeicherten Zustand
zurück. Ein abgestürztes Programm auf dem PC darf keine dunkle Tastatur hinterlassen. Wer das
Zeitfenster nicht braucht, schickt `SET_MODE 0` und ist fertig.

**Die PC-Betriebsart wird nie gespeichert.** `SAVE_STATE` legt immer den Zustand ab, den die
Tastatur aus eigener Kraft zeigt. Anders wäre sie nach dem nächsten Einstecken im Host-Betrieb mit
leerem Bild — und das Zeitfenster, das ein abgestürztes Programm auffängt, greift nach einem
Neustart nicht. Sie wäre einfach dunkel, ohne Ausweg. Dasselbe gilt für die Ebene: die Host-Ebene
ist keine, die gespeichert oder mit `SET_LAYER` gewählt wird.

### Helligkeit

Host-Bilder werden mit der am Gerät eingestellten Helligkeit skaliert (`Wert × Helligkeit / 255`).
Die Vorgabe ist 100 und der Deckel 200, ein gesendetes Weiß erscheint also bei etwa 39 % und
höchstens bei 78 %. Ein Programm, das eine wahrheitsgetreue Vorschau zeigen will, fragt die
Helligkeit mit `GET_STATE` ab, statt sie zu erraten; wer sie ändert, merkt sich den alten Wert,
denn `SET_EFFECT` setzt zwangsläufig auch Effekt und Geschwindigkeit und wählt die Effektebene.

`SAVE_STATE` ist das Einzige, was in den Flash schreibt. Es ist für den Augenblick gedacht, in dem
der Nutzer sagt „so soll es bleiben", nicht für jedes Bild: der Datenflash verträgt rund 100 000
Löschzyklen je Seite, und bei 30 Bildern je Sekunde wäre er in einer Stunde verbraucht.

## RAM

Gemessen am gebauten Abbild (`arm-none-eabi-size -A`, 23.11.2026):

| | Byte |
|---|---|
| `.data` | 1 468 |
| `.bss` | 3 780 |
| Stapel (Haupt 1 024 + Prozess 2 048) | 3 072 |
| **belegt** | **8 328** von 20 480 (41 %) |
| frei für Halde und Spitzen | 12 152 |

Das Protokoll selbst hält zwei Bildpuffer (Sammelpuffer und übergebenes Bild) zu je 204 Byte. Die
Warteschlange hat **einen** Platz; reißt das Budget später, wird zuerst dort gekürzt, bevor
irgendetwas anderes angetastet wird.
