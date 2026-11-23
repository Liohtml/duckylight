// Copyright 2026 Liohtml (@Liohtml)
// SPDX-License-Identifier: GPL-2.0-or-later
// Just enough of QMK to exercise the lighting state machine on the host: the record type the
// keycode handler takes, the few keycodes the tests use, and no-op versions of the RGB Matrix calls
// lighting.c makes. What this file cannot test is whether the right QMK effect comes out -- that is
// what the device check in the plan's task 5 step 6 is for.
#pragma once

#include <stdbool.h>
#include <stdint.h>

#define KC_A 0x0004
#define KC_NO 0x0000

typedef struct {
    struct {
        bool pressed;
    } event;
} keyrecord_t;

#define SF_KEYCODE_BASE 0x7E00u

// RGB Matrix stand-ins. The test only cares that lighting.c keeps its own state consistent.
enum {
    RGB_MATRIX_NONE = 0,
    RGB_MATRIX_SOLID_COLOR,
    RGB_MATRIX_BREATHING,
    RGB_MATRIX_CYCLE_ALL,
    RGB_MATRIX_CYCLE_LEFT_RIGHT,
    RGB_MATRIX_CYCLE_OUT_IN,
    RGB_MATRIX_DUAL_BEACON,
    RGB_MATRIX_RAINDROPS,
    RGB_MATRIX_TYPING_HEATMAP,
    RGB_MATRIX_SOLID_REACTIVE,
    RGB_MATRIX_SOLID_REACTIVE_SIMPLE,
    RGB_MATRIX_SOLID_REACTIVE_MULTIWIDE,
    RGB_MATRIX_SPLASH,
    RGB_MATRIX_MULTISPLASH,
};

void     rgb_matrix_mode_noeeprom(uint8_t mode);
void     rgb_matrix_set_speed_noeeprom(uint8_t speed);
void     rgb_matrix_sethsv_noeeprom(uint8_t h, uint8_t s, uint8_t v);
void     rgb_matrix_set_color(int index, uint8_t r, uint8_t g, uint8_t b);
uint8_t  rgb_matrix_get_mode(void);
uint8_t  rgb_matrix_get_speed(void);
uint8_t  rgb_matrix_get_val(void);
