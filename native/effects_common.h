#ifndef ZGPUI_EFFECTS_COMMON_H
#define ZGPUI_EFFECTS_COMMON_H
#include <math.h>
#include <stdio.h>
#include <string.h>

static int fx_finite_range(float value, float minimum, float maximum) {
    return isfinite(value) && value >= minimum && value <= maximum;
}
static int fx_capture_valid(const ZgpuiEffectCapture *capture, int cached) {
    if (!capture || !capture->id) return 0;
    if (cached && capture->width == 0 && capture->height == 0) return 1;
    return fx_finite_range(capture->x, -32768, 32768) &&
        fx_finite_range(capture->y, -32768, 32768) &&
        fx_finite_range(capture->width, 0.01f, 16384) &&
        fx_finite_range(capture->height, 0.01f, 16384);
}
static int fx_effect_valid(const ZgpuiEffect *effect) {
    if (!effect || (effect->flags & ~15u) ||
        !fx_capture_valid(&effect->source, effect->flags & ZGPUI_EFFECT_CACHE_ONLY)) return 0;
    if (effect->flags & ZGPUI_EFFECT_CAPTURE_ONLY) return 1;
    if (!fx_finite_range(effect->x, -32768, 32768) ||
        !fx_finite_range(effect->y, -32768, 32768) ||
        !fx_finite_range(effect->width, 0.01f, 16384) ||
        !fx_finite_range(effect->height, 0.01f, 16384) ||
        !fx_finite_range(effect->radius, 0, 4096) ||
        !fx_finite_range(effect->blur_radius, 0, 64) ||
        !fx_finite_range(effect->saturation, 0, 2) ||
        !fx_finite_range(effect->opacity, 0, 1) ||
        !fx_finite_range(effect->tint_mix, 0, 1) ||
        !fx_finite_range(effect->noise, 0, 1) ||
        !fx_finite_range(effect->refraction, 0, 32) ||
        !fx_finite_range(effect->border, 0, 4) ||
        !fx_finite_range(effect->highlight, 0, 1) ||
        !fx_finite_range(effect->progressive_blur, -1, 1) ||
        !fx_finite_range(effect->fold_progress, 0, 1) ||
        !fx_finite_range(effect->fold_radians, -1.5f, 1.5f) ||
        !fx_finite_range(effect->fold_perspective, 64, 65536) ||
        !fx_finite_range(effect->dim, 0, 1) ||
        !fx_finite_range(effect->fold_anchor, 0, 1) || effect->fold_axis > 2 ||
        effect->preset > ZGPUI_MATERIAL_CUSTOM ||
        effect->shader_slot > ZGPUI_EFFECT_MAX_SHADERS) return 0;
    for (size_t i = 0; i < 8; ++i)
        if (!fx_finite_range(effect->shader_params[i], -65536, 65536)) return 0;
    return 1;
}
static int fx_config_valid(const ZgpuiEffectConfig *config) {
    return config && config->abi_version == ZGPUI_EFFECT_ABI &&
        config->quality <= 3 && config->max_entries >= 1 &&
        config->max_entries <= ZGPUI_EFFECT_MAX_ENTRIES &&
        config->max_shader_slots <= ZGPUI_EFFECT_MAX_SHADERS &&
        config->max_texture_bytes <= ZGPUI_EFFECT_MAX_BYTES;
}
static ZgpuiEffectConfig fx_default_config(void) {
    ZgpuiEffectConfig config = {ZGPUI_EFFECT_ABI, 2, 16, 8, 32u * 1024u * 1024u};
    return config;
}
static uint32_t fx_fallback_color(const ZgpuiEffect *effect) {
    uint32_t color = effect->tint_rgba;
    float dim = 1 - effect->dim;
    unsigned int r = (unsigned int)((color >> 24) * dim);
    unsigned int g = (unsigned int)(((color >> 16) & 255u) * dim);
    unsigned int b = (unsigned int)(((color >> 8) & 255u) * dim);
    unsigned int a = (unsigned int)((color & 255u) * effect->tint_mix * effect->opacity);
    return (r << 24) | (g << 16) | (b << 8) | a;
}
const char *zgpui_effect_status(int status) {
    switch (status) {
        case ZGPUI_EFFECT_OK: return "Effect operation completed";
        case ZGPUI_EFFECT_FALLBACK: return "Solid tint fallback; requested GPU material is unavailable or disabled";
        case ZGPUI_EFFECT_BUDGET: return "Effect resource budget exceeded";
        case ZGPUI_EFFECT_INVALID: return "Invalid effect parameters or renderer owner";
        case ZGPUI_EFFECT_UNSUPPORTED: return "Effect operation is unsupported by this renderer";
        case ZGPUI_EFFECT_SHADER_ERROR: return "Effect shader compilation or linking failed";
        case ZGPUI_EFFECT_CACHE_MISS: return "Captured effect content is missing or was evicted; capture it again";
        case ZGPUI_EFFECT_ALLOCATION: return "GPU effect resource creation or capture failed";
        default: return "Unknown effect status";
    }
}
#endif
