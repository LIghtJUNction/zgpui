/* Private bridge to the one shared font implementation. No window API. */
#include "font_linux.h"

FontState *zgpui_raster_font_create(const char *family) {
    if (!family) return NULL;
    size_t length = 0;
    while (length < 256 && family[length]) {
        unsigned char c = (unsigned char)family[length++];
        if (c < 32 || c == 127) return NULL;
    }
    if (length > 255) return NULL;
    FontState *state = calloc(1, sizeof(*state));
    if (!state) return NULL;
    memcpy(state->family, family, length + 1);
    if (!face_for_state(state, 'A', 16, 0) ||
        !face_for_state(state, 'A', 16, 1)) {
        font_state_destroy(state);
        free(state);
        return NULL;
    }
    return state;
}

void zgpui_raster_font_destroy(FontState *state) {
    if (!state) return;
    font_state_destroy(state);
    free(state);
}

float zgpui_raster_font_measure(FontState *state, const char *text, int size, int bold) {
    return font_text_advance(state, text, size, bold);
}

ZgpuiBitmap zgpui_raster_font_mask(FontState *state, const char *text, int size, int bold) {
    return font_rasterize(state, text, size, bold);
}

void zgpui_raster_mask_destroy(unsigned char *pixels) {
    free(pixels);
}
