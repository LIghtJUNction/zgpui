#include "zgpui.h"
#ifdef _WIN32
#include <windows.h>
#endif
#ifdef __APPLE__
#include <OpenGL/gl.h>
#else
#include <GL/gl.h>
#endif
#include <stdlib.h>
#include <string.h>
#include <math.h>
#define CACHE_COUNT 96
typedef struct {char *text;int size,bold,width,height;float advance;GLuint texture;unsigned long stamp;} Text;
static Text cache[CACHE_COUNT];
static unsigned long clock_tick;
static int screen_height;
static size_t texture_bytes;
static void drop(Text *item) {
    if(item->text) {texture_bytes -= (size_t)item->width*item->height;free(item->text);glDeleteTextures(1,&item->texture);memset(item,0,sizeof(*item));}
}
static Text *get_text(const char *text,int size,int bold) {
    Text *slot=&cache[0];
    for(int i=0;i<CACHE_COUNT;i++) {
        if(cache[i].text && cache[i].size==size && cache[i].bold==bold && strcmp(cache[i].text,text)==0) {cache[i].stamp=++clock_tick;return &cache[i];}
        if(!cache[i].text || (slot->text && cache[i].stamp<slot->stamp)) slot=&cache[i];
    }
    drop(slot);
    ZgpuiBitmap bitmap=zgpui_rasterize(text,size,bold);
    if(!bitmap.pixels || bitmap.width<=0 || bitmap.height<=0) {free(bitmap.pixels);return NULL;}
    while(texture_bytes+(size_t)bitmap.width*bitmap.height>2097152) {
        Text *oldest=NULL;
        for(int i=0;i<CACHE_COUNT;i++) if(cache[i].text && (!oldest || cache[i].stamp<oldest->stamp)) oldest=&cache[i];
        if(!oldest) break;
        drop(oldest);
    }
    size_t len=strlen(text);
    slot->text=malloc(len+1);
    if(!slot->text) {free(bitmap.pixels);return NULL;}
    memcpy(slot->text,text,len+1);
    slot->size=size;slot->bold=bold;slot->width=bitmap.width;slot->height=bitmap.height;slot->advance=bitmap.advance;slot->stamp=++clock_tick;
    glGenTextures(1,&slot->texture);glBindTexture(GL_TEXTURE_2D,slot->texture);
    glTexParameteri(GL_TEXTURE_2D,GL_TEXTURE_MIN_FILTER,GL_LINEAR);glTexParameteri(GL_TEXTURE_2D,GL_TEXTURE_MAG_FILTER,GL_LINEAR);
    glTexParameteri(GL_TEXTURE_2D,GL_TEXTURE_WRAP_S,GL_CLAMP);glTexParameteri(GL_TEXTURE_2D,GL_TEXTURE_WRAP_T,GL_CLAMP);
    glPixelStorei(GL_UNPACK_ALIGNMENT,1);
    glTexImage2D(GL_TEXTURE_2D,0,GL_ALPHA,bitmap.width,bitmap.height,0,GL_ALPHA,GL_UNSIGNED_BYTE,bitmap.pixels);
    texture_bytes+=(size_t)bitmap.width*bitmap.height;free(bitmap.pixels);return slot;
}
static void color(uint32_t value) {glColor4ub((GLubyte)(value>>24),(GLubyte)(value>>16),(GLubyte)(value>>8),(GLubyte)value);}
void zgpui_begin(int width,int height,uint32_t background) {
    screen_height=height;
    glViewport(0,0,width,height);glMatrixMode(GL_PROJECTION);glLoadIdentity();glOrtho(0,width,height,0,-1,1);
    glMatrixMode(GL_MODELVIEW);glLoadIdentity();glDisable(GL_DEPTH_TEST);glDisable(GL_CULL_FACE);
    glDisable(GL_SCISSOR_TEST);glEnable(GL_BLEND);glBlendFunc(GL_SRC_ALPHA,GL_ONE_MINUS_SRC_ALPHA);
    glClearColor((background>>24)/255.0f,((background>>16)&255)/255.0f,((background>>8)&255)/255.0f,(background&255)/255.0f);glClear(GL_COLOR_BUFFER_BIT);
}
/* Small alpha masks smooth geometry without a multisampled framebuffer. */
static GLuint rounded_masks[65];
static GLuint rounded_mask(float radius) {
    int r=(int)ceilf(radius);if(r<1)r=1;if(r>64)r=64;
    if(rounded_masks[r])return rounded_masks[r];
    int size=r*2;unsigned char *pixels=malloc((size_t)size*size);
    if(!pixels)return 0;
    for(int y=0;y<size;y++)for(int x=0;x<size;x++){
        float dx=x+0.5f-r,dy=y+0.5f-r;
        float coverage=r+0.5f-sqrtf(dx*dx+dy*dy);
        if(coverage<0)coverage=0;if(coverage>1)coverage=1;
        pixels[y*size+x]=(unsigned char)(coverage*255.0f+0.5f);
    }
    glGenTextures(1,&rounded_masks[r]);glBindTexture(GL_TEXTURE_2D,rounded_masks[r]);
    glTexParameteri(GL_TEXTURE_2D,GL_TEXTURE_MIN_FILTER,GL_LINEAR);glTexParameteri(GL_TEXTURE_2D,GL_TEXTURE_MAG_FILTER,GL_LINEAR);
    glTexParameteri(GL_TEXTURE_2D,GL_TEXTURE_WRAP_S,GL_CLAMP);glTexParameteri(GL_TEXTURE_2D,GL_TEXTURE_WRAP_T,GL_CLAMP);
    glPixelStorei(GL_UNPACK_ALIGNMENT,1);glTexImage2D(GL_TEXTURE_2D,0,GL_ALPHA,size,size,0,GL_ALPHA,GL_UNSIGNED_BYTE,pixels);
    free(pixels);return rounded_masks[r];
}
static void quad(float x,float y,float w,float h,float u0,float v0,float u1,float v1){
    if(w<=0||h<=0)return;
    glTexCoord2f(u0,v0);glVertex2f(x,y);glTexCoord2f(u1,v0);glVertex2f(x+w,y);
    glTexCoord2f(u1,v1);glVertex2f(x+w,y+h);glTexCoord2f(u0,v1);glVertex2f(x,y+h);
}
void zgpui_rect(float x,float y,float width,float height,float radius,uint32_t value) {
    if(width<=0||height<=0)return;
    if(radius>width/2)radius=width/2;if(radius>height/2)radius=height/2;
    color(value);
    if(radius<=0){glDisable(GL_TEXTURE_2D);glBegin(GL_QUADS);quad(x,y,width,height,0,0,0,0);glEnd();return;}
    GLuint mask=rounded_mask(radius);if(!mask)return;
    glEnable(GL_TEXTURE_2D);glBindTexture(GL_TEXTURE_2D,mask);glBegin(GL_QUADS);
    float middle_w=width-radius*2,middle_h=height-radius*2;
    quad(x,y,radius,radius,0,0,0.5f,0.5f);
    quad(x+width-radius,y,radius,radius,0.5f,0,1,0.5f);
    quad(x,y+height-radius,radius,radius,0,0.5f,0.5f,1);
    quad(x+width-radius,y+height-radius,radius,radius,0.5f,0.5f,1,1);
    quad(x+radius,y,middle_w,radius,0.5f,0,0.5f,0.5f);
    quad(x+radius,y+height-radius,middle_w,radius,0.5f,0.5f,0.5f,1);
    quad(x,y+radius,radius,middle_h,0,0.5f,0.5f,0.5f);
    quad(x+width-radius,y+radius,radius,middle_h,0.5f,0.5f,1,0.5f);
    quad(x+radius,y+radius,middle_w,middle_h,0.5f,0.5f,0.5f,0.5f);
    glEnd();glDisable(GL_TEXTURE_2D);
}
void zgpui_polygon(const float *points,size_t count,uint32_t value){
    if(!points || count<3 || count>4096)return;
    glDisable(GL_TEXTURE_2D);color(value);glBegin(GL_TRIANGLE_FAN);
    for(size_t i=0;i<count;i++)glVertex2f(points[i*2],points[i*2+1]);
    glEnd();
}
void zgpui_line(float x1,float y1,float x2,float y2,float width,uint32_t value){
    float dx=x2-x1,dy=y2-y1,length=sqrtf(dx*dx+dy*dy);
    if(length<=0 || width<=0)return;
    glPushMatrix();glTranslatef(x1,y1,0);glRotatef(atan2f(dy,dx)*57.295779513f,0,0,1);
    zgpui_rect(-width/2,-width/2,length+width,width,width/2,value);
    glPopMatrix();
}
float zgpui_text(const char *text,float x,float y,int size,int bold,uint32_t value) {
    if(!*text)return 0;
    Text *item=get_text(text,size,bold);if(!item)return 0;
    glEnable(GL_TEXTURE_2D);glBindTexture(GL_TEXTURE_2D,item->texture);color(value);glBegin(GL_QUADS);
    glTexCoord2f(0,0);glVertex2f(x,y);glTexCoord2f(1,0);glVertex2f(x+item->width,y);
    glTexCoord2f(1,1);glVertex2f(x+item->width,y+item->height);glTexCoord2f(0,1);glVertex2f(x,y+item->height);glEnd();glDisable(GL_TEXTURE_2D);return item->advance;
}
float zgpui_measure(const char *text,int size,int bold){if(!*text)return 0;Text *item=get_text(text,size,bold);return item?item->advance:0;}
void zgpui_clip(int x,int y,int width,int height){glEnable(GL_SCISSOR_TEST);glScissor(x,screen_height-y-height,width>0?width:0,height>0?height:0);}
void zgpui_unclip(void){glDisable(GL_SCISSOR_TEST);}
void zgpui_clear_text_cache(void){for(int i=0;i<CACHE_COUNT;i++)drop(&cache[i]);clock_tick=0;}
void zgpui_gpu_destroy(void){zgpui_clear_text_cache();for(int i=1;i<=64;i++){if(rounded_masks[i])glDeleteTextures(1,&rounded_masks[i]);rounded_masks[i]=0;}}
const char *zgpui_gpu_info(void){const GLubyte *s=glGetString(GL_RENDERER);return s?(const char*)s:"unknown";}
