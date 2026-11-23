// Copyright 2026 Liohtml (@Liohtml)
// SPDX-License-Identifier: GPL-2.0-or-later

#pragma once

/* NUC126 USBD gives every logical endpoint its own IN and OUT hardware endpoint
   (hal_usb_lld.c _HW_IN/OUT_EPN), so raw HID can share one endpoint number for both
   directions: keeps us within USB_MAX_ENDPOINTS = 3. */
#define USB_ENDPOINTS_ARE_REORDERABLE

/* QMK's default row-select settle time (CPU_CLOCK / 4 MHz); chibios_config.h defines no
   CPU_CLOCK for NUMICRO. */
#define GPIO_INPUT_PIN_DELAY (NUC126_HCLK / 1000000L / 4)
