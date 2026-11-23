// Copyright 2026 Liohtml (@Liohtml)
// SPDX-License-Identifier: GPL-2.0-or-later
// See host_rgb.h for the design and docs/protokoll.md for the wire format.
#include "host_rgb.h"

#include <string.h>

#include "lighting.h"

static sf_rgb_t staging[SF_LED_COUNT]; // the frame being assembled from chunks
static sf_rgb_t queued[SF_LED_COUNT];  // committed, waiting for sf_host_tick to hand it over
static bool     pending;               // a committed frame is waiting
static uint8_t  staged_seq;            // sequence the staging buffer belongs to
static bool     staged_valid;          // a chunk was refused: the whole frame is void
static uint8_t  last_seq;              // last sequence accepted by a commit
static bool     have_last_seq;
static uint32_t last_commit_ms;
static bool     host_painting; // we put the lighting layer into host mode and must undo that

void sf_host_init(void) {
    memset(staging, 0, sizeof(staging));
    memset(queued, 0, sizeof(queued));
    pending        = false;
    staged_seq     = 0;
    staged_valid   = false;
    last_seq       = 0;
    have_last_seq  = false;
    last_commit_ms = sf_host_now();
    host_painting  = false;
}

static void reply(uint8_t cmd, uint8_t status, const uint8_t *extra, uint8_t extra_len) {
    uint8_t out[SF_HOST_REPORT_SIZE];
    memset(out, 0, sizeof(out));
    out[0] = (uint8_t)(cmd | 0x80);
    out[1] = status;
    if (extra && extra_len) {
        if (extra_len > SF_HOST_REPORT_SIZE - 2) extra_len = SF_HOST_REPORT_SIZE - 2;
        memcpy(out + 2, extra, extra_len);
    }
    sf_host_send(out, sizeof(out));
}

static void cmd_get_info(void) {
    static const char id[] = "SF-RGB"; // firmware tag, so a host tool can tell what it is talking to
    uint8_t           extra[SF_HOST_REPORT_SIZE - 2];
    memset(extra, 0, sizeof(extra));
    extra[0] = SF_HOST_PROTO_VERSION;
    extra[1] = SF_HOST_ROWS;
    extra[2] = SF_HOST_COLS;
    extra[3] = SF_LED_COUNT;
    extra[4] = SF_HOST_LEDS_PER_CHUNK;
    extra[5] = SF_HOST_QUEUE;
    memcpy(extra + 6, id, sizeof(id));
    reply(SF_CMD_GET_INFO, SF_OK, extra, (uint8_t)(6 + sizeof(id)));
}

// A chunk is not answered: at 9 chunks per frame and 30+ frames a second, a reply per chunk would
// cost more USB traffic than the frame itself. Errors surface at the commit.
static void cmd_frame_chunk(const uint8_t *p, uint8_t len) {
    uint8_t seq   = p[0];
    uint8_t first = p[3];
    uint8_t leds  = p[4];

    if (!staged_valid || seq != staged_seq) { // a new frame starts here
        staged_seq   = seq;
        staged_valid = true;
    }
    if (leds > SF_HOST_LEDS_PER_CHUNK || first >= SF_LED_COUNT || first + leds > SF_LED_COUNT ||
        (uint16_t)(5 + leds * 3) > len) {
        staged_valid = false; // the commit will say SF_BAD_ARGUMENT
        return;
    }
    for (uint8_t i = 0; i < leds; i++) {
        staging[first + i].r = p[5 + i * 3];
        staging[first + i].g = p[6 + i * 3];
        staging[first + i].b = p[7 + i * 3];
    }
}

static void cmd_frame_commit(const uint8_t *p) {
    uint8_t seq   = p[0];
    uint8_t extra[2];

    // Order matters: a caller that got its arguments or its sequence wrong is told that, even when
    // the queue happens to be full as well -- otherwise it would retry the same broken frame.
    uint8_t status = SF_OK;
    if (!staged_valid || seq != staged_seq)
        status = staged_valid ? SF_BAD_SEQUENCE : SF_BAD_ARGUMENT;
    else if (have_last_seq && (uint8_t)(seq - last_seq) == 0)
        status = SF_BAD_SEQUENCE;
    else if (pending)
        status = SF_BUSY;

    if (status == SF_OK) {
        memcpy(queued, staging, sizeof(queued));
        pending        = true;
        last_seq       = seq;
        have_last_seq  = true;
        last_commit_ms = sf_host_now();
        staged_valid   = false;

        // Show it at once, and keep the slot marked taken until the tick has run: that is what the
        // free-slot count in the reply tells the host, so it can pace itself.
        sf_lighting_host_frame(0, SF_LED_COUNT, queued);
        if (!host_painting) {
            sf_lighting_set_mode(SF_MODE_HOST);
            host_painting = true;
        }
    } else if (status != SF_BUSY) {
        // Busy means "not yet", so the staged frame stays and the host can retry the commit alone
        // rather than resend all nine chunks. The other two mean the frame is void.
        staged_valid = false;
    }

    extra[0] = seq;
    extra[1] = pending ? 0 : SF_HOST_QUEUE; // free slots
    reply(SF_CMD_FRAME_COMMIT, status, extra, 2);
}

static void cmd_set_mode(const uint8_t *p) {
    if (p[0] > SF_MODE_HOST) {
        reply(SF_CMD_SET_MODE, SF_BAD_ARGUMENT, NULL, 0);
        return;
    }
    sf_lighting_set_mode(p[0]);
    host_painting = p[0] == SF_MODE_HOST;
    if (host_painting) last_commit_ms = sf_host_now();
    reply(SF_CMD_SET_MODE, SF_OK, NULL, 0);
}

static void cmd_set_effect(const uint8_t *p) {
    if (p[0] >= SF_EFFECT_COUNT) {
        reply(SF_CMD_SET_EFFECT, SF_BAD_ARGUMENT, NULL, 0);
        return;
    }
    sf_lighting_set_effect(p[0], p[1]);
    sf_lighting_set_brightness(p[2]);
    reply(SF_CMD_SET_EFFECT, SF_OK, NULL, 0);
}

static void cmd_save_state(void) {
    sf_lighting_save();
    reply(SF_CMD_SAVE_STATE, SF_OK, NULL, 0);
}

// What the program on the PC cannot guess and must not overwrite blindly: the brightness its frames
// are scaled by, and which custom layer SET_KEY writes into.
static void cmd_get_state(void) {
    const sf_lighting_state_t *s = sf_lighting_state();
    uint8_t                    extra[5] = {s->mode, s->effect, s->speed, s->brightness, s->layer};
    reply(SF_CMD_GET_STATE, SF_OK, extra, sizeof(extra));
}

static void cmd_set_layer(const uint8_t *p) {
    if (p[0] > SF_LAYER_CM2) { // the host frame is not a layer anyone selects
        reply(SF_CMD_SET_LAYER, SF_BAD_ARGUMENT, NULL, 0);
        return;
    }
    sf_lighting_select_layer(p[0]);
    reply(SF_CMD_SET_LAYER, SF_OK, NULL, 0);
}

static void cmd_set_key(const uint8_t *p) {
    if (p[0] >= SF_LED_COUNT) {
        reply(SF_CMD_SET_KEY, SF_BAD_ARGUMENT, NULL, 0);
        return;
    }
    sf_lighting_paint_key(p[0], p[1], p[2], p[3]);
    reply(SF_CMD_SET_KEY, SF_OK, NULL, 0);
}

void raw_hid_receive(uint8_t *data, uint8_t length) {
    if (length < 2) return;
    const uint8_t *p = data + 1;

    switch (data[0]) {
        case SF_CMD_GET_INFO:
            cmd_get_info();
            break;
        case SF_CMD_FRAME_CHUNK:
            cmd_frame_chunk(p, (uint8_t)(length - 1));
            break;
        case SF_CMD_FRAME_COMMIT:
            cmd_frame_commit(p);
            break;
        case SF_CMD_SET_MODE:
            cmd_set_mode(p);
            break;
        case SF_CMD_SET_EFFECT:
            cmd_set_effect(p);
            break;
        case SF_CMD_SAVE_STATE:
            cmd_save_state();
            break;
        case SF_CMD_SET_KEY:
            cmd_set_key(p);
            break;
        case SF_CMD_GET_STATE:
            cmd_get_state();
            break;
        case SF_CMD_SET_LAYER:
            cmd_set_layer(p);
            break;
        default:
            // Answered, not dropped: a host tool talking to the wrong firmware learns so instead of
            // waiting for a reply that never comes.
            reply(data[0], SF_BAD_ARGUMENT, NULL, 0);
            break;
    }
}

void sf_host_tick(void) {
    if (pending) pending = false; // the frame has been shown; the slot is free again

    if (host_painting && sf_host_now() - last_commit_ms > SF_HOST_TIMEOUT_MS) {
        // The host went quiet -- a crashed program must not leave the keyboard dark.
        sf_lighting_set_mode(SF_MODE_DEVICE);
        host_painting = false;
    }
}

#ifndef SF_HOST_RGB_HOST_TEST

#include "quantum.h"
#include "raw_hid.h"

uint32_t sf_host_now(void) {
    return timer_read32();
}

void sf_host_send(const uint8_t *data, uint8_t len) {
    raw_hid_send((uint8_t *)data, len);
}

#endif /* SF_HOST_RGB_HOST_TEST */
