#ifndef ZGPUI_NATIVE_H
#define ZGPUI_NATIVE_H
#include <stdint.h>
#include <stddef.h>
typedef struct ZgpuiWindow ZgpuiWindow;
typedef struct { int kind,x,y,width,height,key,modifiers; char text[4096]; } ZgpuiEvent;
typedef struct { unsigned char *pixels; int width,height; float advance; } ZgpuiBitmap;
ZgpuiWindow *zgpui_open(const char *title,int width,int height,char *error,size_t error_size);
void zgpui_close(ZgpuiWindow *window);
int zgpui_event(ZgpuiWindow *window,ZgpuiEvent *event,int timeout_ms);
void zgpui_swap(ZgpuiWindow *window);
void zgpui_clipboard(ZgpuiWindow *window,const char *text);
void zgpui_size(ZgpuiWindow *window,int *width,int *height);
/* Copies the family; failure leaves the old font and text cache intact. */
int zgpui_set_font_family(ZgpuiWindow *window,const char *family,char *error,size_t error_size);
/* Same advance as rasterize, without allocating a bitmap or GPU texture. */
float zgpui_text_advance(const char *text,int size,int bold);
ZgpuiBitmap zgpui_rasterize(const char *text,int size,int bold);
void zgpui_font_shutdown(void);
/* Requires the owning renderer context; preserves geometry and framebuffer. */
void zgpui_clear_text_cache(void);
void zgpui_begin(int width,int height,uint32_t background);
void zgpui_rect(float x,float y,float width,float height,float radius,uint32_t color);
void zgpui_polygon(const float *points,size_t count,uint32_t color);
void zgpui_line(float x1,float y1,float x2,float y2,float width,uint32_t color);
float zgpui_text(const char *text,float x,float y,int size,int bold,uint32_t color);
float zgpui_measure(const char *text,int size,int bold);
void zgpui_clip(int x,int y,int width,int height);
void zgpui_unclip(void);
void zgpui_gpu_destroy(void);
const char *zgpui_gpu_info(void);
#ifdef ZGPUI_SOFTWARE
int zgpui_software_resize(int width,int height);
/* NULL until begin succeeds, and after release or a successful size change. */
uint32_t *zgpui_software_pixels(int *width,int *height);
/* Call only after the presenter has copied the pixels. Next begin clears all. */
void zgpui_software_release(void);
#endif
#endif
