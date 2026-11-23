#!/usr/bin/env bash
# Write a firmware image to the Ducky One 3 SF RGB. The only sanctioned flash path for this project.
#
# USB lives on the Windows host, not in WSL2, so the ISP tool runs on the Windows side: the image is
# copied into the Windows working directory and isp.py is started there through powershell.exe.
# Every write is preceded by the tool's own dry run, which checks the vector table, the model, that
# CONFIG0 still boots the LDROM, and that the image ends below the data flash base.
#
# Never writes LDROM and never writes CONFIG0/CONFIG1: the D+L recovery path must stay intact.
set -euo pipefail

# Where the ISP tool lives on the Windows side, and which Python runs it there. Clone
# https://github.com/mdmower/ducky-one3-isp into that directory first.
#   DUCKY_ISP_WIN_DIR   for example C:\Users\you\ducky-one3-isp
#   DUCKY_ISP_PYTHON    defaults to "python"
if [ -z "${DUCKY_ISP_WIN_DIR:-}" ]; then
  echo "DUCKY_ISP_WIN_DIR ist nicht gesetzt: das Verzeichnis des ISP-Werkzeugs unter Windows." >&2
  exit 2
fi
WIN_DIR="$DUCKY_ISP_WIN_DIR"
WIN_DIR_WSL="$(wslpath -u "$WIN_DIR")"
PY="${DUCKY_ISP_PYTHON:-python}"

usage() { echo "Aufruf: tools/flash.sh <image.bin> [--yes]" >&2; exit 2; }

# The LDROM returns to the application flash on its own when no ISP command follows, so Fn+Esc has
# to be pressed while we are already waiting. isp.py does that waiting on the Windows side.
WAIT=60
[ $# -ge 1 ] || usage
IMAGE="$1"; shift
CONFIRM="no"
[ "${1:-}" = "--yes" ] && CONFIRM="yes"
[ -f "$IMAGE" ] || { echo "Abbild nicht gefunden: $IMAGE" >&2; exit 1; }

BASE="$(basename "$IMAGE")"
install -m 644 "$IMAGE" "$WIN_DIR_WSL/firmware/$BASE"
echo "Abbild kopiert: $WIN_DIR\\firmware\\$BASE  ($(stat -c%s "$IMAGE") Byte, SHA-256 $(sha256sum "$IMAGE" | cut -c1-16)...)"

run_isp() {
  powershell.exe -NoProfile -ExecutionPolicy Bypass -Command \
    "Set-Location '$WIN_DIR'; & '$PY' -I tools\\isp.py $*" 2>&1 | tr -d '\r'
}

echo "== Warte bis zu ${WAIT}s auf den Bootloader (Fn+Esc druecken, oder D+L beim Einstecken) =="
echo "== Trockenlauf =="
run_isp "flash firmware\\$BASE --no-run --wait $WAIT" | tee /tmp/ducky-flash-dry.$$
if ! grep -q "checks passed" /tmp/ducky-flash-dry.$$; then
  rm -f /tmp/ducky-flash-dry.$$
  cat >&2 <<'MSG'

Trockenlauf nicht bestanden. Haeufigster Grund: Die Tastatur ist nicht im Bootloader.
Hineinkommen: Fn+Esc aus eigener Firmware, oder D und L gedrueckt halten und dabei einstecken.
MSG
  exit 1
fi
rm -f /tmp/ducky-flash-dry.$$

if [ "$CONFIRM" != "yes" ]; then
  echo
  echo "Trockenlauf bestanden. Zum Schreiben erneut mit --yes aufrufen."
  exit 0
fi

echo "== Schreiben =="
run_isp "flash firmware\\$BASE --yes --wait $WAIT"
echo "== Version nach dem Schreiben =="
run_isp "version" || true
