#ifndef ZGPUI_IMAGE_H
#define ZGPUI_IMAGE_H
#include <math.h>
#include <stddef.h>
#include <stdint.h>

/* Defensive C ABI validation shared by both native renderers. Rust validates
   before crossing the FFI boundary; C fixtures and internal callers also use it. */
static int image_valid(float x, float y, float width, float height,
                       uint32_t image_width, uint32_t image_height,
                       const unsigned char *rgba, size_t byte_len) {
    if (!rgba || !image_width || !image_height ||
        image_width > SIZE_MAX / image_height ||
        (size_t)image_width * image_height > SIZE_MAX / 4)
        return 0;
    size_t expected = (size_t)image_width * image_height * 4;
    return image_width <= 16384 && image_height <= 16384 &&
           expected <= 256u * 1024u * 1024u && byte_len == expected &&
           isfinite(x) && isfinite(y) && isfinite(width) && isfinite(height) &&
           width > 0 && height > 0 && isfinite(x + width) && isfinite(y + height) &&
           x + width > x && y + height > y;
}
#endif
