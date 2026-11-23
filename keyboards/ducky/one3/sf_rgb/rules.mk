MCU_FAMILY = NUMICRO
MCU_SERIES = NUC126
# NUC1261SG4AE: peripheral-identical to NUC126SG4AE (vendor headers diffed)
MCU_LDSCRIPT = NUC1261SG4AE
MCU_STARTUP = NUC126
BOARD = NUC1261SG4AE
MCU = cortex-m0
ARMV = 6

SRC += mbi5042.c fmc.c lighting.c host_rgb.c

# Our own slot store in fmc.c keeps the lighting settings; QMK's own EEPROM is not used.
EEPROM_DRIVER = transient

# Flashing goes through tools/flash.sh, never through `qmk flash`: the board must be in its
# LDROM bootloader (Fn+Esc or D+L at plug-in) and only our own ISP wrapper checks the image
# against the data-flash base before writing.

# The lighting protocol for the program on the PC (docs/protokoll.md). Raw HID adds its own
# interface, which is why config.h declares the endpoints reorderable.
RAW_ENABLE = yes
