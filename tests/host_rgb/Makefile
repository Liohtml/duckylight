# Host test for the raw HID lighting protocol. host_rgb.c, lighting.c and fmc.c are built with their
# host-test switches; this directory supplies the QMK stand-ins, the clock and the fake flash.
KB := ../../keyboards/ducky/one3/sf_rgb
CFLAGS := -std=c11 -Wall -Wextra -Werror -O1 -g -I$(KB) -I../lighting \
          -DSF_FMC_HOST_TEST -DSF_LIGHTING_HOST_TEST -DSF_HOST_RGB_HOST_TEST

.PHONY: all clean
all: test_protocol
	./test_protocol

test_protocol: test_protocol.c $(KB)/host_rgb.c $(KB)/lighting.c $(KB)/fmc.c
	$(CC) $(CFLAGS) -o $@ test_protocol.c $(KB)/host_rgb.c $(KB)/lighting.c $(KB)/fmc.c

clean:
	rm -f test_protocol
