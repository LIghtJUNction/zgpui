#ifndef ZGPUI_EFFECTS_SOFTWARE_H
#define ZGPUI_EFFECTS_SOFTWARE_H
#include "effects_common.h"

static ZgpuiEffectConfig software_effect_config;
static ZgpuiEffectStats software_effect_stats;
static ZgpuiWindow *software_effect_owner;
static int software_effect_ready(ZgpuiWindow *window) {
    if (!window || (software_effect_owner && software_effect_owner != window)) return 0;
    if (!software_effect_owner) {
        software_effect_owner = window;
        software_effect_config = fx_default_config();
    }
    return 1;
}
static int software_effect_result(int status) {
    software_effect_stats.last_status = (uint32_t)status;
    return status;
}
static void software_effect_destroy(void) {
    memset(&software_effect_config, 0, sizeof(software_effect_config));
    memset(&software_effect_stats, 0, sizeof(software_effect_stats));
    software_effect_owner = NULL;
}
int zgpui_effect_configure(ZgpuiWindow *window, const ZgpuiEffectConfig *config) {
    if (!software_effect_ready(window) || !fx_config_valid(config))
        return software_effect_result(ZGPUI_EFFECT_INVALID);
    software_effect_config = *config;
    return software_effect_result(ZGPUI_EFFECT_OK);
}
int zgpui_effect_capabilities(ZgpuiWindow *window, ZgpuiEffectCapabilities *out) {
    if (!software_effect_ready(window) || !out) return software_effect_result(ZGPUI_EFFECT_INVALID);
    memset(out, 0, sizeof(*out));
    out->abi_version = ZGPUI_EFFECT_ABI;
    out->flags = ZGPUI_EFFECT_SOFTWARE;
    return software_effect_result(ZGPUI_EFFECT_OK);
}
int zgpui_effect_stats(ZgpuiWindow *window, ZgpuiEffectStats *out) {
    if (!software_effect_ready(window) || !out) return software_effect_result(ZGPUI_EFFECT_INVALID);
    software_effect_stats.quality = software_effect_config.quality;
    *out = software_effect_stats;
    return ZGPUI_EFFECT_OK;
}
int zgpui_draw_effect(ZgpuiWindow *window, const ZgpuiEffect *effect) {
    if (!software_effect_ready(window) || !fx_effect_valid(effect))
        return software_effect_result(ZGPUI_EFFECT_INVALID);
    if (effect->flags & (ZGPUI_EFFECT_CAPTURE_ONLY | ZGPUI_EFFECT_CACHE_ONLY | ZGPUI_EFFECT_LAYER))
        return software_effect_result(ZGPUI_EFFECT_UNSUPPORTED);
    zgpui_rect(effect->x, effect->y, effect->width, effect->height, effect->radius,
               fx_fallback_color(effect));
    software_effect_stats.draw_calls++;
    software_effect_stats.fallbacks++;
    return software_effect_result(ZGPUI_EFFECT_FALLBACK);
}
int zgpui_effect_capture(ZgpuiWindow *window, const ZgpuiEffectCapture *capture) {
    return software_effect_result(software_effect_ready(window) && fx_capture_valid(capture, 0)
        ? ZGPUI_EFFECT_UNSUPPORTED : ZGPUI_EFFECT_INVALID);
}
int zgpui_effect_invalidate(ZgpuiWindow *window, uint64_t id) {
    return software_effect_result(software_effect_ready(window) && id
        ? ZGPUI_EFFECT_OK : ZGPUI_EFFECT_INVALID);
}
int zgpui_effect_clear(ZgpuiWindow *window) {
    return software_effect_result(software_effect_ready(window) ? ZGPUI_EFFECT_OK : ZGPUI_EFFECT_INVALID);
}
int zgpui_effect_shader_register(ZgpuiWindow *window, const char *source, size_t length,
                                 uint32_t *slot, char *error, size_t capacity) {
    (void)source; (void)length;
    if (slot) *slot = 0;
    if (error && capacity) snprintf(error, capacity, "Custom GPU shaders are unavailable in the CPU renderer.");
    return software_effect_result(software_effect_ready(window) ? ZGPUI_EFFECT_UNSUPPORTED : ZGPUI_EFFECT_INVALID);
}
int zgpui_effect_shader_remove(ZgpuiWindow *window, uint32_t slot) {
    (void)slot;
    return software_effect_result(software_effect_ready(window) ? ZGPUI_EFFECT_UNSUPPORTED : ZGPUI_EFFECT_INVALID);
}
int zgpui_effect_finish(ZgpuiWindow *window) {
    /* CPU drawing is already synchronous; there is no GPU queue to wait for. */
    return software_effect_result(software_effect_ready(window) ? ZGPUI_EFFECT_OK : ZGPUI_EFFECT_INVALID);
}
#endif
