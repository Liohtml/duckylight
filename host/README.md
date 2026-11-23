# duckylight

Licht für die **Ducky One 3 SF RGB**: ein Bild, die Farben des Bildschirms, einzelne Tasten,
Animationen oder die Musik, die gerade läuft — auf der Tastatur, live. Für Windows und Linux.

Voraussetzung ist die eigene **SF-RGB-Firmware** auf der Tastatur (siehe `docs/bauen-und-flashen.md`).
Mit der Werksfirmware erkennt duckylight die Tastatur und sagt, was fehlt.

## Installieren

**Windows** — `duckylight-…-windows.exe` herunterladen und doppelklicken. Im Fenster auf
**Installieren** klicken: das Programm legt sich ins Startmenü und startet künftig mit Windows.
Keine Adminrechte nötig. Entfernen wie jede andere App unter „Installierte Apps".

**Linux** — `duckylight-…-linux-x86_64.tar.gz` entpacken und `./install.sh` ausführen. Das legt das
Programm nach `~/.local/bin` und richtet einmalig (mit `sudo`) eine udev-Regel ein, damit die
Tastatur ohne root erreichbar ist. Bildschirmfarben gehen unter Hyprland, Sway und X11; unter GNOME
und KDE mit Wayland nicht (dort gibt das System den Bildschirm nur über einen Nachfrage-Dialog frei).
Für den Musikmodus wird `parec` gebraucht (Paket `libpulse` bzw. `pulseaudio-utils`).

## Bedienen

Oben die Tastatur, wie sie gerade leuchtet. Darunter sechs Betriebsarten — was man wählt, läuft sofort:

| | |
|---|---|
| **Tastatur** | die eingebauten Effekte der Tastatur; laufen auch ohne dieses Programm |
| **Bild** | ein Bild hineinziehen, den hellen Rahmen dorthin schieben, wo es passt |
| **Bildschirm** | jede Taste nimmt die Farbe des Bildschirmbereichs über ihr |
| **Tasten** | Farbe wählen und auf der Tastatur malen; „In der Tastatur speichern" brennt die Belegung ein, sie leuchtet dann auch ohne PC |
| **Animation** | sechs Bewegungen oder ein eigenes GIF |
| **Musik** | folgt dem, was der Computer abspielt — Bässe links, Höhen rechts |

Das Fenster zu schließen beendet das Licht nicht: duckylight läuft im Infobereich weiter, das Symbol
dort holt das Fenster zurück oder beendet das Programm.

## Aus Skripten

```
duckylight screen                  Bildschirmfarben, bis Strg-C
duckylight image bild.png          ein Bild zeigen
duckylight anim fire --speed 1.5   rainbow, wave, breathe, wheel, aurora, fire
duckylight gif tanz.gif
duckylight music
duckylight profile "Arbeit" --slot 1   eine gespeicherte Belegung in die Tastatur brennen
duckylight device                  das Licht an die Tastatur zurückgeben
duckylight info | state | probe    was Firmware und Rechner melden
```

Genau ein Programm hält die Tastatur. Läuft duckylight schon (auch im Infobereich), endet ein
zweiter Aufruf mit Rückgabecode 3.

## Was bewusst fehlt

- **Effekte, die auf Tastendrücke reagieren.** Dafür müsste das Programm jeden Anschlag des Systems
  mitlesen. Die Tastatur kann das selbst, ohne dass jemand mitliest: Betriebsart „Tastatur".
- **Musik über das Mikrofon.** Es wird nur mitgeschnitten, was der Computer abspielt. Ein Mikrofon
  würde den Raum aufnehmen.

## Bauen

```
cd host
cargo test --no-default-features          # die Kernlogik, überall
LINUX_BUILD_HOST=user@rechner packaging/build-release.sh   # beide Dateien nach dist/
```

Die Windows-Datei wird unter Linux quergebaut (`x86_64-pc-windows-gnu`, mingw-w64). Der Linux-Bau
braucht die Entwicklungspakete von GTK 3, ALSA, PipeWire und D-Bus.
