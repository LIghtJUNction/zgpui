#ifndef ZGPUI_EFFECTS_H
#define ZGPUI_EFFECTS_H

/* Versioned additive ABI. All rectangles use top-left logical framebuffer
   coordinates. These are in-window renderer effects, never OS compositor Mica. */
#define ZGPUI_EFFECT_ABI 1u
#define ZGPUI_EFFECT_MAX_ENTRIES 16u
#define ZGPUI_EFFECT_MAX_SHADERS 8u
#define ZGPUI_EFFECT_MAX_SHADER_BYTES 16384u
#define ZGPUI_EFFECT_MAX_BYTES (256u * 1024u * 1024u)

enum {
    ZGPUI_EFFECT_OK = 0, ZGPUI_EFFECT_FALLBACK = 1,
    ZGPUI_EFFECT_BUDGET = 2, ZGPUI_EFFECT_INVALID = 3,
    ZGPUI_EFFECT_UNSUPPORTED = 4, ZGPUI_EFFECT_SHADER_ERROR = 5,
    ZGPUI_EFFECT_CACHE_MISS = 6, ZGPUI_EFFECT_ALLOCATION = 7
};
enum {
    ZGPUI_EFFECT_GPU = 1u, ZGPUI_EFFECT_CAPTURE = 2u,
    ZGPUI_EFFECT_BLUR = 4u, ZGPUI_EFFECT_CUSTOM_SHADER = 8u,
    ZGPUI_EFFECT_PROJECTIVE_FOLD = 16u, ZGPUI_EFFECT_SOFTWARE = 32u
};
enum {
    ZGPUI_EFFECT_LAYER = 1u,       /* UV attached to captured complete content. */
    ZGPUI_EFFECT_CACHE_ONLY = 2u,  /* Never capture; missing entry is an error. */
    ZGPUI_EFFECT_REFRESH = 4u,     /* Explicitly capture this revision again. */
    ZGPUI_EFFECT_CAPTURE_ONLY = 8u
};
enum {
    ZGPUI_MATERIAL_FROSTED = 0u, ZGPUI_MATERIAL_MICA = 1u,
    ZGPUI_MATERIAL_LIQUID = 2u, ZGPUI_MATERIAL_PAPER = 3u,
    ZGPUI_MATERIAL_CUSTOM = 4u
};

typedef struct {
    uint32_t abi_version, quality, max_entries, max_shader_slots;
    uint64_t max_texture_bytes;
} ZgpuiEffectConfig;

typedef struct {
    uint32_t abi_version, flags, max_texture_size, max_entries;
    uint32_t max_shader_slots, max_shader_bytes;
    uint64_t max_texture_bytes;
} ZgpuiEffectCapabilities;

typedef struct {
    uint64_t texture_bytes, peak_texture_bytes, captures, cache_hits;
    uint64_t cache_misses, evictions, blur_passes, draw_calls;
    uint64_t shader_compiles, fallbacks;
    uint32_t live_entries, live_programs, last_status, quality;
} ZgpuiEffectStats;

typedef struct {
    uint64_t id, revision;
    float x, y, width, height;
} ZgpuiEffectCapture;

typedef struct {
    ZgpuiEffectCapture source;
    float x, y, width, height;
    float radius, blur_radius, saturation, opacity;
    uint32_t tint_rgba, preset;
    float tint_mix, noise, refraction, border;
    /* Positive progressive blur increases with distance from the fold hinge;
     * without a fold it increases from top to bottom. Negative values reverse it. */
    float highlight, progressive_blur;
    /* Dim is uniform on a flat surface and increases away from an active hinge. */
    float fold_progress, fold_radians, fold_perspective, dim;
    float fold_anchor;
    uint32_t fold_axis, flags, shader_slot;
    float shader_params[8];
} ZgpuiEffect;

/* quality: 0=disabled, 1=low, 2=balanced, 3=high. Budget changes evict owned
   effect resources as needed. max_entries 1..16, shaders 0..8, bytes <=256MiB.
   Defaults: balanced, 16 entries, 8 custom shaders, 32MiB effect textures. */
int zgpui_effect_configure(ZgpuiWindow *, const ZgpuiEffectConfig *);
int zgpui_effect_capabilities(ZgpuiWindow *, ZgpuiEffectCapabilities *);
int zgpui_effect_stats(ZgpuiWindow *, ZgpuiEffectStats *);
int zgpui_draw_effect(ZgpuiWindow *, const ZgpuiEffect *);
/* capture caches id/revision/source rectangle; unchanged keys reuse pixels.
   Draw defaults to fixed screen-space backdrop UV. LAYER attaches UV to the
   folded content. CACHE_ONLY permits source width=height=0 to use its saved rect.
   Target/material/fold changes never implicitly recapture an existing source. */
int zgpui_effect_capture(ZgpuiWindow *, const ZgpuiEffectCapture *);
int zgpui_effect_invalidate(ZgpuiWindow *, uint64_t id);
int zgpui_effect_clear(ZgpuiWindow *);
/* Fragment shader contract: GLSL 1.20 function
   vec4 effect(vec2 local_uv, vec2 screen_uv, vec4 base_color).
   The renderer supplies main(), rounded clipping, opacity and projective vertex
   transform. Eight float parameters are exposed as u_params[8]. Source is
   bounded to 16KiB; registration compiles synchronously and reports diagnostics.
   slot numbers are 1..8, with 0 reserved for built-in material programs. */
int zgpui_effect_shader_register(ZgpuiWindow *, const char *, size_t,
                                 uint32_t *, char *, size_t);
int zgpui_effect_shader_remove(ZgpuiWindow *, uint32_t);
const char *zgpui_effect_status(int);
/* Explicit benchmark synchronization only; ordinary drawing never calls it.
   The CPU renderer completes drawing synchronously and returns OK as a no-op. */
int zgpui_effect_finish(ZgpuiWindow *);

#endif
