# Ducky One 3 SF RGB — hergeleitete Hardware-Zuordnung

Quelle für alles hier: `build/One_3_SF_RGB_V1.16.bin`, SHA-256
`966742624d64e5d6de7efed1dc10ff7e080f5e99298397409cfb5264b5568844` — extrahiert aus dem offiziellen
Updater `Ducky_One_3_SF_RGB_V1.16.exe` und dreifach gegengeprüft (eigene Extraktion, Hash-Liste von
`ducky-one3-isp`, Patch-Grundlage von `DuckyOne3-RGB-Linux`). Offsets beziehen sich auf dieses Image
ab `0x0000`. Vergleichsmaßstab ist der Full-Size-Port aus QMK-PR #26499 (gleicher Chip).

## Chip und Adressraum

| Eigenschaft | Wert | Beleg |
|---|---|---|
| MCU | NUC1261SG4AE, PDID `0x01205212`, 256 KB Flash, 20 KB RAM | `isp.py info` am Gerät |
| Bootloader | LDROM, Version `0x33`; App `3233:5311`, LDROM `3233:5310` bzw. `0416:3f00` nach D+L | ebd. |
| CONFIG0 | `0xF7FFFF7C` — bootet immer LDROM, LOCK gesetzt (kein SWD) | ebd. |
| CONFIG1 | `0x00033200` — Datenflash ab `0x33200` | ebd., nach dem V1.16-Update |
| GPIO-Portblöcke | `0x50004000 + Port*0x40`, Ports A…F | Literale `0x50004000/80/C0`, `0x50004100/140` |
| **Einzelpin-Datenregister (PDIO)** | **`0x50004200 + Port*0x40 + Pin*4`** | `0x36aa` lädt `0x50004280` und schreibt 0/1 nach +0x00/+0x08/+0x0C, unmittelbar nach Konfiguration von `SYS_GPC_MFPL` (`0x50000040`) — Port C passt zu `0x50004200 + 2*0x40` |

Die PDIO-Basis ist der einzige Punkt, an dem ich zunächst falsch lag (`0x50004800` angenommen). Die
Korrektur ist durch den Port-C-Zusammenhang belegt und wird durch den Tipp-Test gegengeprüft.

## Matrix — belegt

Die Firmware baut im SRAM bei `0x20002348` eine Deskriptortabelle auf, Einträge à 8 Byte:

```
+0  Zeiger auf den GPIO-Portblock (4 Byte)
+4  Port-Index (1 Byte; 0 = A, 1 = B, 2 = C, 3 = D, 4 = E, 5 = F)
+5  Pin-Nummer (1 Byte)
+6  Füllbytes
```

Aufgebaut von der Routine ab `0x4a1e`. Einträge 0–14 sind die Spalten, 15–19 die Zeilen.

| | Pins | Beleg |
|---|---|---|
| Spalten (15) | `A3 A2 A1 A0 B0 B1 B2 B3 B4 B8 B11 E2 B15 B5 B6` | Schreibfolge `0x4a62`–`0x4b1e` |
| Zeilen (5) | `D0 D8 D9 D1 D2` | Schreibfolge `0x4a26`–`0x4a5a` |
| Diodenrichtung | COL2ROW | wie Referenzport; Zeilen werden gelesen, Spalten getrieben |

**Gegenprobe:** Der Full-Size-Port hat 16 Spalten `A3 A2 A1 A0 B0 B1 B2 B3 B4 B8 B11 E2 B15 B5 B6 B7`
und 8 Zeilen `D0 D8 D9 D1 D2 D3 F0 F1`. Die SF-Listen sind davon exakt die ersten 15 bzw. 5 Einträge.
Zwei unabhängig entstandene Quellen, gleiche Reihenfolge.

Port B erscheint in keinem einzigen Literal, weil die Firmware ihn als `PA + 0x40` berechnet. Ein
reiner Literal-Scan hätte ihn übersehen — Grund, warum die Deskriptortabelle die belastbare Quelle ist.

## Tastentabelle — belegt

Die ersten 75 Byte der entpackten Startup-Daten (`build/startup_sram.bin`, SRAM `0x200002E0`) sind die
HID-Codes in Matrixreihenfolge, 5 × 15:

```
Reihe 0  Esc 1 2 3 4 5 6 7 8 9 0 - = BSpc Del
Reihe 1  Tab Q W E R T Y U I O P [ ] \ PgUp
Reihe 2  Caps A S D F G H J K L ; ' \ Enter PgDn
Reihe 3  --  ISO< Z X C V B N M , . / -- -- Up
Reihe 4  --  --  -- -- -- Space -- -- 0x62 -- -- -- Lft Dn Rgt
```

`0x64` in Reihe 3 ist die ISO-Taste links neben Y und bestätigt das ISO-Layout. Die Nullen stehen für
Modifikatoren, die die Firmware über eine eigene Tabelle behandelt — in Aufgabe 2 zu klären.

## LED-Kette — belegt

Dieselbe Deskriptortabellen-Technik: Die Routine ab `0x3838` baut bei SRAM `0x200025CC` eine Tabelle
mit Einträgen à 12 Byte `{Portblock-Zeiger, Port-Index, Pin}` auf, fünf Einträge.

| | Wert | Beleg |
|---|---|---|
| LED-Zeilen (5, aktiv high) | `C6 C5 D7 D15 D14` | Tabelle `0x200025CC`, Schreibfolge `0x383a`–`0x3870` |
| SDI / DCLK / LE | `C3` / `C0` / `C2` | `GPIO_SetMode`-Aufrufe `0x3806`–`0x382a`; Bit-Schiebemuster auf `0x50004280` +0x0C/+0x00/+0x08 |
| LED-Versorgung | `C7`, aktiv low | `0x37b2`–`0x37c2` |
| GCLK | `C1` über PWM0 CH1, PCLK 48 MHz / 24 = 2 MHz | Referenzport; im SF-Image `SYS_GPC_MFPL`-Nibble für Pin 1 |
| Auffrischung | TMR1 aus HIRC, 1 kHz | `CLK_APBCLK0`-Zugriffe; Referenzport |
| Kette | 3 × MBI5042, je 16 Kanäle, ein Chip je Farbe (0=rot, 1=grün, 2=blau), Chip 0 zuletzt geschoben | Referenzport, gleicher Chip |
| Bildpuffer | `px[5][16][3]` — 5 LED-Zeilen × 16 Kanäle × 3 Farben | abgeleitet aus Zeilenzahl |
| DIP-Schalter | `E9` wird für den Lesevorgang auf low gezogen, DIP1–4 an den Matrixzeilen `D0 D8 D9 D1` | Referenzport `dip.c`, Stock-Fundstelle V1.15 @0x9b8x |

**Gegenprobe:** Der Full-Size-Port hat acht LED-Zeilen `C6 C5 D7 D15 D14 D13 D12 F2`. Die fünf
SF-Zeilen sind davon exakt die ersten fünf — dieselbe Verkürzung wie bei Matrixzeilen und -spalten.

Die Basis der Einzelpin-Register ist damit doppelt belegt: Der Referenzport rechnet
`GPIO_PIN_DATA_BASE + Port*0x40 + Pin*4` und nennt `PDIO(2,0)` ausdrücklich PC0, was `0x50004200`
ergibt — identisch mit meiner Herleitung aus dem Stock-Image. Das Literal bei `0x3ad4` im SF-Image
ist `0x50004200`.

## Abbildung LED-Index auf Steckplatz — belegt

Der Steckplatz einer LED **ist** ihre Matrixposition: `Steckplatz = Zeile << 4 | Spalte`. Damit ist
`led_hw[]` in `led_map.h` die Identität über die 68 ISO-Positionen.

Gegenprobe am Gerät (23.11.2026): Mit dem QMK-Effekt `SOLID_COLOR` leuchten alle 68 Tasten
gleichmäßig, keine dunklen Löcher, nichts an Stellen ohne Taste, Farben richtig. `keymaps/ledtest`
läuft den Bildpuffer Steckplatz für Steckplatz ab und bleibt als Werkzeug im Baum, falls sich die
Annahme später an einer Stelle doch nicht hält.

Frei bleiben die Steckplätze, auf denen keine ISO-Taste sitzt: Kanal 15 in allen fünf Reihen,
(1,13) — der ANSI-Rückschrägstrich —, (3,12) und in Reihe 4 die Kanäle 3, 4, 6, 7, 8. Der Platz
(4,8) ist im Full-Size die Ziffernblock-Null.

## Indikatoren — Entscheidung statt Befund

Die SF hat keine eigene LED-Reihe für Indikatoren, wie sie das Full-Size-Modell mit Reihe 7,
Kanäle 4–7 (Num, Caps, Scroll, Vierte) hat — dort gibt es nur fünf Reihen, und alle fünf tragen
Tasten. Umschaltfeststellung und Windows-Sperre werden deshalb auf der jeweiligen **Tastenbeleuchtung
selbst** angezeigt (Aufgabe 5). Das ist eine Festlegung, kein Befund aus dem Werksabbild: Beim
Leuchttest war nichts an Stellen ohne Taste zu sehen, was gegen separate Indikator-LEDs spricht,
aber nicht beweist, dass die Werksfirmware es genauso macht.

## Rettungsweg

Gemessen am 23.11.2026, nicht hergeleitet:

* `D`+`L` beim Einstecken führt immer ins LDROM (`0416:3f00`), unabhängig vom Anwendungsflash.
* `tools/flash.sh build/One_3_SF_RGB_V1.16.bin --yes` stellt den Werksstand her; danach meldet
  `isp.py version` wieder `DK-V1.16-251110`. Einmal durchgeführt und bestätigt.
* `Fn`+`Esc` aus unserer Firmware springt ebenfalls ins LDROM. Ohne folgenden ISP-Befehl kehrt das
  LDROM von selbst ins Anwendungsflash zurück — der Sprung muss also unmittelbar vor dem Schreiben
  ausgelöst werden.
