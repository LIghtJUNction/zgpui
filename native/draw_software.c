/* CPU rasterizer: one client-owned framebuffer, no GL or worker process. */
#define _DEFAULT_SOURCE 1
#include "zgpui.h"
#include <math.h>
#include <stdlib.h>
#include <string.h>
#include <sys/mman.h>
#include "effects_software.h"

#define CACHE_COUNT 96
#define CACHE_BYTES (2u * 1024u * 1024u)
#define SURFACE_BYTES (256u * 1024u * 1024u)
typedef struct {
    char *text;
    unsigned char *pixels;
    int size, bold, width, height;
    float advance;
    unsigned long stamp;
} Text;
static Text cache[CACHE_COUNT];
static unsigned long clock_tick;
static size_t text_bytes;
static uint32_t *pixels;
static size_t pixel_bytes;
static int frame_ready;
static int screen_width, screen_height;
static int clip_left, clip_top, clip_right, clip_bottom;

int zgpui_software_resize(int width, int height) {
    if (width < 1 || height < 1 || width > 16384 || height > 16384 ||
        (size_t)width > SURFACE_BYTES / sizeof(uint32_t) / (size_t)height)
        return 0;
    if (width == screen_width && height == screen_height && pixels) return 1;
    size_t bytes = (size_t)width * (size_t)height * sizeof(*pixels);
    uint32_t *next = mmap(NULL, bytes, PROT_READ | PROT_WRITE,
                          MAP_PRIVATE | MAP_ANONYMOUS, -1, 0);
    if (next == MAP_FAILED) return 0;
    /* Allocate first: a failed resize leaves the old frame and size intact. */
    if (pixels) munmap(pixels, pixel_bytes);
    pixels = next; pixel_bytes = bytes; frame_ready = 0;
    screen_width = width; screen_height = height;
    zgpui_unclip();
    return 1;
}
uint32_t *zgpui_software_pixels(int *width, int *height) {
    *width = screen_width; *height = screen_height;
    return frame_ready ? pixels : NULL;
}
void zgpui_software_release(void) {
    if (!pixels || !frame_ready) return;
    /* No draw or repeated present may use discarded contents before begin. */
    frame_ready = 0;
    /* Best effort; failure only retains RSS. The next begin still clears all. */
    (void)madvise(pixels, pixel_bytes, MADV_DONTNEED);
}
static float clamp(float value, float low, float high) {
    return value < low ? low : value > high ? high : value;
}
static void blend(int x, int y, uint32_t value, unsigned int coverage) {
    unsigned int alpha = ((value & 255u) * coverage + 127u) / 255u;
    if (!alpha) return;
    uint32_t *dst = &pixels[(size_t)y * (size_t)screen_width + (size_t)x];
    if (alpha == 255u) { *dst = value >> 8; return; }
    unsigned int inverse = 255u - alpha;
    unsigned int r = ((value >> 24) * alpha + ((*dst >> 16) & 255u) * inverse + 127u) / 255u;
    unsigned int g = (((value >> 16) & 255u) * alpha + ((*dst >> 8) & 255u) * inverse + 127u) / 255u;
    unsigned int b = (((value >> 8) & 255u) * alpha + (*dst & 255u) * inverse + 127u) / 255u;
    *dst = (r << 16) | (g << 8) | b;
}
/* Clamp before integer conversion, including hostile/out-of-window geometry. */
static int bounds(float x0, float y0, float x1, float y1, int out[4]) {
    if (!frame_ready || !pixels || !isfinite(x0) || !isfinite(y0) || !isfinite(x1) || !isfinite(y1) ||
        x1 <= x0 || y1 <= y0 || clip_right <= clip_left || clip_bottom <= clip_top)
        return 0;
    out[0] = (int)floorf(clamp(x0, (float)clip_left, (float)clip_right));
    out[1] = (int)floorf(clamp(y0, (float)clip_top, (float)clip_bottom));
    out[2] = (int)ceilf(clamp(x1, (float)clip_left, (float)clip_right));
    out[3] = (int)ceilf(clamp(y1, (float)clip_top, (float)clip_bottom));
    return out[2] > out[0] && out[3] > out[1];
}
void zgpui_begin(int width, int height, uint32_t background) {
    frame_ready = 0;
    if (!zgpui_software_resize(width, height)) return;
    zgpui_unclip();
    size_t count = (size_t)width * (size_t)height;
    for (size_t i = 0; i < count; ++i) pixels[i] = background >> 8;
    frame_ready = 1;
}
/* The fully covered part of a scanline needs no distance-field work. */
static void fill_span(int row, int first, int last, uint32_t color) {
    uint32_t *dst = pixels + (size_t)row * (size_t)screen_width + (size_t)first;
    unsigned int alpha = color & 255u;
    if (alpha == 255u) {
        uint32_t value = color >> 8;
        for (int col = first; col < last; ++col) *dst++ = value;
        return;
    }
    unsigned int inverse = 255u - alpha;
    unsigned int red = (color >> 24) * alpha + 127u;
    unsigned int green = ((color >> 16) & 255u) * alpha + 127u;
    unsigned int blue = ((color >> 8) & 255u) * alpha + 127u;
    for (int col = first; col < last; ++col, ++dst) {
        unsigned int r = (red + ((*dst >> 16) & 255u) * inverse) / 255u;
        unsigned int g = (green + ((*dst >> 8) & 255u) * inverse) / 255u;
        unsigned int b = (blue + (*dst & 255u) * inverse) / 255u;
        *dst = (r << 16) | (g << 8) | b;
    }
}
static unsigned int rect_coverage(float qx, float qy, float radius) {
    float ox = fmaxf(qx, 0), oy = fmaxf(qy, 0);
    float distance = sqrtf(ox * ox + oy * oy) + fminf(fmaxf(qx, qy), 0) - radius;
    return (unsigned int)(clamp(0.5f - distance, 0, 1) * 255.0f + 0.5f);
}
void zgpui_rect(float x, float y, float width, float height, float radius, uint32_t color) {
    int b[4];
    if (!(color & 255u) || !isfinite(radius) || !bounds(x, y, x + width, y + height, b)) return;
    radius = clamp(radius, 0, fminf(width, height) * 0.5f);
    float cx = x + width * 0.5f, cy = y + height * 0.5f;
    float hx = width * 0.5f - radius, hy = height * 0.5f - radius;
    float full = radius - 0.5f;
    for (int row = b[1]; row < b[3]; ++row) {
        float qy = fabsf(row + 0.5f - cy) - hy;
        int first = b[0], last = b[0];
        if (qy <= full) {
            /* In the middle rows the span reaches the straight edges; in the
               corner rows the central rectangle stays entirely inside the arc. */
            float half = hx + (qy <= 0 ? full : fminf(full, 0));
            if (half >= 0) {
                first = (int)ceilf(clamp(cx - half - 0.5f, (float)b[0], (float)b[2]));
                last = (int)floorf(clamp(cx + half - 0.5f, (float)b[0] - 1, (float)b[2] - 1)) + 1;
                if (last < first) last = first;
                /* Keep the original rounding at fractional boundaries. Coverage
                   cannot decrease towards the center of a rounded rectangle. */
                while (first < last && rect_coverage(fabsf(first + 0.5f - cx) - hx, qy, radius) != 255u) ++first;
                while (last > first && rect_coverage(fabsf(last - 0.5f - cx) - hx, qy, radius) != 255u) --last;
            }
        }
        for (int col = b[0]; col < first; ++col)
            blend(col, row, color, rect_coverage(fabsf(col + 0.5f - cx) - hx, qy, radius));
        fill_span(row, first, last, color);
        for (int col = last; col < b[2]; ++col)
            blend(col, row, color, rect_coverage(fabsf(col + 0.5f - cx) - hx, qy, radius));
    }
}
void zgpui_line(float x1, float y1, float x2, float y2, float width, uint32_t color) {
    if (!isfinite(width) || width <= 0) return;
    float dx = x2 - x1, dy = y2 - y1, length2 = dx * dx + dy * dy;
    if (!isfinite(length2) || length2 <= 0) return;
    float radius = width * 0.5f;
    int b[4];
    if (!bounds(fminf(x1, x2) - radius - 1, fminf(y1, y2) - radius - 1,
                fmaxf(x1, x2) + radius + 1, fmaxf(y1, y2) + radius + 1, b)) return;
    for (int row = b[1]; row < b[3]; ++row) {
        for (int col = b[0]; col < b[2]; ++col) {
            float px = col + 0.5f - x1, py = row + 0.5f - y1;
            float t = clamp((px * dx + py * dy) / length2, 0, 1);
            px -= t * dx; py -= t * dy;
            unsigned int coverage = (unsigned int)(clamp(radius + 0.5f - sqrtf(px * px + py * py), 0, 1) * 255.0f + 0.5f);
            blend(col, row, color, coverage);
        }
    }
}
void zgpui_polygon(const float *points, size_t count, uint32_t color) {
    if (!points || count < 3 || count > 4096) return;
    float x0 = points[0], x1 = x0, y0 = points[1], y1 = y0;
    for (size_t i = 0; i < count; ++i) {
        float x = points[i * 2], y = points[i * 2 + 1];
        if (!isfinite(x) || !isfinite(y)) return;
        x0 = fminf(x0, x); x1 = fmaxf(x1, x);
        y0 = fminf(y0, y); y1 = fmaxf(y1, y);
    }
    int b[4];
    if (!bounds(x0, y0, x1, y1, b)) return;
    for (int row = b[1]; row < b[3]; ++row) {
        float left[4], right[4];
        for (int sample = 0; sample < 4; ++sample) {
            float y = row + (sample + 0.5f) / 4.0f;
            left[sample] = (float)b[2]; right[sample] = (float)b[0];
            for (size_t i = 0, previous = count - 1; i < count; previous = i++) {
                float ax = points[previous * 2], ay = points[previous * 2 + 1];
                float bx = points[i * 2], by = points[i * 2 + 1];
                if ((ay <= y && by > y) || (by <= y && ay > y)) {
                    float x = ax + (y - ay) / (by - ay) * (bx - ax);
                    left[sample] = fminf(left[sample], x);
                    right[sample] = fmaxf(right[sample], x);
                }
            }
        }
        for (int col = b[0]; col < b[2]; ++col) {
            float covered = 0;
            for (int sample = 0; sample < 4; ++sample)
                covered += clamp(fminf(col + 1.0f, right[sample]) - fmaxf((float)col, left[sample]), 0, 1);
            blend(col, row, color, (unsigned int)(covered * 63.75f + 0.5f));
        }
    }
}
static void drop(Text *item) {
    if (!item->text) return;
    text_bytes -= (size_t)item->width * (size_t)item->height;
    free(item->text); free(item->pixels);
    memset(item, 0, sizeof(*item));
}
static Text *get_text(const char *text, int size, int bold) {
    Text *slot = &cache[0];
    for (int i = 0; i < CACHE_COUNT; ++i) {
        if (cache[i].text && cache[i].size == size && cache[i].bold == bold && strcmp(cache[i].text, text) == 0) {
            cache[i].stamp = ++clock_tick; return &cache[i];
        }
        if (!cache[i].text || (slot->text && cache[i].stamp < slot->stamp)) slot = &cache[i];
    }
    drop(slot);
    ZgpuiBitmap bitmap = zgpui_rasterize(text, size, bold);
    if (!bitmap.pixels || bitmap.width <= 0 || bitmap.height <= 0 ||
        (size_t)bitmap.width > CACHE_BYTES / (size_t)bitmap.height) {
        free(bitmap.pixels); return NULL;
    }
    size_t bytes = (size_t)bitmap.width * (size_t)bitmap.height;
    while (text_bytes + bytes > CACHE_BYTES) {
        Text *oldest = NULL;
        for (int i = 0; i < CACHE_COUNT; ++i)
            if (cache[i].text && (!oldest || cache[i].stamp < oldest->stamp)) oldest = &cache[i];
        if (!oldest) break;
        drop(oldest);
    }
    size_t len = strlen(text);
    slot->text = malloc(len + 1);
    if (!slot->text) { free(bitmap.pixels); return NULL; }
    memcpy(slot->text, text, len + 1);
    slot->pixels = bitmap.pixels;
    slot->size = size; slot->bold = bold; slot->width = bitmap.width; slot->height = bitmap.height;
    slot->advance = bitmap.advance; slot->stamp = ++clock_tick;
    text_bytes += bytes;
    return slot;
}
float zgpui_text(const char *text, float x, float y, int size, int bold, uint32_t color) {
    if (!text || !*text) return 0;
    Text *item = get_text(text, size, bold);
    if (!item) return 0;
    int b[4];
    if (!isfinite(x) || !isfinite(y) || !bounds(x, y, x + item->width, y + item->height, b)) return item->advance;
    float origin_x = floorf(x), origin_y = floorf(y);
    for (int row = b[1]; row < b[3]; ++row) {
        for (int col = b[0]; col < b[2]; ++col) {
            float sx = col - origin_x, sy = row - origin_y;
            if (sx >= 0 && sy >= 0 && sx < item->width && sy < item->height)
                blend(col, row, color, item->pixels[(size_t)sy * (size_t)item->width + (size_t)sx]);
        }
    }
    return item->advance;
}
float zgpui_measure(const char *text, int size, int bold) {
    if (!text || !*text) return 0;
    /* Probing widths must not allocate masks or evict visible text. */
    for (int i = 0; i < CACHE_COUNT; ++i)
        if (cache[i].text && cache[i].size == size && cache[i].bold == bold &&
            strcmp(cache[i].text, text) == 0) {
            cache[i].stamp = ++clock_tick;
            return cache[i].advance;
        }
    return zgpui_text_advance(text, size, bold);
}
void zgpui_clip(int x, int y, int width, int height) {
    long long right = (long long)x + (width > 0 ? width : 0);
    long long bottom = (long long)y + (height > 0 ? height : 0);
    clip_left = x < 0 ? 0 : x > screen_width ? screen_width : x;
    clip_top = y < 0 ? 0 : y > screen_height ? screen_height : y;
    clip_right = right < 0 ? 0 : right > screen_width ? screen_width : (int)right;
    clip_bottom = bottom < 0 ? 0 : bottom > screen_height ? screen_height : (int)bottom;
}
void zgpui_unclip(void) {
    clip_left = clip_top = 0;
    clip_right = screen_width; clip_bottom = screen_height;
}
void zgpui_clear_text_cache(void) {
    for (int i = 0; i < CACHE_COUNT; ++i) drop(&cache[i]);
    clock_tick = 0;
}
void zgpui_gpu_destroy(void) {
    software_effect_destroy();
    zgpui_clear_text_cache();
    if (pixels) munmap(pixels, pixel_bytes);
    pixels = NULL; pixel_bytes = 0; frame_ready = 0;
    screen_width = screen_height = 0;
    clip_left = clip_top = clip_right = clip_bottom = 0;
    clock_tick = 0;
}
const char *zgpui_gpu_info(void) { return "software (CPU framebuffer + X11)"; }
