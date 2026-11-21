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
#include <float.h>
#include <limits.h>
#include <math.h>
#include <stdlib.h>
#include <string.h>
typedef struct {
    FT_Library library;
    FT_Face faces[12];
    char *paths[12];
    int count;
    unsigned int weights[12];
    int indices[12];
    double scales[12];
    char family[256];
} FontState;
/* Bitmap-only fonts such as Noto Color Emoji cannot satisfy arbitrary pixel
 * sizes. Select a real strike and retain its ppem scale, not its line height. */
static int font_nearest_strike(FT_Face face,int size){
    int best=-1;double distance=DBL_MAX,ppem=0;
    if(!face||size<1||size>96||!face->available_sizes)return -1;
    for(int i=0;i<face->num_fixed_sizes;i++){
        double candidate=(double)face->available_sizes[i].y_ppem;
        if(candidate<=0)continue;
        double delta=fabs(candidate-(double)size*64.0);
        if(delta<distance||(delta==distance&&candidate>ppem)){
            best=i;distance=delta;ppem=candidate;
        }
    }
    return best;
}
static double font_prepare_size(FT_Face face,int size){
    if(!face||size<1||size>96)return 0;
    FT_Error error=FT_Set_Pixel_Sizes(face,0,(FT_UInt)size);
    if(!error&&FT_IS_SCALABLE(face))return 1;
    int strike=font_nearest_strike(face,size);
    if(strike>=0){
        if(FT_Select_Size(face,strike))return 0;
        double scale=(double)size*64.0/(double)face->available_sizes[strike].y_ppem;
        return isfinite(scale)&&scale>0?scale:0;
    }
    return error?0:1;
}
static double font_face_scale(const FontState *state,FT_Face face){
    for(int i=0;i<state->count;i++)if(state->faces[i]==face)return state->scales[i];
    return 0;
}
static FT_Face face_for_state(FontState *state,uint32_t cp,int size,int bold){
    if(size<1||size>96)return NULL;
    if(!state->library&&FT_Init_FreeType(&state->library))return NULL;
    bold=!!bold;
    /* Reuse an already opened face before asking fontconfig to enumerate the
       installed fonts. A face owns its file independently of the match config. */
    for(int i=0;i<state->count;i++){
        if((state->weights[i]&(1u<<bold))&&FT_Get_Char_Index(state->faces[i],cp)){
            double scale=font_prepare_size(state->faces[i],size);
            if(scale){state->scales[i]=scale;return state->faces[i];}
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
    if(face){
        double scale=font_prepare_size(face,size);
        if(!scale)return NULL;
        for(int i=0;i<state->count;i++)if(state->faces[i]==face)state->scales[i]=scale;
    }
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
static double font_glyph_advance(FT_Face face,double scale){
    /* Keep the existing hinted integer advance for ordinary outline fonts. */
    return scale==1?(double)(face->glyph->advance.x>>6):(double)face->glyph->advance.x/64.0*scale;
}
static double font_text_metrics(FontState *state,const char *text,int size,int bold,
                                double *ascent,double *descent){
    if(!text||!*text||size<1||size>96)return 0;
    FT_Face regular=face_for_state(state,'A',size,bold);if(!regular)return 0;
    double width=0;const unsigned char *p=(const unsigned char*)text;
    while(*p){
        uint32_t cp=codepoint(&p);
        FT_Face f=FT_Get_Char_Index(regular,cp)?regular:face_for_state(state,cp,size,bold);
        /* Default loading provides the same hinted advance without rendering. */
        if(f&&!FT_Load_Char(f,cp,FT_LOAD_DEFAULT)){
            double scale=font_face_scale(state,f);
            if(!isfinite(scale)||scale<=0)continue;
            width+=font_glyph_advance(f,scale);
            if(width>FLT_MAX)return FLT_MAX;
            if(width<-FLT_MAX)return -FLT_MAX;
            if(ascent&&descent){
                double top=ceil((double)f->glyph->metrics.horiBearingY/64.0*scale);
                double bottom=ceil(((double)f->glyph->metrics.height-
                                    (double)f->glyph->metrics.horiBearingY)/64.0*scale);
                if(top>*ascent)*ascent=top;
                if(bottom>*descent)*descent=bottom;
            }
        }
    }
    return width;
}
static float font_text_advance(FontState *state,const char *text,int size,int bold){
    return (float)font_text_metrics(state,text,size,bold,NULL,NULL);
}
/* FreeType width counts pixels, while pitch counts bytes. Embedded glyphs may
 * be packed, and a negative pitch stores the bottom row first. Keep the same
 * bounded coverage representation for native windows and the Rust bridge.
 * Unsupported color/subpixel formats are rejected rather than read as gray. */
static int font_bitmap_copy(ZgpuiBitmap *out,const FT_Bitmap *bitmap,int64_t x,int64_t y){
    if(!bitmap->width||!bitmap->rows)return 1;
    if(!bitmap->buffer||bitmap->width>8192||bitmap->rows>256)return 0;
    unsigned int bits,levels;
    switch(bitmap->pixel_mode){
        case FT_PIXEL_MODE_MONO:bits=1;levels=2;break;
        case FT_PIXEL_MODE_GRAY2:bits=2;levels=4;break;
        case FT_PIXEL_MODE_GRAY4:bits=4;levels=16;break;
        case FT_PIXEL_MODE_GRAY:
            bits=8;levels=bitmap->num_grays;
            if(levels<2||levels>256)return 0;
            break;
        default:return 0;
    }
    int64_t signed_pitch=bitmap->pitch;
    size_t pitch=(size_t)(signed_pitch<0?-signed_pitch:signed_pitch);
    size_t row_bytes=((size_t)bitmap->width*bits+7)/8;
    if(pitch<row_bytes||pitch>8192)return 0;
    if(x<=-(int64_t)bitmap->width||y<=-(int64_t)bitmap->rows||x>=out->width||y>=out->height)return 1;
    for(unsigned int row=0;row<bitmap->rows;row++){
        int64_t dy=y+row;
        if(dy<0||dy>=out->height)continue;
        size_t source_row=signed_pitch<0?bitmap->rows-1-row:row;
        const unsigned char *source=bitmap->buffer+source_row*pitch;
        for(unsigned int col=0;col<bitmap->width;col++){
            int64_t dx=x+col;
            if(dx<0||dx>=out->width)continue;
            size_t bit=(size_t)col*bits;
            unsigned int value=(source[bit/8]>>(8-bits-bit%8))&((1u<<bits)-1);
            if(value>=levels)value=levels-1;
            unsigned int coverage=(value*255+(levels-1)/2)/(levels-1);
            out->pixels[(size_t)dy*out->width+(size_t)dx]=(unsigned char)coverage;
        }
    }
    return 1;
}
/* Reuse the validated coverage conversion, including packed/negative-pitch
 * sources. A strike needs at most the existing 8192 * 256 source-byte bound;
 * outline glyphs retain the allocation-free unscaled path. FreeType converts
 * embedded color bitmaps to GRAY when FT_LOAD_COLOR is not requested. */
static int font_bitmap_copy_scaled(ZgpuiBitmap *out,const FT_Bitmap *bitmap,
                                   int64_t x,int64_t y,double scale){
    if(!isfinite(scale)||scale<=0)return 0;
    if(scale==1)return font_bitmap_copy(out,bitmap,x,y);
    if(!bitmap->width||!bitmap->rows)return 1;
    if(!bitmap->buffer||bitmap->width>8192||bitmap->rows>256)return 0;
    double scaled_width=ceil((double)bitmap->width*scale);
    double scaled_height=ceil((double)bitmap->rows*scale);
    if(!isfinite(scaled_width)||!isfinite(scaled_height)||
       scaled_width<1||scaled_width>8192||scaled_height<1||scaled_height>256)return 0;
    int width=(int)scaled_width,height=(int)scaled_height;
    ZgpuiBitmap source={.width=(int)bitmap->width,.height=(int)bitmap->rows};
    source.pixels=malloc((size_t)source.width*(size_t)source.height);
    if(!source.pixels)return 0;
    if(!font_bitmap_copy(&source,bitmap,0,0)){free(source.pixels);return 0;}
    /* Clip before addition: even INT64_MIN/MAX origins cannot wrap around. */
    if(x<=-(int64_t)width||y<=-(int64_t)height||x>=out->width||y>=out->height){
        free(source.pixels);return 1;
    }
    for(int row=0;row<height;row++){
        int64_t dy=y+row;
        if(dy<0||dy>=out->height)continue;
        double sy=((double)row+0.5)/scale-0.5;
        sy=fmax(0,fmin(sy,(double)source.height-1));
        int y0=(int)sy,y1=y0+1<source.height?y0+1:y0;
        double fy=sy-y0;
        for(int col=0;col<width;col++){
            int64_t dx=x+col;
            if(dx<0||dx>=out->width)continue;
            double sx=((double)col+0.5)/scale-0.5;
            sx=fmax(0,fmin(sx,(double)source.width-1));
            int x0=(int)sx,x1=x0+1<source.width?x0+1:x0;
            double fx=sx-x0;
            const unsigned char *upper=source.pixels+(size_t)y0*source.width;
            const unsigned char *lower=source.pixels+(size_t)y1*source.width;
            double top=upper[x0]*(1-fx)+upper[x1]*fx;
            double bottom=lower[x0]*(1-fx)+lower[x1]*fx;
            out->pixels[(size_t)dy*out->width+(size_t)dx]=
                (unsigned char)(top*(1-fy)+bottom*fy+0.5);
        }
    }
    free(source.pixels);return 1;
}
static ZgpuiBitmap font_rasterize(FontState *state,const char *text,int size,int bold){
    ZgpuiBitmap out={0};if(!text||size<1||size>96)return out;FT_Face regular=face_for_state(state,'A',size,bold);if(!regular)return out;
    double scale=font_face_scale(state,regular);
    double ascent=ceil((double)regular->size->metrics.ascender/64.0*scale);
    double descent=ceil(-(double)regular->size->metrics.descender/64.0*scale);
    double advance=font_text_metrics(state,text,size,bold,&ascent,&descent);
    if(!isfinite(advance)||advance<0||advance>8190||!isfinite(ascent)||!isfinite(descent)||
       ascent<0||descent<0||ascent+descent+2>256)return out;
    int width=(int)ceil(advance)+2,height=(int)(ascent+descent)+2;
    if(width<1||height<1)return out;
    out.pixels=calloc((size_t)width,height);if(!out.pixels)return out;
    out.width=width;out.height=height;out.advance=(float)advance;
    double x=1;const unsigned char *p=(const unsigned char*)text;
    while(*p){uint32_t cp=codepoint(&p);FT_Face f=FT_Get_Char_Index(regular,cp)?regular:face_for_state(state,cp,size,bold);if(!f||FT_Load_Char(f,cp,FT_LOAD_RENDER))continue;FT_GlyphSlot g=f->glyph;
        scale=font_face_scale(state,f);
        double left=floor(x+(double)g->bitmap_left*scale+0.5);
        double top=floor(ascent-(double)g->bitmap_top*scale+1.5);
        if(!isfinite(left)||!isfinite(top)||left<INT_MIN||left>INT_MAX||top<INT_MIN||top>INT_MAX||
           !font_bitmap_copy_scaled(&out,&g->bitmap,(int64_t)left,(int64_t)top,scale)){
            free(out.pixels);return (ZgpuiBitmap){0};
        }
        x+=font_glyph_advance(f,scale);
    }return out;
}
#endif
