# Ducky One 3 SF RGB (ISO)

65 % ISO, 68 keys, NUC1261SG4AE. Matrix, LED chain and key table were derived from the shipped
firmware; the derivation and its evidence are in `docs/hardware-sf.md` of the project repo that
carries this port.

* Keyboard Maintainer: Liohtml (@Liohtml)
* Hardware Supported: Ducky One 3 SF RGB, ISO layout, shipped firmware `DK-V1.16-251110`
* Hardware Availability: <https://ducky.global/products/one-3-rgb>

Make example for this keyboard (after setting up your build environment):

    make ducky/one3/sf_rgb:default

## Flashing

Put the board into its LDROM bootloader, then write the image with the project's wrapper:

    tools/flash.sh .build/ducky_one3_sf_rgb_default.bin

Bootloader entry: `Fn`+`Esc` from this firmware, or hold `D`+`L` while plugging the board in. The
`D`+`L` route works regardless of what is in the application flash, which is why `CONFIG0`/`CONFIG1`
must never be written: they hold the boot select that keeps that route open.

## Restoring the shipped firmware

    tools/flash.sh build/One_3_SF_RGB_V1.16.bin

See [docs/bauen-und-flashen.md](../../../../docs/bauen-und-flashen.md).
