/* Shared Linux font implementation for native windows and zgpui-raster.
 * Each caller owns its FontState; no display or process-global font config.
 * Keep native wrappers and the headless adapter on this one implementation. */
#ifndef ZGPUI_FONT_LINUX_H
#define ZGPUI_FONT_LINUX_H
#include "zgpui.h"
#include <fontconfig/fontconfig.h>
#ifdef __GLIBC__
#include <malloc.h>
#endif
#include <ft2build.h>
#include FT_FREETYPE_H
#include <stdlib.h>
#include <string.h>
typedef struct {
    FT_Library library;
    FT_Face faces[12];
    char *paths[12];
    int count;
    unsigned int weights[12];
    int indices[12];
    char family[256];
} FontState;
static FT_Face face_for_state(FontState *state,uint32_t cp,int size,int bold){
    if(!state->library&&FT_Init_FreeType(&state->library))return NULL;
    bold=!!bold;
    /* Reuse an already opened face before asking fontconfig to enumerate the
       installed fonts. A face owns its file independently of the match config. */
    for(int i=0;i<state->count;i++){
        if((state->weights[i]&(1u<<bold))&&FT_Get_Char_Index(state->faces[i],cp)){
            FT_Set_Pixel_Sizes(state->faces[i],0,size);
            return state->faces[i];
        }
    }
    /* A private, short-lived config avoids pinning every system font cache in
       an otherwise idle window, and does not mutate a host application's config. */
    FcConfig *config=FcInitLoadConfigAndFonts();
    if(!config)return NULL;
    FcPattern *pattern=FcPatternCreate();
    FcCharSet *charset=FcCharSetCreate();
    if(!pattern||!charset){
        if(pattern)FcPatternDestroy(pattern);
        if(charset)FcCharSetDestroy(charset);
        FcConfigDestroy(config);
        return NULL;
    }
    FcPatternAddString(pattern,FC_FAMILY,(const FcChar8*)(state->family[0]?state->family:"sans"));
    if(state->family[0] && strcmp(state->family,"monospace"))
        FcPatternAddString(pattern,FC_FAMILY,(const FcChar8*)"monospace");
    FcPatternAddInteger(pattern,FC_WEIGHT,bold?FC_WEIGHT_BOLD:FC_WEIGHT_REGULAR);
    FcCharSetAddChar(charset,cp);
    FcPatternAddCharSet(pattern,FC_CHARSET,charset);
    FcCharSetDestroy(charset);
    /* Match the actual raster size. Configuration substitution keeps system and
       user aliases, including FC_LANG/LC_ALL/LC_CTYPE/LANG preferences. */
    FcPatternAddInteger(pattern,FC_SLANT,FC_SLANT_ROMAN);
    FcPatternAddInteger(pattern,FC_WIDTH,FC_WIDTH_NORMAL);
    FcPatternAddBool(pattern,FC_SCALABLE,FcTrue);
    FcPatternAddDouble(pattern,FC_PIXEL_SIZE,(double)size);
    FcConfigSubstitute(config,pattern,FcMatchPattern);
#if FC_VERSION >= 21700
    /* The legacy wrapper passes NULL and initializes a process-wide config in
       Fontconfig 2.17+. Keep all default substitution on our private config. */
    FcConfigSetDefaultSubstitute(config,pattern);
#else
    /* Before 2.17 defaults are independent of FcConfig; this preserves locale
       and user matching semantics without requiring a newer system library. */
    FcDefaultSubstitute(pattern);
#endif
    FcResult result;
    FcPattern *match=FcFontMatch(config,pattern,&result);
    FcPatternDestroy(pattern);
    FT_Face face=NULL;
    if(match){
        FcChar8 *path=NULL;
        int index=0;
        FcPatternGetString(match,FC_FILE,0,&path);
        FcPatternGetInteger(match,FC_INDEX,0,&index);
        if(path){
            for(int i=0;i<state->count;i++){
                if(state->indices[i]==index&&strcmp(state->paths[i],(const char*)path)==0){
                    state->weights[i]|=1u<<bold;
                    face=state->faces[i];break;
                }
            }
            if(!face&&state->count<12&&!FT_New_Face(state->library,(const char*)path,index,&face)){
                char *owned=strdup((const char*)path);
                if(owned){
                    state->paths[state->count]=owned;
                    state->indices[state->count]=index;
                    state->weights[state->count]=1u<<bold;
                    state->faces[state->count++]=face;
                }else{
                    FT_Done_Face(face);face=NULL;
                }
            }
        }
        FcPatternDestroy(match);
    }
    FcConfigDestroy(config);
#ifdef __GLIBC__
    /* Return the now unused enumeration arena, after retaining only FT faces. */
    malloc_trim(0);
#endif
    if(face)FT_Set_Pixel_Sizes(face,0,size);
    return face;
}
static void font_state_destroy(FontState *state){
    for(int i=0;i<state->count;i++){FT_Done_Face(state->faces[i]);free(state->paths[i]);}
    if(state->library)FT_Done_FreeType(state->library);
    memset(state,0,sizeof(*state));
}
static uint32_t codepoint(const unsigned char **p){
    uint32_t c=*(*p)++;if(c<0x80)return c;unsigned int count=c<0xe0?1:c<0xf0?2:3;c&=(1u<<(6-count))-1;
    while(count--&&**p){c=(c<<6)|(*(*p)++&63);}return c;
}
static float font_text_advance(FontState *state,const char *text,int size,int bold){
    if(!text||!*text||size<1||size>96)return 0;
    FT_Face regular=face_for_state(state,'A',size,bold);if(!regular)return 0;
    double width=0;const unsigned char *p=(const unsigned char*)text;
    while(*p){
        uint32_t cp=codepoint(&p);
        FT_Face f=FT_Get_Char_Index(regular,cp)?regular:face_for_state(state,cp,size,bold);
        /* Default loading provides the same hinted advance without rendering. */
        if(f&&!FT_Load_Char(f,cp,FT_LOAD_DEFAULT))width+=(double)(f->glyph->advance.x>>6);
    }
    return (float)width;
}
static ZgpuiBitmap font_rasterize(FontState *state,const char *text,int size,int bold){
    ZgpuiBitmap out={0};if(size<1||size>96)return out;FT_Face regular=face_for_state(state,'A',size,bold);if(!regular)return out;
    int ascent=(int)(regular->size->metrics.ascender>>6),descent=(int)(-regular->size->metrics.descender>>6);
    float advance=font_text_advance(state,text,size,bold);
    if(advance<0||advance>8190)return out;
    int width=(int)advance+2,height=ascent+descent+2;
    if(width<1||height<1||height>256)return out;
    out.pixels=calloc((size_t)width,height);if(!out.pixels)return out;
    out.width=width;out.height=height;out.advance=advance;
    int x=1;const unsigned char *p=(const unsigned char*)text;
    while(*p){uint32_t cp=codepoint(&p);FT_Face f=FT_Get_Char_Index(regular,cp)?regular:face_for_state(state,cp,size,bold);if(!f||FT_Load_Char(f,cp,FT_LOAD_RENDER))continue;FT_GlyphSlot g=f->glyph;
        for(unsigned int row=0;row<g->bitmap.rows;row++)for(unsigned int col=0;col<g->bitmap.width;col++){int dx=x+g->bitmap_left+(int)col,dy=ascent-g->bitmap_top+(int)row+1;if(dx>=0&&dx<width&&dy>=0&&dy<height)out.pixels[(size_t)dy*width+dx]=g->bitmap.buffer[row*(unsigned int)g->bitmap.pitch+col];}
        x+=(int)(g->advance.x>>6);
    }return out;
}
#endif
