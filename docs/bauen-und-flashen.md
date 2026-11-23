# Bauen und Flashen

## Werkzeuge

| Werkzeug | Version / Commit | Ort |
|---|---|---|
| ARM-Übersetzer | xPack GNU Arm Embedded GCC 15.2.1 (20251203) | `~/.local/xpack-arm-none-eabi-gcc` |
| QMK-CLI | 1.2.0 | `~/.local/qmk-venv` |
| QMK-Baum | `a209d1ce` (Zweig `develop`, flacher Klon) | `firmware/` |
| ChibiOS | `6170ddf9` (QMK-Submodul) | `firmware/lib/chibios` |
| **ChibiOS-Contrib** | **`37c06f92`** — Merge von PR #454, der den NUC126-HAL bringt | `firmware/lib/chibios-contrib` |
| LUFA | `549b9732` (QMK-Submodul) | `firmware/lib/lufa` |
| printf | `c2e3b4e1` (QMK-Submodul) | `firmware/lib/printf` |

`firmware/` ist ein Klon, kein Submodul, und bewusst nicht mitversioniert (`.gitignore`). Der
NUC126-HAL steckt nicht in QMKs eigenem ChibiOS-Contrib-Fork, sondern nur im Upstream. Er kommt
deshalb als losgelöster Checkout von `37c06f92` aus `github.com/ChibiOS/ChibiOS-Contrib` (Standardzweig
`chibios-21.11.x`, **nicht** `master`). GitHub liefert einen nackten SHA nicht direkt aus, darum:

```bash
cd firmware/lib/chibios-contrib
git remote add upstream https://github.com/ChibiOS/ChibiOS-Contrib.git
git fetch --depth 300 upstream chibios-21.11.x
git checkout --detach 37c06f9
```

Der Referenzport des Full-Size-Modells liegt als `firmware/keyboards/ducky/one3/rgb` im Baum
(QMK-PR #26499, als Patch in `build/pr26499.diff`). Er ist die Vorlage für Boarddateien,
Linkerskript und später den MBI5042-Treiber — und die unabhängige Gegenprobe für die aus dem
Werksabbild hergeleitete Hardwarezuordnung.

## Wo der Port liegt

Die Quelle des Ports liegt im Projektrepo unter `keyboards/ducky/one3/sf_rgb/` und ist von dort als
Symlink in den Klon gehängt:

```bash
ln -s ../../../../keyboards/ducky/one3/sf_rgb firmware/keyboards/ducky/one3/sf_rgb
```

Grund: `firmware/` ist ein eigener Klon und ignoriert; was darin liegt, wäre nicht versioniert. So
baut QMK unverändert gegen den Pfad, den es erwartet, und die Dateien hängen trotzdem an unserer
Historie.

## Bauen

```bash
export PATH="$HOME/.local/xpack-arm-none-eabi-gcc/bin:$HOME/.local/qmk-venv/bin:$PATH"
cd firmware
qmk lint -kb ducky/one3/sf_rgb
qmk compile -kb ducky/one3/sf_rgb -km default
```

Ergebnis: `firmware/.build/ducky_one3_sf_rgb_default.bin`.

## Flashen

USB hängt am Windows-Rechner, nicht in WSL2. `tools/flash.sh` kopiert das Abbild in das
Windows-Arbeitsverzeichnis und startet `isp.py` dort über `powershell.exe`. Ohne `--yes` läuft nur
der Trockenlauf.

```bash
tools/flash.sh firmware/.build/ducky_one3_sf_rgb_default.bin          # nur prüfen
tools/flash.sh firmware/.build/ducky_one3_sf_rgb_default.bin --yes    # schreiben
```

In den Bootloader kommt die Tastatur über `Fn`+`Esc` (eigene Firmware) oder indem man `D` und `L`
gedrückt hält und dabei einsteckt. Der zweite Weg hängt nicht an der Firmware im Anwendungsflash und
ist der Rettungsweg.

## Werksstand zurückspielen

```bash
tools/flash.sh build/One_3_SF_RGB_V1.16.bin --yes
```

Danach meldet `isp.py version` wieder `DK-V1.16-251110`. Das Werksabbild liegt lokal und wird nicht
veröffentlicht.

## Zwei Verbote

* **LDROM nie schreiben.** Dort sitzt der Bootloader, der den Rettungsweg trägt.
* **CONFIG0 und CONFIG1 nie schreiben.** `CONFIG0 = 0xF7FFFF7C` startet immer aus dem LDROM, und
  genau das hält `D`+`L` am Leben. Darum steht in `mcuconf.h` `NUC126_CONFIG_ENABLED FALSE`.
