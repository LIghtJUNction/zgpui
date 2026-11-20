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
#define TEXT_CACHE_BYTES 2097152u
#define MAX_TEXT_WIDTH 8192
#define MAX_TEXT_HEIGHT 256
#define MAX_TEXT_TILES 1024
typedef struct {
    char *text;
    int size,bold,width,height,columns,rows,tile_width,tile_height,border_x,border_y;
    float advance;
    GLuint *textures;
    size_t bytes;
    unsigned long stamp;
} Text;
static Text cache[CACHE_COUNT];
static unsigned long clock_tick;
static int screen_height;
static size_t texture_bytes;
static int texture_limit;
static int power_of_two(int value) {
    if(value<1 || value>MAX_TEXT_WIDTH)return 0;
    int size=1;
    while(size<value)size*=2;
    return size;
}
static int native_texture_limit(void) {
    if(texture_limit)return texture_limit;
    GLint maximum=0;
    glGetIntegerv(GL_MAX_TEXTURE_SIZE,&maximum);
    /* OpenGL 1.1 guarantees at least 64. Keep arithmetic and tile counts bounded. */
    if(maximum<64)return 0;
    int limit=64;
    while(limit<MAX_TEXT_WIDTH && limit<=maximum/2)limit*=2;
    texture_limit=limit;
    return limit;
}
static int upload_alpha(const unsigned char *pixels,int image_width,int image_height,
                        int x,int y,int width,int height,int border_x,int border_y,
                        int limit,GLuint *texture) {
    if(!pixels || image_width<1 || image_width>MAX_TEXT_WIDTH || image_height<1 ||
       image_height>MAX_TEXT_HEIGHT || x<0 || x>=image_width || y<0 || y>=image_height ||
       width<1 || width>image_width-x || height<1 || height>image_height-y ||
       border_x<0 || border_x>1 || border_y<0 || border_y>1 ||
       limit<64 || limit>MAX_TEXT_WIDTH || width>limit-border_x*2 || height>limit-border_y*2)
        return 0;
    int content_width=width+border_x*2,content_height=height+border_y*2;
    int storage_width=power_of_two(content_width),storage_height=power_of_two(content_height);
    if(!storage_width || !storage_height || storage_width>limit || storage_height>limit)
        return 0;
    unsigned char *padded=calloc((size_t)storage_width,storage_height);
    if(!padded)return 0;
    /* Neighbour pixels at tile boundaries preserve linear sampling at fractional
       positions. Outside the complete bitmap the mask remains transparent. */
    for(int row=0;row<content_height;row++){
        int source_y=y+row-border_y;
        if(source_y<0 || source_y>=image_height)continue;
        int first=x-border_x,last=first+content_width,destination_x=0;
        if(first<0){destination_x=-first;first=0;}
        if(last>image_width)last=image_width;
        if(last>first)memcpy(padded+(size_t)row*storage_width+destination_x,
                            pixels+(size_t)source_y*image_width+first,(size_t)(last-first));
    }
    /* Do not confuse an earlier context error with this owned upload. */
    for(int i=0;i<16 && glGetError()!=GL_NO_ERROR;i++){}
    GLuint id=0;
    glGenTextures(1,&id);
    glBindTexture(GL_TEXTURE_2D,id);
    glTexParameteri(GL_TEXTURE_2D,GL_TEXTURE_MIN_FILTER,GL_LINEAR);
    glTexParameteri(GL_TEXTURE_2D,GL_TEXTURE_MAG_FILTER,GL_LINEAR);
    glTexParameteri(GL_TEXTURE_2D,GL_TEXTURE_WRAP_S,GL_CLAMP);
    glTexParameteri(GL_TEXTURE_2D,GL_TEXTURE_WRAP_T,GL_CLAMP);
    glPixelStorei(GL_UNPACK_ALIGNMENT,1);
    glTexImage2D(GL_TEXTURE_2D,0,GL_ALPHA,storage_width,storage_height,0,
                 GL_ALPHA,GL_UNSIGNED_BYTE,padded);
    GLenum error=glGetError();
    free(padded);
    if(!id || error!=GL_NO_ERROR){if(id)glDeleteTextures(1,&id);return 0;}
    *texture=id;
    return 1;
}
static void drop(Text *item) {
    if(item->text) {
        texture_bytes-=item->bytes;
        glDeleteTextures(item->columns*item->rows,item->textures);
        free(item->textures);free(item->text);memset(item,0,sizeof(*item));
    }
}
static Text *get_text(const char *text,int size,int bold) {
    Text *slot=&cache[0];
    for(int i=0;i<CACHE_COUNT;i++) {
        if(cache[i].text && cache[i].size==size && cache[i].bold==bold && strcmp(cache[i].text,text)==0) {cache[i].stamp=++clock_tick;return &cache[i];}
        if(!cache[i].text || (slot->text && cache[i].stamp<slot->stamp)) slot=&cache[i];
    }
    drop(slot);
    ZgpuiBitmap bitmap=zgpui_rasterize(text,size,bold);
    int limit=native_texture_limit();
    if(!bitmap.pixels || bitmap.width<=0 || bitmap.width>MAX_TEXT_WIDTH ||
       bitmap.height<=0 || bitmap.height>MAX_TEXT_HEIGHT || !limit) {free(bitmap.pixels);return NULL;}
    int border_x=bitmap.width>limit,border_y=bitmap.height>limit;
    int tile_width=limit-border_x*2,tile_height=limit-border_y*2;
    int columns=(bitmap.width+tile_width-1)/tile_width,rows=(bitmap.height+tile_height-1)/tile_height;
    if(columns>MAX_TEXT_TILES/rows){free(bitmap.pixels);return NULL;}
    size_t bytes=0;
    for(int row=0;row<rows;row++)for(int col=0;col<columns;col++){
        int width=bitmap.width-col*tile_width,height=bitmap.height-row*tile_height;
        if(width>tile_width)width=tile_width;
        if(height>tile_height)height=tile_height;
        size_t part=(size_t)power_of_two(width+border_x*2)*power_of_two(height+border_y*2);
        if(part>TEXT_CACHE_BYTES-bytes){free(bitmap.pixels);return NULL;}
        bytes+=part;
    }
    while(bytes>TEXT_CACHE_BYTES-texture_bytes) {
        Text *oldest=NULL;
        for(int i=0;i<CACHE_COUNT;i++) if(cache[i].text && (!oldest || cache[i].stamp<oldest->stamp)) oldest=&cache[i];
        if(!oldest) break;
        drop(oldest);
    }
    size_t len=strlen(text);
    char *copy=malloc(len+1);
    GLuint *textures=calloc((size_t)columns*rows,sizeof(*textures));
    if(!copy || !textures){free(copy);free(textures);free(bitmap.pixels);return NULL;}
    memcpy(copy,text,len+1);
    for(int row=0;row<rows;row++)for(int col=0;col<columns;col++){
        int x=col*tile_width,y=row*tile_height,width=bitmap.width-x,height=bitmap.height-y;
        if(width>tile_width)width=tile_width;
        if(height>tile_height)height=tile_height;
        if(!upload_alpha(bitmap.pixels,bitmap.width,bitmap.height,x,y,width,height,
                         border_x,border_y,limit,&textures[row*columns+col])){
            glDeleteTextures(columns*rows,textures);
            free(textures);free(copy);free(bitmap.pixels);return NULL;
        }
    }
    slot->text=copy;slot->textures=textures;slot->columns=columns;slot->rows=rows;
    slot->tile_width=tile_width;slot->tile_height=tile_height;slot->border_x=border_x;
    slot->border_y=border_y;slot->bytes=bytes;
    slot->size=size;slot->bold=bold;slot->width=bitmap.width;slot->height=bitmap.height;
    slot->advance=bitmap.advance;slot->stamp=++clock_tick;
    texture_bytes+=bytes;free(bitmap.pixels);return slot;
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
typedef struct {GLuint texture;float end;} RoundedMask;
static RoundedMask rounded_masks[65];
static RoundedMask *rounded_mask(float radius) {
    int r=(int)ceilf(radius);if(r<1)r=1;if(r>64)r=64;
    RoundedMask *mask=&rounded_masks[r];
    if(mask->texture)return mask;
    int limit=native_texture_limit();if(!limit)return NULL;
    int size=r*2;if(size>limit)size=limit;
    float scale=(float)(r*2)/size;
    unsigned char *pixels=malloc((size_t)size*size);
    if(!pixels)return NULL;
    for(int y=0;y<size;y++)for(int x=0;x<size;x++){
        float dx=(x+0.5f)*scale-r,dy=(y+0.5f)*scale-r;
        float coverage=(r-sqrtf(dx*dx+dy*dy))/scale+0.5f;
        if(coverage<0)coverage=0;if(coverage>1)coverage=1;
        pixels[y*size+x]=(unsigned char)(coverage*255.0f+0.5f);
    }
    int ok=upload_alpha(pixels,size,size,0,0,size,size,0,0,limit,&mask->texture);
    free(pixels);
    if(!ok)return NULL;
    mask->end=(float)size/power_of_two(size);
    return mask;
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
    RoundedMask *mask=rounded_mask(radius);if(!mask)return;
    glEnable(GL_TEXTURE_2D);glBindTexture(GL_TEXTURE_2D,mask->texture);glBegin(GL_QUADS);
    float middle_w=width-radius*2,middle_h=height-radius*2;
    float end=mask->end,half=end/2;
    quad(x,y,radius,radius,0,0,half,half);
    quad(x+width-radius,y,radius,radius,half,0,end,half);
    quad(x,y+height-radius,radius,radius,0,half,half,end);
    quad(x+width-radius,y+height-radius,radius,radius,half,half,end,end);
    quad(x+radius,y,middle_w,radius,half,0,half,half);
    quad(x+radius,y+height-radius,middle_w,radius,half,half,half,end);
    quad(x,y+radius,radius,middle_h,0,half,half,half);
    quad(x+width-radius,y+radius,radius,middle_h,half,half,end,half);
    quad(x+radius,y+radius,middle_w,middle_h,half,half,half,half);
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
    glEnable(GL_TEXTURE_2D);color(value);
    for(int row=0;row<item->rows;row++)for(int col=0;col<item->columns;col++){
        int dx=col*item->tile_width,dy=row*item->tile_height;
        int width=item->width-dx,height=item->height-dy;
        if(width>item->tile_width)width=item->tile_width;
        if(height>item->tile_height)height=item->tile_height;
        float storage_width=(float)power_of_two(width+item->border_x*2);
        float storage_height=(float)power_of_two(height+item->border_y*2);
        glBindTexture(GL_TEXTURE_2D,item->textures[row*item->columns+col]);
        glBegin(GL_QUADS);
        quad(x+dx,y+dy,width,height,item->border_x/storage_width,item->border_y/storage_height,
             (width+item->border_x)/storage_width,(height+item->border_y)/storage_height);
        glEnd();
    }
    glDisable(GL_TEXTURE_2D);return item->advance;
}
float zgpui_measure(const char *text,int size,int bold){if(!*text)return 0;Text *item=get_text(text,size,bold);return item?item->advance:0;}
void zgpui_clip(int x,int y,int width,int height){glEnable(GL_SCISSOR_TEST);glScissor(x,screen_height-y-height,width>0?width:0,height>0?height:0);}
void zgpui_unclip(void){glDisable(GL_SCISSOR_TEST);}
void zgpui_clear_text_cache(void){for(int i=0;i<CACHE_COUNT;i++)drop(&cache[i]);clock_tick=0;}
void zgpui_gpu_destroy(void){zgpui_clear_text_cache();for(int i=1;i<=64;i++){if(rounded_masks[i].texture)glDeleteTextures(1,&rounded_masks[i].texture);memset(&rounded_masks[i],0,sizeof(rounded_masks[i]));}texture_limit=0;}
const char *zgpui_gpu_info(void){const GLubyte *s=glGetString(GL_RENDERER);return s?(const char*)s:"unknown";}
