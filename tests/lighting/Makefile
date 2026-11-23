# Host test for the lighting state machine. lighting.c and fmc.c are built with their host-test
# switches, which leave out everything that touches the MCU; this directory supplies the QMK
# stand-ins (qmk_stub.h) and the fake flash.
KB := ../../keyboards/ducky/one3/sf_rgb
CFLAGS := -std=c11 -Wall -Wextra -Werror -O1 -g -I$(KB) -I. -DSF_FMC_HOST_TEST -DSF_LIGHTING_HOST_TEST

.PHONY: all clean
all: test_lighting_state
	./test_lighting_state

test_lighting_state: test_lighting_state.c $(KB)/lighting.c $(KB)/fmc.c $(KB)/lighting.h $(KB)/fmc.h qmk_stub.h
	$(CC) $(CFLAGS) -o $@ test_lighting_state.c $(KB)/lighting.c $(KB)/fmc.c

clean:
	rm -f test_lighting_state
