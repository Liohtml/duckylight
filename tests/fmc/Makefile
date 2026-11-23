# Host test for the slot logic of the SF settings store. fmc.c is built with SF_FMC_HOST_TEST, which
# leaves out everything that touches the MCU's flash controller; the test supplies the three
# accessors itself.
KB := ../../keyboards/ducky/one3/sf_rgb
CFLAGS := -std=c11 -Wall -Wextra -Werror -O1 -g -I$(KB) -DSF_FMC_HOST_TEST

.PHONY: all clean
all: test_fmc_slots
	./test_fmc_slots

test_fmc_slots: test_fmc_slots.c $(KB)/fmc.c $(KB)/fmc.h
	$(CC) $(CFLAGS) -o $@ test_fmc_slots.c $(KB)/fmc.c

clean:
	rm -f test_fmc_slots
