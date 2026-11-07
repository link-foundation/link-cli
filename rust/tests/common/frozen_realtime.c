// Test-only frozen wall clock. Monotonic clocks retain real subprocess deadlines.
#define _GNU_SOURCE
#include <dlfcn.h>
#include <time.h>

int clock_gettime(clockid_t clock, struct timespec *value) {
    if (clock == CLOCK_REALTIME) {
        value->tv_sec = 1730000000;
        value->tv_nsec = 0;
        return 0;
    }
    int (*real_clock_gettime)(clockid_t, struct timespec *) =
        dlsym(RTLD_NEXT, "clock_gettime");
    return real_clock_gettime(clock, value);
}
