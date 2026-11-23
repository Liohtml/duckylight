// Copyright 2026 Liohtml (@Liohtml)
// SPDX-License-Identifier: GPL-2.0-or-later
// The LED count on its own, so the settings store can size its per-key table without pulling in
// led_map.h (whose led_hw[] table would be an unused static in every other translation unit).
#pragma once

#define SF_LED_COUNT 68

// LED indices of the two keys that double as indicators. The SF has no separate indicator LEDs
// (docs/hardware-sf.md), so caps lock shows on the Caps key and the Windows lock on the Win key.
#define SF_LED_CAPS 29
#define SF_LED_GUI 59
