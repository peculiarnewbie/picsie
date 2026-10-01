/* External benchmark observer only; this is not editor or rendering code. */
#include <stddef.h>
#include <stdint.h>
#include <string.h>

int picsie_screen_matches(const uint8_t *a, const uint8_t *b,
                          size_t length, unsigned int tolerance) {
    if (tolerance == 0) return memcmp(a, b, length) == 0;
    uint8_t mismatch = 0;
    for (size_t i = 0; i < length; i++) {
        uint8_t delta = a[i] > b[i] ? a[i] - b[i] : b[i] - a[i];
        mismatch |= delta > tolerance;
    }
    return mismatch == 0;
}
