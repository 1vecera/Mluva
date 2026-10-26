/* A monotonic clock the showreel capture steps itself, for frame-exact takes of GTK animations.
 *
 * Preloaded into the capture driver only. Until the driver calls mluva_take_clock_set(), every
 * clock passes through. Afterwards CLOCK_MONOTONIC and its variants return the time the driver
 * set, so GLib, GDK's frame clock and libadwaita's animations advance exactly one frame per step,
 * however long the software renderer takes to paint it. Wall-clock time is untouched.
 *
 * Build: cc -O2 -shared -fPIC -o take_clock.so take_clock.c
 */
#define _GNU_SOURCE
#include <dlfcn.h>
#include <stdatomic.h>
#include <stdint.h>
#include <time.h>

static int (*real_clock_gettime)(clockid_t, struct timespec *);
static _Atomic int64_t virtual_ns = -1;

void mluva_take_clock_set(int64_t ns) { atomic_store(&virtual_ns, ns); }

int clock_gettime(clockid_t id, struct timespec *ts) {
    if (!real_clock_gettime) real_clock_gettime = dlsym(RTLD_NEXT, "clock_gettime");
    int64_t ns = atomic_load(&virtual_ns);
    if (ns >= 0 && (id == CLOCK_MONOTONIC || id == CLOCK_MONOTONIC_RAW || id == CLOCK_MONOTONIC_COARSE ||
                    id == CLOCK_BOOTTIME)) {
        ts->tv_sec = ns / 1000000000;
        ts->tv_nsec = ns % 1000000000;
        return 0;
    }
    return real_clock_gettime(id, ts);
}
