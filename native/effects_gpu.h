#ifndef ZGPUI_EFFECTS_GPU_H
#define ZGPUI_EFFECTS_GPU_H
#include "effects_common.h"
#include "effects_shaders.h"
#if !defined(_WIN32) && !defined(__APPLE__)
#include <GL/glx.h>
#elif defined(__APPLE__)
#include <dlfcn.h>
#endif
#ifndef APIENTRY
#define APIENTRY
#endif

/* Keep the old OpenGL 1.1 drawing path buildable, including Win32's 1.1 headers.
   Optional 2.x shaders/FBO entry points are resolved only on an effects request. */
#define FX_FRAGMENT_SHADER 0x8B30
#define FX_VERTEX_SHADER 0x8B31
#define FX_COMPILE_STATUS 0x8B81
#define FX_LINK_STATUS 0x8B82
#define FX_CURRENT_PROGRAM 0x8B8D
#define FX_SHADING_VERSION 0x8B8C
#define FX_FRAMEBUFFER 0x8D40
#define FX_FRAMEBUFFER_BINDING 0x8CA6
#define FX_COLOR_ATTACHMENT0 0x8CE0
#define FX_FRAMEBUFFER_COMPLETE 0x8CD5
#define FX_TEXTURE0 0x84C0
#define FX_ACTIVE_TEXTURE 0x84E0
#define FX_CLAMP_TO_EDGE 0x812F
#define FX_RGBA8 0x8058
#define FX_MARGIN 64

#define FX_FUNCTIONS(M) \
 M(GLuint,CreateShader,(GLenum)) \
 M(void,ShaderSource,(GLuint,GLsizei,const char *const *,const GLint *)) \
 M(void,CompileShader,(GLuint)) \
 M(void,GetShaderiv,(GLuint,GLenum,GLint *)) \
 M(void,GetShaderInfoLog,(GLuint,GLsizei,GLsizei *,char *)) \
 M(void,DeleteShader,(GLuint)) \
 M(GLuint,CreateProgram,(void)) \
 M(void,AttachShader,(GLuint,GLuint)) \
 M(void,LinkProgram,(GLuint)) \
 M(void,GetProgramiv,(GLuint,GLenum,GLint *)) \
 M(void,GetProgramInfoLog,(GLuint,GLsizei,GLsizei *,char *)) \
 M(void,DeleteProgram,(GLuint)) \
 M(void,UseProgram,(GLuint)) \
 M(GLint,GetUniformLocation,(GLuint,const char *)) \
 M(void,Uniform1i,(GLint,GLint)) \
 M(void,Uniform1f,(GLint,GLfloat)) \
 M(void,Uniform2f,(GLint,GLfloat,GLfloat)) \
 M(void,Uniform4f,(GLint,GLfloat,GLfloat,GLfloat,GLfloat)) \
 M(void,ActiveTexture,(GLenum)) \
 M(void,GenFramebuffers,(GLsizei,GLuint *)) \
 M(void,DeleteFramebuffers,(GLsizei,const GLuint *)) \
 M(void,BindFramebuffer,(GLenum,GLuint)) \
 M(void,FramebufferTexture2D,(GLenum,GLenum,GLenum,GLuint,GLint)) \
 M(GLenum,CheckFramebufferStatus,(GLenum))
#define FX_TYPE(result,name,args) typedef result (APIENTRY *Fx##name) args;
FX_FUNCTIONS(FX_TYPE)
#undef FX_TYPE
typedef struct {
#define FX_FIELD(result,name,args) Fx##name name;
    FX_FUNCTIONS(FX_FIELD)
#undef FX_FIELD
} FxFunctions;

typedef struct {
    GLuint id;
    GLint rect, view, fold, anchor, axis, raw, blurred, source_rect, capture;
    GLint raw_size, blur_size, tint, material, surface, blur_content, preset;
    GLint layer, progressive, highlight, params[8], bsource, bspan, bstep, bbounds;
} FxProgram;
typedef struct {
    ZgpuiEffectCapture source;
    GLuint raw, horizontal, blurred;
    int x, y, width, height, storage_width, storage_height;
    int blur_width, blur_height, blur_storage_width, blur_storage_height;
    int valid, blur_valid;
    float blur_radius;
    uint32_t blur_quality;
    uint64_t bytes, stamp;
} FxEntry;
typedef struct {
    ZgpuiWindow *owner;
    ZgpuiEffectConfig config;
    ZgpuiEffectStats stats;
    FxFunctions gl;
    FxEntry entries[ZGPUI_EFFECT_MAX_ENTRIES];
    FxProgram material, blur, custom[ZGPUI_EFFECT_MAX_SHADERS];
    GLuint framebuffer;
    uint32_t flags;
    int loaded, max_texture_size, width, height;
    uint64_t clock;
} FxState;
static FxState fx;

static void *fx_proc(const char *name) {
#ifdef ZGPUI_EFFECTS_TEST_LOADER
    return zgpui_test_gl_proc(name);
#elif defined(_WIN32)
    union { PROC function; void *object; uintptr_t bits; } address;
    address.function = wglGetProcAddress(name);
    if (!address.function || address.bits <= 3u || address.bits == UINTPTR_MAX)
        address.function = GetProcAddress(GetModuleHandleA("opengl32.dll"), name);
    return address.object;
#elif defined(__APPLE__)
    return dlsym(RTLD_DEFAULT, name);
#else
    union { __GLXextFuncPtr function; void *object; } address;
    address.function = glXGetProcAddressARB((const GLubyte *)name);
    return address.object;
#endif
}
static int fx_extension(const char *extensions, const char *name) {
    if (!extensions) return 0;
    size_t length = strlen(name);
    const char *found = extensions;
    while ((found = strstr(found, name))) {
        if ((found == extensions || found[-1] == ' ') &&
            (!found[length] || found[length] == ' ')) return 1;
        found += length;
    }
    return 0;
}
static int fx_ready(ZgpuiWindow *window) {
    if (!window || (fx.owner && fx.owner != window)) return 0;
    if (!fx.owner) { fx.owner = window; fx.config = fx_default_config(); }
    if (fx.loaded) return 1;
    const char *version = (const char *)glGetString(GL_VERSION);
    if (!version) return 0;
    fx.loaded = 1;
    fx.flags = ZGPUI_EFFECT_GPU;
    glGetIntegerv(GL_MAX_TEXTURE_SIZE, &fx.max_texture_size);
    if (fx.max_texture_size > 16384) fx.max_texture_size = 16384;
    int major = 0, minor = 0;
    (void)sscanf(version, "%d.%d", &major, &minor);
    if (major < 2 || fx.max_texture_size < 64) return 1;
    const char *language = (const char *)glGetString(FX_SHADING_VERSION);
    int language_major = 0, language_minor = 0;
    if (!language || sscanf(language, "%d.%d", &language_major, &language_minor) != 2 ||
        (language_major < 1 || (language_major == 1 && language_minor < 20))) return 1;
#define FX_ASSIGN(name,symbol) do { void *address = fx_proc(symbol); memcpy(&fx.gl.name, &address, sizeof(fx.gl.name)); } while (0)
#define FX_LOAD(result,name,args) FX_ASSIGN(name,"gl" #name);
    FX_FUNCTIONS(FX_LOAD)
#undef FX_LOAD
    if (!fx.gl.CreateShader || !fx.gl.ShaderSource || !fx.gl.CompileShader ||
        !fx.gl.GetShaderiv || !fx.gl.GetShaderInfoLog || !fx.gl.DeleteShader ||
        !fx.gl.CreateProgram || !fx.gl.AttachShader || !fx.gl.LinkProgram ||
        !fx.gl.GetProgramiv || !fx.gl.GetProgramInfoLog || !fx.gl.DeleteProgram ||
        !fx.gl.UseProgram || !fx.gl.GetUniformLocation || !fx.gl.Uniform1i ||
        !fx.gl.Uniform1f || !fx.gl.Uniform2f || !fx.gl.Uniform4f || !fx.gl.ActiveTexture) return 1;
    fx.flags |= ZGPUI_EFFECT_CAPTURE | ZGPUI_EFFECT_CUSTOM_SHADER | ZGPUI_EFFECT_PROJECTIVE_FOLD;
    const char *extensions = (const char *)glGetString(GL_EXTENSIONS);
    int core_fbo = major >= 3 || fx_extension(extensions, "GL_ARB_framebuffer_object");
    int ext_fbo = fx_extension(extensions, "GL_EXT_framebuffer_object");
    if (!core_fbo && !ext_fbo) return 1;
    /* macOS legacy 2.1 exports both names but only promises the EXT commands. */
#define FX_FBO_LOAD(name) if (!core_fbo || (!fx.gl.name && ext_fbo)) { FX_ASSIGN(name,"gl" #name "EXT"); }
    FX_FBO_LOAD(GenFramebuffers); FX_FBO_LOAD(DeleteFramebuffers);
    FX_FBO_LOAD(BindFramebuffer); FX_FBO_LOAD(FramebufferTexture2D);
    FX_FBO_LOAD(CheckFramebufferStatus);
#undef FX_FBO_LOAD
#undef FX_ASSIGN
    if (fx.gl.GenFramebuffers && fx.gl.DeleteFramebuffers && fx.gl.BindFramebuffer &&
        fx.gl.FramebufferTexture2D && fx.gl.CheckFramebufferStatus) fx.flags |= ZGPUI_EFFECT_BLUR;
    return 1;
}
static int fx_result(int status) { fx.stats.last_status = (uint32_t)status; return status; }
static float fx_clamp(float value, float minimum, float maximum) {
    return value < minimum ? minimum : value > maximum ? maximum : value;
}
static void fx_error(char *out, size_t capacity, const char *message) {
    if (out && capacity) snprintf(out, capacity, "%s", message);
}
static int fx_power_two(int value) {
    if (value < 1 || value > fx.max_texture_size) return 0;
    int size = 1;
    while (size < value) size *= 2;
    return size <= fx.max_texture_size ? size : 0;
}
static void fx_drop_blur(FxEntry *entry) {
    GLuint textures[2] = {entry->horizontal, entry->blurred};
    if (textures[0] || textures[1]) glDeleteTextures(2, textures);
    uint64_t bytes = (uint64_t)entry->blur_storage_width * (uint64_t)entry->blur_storage_height * 8u;
    if (entry->horizontal || entry->blurred) { fx.stats.texture_bytes -= bytes; entry->bytes -= bytes; }
    entry->horizontal = entry->blurred = 0;
    entry->blur_width = entry->blur_height = 0;
    entry->blur_storage_width = entry->blur_storage_height = 0;
    entry->blur_valid = 0;
}
static void fx_drop_entry(FxEntry *entry, int eviction) {
    if (!entry->raw) { memset(entry, 0, sizeof(*entry)); return; }
    fx_drop_blur(entry);
    glDeleteTextures(1, &entry->raw);
    fx.stats.texture_bytes -= entry->bytes;
    fx.stats.live_entries--;
    if (eviction) fx.stats.evictions++;
    memset(entry, 0, sizeof(*entry));
}
static void fx_clear_entries(int eviction) {
    for (size_t i = 0; i < ZGPUI_EFFECT_MAX_ENTRIES; ++i) fx_drop_entry(&fx.entries[i], eviction);
}
static FxEntry *fx_oldest(FxEntry *exclude) {
    FxEntry *oldest = NULL;
    for (size_t i = 0; i < ZGPUI_EFFECT_MAX_ENTRIES; ++i) {
        FxEntry *entry = &fx.entries[i];
        if (entry != exclude && entry->raw && (!oldest || entry->stamp < oldest->stamp)) oldest = entry;
    }
    return oldest;
}
static int fx_reserve(uint64_t bytes, FxEntry *exclude) {
    if (bytes > fx.config.max_texture_bytes) return 0;
    while (fx.stats.texture_bytes > fx.config.max_texture_bytes - bytes) {
        FxEntry *entry = fx_oldest(exclude);
        if (!entry) return 0;
        fx_drop_entry(entry, 1);
    }
    return 1;
}
static void fx_account(uint64_t bytes) {
    fx.stats.texture_bytes += bytes;
    if (fx.stats.texture_bytes > fx.stats.peak_texture_bytes)
        fx.stats.peak_texture_bytes = fx.stats.texture_bytes;
}
static void fx_clear_errors(void) { for (int i = 0; i < 16 && glGetError() != GL_NO_ERROR; ++i) {} }
static GLuint fx_texture(int width, int height) {
    GLuint texture = 0;
    fx_clear_errors();
    glGenTextures(1, &texture);
    if (!texture || glGetError() != GL_NO_ERROR) { if (texture) glDeleteTextures(1, &texture); return 0; }
    glBindTexture(GL_TEXTURE_2D, texture);
    glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_MIN_FILTER, GL_LINEAR);
    glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_MAG_FILTER, GL_LINEAR);
    glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_WRAP_S, FX_CLAMP_TO_EDGE);
    glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_WRAP_T, FX_CLAMP_TO_EDGE);
    glTexImage2D(GL_TEXTURE_2D, 0, FX_RGBA8, width, height, 0, GL_RGBA, GL_UNSIGNED_BYTE, NULL);
    if (glGetError() != GL_NO_ERROR) { glDeleteTextures(1, &texture); return 0; }
    return texture;
}

typedef struct {
    GLint program, framebuffer, active_texture, matrix_mode, scissor[4];
    GLboolean scissor_enabled;
} FxSaved;
static void fx_save(FxSaved *state) {
    glGetIntegerv(FX_CURRENT_PROGRAM, &state->program);
    state->framebuffer = 0;
    if (fx.flags & ZGPUI_EFFECT_BLUR) glGetIntegerv(FX_FRAMEBUFFER_BINDING, &state->framebuffer);
    glGetIntegerv(FX_ACTIVE_TEXTURE, &state->active_texture);
    glGetIntegerv(GL_MATRIX_MODE, &state->matrix_mode);
    glGetIntegerv(GL_SCISSOR_BOX, state->scissor);
    state->scissor_enabled = glIsEnabled(GL_SCISSOR_TEST);
    glPushAttrib(GL_ALL_ATTRIB_BITS);
    glMatrixMode(GL_PROJECTION); glPushMatrix();
    glMatrixMode(GL_MODELVIEW); glPushMatrix();
    fx.gl.ActiveTexture(FX_TEXTURE0);
    glDisable(GL_DEPTH_TEST); glDisable(GL_CULL_FACE);
}
static void fx_restore(const FxSaved *state) {
    if (fx.flags & ZGPUI_EFFECT_BLUR) fx.gl.BindFramebuffer(FX_FRAMEBUFFER, (GLuint)state->framebuffer);
    fx.gl.UseProgram((GLuint)state->program);
    glMatrixMode(GL_MODELVIEW); glPopMatrix();
    glMatrixMode(GL_PROJECTION); glPopMatrix();
    glPopAttrib();
    glMatrixMode((GLenum)state->matrix_mode);
    fx.gl.ActiveTexture((GLenum)state->active_texture);
}
static GLint fx_location(GLuint program, const char *name) { return fx.gl.GetUniformLocation(program, name); }
static void fx_uniform_locations(FxProgram *program) {
#define FX_LOCATION(field,name) program->field = fx_location(program->id, name)
    FX_LOCATION(rect,"u_rect"); FX_LOCATION(view,"u_view"); FX_LOCATION(fold,"u_fold");
    FX_LOCATION(anchor,"u_anchor"); FX_LOCATION(axis,"u_axis"); FX_LOCATION(raw,"u_raw");
    FX_LOCATION(blurred,"u_blurred"); FX_LOCATION(source_rect,"u_source_rect"); FX_LOCATION(capture,"u_capture");
    FX_LOCATION(raw_size,"u_raw_size"); FX_LOCATION(blur_size,"u_blur_size"); FX_LOCATION(tint,"u_tint");
    FX_LOCATION(material,"u_material"); FX_LOCATION(surface,"u_surface"); FX_LOCATION(blur_content,"u_blur_content");
    FX_LOCATION(preset,"u_preset"); FX_LOCATION(layer,"u_layer"); FX_LOCATION(progressive,"u_progressive");
    FX_LOCATION(highlight,"u_highlight"); FX_LOCATION(bsource,"u_source"); FX_LOCATION(bspan,"u_span");
    FX_LOCATION(bstep,"u_step"); FX_LOCATION(bbounds,"u_bounds");
#undef FX_LOCATION
    for (int i = 0; i < 8; ++i) {
        char name[32]; snprintf(name, sizeof(name), "u_params[%d]", i);
        program->params[i] = fx_location(program->id, name);
    }
}
static GLuint fx_compile_shader(GLenum type, GLsizei count, const char *const *parts,
                                const GLint *lengths, char *error, size_t capacity) {
    GLuint shader = fx.gl.CreateShader(type);
    if (!shader) { fx_error(error, capacity, "Cannot create shader object."); return 0; }
    fx.gl.ShaderSource(shader, count, parts, lengths);
    fx.gl.CompileShader(shader);
    GLint success = 0; fx.gl.GetShaderiv(shader, FX_COMPILE_STATUS, &success);
    if (!success) {
        if (error && capacity) {
            GLsizei length = 0;
            fx.gl.GetShaderInfoLog(shader, (GLsizei)(capacity > 4096 ? 4096 : capacity), &length, error);
            error[capacity - 1] = 0;
            if (!error[0]) fx_error(error, capacity, "Shader compilation failed without a driver diagnostic.");
        }
        fx.gl.DeleteShader(shader); return 0;
    }
    return shader;
}
static int fx_compile_program(FxProgram *out, int blur, const char *custom, size_t length,
                              char *error, size_t capacity) {
    fx.stats.shader_compiles++;
    if (error && capacity) error[0] = 0;
    const char *vertex_source = blur ? fx_blur_vertex : fx_projective_vertex;
    GLuint vertex = fx_compile_shader(FX_VERTEX_SHADER, 1, &vertex_source, NULL, error, capacity);
    if (!vertex) return 0;
    const char *parts[3] = {fx_material_head, custom ? custom : fx_default_effect, fx_material_main};
    GLint sizes[3] = {(GLint)strlen(parts[0]), (GLint)(custom ? length : strlen(parts[1])), (GLint)strlen(parts[2])};
    const char *blur_source = fx_blur_fragment;
    GLuint fragment = blur ? fx_compile_shader(FX_FRAGMENT_SHADER, 1, &blur_source, NULL, error, capacity)
        : fx_compile_shader(FX_FRAGMENT_SHADER, 3, parts, sizes, error, capacity);
    if (!fragment) { fx.gl.DeleteShader(vertex); return 0; }
    GLuint program = fx.gl.CreateProgram();
    if (!program) { fx.gl.DeleteShader(vertex); fx.gl.DeleteShader(fragment); fx_error(error, capacity, "Cannot create program object."); return 0; }
    fx.gl.AttachShader(program, vertex); fx.gl.AttachShader(program, fragment); fx.gl.LinkProgram(program);
    GLint success = 0; fx.gl.GetProgramiv(program, FX_LINK_STATUS, &success);
    fx.gl.DeleteShader(vertex); fx.gl.DeleteShader(fragment);
    if (!success) {
        if (error && capacity) {
            GLsizei written = 0;
            fx.gl.GetProgramInfoLog(program, (GLsizei)(capacity > 4096 ? 4096 : capacity), &written, error);
            error[capacity - 1] = 0;
            if (!error[0]) fx_error(error, capacity, "Shader linking failed without a driver diagnostic.");
        }
        fx.gl.DeleteProgram(program); return 0;
    }
    memset(out, 0, sizeof(*out)); out->id = program;
    fx_uniform_locations(out); fx.stats.live_programs++;
    return 1;
}
static void fx_drop_program(FxProgram *program) {
    if (program->id) { fx.gl.DeleteProgram(program->id); fx.stats.live_programs--; }
    memset(program, 0, sizeof(*program));
}
static int fx_source_equal(const ZgpuiEffectCapture *a, const ZgpuiEffectCapture *b) {
    return a->id == b->id && a->revision == b->revision && a->x == b->x && a->y == b->y &&
        a->width == b->width && a->height == b->height;
}
static FxEntry *fx_find(uint64_t id) {
    for (size_t i = 0; i < ZGPUI_EFFECT_MAX_ENTRIES; ++i)
        if (fx.entries[i].raw && fx.entries[i].source.id == id) return &fx.entries[i];
    return NULL;
}
static int fx_get_capture(const ZgpuiEffectCapture *source, uint32_t flags, FxEntry **out) {
    FxEntry *entry = fx_find(source->id);
    int implicit_rect = (flags & ZGPUI_EFFECT_CACHE_ONLY) && source->width == 0 && source->height == 0;
    int matches = entry && entry->valid && (implicit_rect ? entry->source.revision == source->revision
                                                        : fx_source_equal(&entry->source, source));
    if (matches && !(flags & ZGPUI_EFFECT_REFRESH)) {
        fx.stats.cache_hits++; entry->stamp = ++fx.clock; *out = entry; return ZGPUI_EFFECT_OK;
    }
    fx.stats.cache_misses++;
    if (flags & ZGPUI_EFFECT_CACHE_ONLY) return ZGPUI_EFFECT_CACHE_MISS;
    if (fx.width < 1 || fx.height < 1) return ZGPUI_EFFECT_INVALID;
    int left = (int)floorf(fx_clamp(source->x - FX_MARGIN, 0, (float)fx.width));
    int top = (int)floorf(fx_clamp(source->y - FX_MARGIN, 0, (float)fx.height));
    int right = (int)ceilf(fx_clamp(source->x + source->width + FX_MARGIN, 0, (float)fx.width));
    int bottom = (int)ceilf(fx_clamp(source->y + source->height + FX_MARGIN, 0, (float)fx.height));
    int width = right - left, height = bottom - top;
    if (width < 1 || height < 1) return ZGPUI_EFFECT_INVALID;
    int storage_width = fx_power_two(width), storage_height = fx_power_two(height);
    if (!storage_width || !storage_height) return ZGPUI_EFFECT_BUDGET;
    uint64_t bytes = (uint64_t)storage_width * (uint64_t)storage_height * 4u;
    if (bytes > fx.config.max_texture_bytes) return ZGPUI_EFFECT_BUDGET;
    if (entry && (entry->storage_width != storage_width || entry->storage_height != storage_height)) {
        fx_drop_entry(entry, 0); entry = NULL;
    }
    if (!entry) {
        while (fx.stats.live_entries >= fx.config.max_entries) {
            FxEntry *oldest = fx_oldest(NULL); if (!oldest) return ZGPUI_EFFECT_BUDGET;
            fx_drop_entry(oldest, 1);
        }
        if (!fx_reserve(bytes, NULL)) return ZGPUI_EFFECT_BUDGET;
        for (size_t i = 0; i < ZGPUI_EFFECT_MAX_ENTRIES; ++i)
            if (!fx.entries[i].raw) { entry = &fx.entries[i]; break; }
        if (!entry) return ZGPUI_EFFECT_BUDGET;
        entry->raw = fx_texture(storage_width, storage_height);
        if (!entry->raw) return ZGPUI_EFFECT_ALLOCATION;
        entry->storage_width = storage_width; entry->storage_height = storage_height;
        entry->bytes = bytes; fx_account(bytes); fx.stats.live_entries++;
    }
    entry->source = *source; entry->x = left; entry->y = fx.height - bottom;
    entry->width = width; entry->height = height; entry->stamp = ++fx.clock;
    entry->valid = entry->blur_valid = 0;
    glBindTexture(GL_TEXTURE_2D, entry->raw);
    fx_clear_errors();
    glCopyTexSubImage2D(GL_TEXTURE_2D, 0, 0, 0, left, fx.height - bottom, width, height);
    if (glGetError() != GL_NO_ERROR) { fx_drop_entry(entry, 0); return ZGPUI_EFFECT_ALLOCATION; }
    entry->valid = 1; fx.stats.captures++; *out = entry;
    return ZGPUI_EFFECT_OK;
}
static void fx_quad(float width, float height) {
    glBegin(GL_QUADS);
    glTexCoord2f(0,0); glVertex2f(0,0);
    glTexCoord2f(1,0); glVertex2f(width,0);
    glTexCoord2f(1,1); glVertex2f(width,height);
    glTexCoord2f(0,1); glVertex2f(0,height);
    glEnd();
}
static int fx_blur_entry(FxEntry *entry, float radius) {
    if (radius <= 0) return ZGPUI_EFFECT_OK;
    if (!(fx.flags & ZGPUI_EFFECT_BLUR)) return ZGPUI_EFFECT_UNSUPPORTED;
    if (entry->blur_valid && entry->blur_radius == radius && entry->blur_quality == fx.config.quality)
        return ZGPUI_EFFECT_OK;
    if (!fx.blur.id && !fx_compile_program(&fx.blur, 1, NULL, 0, NULL, 0)) return ZGPUI_EFFECT_SHADER_ERROR;
    int divisor = fx.config.quality == 1 ? 4 : fx.config.quality == 2 ? 2 : 1;
    int width = (entry->width + divisor - 1) / divisor;
    int height = (entry->height + divisor - 1) / divisor;
    int storage_width = fx_power_two(width), storage_height = fx_power_two(height);
    if (!storage_width || !storage_height) return ZGPUI_EFFECT_BUDGET;
    if (entry->horizontal && (storage_width != entry->blur_storage_width || storage_height != entry->blur_storage_height))
        fx_drop_blur(entry);
    if (!entry->horizontal) {
        uint64_t bytes = (uint64_t)storage_width * (uint64_t)storage_height * 8u;
        if (!fx_reserve(bytes, entry)) return ZGPUI_EFFECT_BUDGET;
        GLuint horizontal = fx_texture(storage_width, storage_height);
        GLuint blurred = horizontal ? fx_texture(storage_width, storage_height) : 0;
        if (!horizontal || !blurred) {
            GLuint textures[2] = {horizontal, blurred}; glDeleteTextures(2, textures);
            return ZGPUI_EFFECT_ALLOCATION;
        }
        entry->horizontal = horizontal; entry->blurred = blurred;
        entry->blur_storage_width = storage_width; entry->blur_storage_height = storage_height;
        entry->bytes += bytes; fx_account(bytes);
    }
    entry->blur_width = width; entry->blur_height = height;
    if (!fx.framebuffer) {
        fx.gl.GenFramebuffers(1, &fx.framebuffer);
        if (!fx.framebuffer) return ZGPUI_EFFECT_ALLOCATION;
    }
    fx.gl.BindFramebuffer(FX_FRAMEBUFFER, fx.framebuffer);
    glDrawBuffer(FX_COLOR_ATTACHMENT0);
    glDisable(GL_SCISSOR_TEST); glDisable(GL_BLEND); glDisable(GL_DEPTH_TEST);
    glViewport(0, 0, width, height);
    glMatrixMode(GL_PROJECTION); glLoadIdentity(); glOrtho(0, width, 0, height, -1, 1);
    glMatrixMode(GL_MODELVIEW); glLoadIdentity();
    fx.gl.UseProgram(fx.blur.id); fx.gl.Uniform1i(fx.blur.bsource, 0);
    for (int pass = 0; pass < 2; ++pass) {
        GLuint target = pass ? entry->blurred : entry->horizontal;
        fx.gl.FramebufferTexture2D(FX_FRAMEBUFFER, FX_COLOR_ATTACHMENT0, GL_TEXTURE_2D, target, 0);
        if (fx.gl.CheckFramebufferStatus(FX_FRAMEBUFFER) != FX_FRAMEBUFFER_COMPLETE) {
            fx.gl.FramebufferTexture2D(FX_FRAMEBUFFER, FX_COLOR_ATTACHMENT0, GL_TEXTURE_2D, 0, 0);
            return ZGPUI_EFFECT_ALLOCATION;
        }
        int sw = pass ? storage_width : entry->storage_width;
        int sh = pass ? storage_height : entry->storage_height;
        int cw = pass ? width : entry->width, ch = pass ? height : entry->height;
        glBindTexture(GL_TEXTURE_2D, pass ? entry->horizontal : entry->raw);
        fx.gl.Uniform2f(fx.blur.bspan, (float)cw/sw, (float)ch/sh);
        fx.gl.Uniform4f(fx.blur.bbounds, 0.5f/sw, 0.5f/sh, (cw-0.5f)/sw, (ch-0.5f)/sh);
        fx.gl.Uniform2f(fx.blur.bstep, pass ? 0 : radius/(3.230769231f*sw),
                         pass ? radius/(3.230769231f*divisor*sh) : 0);
        fx_quad((float)width, (float)height); fx.stats.blur_passes++;
    }
    /* Do not let an unbound FBO retain deleted textures after cache eviction. */
    fx.gl.FramebufferTexture2D(FX_FRAMEBUFFER, FX_COLOR_ATTACHMENT0, GL_TEXTURE_2D, 0, 0);
    entry->blur_radius = radius; entry->blur_quality = fx.config.quality; entry->blur_valid = 1;
    return ZGPUI_EFFECT_OK;
}
static int fx_fallback(const ZgpuiEffect *effect, int status) {
    if (!(effect->flags & (ZGPUI_EFFECT_CAPTURE_ONLY | ZGPUI_EFFECT_CACHE_ONLY | ZGPUI_EFFECT_LAYER))) {
        zgpui_rect(effect->x, effect->y, effect->width, effect->height, effect->radius, fx_fallback_color(effect));
        fx.stats.draw_calls++; fx.stats.fallbacks++;
    }
    return fx_result(status);
}
int zgpui_effect_configure(ZgpuiWindow *window, const ZgpuiEffectConfig *config) {
    if (!fx_ready(window) || !fx_config_valid(config)) return fx_result(ZGPUI_EFFECT_INVALID);
    for (size_t i = config->max_shader_slots; i < ZGPUI_EFFECT_MAX_SHADERS; ++i)
        if (fx.custom[i].id) return fx_result(ZGPUI_EFFECT_BUDGET);
    fx.config = *config;
    while (fx.stats.texture_bytes > config->max_texture_bytes || fx.stats.live_entries > config->max_entries) {
        FxEntry *entry = fx_oldest(NULL); if (!entry) break; fx_drop_entry(entry, 1);
    }
    if (!config->quality) fx_clear_entries(1);
    return fx_result(ZGPUI_EFFECT_OK);
}
int zgpui_effect_capabilities(ZgpuiWindow *window, ZgpuiEffectCapabilities *out) {
    if (!fx_ready(window) || !out) return fx_result(ZGPUI_EFFECT_INVALID);
    out->abi_version = ZGPUI_EFFECT_ABI; out->flags = fx.flags;
    out->max_texture_size = (uint32_t)(fx.max_texture_size > 0 ? fx.max_texture_size : 0);
    out->max_entries = fx.config.max_entries; out->max_shader_slots = fx.config.max_shader_slots;
    out->max_shader_bytes = ZGPUI_EFFECT_MAX_SHADER_BYTES; out->max_texture_bytes = fx.config.max_texture_bytes;
    return fx_result(ZGPUI_EFFECT_OK);
}
int zgpui_effect_stats(ZgpuiWindow *window, ZgpuiEffectStats *out) {
    if (!fx_ready(window) || !out) return fx_result(ZGPUI_EFFECT_INVALID);
    fx.stats.quality = fx.config.quality; *out = fx.stats; return ZGPUI_EFFECT_OK;
}
int zgpui_effect_capture(ZgpuiWindow *window, const ZgpuiEffectCapture *capture) {
    if (!fx_ready(window) || !fx_capture_valid(capture, 0)) return fx_result(ZGPUI_EFFECT_INVALID);
    if (!(fx.flags & ZGPUI_EFFECT_CAPTURE) || !fx.config.quality) return fx_result(ZGPUI_EFFECT_UNSUPPORTED);
    FxSaved state; fx_save(&state);
    FxEntry *entry = NULL; int status = fx_get_capture(capture, 0, &entry);
    fx_restore(&state); return fx_result(status);
}
int zgpui_draw_effect(ZgpuiWindow *window, const ZgpuiEffect *effect) {
    if (!fx_ready(window) || !fx_effect_valid(effect)) return fx_result(ZGPUI_EFFECT_INVALID);
    if (!(fx.flags & ZGPUI_EFFECT_CAPTURE) || !fx.config.quality) {
        int status = effect->flags & (ZGPUI_EFFECT_CAPTURE_ONLY | ZGPUI_EFFECT_CACHE_ONLY | ZGPUI_EFFECT_LAYER)
            ? ZGPUI_EFFECT_UNSUPPORTED : ZGPUI_EFFECT_FALLBACK;
        return fx_fallback(effect, status);
    }
    if (effect->flags & ZGPUI_EFFECT_CAPTURE_ONLY) {
        FxSaved state; fx_save(&state); FxEntry *entry = NULL;
        int status = fx_get_capture(&effect->source, effect->flags, &entry);
        fx_restore(&state); return fx_result(status);
    }
    FxProgram *program = &fx.material;
    if (effect->preset == ZGPUI_MATERIAL_CUSTOM || effect->shader_slot) {
        if (!effect->shader_slot || effect->shader_slot > fx.config.max_shader_slots ||
            !fx.custom[effect->shader_slot - 1].id) return fx_result(ZGPUI_EFFECT_INVALID);
        program = &fx.custom[effect->shader_slot - 1];
    } else if (!program->id && !fx_compile_program(program, 0, NULL, 0, NULL, 0))
        return fx_fallback(effect, ZGPUI_EFFECT_SHADER_ERROR);
    FxSaved state; fx_save(&state);
    FxEntry *entry = NULL; int status = fx_get_capture(&effect->source, effect->flags, &entry);
    if (status != ZGPUI_EFFECT_OK) { fx_restore(&state); return fx_result(status); }
    status = fx_blur_entry(entry, effect->blur_radius);
    if (status != ZGPUI_EFFECT_OK) { fx_restore(&state); return fx_result(status); }
    if (fx.flags & ZGPUI_EFFECT_BLUR) fx.gl.BindFramebuffer(FX_FRAMEBUFFER, (GLuint)state.framebuffer);
    glDrawBuffer(state.framebuffer ? FX_COLOR_ATTACHMENT0 : GL_BACK);
    glViewport(0, 0, fx.width, fx.height);
    if (state.scissor_enabled) { glEnable(GL_SCISSOR_TEST); glScissor(state.scissor[0], state.scissor[1], state.scissor[2], state.scissor[3]); }
    else glDisable(GL_SCISSOR_TEST);
    glEnable(GL_BLEND); glBlendFunc(GL_SRC_ALPHA, GL_ONE_MINUS_SRC_ALPHA);
    fx.gl.UseProgram(program->id);
    fx.gl.ActiveTexture(FX_TEXTURE0); glBindTexture(GL_TEXTURE_2D, entry->raw);
    fx.gl.ActiveTexture(FX_TEXTURE0+1); glBindTexture(GL_TEXTURE_2D, effect->blur_radius > 0 ? entry->blurred : entry->raw);
    fx.gl.Uniform1i(program->raw, 0); fx.gl.Uniform1i(program->blurred, 1);
    fx.gl.Uniform4f(program->rect, effect->x, effect->y, effect->width, effect->height);
    fx.gl.Uniform2f(program->view, (float)fx.width, (float)fx.height);
    fx.gl.Uniform4f(program->source_rect, entry->source.x, entry->source.y, entry->source.width, entry->source.height);
    fx.gl.Uniform4f(program->capture, (float)entry->x, (float)entry->y, (float)entry->width, (float)entry->height);
    fx.gl.Uniform2f(program->raw_size, (float)entry->storage_width, (float)entry->storage_height);
    fx.gl.Uniform2f(program->blur_size, (float)(effect->blur_radius > 0 ? entry->blur_storage_width : entry->storage_width),
                     (float)(effect->blur_radius > 0 ? entry->blur_storage_height : entry->storage_height));
    fx.gl.Uniform2f(program->blur_content, (float)(effect->blur_radius > 0 ? entry->blur_width : entry->width),
                     (float)(effect->blur_radius > 0 ? entry->blur_height : entry->height));
    fx.gl.Uniform4f(program->tint, (effect->tint_rgba>>24)/255.0f, ((effect->tint_rgba>>16)&255)/255.0f,
                     ((effect->tint_rgba>>8)&255)/255.0f, (effect->tint_rgba&255)/255.0f*effect->tint_mix);
    fx.gl.Uniform4f(program->material, effect->radius, effect->blur_radius > 0 ? 1.0f : 0.0f, effect->saturation, effect->opacity);
    fx.gl.Uniform4f(program->surface, 0, effect->noise, effect->refraction, effect->border);
    fx.gl.Uniform4f(program->fold, effect->fold_progress, effect->fold_radians, effect->fold_perspective, effect->dim);
    fx.gl.Uniform1f(program->anchor, effect->fold_anchor); fx.gl.Uniform1i(program->axis, (GLint)effect->fold_axis);
    fx.gl.Uniform1i(program->preset, (GLint)effect->preset); fx.gl.Uniform1i(program->layer, (effect->flags & ZGPUI_EFFECT_LAYER) != 0);
    fx.gl.Uniform1f(program->progressive, effect->progressive_blur); fx.gl.Uniform1f(program->highlight, effect->highlight);
    for (int i = 0; i < 8; ++i) if (program->params[i] >= 0) fx.gl.Uniform1f(program->params[i], effect->shader_params[i]);
    fx_quad(1, 1); fx.stats.draw_calls++;
    fx_restore(&state); return fx_result(ZGPUI_EFFECT_OK);
}
int zgpui_effect_invalidate(ZgpuiWindow *window, uint64_t id) {
    if (!fx_ready(window) || !id) return fx_result(ZGPUI_EFFECT_INVALID);
    FxEntry *entry = fx_find(id); if (entry) fx_drop_entry(entry, 0);
    return fx_result(ZGPUI_EFFECT_OK);
}
int zgpui_effect_clear(ZgpuiWindow *window) {
    if (!fx_ready(window)) return fx_result(ZGPUI_EFFECT_INVALID);
    fx_clear_entries(0); return fx_result(ZGPUI_EFFECT_OK);
}
int zgpui_effect_shader_register(ZgpuiWindow *window, const char *source, size_t length,
                                 uint32_t *slot, char *error, size_t capacity) {
    if (slot) *slot = 0;
    if (error && capacity) error[0] = 0;
    if (!fx_ready(window) || !source || !slot || length == 0 || length > ZGPUI_EFFECT_MAX_SHADER_BYTES || memchr(source, 0, length)) {
        fx_error(error, capacity, "Shader source must contain 1..16384 UTF-8 bytes without NUL.");
        return fx_result(ZGPUI_EFFECT_INVALID);
    }
    if (!(fx.flags & ZGPUI_EFFECT_CUSTOM_SHADER)) {
        fx_error(error, capacity, "GLSL 1.20 custom shaders are unavailable in this context.");
        return fx_result(ZGPUI_EFFECT_UNSUPPORTED);
    }
    for (uint32_t i = 0; i < fx.config.max_shader_slots; ++i) if (!fx.custom[i].id) {
        if (!fx_compile_program(&fx.custom[i], 0, source, length, error, capacity)) return fx_result(ZGPUI_EFFECT_SHADER_ERROR);
        *slot = i + 1; return fx_result(ZGPUI_EFFECT_OK);
    }
    fx_error(error, capacity, "Custom shader slots are full; remove a shader before registering another.");
    return fx_result(ZGPUI_EFFECT_BUDGET);
}
int zgpui_effect_shader_remove(ZgpuiWindow *window, uint32_t slot) {
    if (!fx_ready(window) || slot < 1 || slot > ZGPUI_EFFECT_MAX_SHADERS) return fx_result(ZGPUI_EFFECT_INVALID);
    fx_drop_program(&fx.custom[slot - 1]); return fx_result(ZGPUI_EFFECT_OK);
}
int zgpui_effect_finish(ZgpuiWindow *window) {
    if (!fx_ready(window)) return fx_result(ZGPUI_EFFECT_INVALID);
    glFinish(); return fx_result(ZGPUI_EFFECT_OK);
}
static void fx_frame_begin(int width, int height) {
    if (fx.width != width || fx.height != height) fx_clear_entries(1);
    fx.width = width > 0 && width <= 16384 ? width : 0;
    fx.height = height > 0 && height <= 16384 ? height : 0;
}
static void fx_destroy(void) {
    fx_clear_entries(0);
    fx_drop_program(&fx.material); fx_drop_program(&fx.blur);
    for (size_t i = 0; i < ZGPUI_EFFECT_MAX_SHADERS; ++i) fx_drop_program(&fx.custom[i]);
    if (fx.framebuffer) fx.gl.DeleteFramebuffers(1, &fx.framebuffer);
    memset(&fx, 0, sizeof(fx));
}
#undef FX_FUNCTIONS
#endif
