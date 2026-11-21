#!/usr/bin/env python3
"""Check Linux font matching, real glyphs and ownership with installed fonts.

Requires scalable Latin/CJK fonts, for example Liberation Sans and Noto Sans CJK.
Install Noto Color Emoji to exercise the fixed-strike regression (otherwise that
one test explicitly skips).
"""
import argparse
import hashlib
import json
import os
import select
from pathlib import Path
import shlex
import subprocess
import sys
import tempfile
import unittest
from xml.sax.saxutils import escape

ROOT = Path(__file__).resolve().parents[1]
SOURCE = r"""
#include <assert.h>
#include <fontconfig/fontconfig.h>
#include <limits.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
static int config_loads, config_destroys, fail_next_load;
static FcConfig *tracked_load(void) {
    if(fail_next_load){fail_next_load=0;return NULL;}
    ++config_loads;
    return FcInitLoadConfigAndFonts();
}
static void tracked_destroy(FcConfig *config) {
    ++config_destroys;
    FcConfigDestroy(config);
}
#define FcInitLoadConfigAndFonts tracked_load
#define FcConfigDestroy tracked_destroy
#include "linux.c"
#undef FcInitLoadConfigAndFonts
#undef FcConfigDestroy

static FcPattern *reference_match(uint32_t cp, int size, int bold) {
    FcConfig *config=FcInitLoadConfigAndFonts();
    FcPattern *p=FcPatternCreate();
    FcCharSet *chars=FcCharSetCreate();
    assert(config && p && chars);
    assert(FcCharSetAddChar(chars,cp));
    assert(FcPatternAddString(p,FC_FAMILY,(const FcChar8 *)"sans"));
    assert(FcPatternAddInteger(p,FC_WEIGHT,bold?FC_WEIGHT_BOLD:FC_WEIGHT_REGULAR));
    assert(FcPatternAddCharSet(p,FC_CHARSET,chars));
    assert(FcPatternAddInteger(p,FC_SLANT,FC_SLANT_ROMAN));
    assert(FcPatternAddInteger(p,FC_WIDTH,FC_WIDTH_NORMAL));
    assert(FcPatternAddBool(p,FC_SCALABLE,FcTrue));
    assert(FcPatternAddDouble(p,FC_PIXEL_SIZE,(double)size));
    assert(FcConfigSubstitute(config,p,FcMatchPattern));
#if FC_VERSION >= 21700
    FcConfigSetDefaultSubstitute(config,p);
#else
    FcDefaultSubstitute(p);
#endif
    FcResult result;
    FcPattern *match=FcFontMatch(config,p,&result);
    assert(match && result==FcResultMatch);
    FcCharSetDestroy(chars);
    FcPatternDestroy(p);
    FcConfigDestroy(config);
    return match;
}
static uint64_t bitmap_hash(ZgpuiBitmap b) {
    assert(b.pixels && b.width>2 && b.height>2 && b.advance>0);
    uint64_t h=1469598103934665603ULL;
    size_t ink=0;
    for(size_t i=0;i<(size_t)b.width*(size_t)b.height;i++) {
        ink+=b.pixels[i]!=0;
        h=(h^b.pixels[i])*1099511628211ULL;
    }
    assert(ink>0);
    return h;
}
static void bitmap_formats(void) {
    unsigned char pixels[18]={0};
    ZgpuiBitmap out={.pixels=pixels,.width=9,.height=2};
    unsigned char mono[]={0x80,0x80,0x55,0x00};
    unsigned char expected[]={255,0,0,0,0,0,0,0,255,0,255,0,255,0,255,0,255,0};
    FT_Bitmap bitmap={.width=9,.rows=2,.pitch=2,.buffer=mono,.pixel_mode=FT_PIXEL_MODE_MONO};
    assert(font_bitmap_copy(&out,&bitmap,0,0));
    assert(!memcmp(pixels,expected,sizeof(pixels)));
    unsigned char bottom_first[]={0x55,0x00,0x80,0x80};
    bitmap.buffer=bottom_first;bitmap.pitch=-2;
    memset(pixels,0,sizeof(pixels));
    assert(font_bitmap_copy(&out,&bitmap,0,0));
    assert(!memcmp(pixels,expected,sizeof(pixels)));

    out.width=5;out.height=1;
    unsigned char gray2[]={0x1b,0xc0};
    bitmap=(FT_Bitmap){.width=5,.rows=1,.pitch=2,.buffer=gray2,.pixel_mode=FT_PIXEL_MODE_GRAY2};
    assert(font_bitmap_copy(&out,&bitmap,0,0));
    assert(!memcmp(pixels,(unsigned char[]){0,85,170,255,255},5));
    unsigned char gray4[]={0x1f,0x80};
    bitmap=(FT_Bitmap){.width=3,.rows=1,.pitch=2,.buffer=gray4,.pixel_mode=FT_PIXEL_MODE_GRAY4};
    assert(font_bitmap_copy(&out,&bitmap,0,0));
    assert(!memcmp(pixels,(unsigned char[]){17,255,136},3));

    unsigned char gray[]={7,1,0,99,0,3,7,99};
    out.width=3;out.height=2;
    bitmap=(FT_Bitmap){.width=3,.rows=2,.pitch=-4,.buffer=gray,.pixel_mode=FT_PIXEL_MODE_GRAY,.num_grays=8};
    assert(font_bitmap_copy(&out,&bitmap,0,0));
    assert(!memcmp(pixels,(unsigned char[]){0,109,255,255,36,0},6));
    memset(pixels,42,sizeof(pixels));
    assert(font_bitmap_copy(&out,&bitmap,-1,-1));
    assert(!memcmp(pixels,(unsigned char[]){36,0,42,42,42,42},6));

    memset(pixels,42,sizeof(pixels));
    bitmap.pitch=2;assert(!font_bitmap_copy(&out,&bitmap,0,0));
    bitmap.pitch=INT_MIN;assert(!font_bitmap_copy(&out,&bitmap,0,0));
    bitmap.pitch=4;bitmap.num_grays=1;assert(!font_bitmap_copy(&out,&bitmap,0,0));
    bitmap.num_grays=256;bitmap.pixel_mode=FT_PIXEL_MODE_LCD;
    assert(!font_bitmap_copy(&out,&bitmap,0,0));
    bitmap.pixel_mode=FT_PIXEL_MODE_GRAY;bitmap.buffer=NULL;
    assert(!font_bitmap_copy(&out,&bitmap,0,0));
    bitmap.buffer=gray;bitmap.width=8193;assert(!font_bitmap_copy(&out,&bitmap,0,0));
    bitmap.width=3;bitmap.rows=257;assert(!font_bitmap_copy(&out,&bitmap,0,0));
    for(size_t i=0;i<sizeof(pixels);i++)assert(pixels[i]==42);
    bitmap=(FT_Bitmap){0};assert(font_bitmap_copy(&out,&bitmap,0,0));
}
static void bitmap_scaling(void) {
    unsigned char pixels[16]={0};
    ZgpuiBitmap out={.pixels=pixels,.width=4,.height=4};
    unsigned char gray[]={0,64,128,255};
    FT_Bitmap bitmap={.width=2,.rows=2,.pitch=2,.buffer=gray,
                      .pixel_mode=FT_PIXEL_MODE_GRAY,.num_grays=256};
    const unsigned char expected[]={0,16,48,64,32,52,92,112,
                                    96,124,179,207,128,160,223,255};
    assert(font_bitmap_copy_scaled(&out,&bitmap,0,0,2));
    assert(!memcmp(pixels,expected,sizeof(expected)));
    memset(pixels,42,sizeof(pixels));
    assert(font_bitmap_copy_scaled(&out,&bitmap,0,0,0.5));
    assert(pixels[0]==112);
    for(size_t i=1;i<sizeof(pixels);i++)assert(pixels[i]==42);

    unsigned char bottom_first[]={128,255,99,0,64,99};
    bitmap.buffer=bottom_first;bitmap.pitch=-3;
    assert(font_bitmap_copy_scaled(&out,&bitmap,0,0,2));
    assert(!memcmp(pixels,expected,sizeof(expected)));
    memset(pixels,42,sizeof(pixels));
    assert(font_bitmap_copy_scaled(&out,&bitmap,-2,-2,2));
    assert(pixels[0]==179&&pixels[1]==207&&pixels[4]==223&&pixels[5]==255);
    assert(pixels[2]==42&&pixels[6]==42&&pixels[8]==42);

    unsigned char mono[]={0xa0,0x50};
    bitmap=(FT_Bitmap){.width=4,.rows=2,.pitch=1,.buffer=mono,.pixel_mode=FT_PIXEL_MODE_MONO};
    memset(pixels,42,sizeof(pixels));
    assert(font_bitmap_copy_scaled(&out,&bitmap,0,0,0.5));
    assert(pixels[0]==128&&pixels[1]==128&&pixels[2]==42);

    memset(pixels,42,sizeof(pixels));
    const double invalid[]={0,-1,NAN,INFINITY,-INFINITY,DBL_MAX};
    for(size_t i=0;i<sizeof(invalid)/sizeof(invalid[0]);i++)
        assert(!font_bitmap_copy_scaled(&out,&bitmap,0,0,invalid[i]));
    bitmap.pitch=INT_MIN;assert(!font_bitmap_copy_scaled(&out,&bitmap,0,0,0.5));
    bitmap.pitch=1;bitmap.width=8192;
    assert(!font_bitmap_copy_scaled(&out,&bitmap,0,0,2));
    bitmap.width=4;bitmap.rows=256;
    assert(!font_bitmap_copy_scaled(&out,&bitmap,0,0,2));
    bitmap.rows=2;bitmap.pixel_mode=FT_PIXEL_MODE_BGRA;
    assert(!font_bitmap_copy_scaled(&out,&bitmap,0,0,0.5));
    bitmap.pixel_mode=FT_PIXEL_MODE_MONO;
    for(int scale=1;scale<=2;scale++){
        assert(font_bitmap_copy_scaled(&out,&bitmap,INT64_MIN,INT64_MAX,scale));
        assert(font_bitmap_copy_scaled(&out,&bitmap,INT64_MAX,INT64_MIN,scale));
    }
    for(size_t i=0;i<sizeof(pixels);i++)assert(pixels[i]==42);

    FT_Bitmap_Size strikes[]={
        {.height=100,.y_ppem=0}, {.height=100,.y_ppem=8*64+32},
        {.height=1,.y_ppem=16*64}, {.height=1,.y_ppem=32*64},
        {.height=100,.y_ppem=-64},
    };
    struct FT_FaceRec_ face={.num_fixed_sizes=5,.available_sizes=strikes};
    assert(font_nearest_strike(&face,12)==1); /* 8.5 ppem, not height=100. */
    assert(font_nearest_strike(&face,16)==2);
    assert(font_nearest_strike(&face,24)==3); /* Equal distance: prefer downsampling. */
    assert(font_nearest_strike(&face,0)==-1&&font_nearest_strike(&face,97)==-1);
    face.num_fixed_sizes=0;assert(font_nearest_strike(&face,16)==-1);
}
static int fixed_emoji(void) {
    /* Missing test data may skip; a broken application size selection may not. */
    FcConfig *config=FcInitLoadConfigAndFonts();
    FcPattern *pattern=FcPatternCreate();
    FcObjectSet *objects=FcObjectSetBuild(FC_FAMILY,NULL);
    assert(config&&pattern&&objects);
    assert(FcPatternAddString(pattern,FC_FAMILY,(const FcChar8 *)"Noto Color Emoji"));
    FcFontSet *installed=FcFontList(config,pattern,objects);
    int available=installed&&installed->nfont>0;
    if(installed)FcFontSetDestroy(installed);
    FcObjectSetDestroy(objects);FcPatternDestroy(pattern);FcConfigDestroy(config);
    if(!available){
        fputs("SKIP: install Noto Color Emoji for the fixed-strike test\n",stderr);
        return 77;
    }
    /* Exercise the application's default sans + emoji fallback. Requesting
     * an emoji-only family for the Latin primary is a different font policy. */
    FontState state={0};
    FT_Face face=face_for_state(&state,0x1f44b,16,0);
    assert(face&&FT_Get_Char_Index(face,0x1f44b)&&FT_HAS_FIXED_SIZES(face));
    const int sizes[]={12,16,24,32,48,96};
    const char *wave="\xf0\x9f\x91\x8b";
    float at_sixteen=0;
    for(int bold=0;bold<2;bold++)for(size_t i=0;i<sizeof(sizes)/sizeof(sizes[0]);i++){
        int size=sizes[i];
        face=face_for_state(&state,0x1f44b,size,bold);assert(face);
        assert(!FT_Load_Char(face,0x1f44b,FT_LOAD_RENDER));
        /* No color ABI: FreeType supplies the grayscale coverage fallback. */
        assert(face->glyph->bitmap.pixel_mode==FT_PIXEL_MODE_GRAY);
        assert(face->glyph->bitmap.width&&face->glyph->bitmap.rows);
        ZgpuiBitmap bitmap=font_rasterize(&state,wave,size,bold);
        assert(bitmap_hash(bitmap));
        float advance=font_text_advance(&state,wave,size,bold);
        assert(advance==bitmap.advance&&advance>size/2.0&&advance<size*2.0);
        assert(bitmap.width>=advance&&bitmap.width<=size*2+2);
        assert(bitmap.height<=size*2+4);
        if(size==16)at_sixteen=advance;
        if(size==32)assert(fabs((double)advance-2.0*at_sixteen)<1.0);
        free(bitmap.pixels);
        bitmap=font_rasterize(&state,"你好，世界 👋 — café",size,bold);
        assert(bitmap_hash(bitmap));
        assert(bitmap.advance==font_text_advance(&state,"你好，世界 👋 — café",size,bold));
        free(bitmap.pixels);
    }
    ZgpuiBitmap small=font_rasterize(&state,wave,16,0);
    uint64_t hash=bitmap_hash(small);int width=small.width,height=small.height;
    free(small.pixels);
    int loaded=config_loads;
    for(int i=0;i<12;i++){
        ZgpuiBitmap large=font_rasterize(&state,wave,48,0);
        assert(bitmap_hash(large));free(large.pixels);
        small=font_rasterize(&state,wave,16,0);
        assert(small.width==width&&small.height==height&&bitmap_hash(small)==hash);
        free(small.pixels);
    }
    assert(config_loads==loaded&&config_loads==config_destroys);
    font_state_destroy(&state);
    return 0;
}
static void measurement(void) {
    const char *samples[]={"Window Backup Play 012345", "Minecraft 世界 备份 启动器",
        "日本語 한국어", "Cafe\xcc\x81 \xce\xa9 \xd0\x96", "AV fi ffi"};
    for(int bold=0;bold<2;bold++)for(int size=12;size<=32;size+=10)
        for(size_t i=0;i<sizeof(samples)/sizeof(samples[0]);i++){
            float advance=zgpui_text_advance(samples[i],size,bold);
            /* Independently preserve the pre-optimization rendered-glyph width. */
            float rendered_advance=0;
            FT_Face regular=face_for_state(&fonts,'A',size,bold);assert(regular);
            const unsigned char *p=(const unsigned char*)samples[i];
            while(*p){
                uint32_t cp=codepoint(&p);
                FT_Face face=FT_Get_Char_Index(regular,cp)?regular:face_for_state(&fonts,cp,size,bold);
                if(face&&!FT_Load_Char(face,cp,FT_LOAD_RENDER))
                    rendered_advance+=(float)(face->glyph->advance.x>>6);
            }
            ZgpuiBitmap bitmap=zgpui_rasterize(samples[i],size,bold);
            assert(bitmap_hash(bitmap));
            assert(advance==rendered_advance);
            assert(advance==bitmap.advance && advance==zgpui_measure(samples[i],size,bold));
            free(bitmap.pixels);
        }
    char long_text[16001];memset(long_text,'W',sizeof(long_text)-1);long_text[16000]=0;
    /* The bitmap limit must not turn an overflowing label into zero width. */
    assert(zgpui_text_advance(long_text,20,0)>8192);
    assert(zgpui_measure(long_text,20,0)>8192);
    ZgpuiBitmap large=zgpui_rasterize(long_text,20,0);assert(!large.pixels);
    assert(zgpui_text_advance("",20,0)==0);
    assert(zgpui_text_advance("invalid",0,0)==0);
    zgpui_gpu_destroy();zgpui_font_shutdown();
    assert(config_loads==config_destroys);
}
static void matching(void) {
    const uint32_t samples[]={0x41,0xe9,0x3a9,0x416,0x9aa8,0x6f22,0x3042,0xac00};
    for(size_t s=0;s<sizeof(samples)/sizeof(samples[0]);s++) {
        for(int bold=0;bold<2;bold++) {
            zgpui_font_shutdown();
            FcPattern *expected=reference_match(samples[s],19,bold);
            FcChar8 *path=NULL;int index=0;
            assert(FcPatternGetString(expected,FC_FILE,0,&path)==FcResultMatch);
            assert(FcPatternGetInteger(expected,FC_INDEX,0,&index)==FcResultMatch);
            FT_Face face=face_for_state(&fonts,samples[s],19,bold);
            assert(face && fonts.count==1);
            if(strcmp(fonts.paths[0],(char *)path) || fonts.indices[0]!=index) {
                fprintf(stderr,"U+%04x weight=%d expected %s#%d, got %s#%d\n",
                        samples[s],bold,path,index,fonts.paths[0],fonts.indices[0]);
                abort();
            }
            /* These fixtures require a font that actually contains the glyph. */
            assert(FT_Get_Char_Index(face,samples[s]));
            assert(!FT_Load_Char(face,samples[s],FT_LOAD_RENDER));
            assert(face->glyph->bitmap.width && face->glyph->bitmap.rows);
            FcPatternDestroy(expected);
        }
    }
    zgpui_font_shutdown();
    assert(config_loads==config_destroys);
}
static void caching(void) {
    ZgpuiBitmap first=zgpui_rasterize("Window Backup Play 012345",18,0);
    uint64_t expected=bitmap_hash(first);
    int width=first.width,height=first.height;
    free(first.pixels);
    assert(config_loads==1 && config_destroys==1);
    for(int i=0;i<12;i++) {
        ZgpuiBitmap large=zgpui_rasterize("Window Backup Play 012345",32,0);
        assert(bitmap_hash(large));free(large.pixels);
        ZgpuiBitmap again=zgpui_rasterize("Window Backup Play 012345",18,0);
        assert(again.width==width && again.height==height);
        assert(bitmap_hash(again)==expected);free(again.pixels);
    }
    assert(config_loads==1);
    ZgpuiBitmap cjk=zgpui_rasterize("Minecraft 世界 备份 启动器",18,0);
    uint64_t cjk_hash=bitmap_hash(cjk);
    free(cjk.pixels);
    int loaded=config_loads;
    /* The primary sans face may already cover CJK. */
    assert(loaded>=1);
    for(int i=0;i<12;i++) {
        cjk=zgpui_rasterize("Minecraft 世界 备份 启动器",18,0);
        assert(bitmap_hash(cjk)==cjk_hash);free(cjk.pixels);
    }
    assert(config_loads==loaded && config_loads==config_destroys);
}
static void shared_weight(void) {
    FT_Face regular=face_for_state(&fonts,'A',18,0);
    FT_Face bold=face_for_state(&fonts,'A',18,1);
    assert(regular && bold && regular==bold && fonts.count==1);
    int loaded=config_loads;
    assert(face_for_state(&fonts,'Z',18,1)==bold);
    assert(face_for_state(&fonts,'Z',24,0)==regular);
    assert(config_loads==loaded && config_loads==config_destroys);
}
static void host_config(void) {
    FcConfig *host=FcInitLoadConfigAndFonts();
    assert(host && FcConfigSetCurrent(host));
    assert(FcConfigGetCurrent()==host);
    FcFontSet *fonts=FcConfigGetFonts(host,FcSetSystem);
    assert(fonts && fonts->nfont>0);
    int count=fonts->nfont;
    ZgpuiBitmap b=zgpui_rasterize("Host configuration remains usable",20,0);
    assert(bitmap_hash(b));free(b.pixels);
    zgpui_font_shutdown();
    assert(FcConfigGetCurrent()==host);
    assert(FcConfigGetFonts(host,FcSetSystem)->nfont==count);
    FcConfigDestroy(host);
}
static void lifecycle(void) {
    for(int i=0;i<24;i++) {
        ZgpuiBitmap b=zgpui_rasterize("World 世界 Backup 备份",16+i%4,i%2);
        assert(bitmap_hash(b));free(b.pixels);
        zgpui_font_shutdown();
        assert(fonts.count==0 && !fonts.library);
        zgpui_font_shutdown();
    }
    assert(config_loads==config_destroys);
}
static void family_transaction(void) {
#ifdef ZGPUI_SOFTWARE
    /* No display operations are needed to exercise the prepare/commit boundary. */
    ZgpuiWindow window={0};window.display=(Display *)(uintptr_t)1;
    FcConfig *host=FcInitLoadConfigAndFonts();assert(host && FcConfigSetCurrent(host));
    zgpui_begin(128,64,0x112233ff);int width=0,height=0;
    uint32_t *frame=zgpui_software_pixels(&width,&height);
    assert(frame && width==128 && height==64 && frame[0]==0x112233);
    float narrow=zgpui_measure("iiiiiiii",20,0),wide=zgpui_measure("WWWWWWWW",20,0);
    assert(narrow>0 && wide>0);
    FT_Library old_library=fonts.library;FT_Face old_face=fonts.faces[0];
    char error[256]={0};fail_next_load=1;
    assert(!zgpui_set_font_family(&window,"monospace",error,sizeof(error)) && error[0]);
    assert(fonts.library==old_library && fonts.faces[0]==old_face && !fonts.family[0]);
    assert(zgpui_software_pixels(&width,&height)==frame && frame[0]==0x112233);
    assert(zgpui_measure("iiiiiiii",20,0)==narrow && zgpui_measure("WWWWWWWW",20,0)==wide);
    for(int i=0;i<12;i++){
        assert(zgpui_set_font_family(&window,"monospace",error,sizeof(error)));
        assert(zgpui_software_pixels(&width,&height)==frame && frame[0]==0x112233);
        assert(zgpui_measure("iiiiiiii",20,0)==zgpui_measure("WWWWWWWW",20,0));
        ZgpuiBitmap cjk=zgpui_rasterize("中文 世界 备份",20,i%2);
        assert(bitmap_hash(cjk));free(cjk.pixels);
        assert(fonts.count<=12 && FcConfigGetCurrent()==host);
        assert(zgpui_set_font_family(&window,"",error,sizeof(error)));
        assert(zgpui_measure("iiiiiiii",20,0)==narrow && zgpui_measure("WWWWWWWW",20,0)==wide);
    }
    assert(zgpui_set_font_family(&window,"zgpui-uninstalled-font-97493",error,sizeof(error)));
    assert(zgpui_measure("iiiiiiii",20,0)==zgpui_measure("WWWWWWWW",20,0));
    zgpui_gpu_destroy();zgpui_font_shutdown();
    assert(!fonts.count && !fonts.library && !fonts.family[0]);
    assert(config_loads==config_destroys);FcConfigDestroy(host);
#else
    abort(); /* The unit fixture is explicitly the software renderer. */
#endif
}
static void window_probe(void) {
    char error[256]={0};
    ZgpuiWindow *w=zgpui_open("zgpui-font-regression",1120,760,error,sizeof(error));
    if(!w){fprintf(stderr,"%s\n",error);exit(1);}
    zgpui_begin(1120,760,0x101112ff);
    zgpui_text("Font rendering",44,36,28,1,0xe8e6e3ff);
    zgpui_text("Regular: Minecraft Worlds 0123456789",44,112,20,0,0xe8e6e3ff);
    zgpui_text("Bold: Minecraft Worlds 0123456789",44,164,20,1,0xe8e6e3ff);
    zgpui_text("中文：世界 备份 资源 启动器",44,232,22,0,0xe8e6e3ff);
    zgpui_text("粗体：世界 备份 资源 启动器",44,286,22,1,0xe8e6e3ff);
    zgpui_text("日本語：世界を保存 · 한국어: 세계 백업",44,354,22,0,0xe8e6e3ff);
    zgpui_text("Café déjà vu · Ελληνικά · Кириллица",44,422,22,0,0xe8e6e3ff);
    zgpui_text("12 px: World 世界 备份",44,496,12,0,0xe8e6e3ff);
    zgpui_text("32 px: World 世界 备份",44,542,32,1,0xe8e6e3ff);
    zgpui_text("Emoji: 你好，世界 👋 — café",44,614,22,0,0xe8e6e3ff);
    zgpui_swap(w);
    puts("READY");fflush(stdout);
    for(int i=0;i<120;i++) {
        ZgpuiEvent event;
        if(zgpui_event(w,&event,100) && event.kind==7)break;
    }
    zgpui_close(w);
}
int main(int argc,char **argv) {
    assert(argc==2);
    if(!strcmp(argv[1],"measurement"))measurement();
    else if(!strcmp(argv[1],"bitmap-formats"))bitmap_formats();
    else if(!strcmp(argv[1],"bitmap-scaling"))bitmap_scaling();
    else if(!strcmp(argv[1],"fixed-emoji")){int result=fixed_emoji();if(result){FcFini();return result;}}
    else if(!strcmp(argv[1],"matching"))matching();
    else if(!strcmp(argv[1],"caching"))caching();
    else if(!strcmp(argv[1],"shared-weight"))shared_weight();
    else if(!strcmp(argv[1],"host-config"))host_config();
    else if(!strcmp(argv[1],"lifecycle"))lifecycle();
    else if(!strcmp(argv[1],"family-transaction"))family_transaction();
    else if(!strcmp(argv[1],"window"))window_probe();
    else return 2;
    zgpui_font_shutdown();
    FcFini(); /* The test process, not the library, owns Fontconfig global teardown. */
    return 0;
}
"""

def build_fixture(directory, *, sanitize=True, software=True):
    directory=Path(directory)
    directory.mkdir(parents=True,exist_ok=True)
    fixture=directory/"font-fixture.c"
    fixture.write_text(SOURCE)
    binary=directory/"font-test"
    packages=["fontconfig","freetype2","x11",*([] if software else ["gl"])]
    flags=shlex.split(subprocess.check_output(
        ["pkg-config","--cflags","--libs",*packages],text=True))
    command=[os.environ.get("CC","cc"),"-std=c11","-D_DEFAULT_SOURCE",
             "-O1" if sanitize else "-O2","-g","-Wall","-Wextra","-Werror",
             "-Wno-misleading-indentation",*(["-DZGPUI_SOFTWARE"] if software else []),
             "-I",str(ROOT/"native"),str(fixture),str(ROOT/"native"/("draw_software.c" if software else "draw.c")),
             *flags,"-lm","-o",str(binary)]
    if sanitize:
        command[1:1]=["-fsanitize=address,undefined","-fno-omit-frame-pointer"]
    subprocess.run(command,check=True)
    return binary


def check_window(features, output):
    """Present real glyphs, keep the screenshot, and reject an empty frame."""
    if not sys.platform.startswith("linux"):
        raise RuntimeError("The font window probe requires Linux")
    if not os.environ.get("DISPLAY") or os.environ["DISPLAY"] == ":0":
        raise RuntimeError("Run the font window probe under a private xvfb-run display")
    output = Path(output).resolve()
    output.parent.mkdir(parents=True, exist_ok=True)
    screenshot = output.with_suffix(".png")
    env = {**os.environ, "FC_LANG": "zh-cn", "LC_ALL": "C.UTF-8"}
    env.pop("FONTCONFIG_FILE", None)
    env.pop("WAYLAND_DISPLAY", None)
    with tempfile.TemporaryDirectory(prefix="zgpui-font-window-") as directory:
        binary = build_fixture(directory, sanitize=False,
                               software=features == "linux-software")
        proc = subprocess.Popen(
            [str(binary), "window"], stdout=subprocess.PIPE, stderr=subprocess.PIPE,
            text=True, env=env,
        )
        try:
            if not select.select([proc.stdout], [], [], 20)[0]:
                raise RuntimeError("Font window did not finish its first frame")
            if proc.stdout.readline().strip() != "READY":
                if proc.poll() is not None:
                    raise RuntimeError("Font window failed: " + proc.stderr.read())
                raise RuntimeError("Font window did not acknowledge presentation")
            windows = subprocess.check_output(
                ["xdotool", "search", "--onlyvisible", "--name",
                 "^zgpui-font-regression$"], text=True, env=env,
            ).splitlines()
            if len(windows) != 1:
                raise RuntimeError("Expected one owned font probe window")
            subprocess.run(
                ["import", "-window", windows[0], "-strip", str(screenshot)],
                check=True, env=env, timeout=10,
            )
            width, height, colors = map(int, subprocess.check_output(
                ["identify", "-format", "%w %h %k", str(screenshot)],
                text=True, env=env, timeout=10,
            ).split())
            if (width, height) != (1120, 760) or colors <= 2:
                raise RuntimeError(
                    f"Missing or empty glyph frame: {width}x{height}, {colors} colors"
                )
            stdout, stderr = proc.communicate(timeout=20)
            if proc.returncode != 0:
                raise RuntimeError(f"Font window exit {proc.returncode}: {stderr}")
        finally:
            if proc.poll() is None:
                proc.terminate()
                try:
                    proc.communicate(timeout=5)
                except subprocess.TimeoutExpired:
                    proc.kill()
                    proc.communicate()
        report = {
            "scope": "real native font presentation on a private X11 display",
            "features": features,
            "window": [width, height],
            "colors": colors,
            "scripts": ["Latin", "CJK", "Greek", "Cyrillic", "Emoji"],
            "weights": ["regular", "bold"],
            "pixel_sizes": [12, 20, 22, 28, 32],
            "locale": "zh-cn",
            "screenshot": screenshot.name,
            "screenshot_sha256": hashlib.sha256(screenshot.read_bytes()).hexdigest(),
            "exit_code": proc.returncode,
            "hardware_gpu_benchmark": False,
            "memory_benchmark": False,
        }
        output.write_text(json.dumps(report, indent=2) + "\n")
        print(json.dumps(report))

class LinuxFontTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        if not sys.platform.startswith("linux"):
            raise unittest.SkipTest("Linux FreeType/fontconfig regression")
        cls.tmp=tempfile.TemporaryDirectory(prefix="zgpui-font-test-")
        cls.addClassCleanup(cls.tmp.cleanup)
        cls.binary=build_fixture(cls.tmp.name)
        confdir=subprocess.check_output(
            ["pkg-config","--variable=confdir","fontconfig"],text=True).strip()
        cls.system_config=Path(confdir)/"fonts.conf"

    def run_case(self,case,environment=None,*,allow_missing_emoji=False):
        env={k:v for k,v in os.environ.items()
             if k not in {"FC_LANG","LC_ALL","LC_CTYPE","LANG","FONTCONFIG_FILE"}}
        env.update({"LANG":"C.UTF-8","ASAN_OPTIONS":"detect_leaks=1:halt_on_error=1",
                    "UBSAN_OPTIONS":"halt_on_error=1"})
        env.update(environment or {})
        result=subprocess.run([str(self.binary),case],check=False,env=env,timeout=60)
        if result.returncode==77 and allow_missing_emoji:
            self.skipTest("Install Noto Color Emoji to exercise fixed bitmap strikes")
        result.check_returncode()

    def test_measurement_matches_real_glyph_advance(self):
        self.run_case("measurement")

    def test_packed_gray_negative_pitch_and_invalid_bitmap_bounds(self):
        self.run_case("bitmap-formats")

    def test_scaled_coverage_pitch_clipping_and_numeric_bounds(self):
        self.run_case("bitmap-scaling")

    def test_fixed_emoji_ink_metrics_and_size_cache(self):
        self.run_case("fixed-emoji",allow_missing_emoji=True)

    def test_locale_matching_uses_system_defaults(self):
        for environment in [
            {"LANG":"C.UTF-8"},
            {"FC_LANG":"zh-cn","LC_ALL":"ja_JP.UTF-8"},
            {"FC_LANG":"zh-tw"},
            {"FC_LANG":"ja"},
            {"FC_LANG":"ko"},
            {"LC_ALL":"ja_JP.UTF-8","LC_CTYPE":"zh_CN.UTF-8","LANG":"en_US.UTF-8"},
            {"LC_CTYPE":"zh_TW.UTF-8","LANG":"en_US.UTF-8"},
        ]:
            with self.subTest(environment=environment):
                self.run_case("matching",environment)

    def test_reuses_faces_and_preserves_raster_after_size_changes(self):
        self.run_case("caching",{"FC_LANG":"zh-cn"})

    def test_one_face_can_satisfy_both_requested_weights(self):
        if not self.system_config.is_file():
            self.skipTest("system Fontconfig configuration is unavailable")
        config=Path(self.tmp.name)/"same-weight.conf"
        config.write_text(
            '<?xml version="1.0"?><!DOCTYPE fontconfig SYSTEM "urn:fontconfig:fonts.dtd">'
            '<fontconfig><include ignore_missing="no">'
            +escape(str(self.system_config))
            +'</include><match target="pattern"><edit name="weight" mode="assign">'
            '<const>regular</const></edit></match></fontconfig>')
        self.run_case("shared-weight",{"FONTCONFIG_FILE":str(config)})

    def test_shutdown_preserves_an_existing_host_configuration(self):
        self.run_case("host-config")

    def test_repeated_open_rasterize_shutdown_is_clean(self):
        self.run_case("lifecycle",{"FC_LANG":"zh-cn"})

    def test_font_switch_failure_preserves_cache_and_host_config(self):
        self.run_case("family-transaction",{"FC_LANG":"zh-cn"})

if __name__=="__main__":
    if "--window" in sys.argv:
        parser = argparse.ArgumentParser(description=__doc__)
        parser.add_argument("--window", action="store_true", required=True)
        parser.add_argument("--features", choices=("native", "linux-software"), required=True)
        parser.add_argument("--output", required=True)
        args = parser.parse_args()
        check_window(args.features, args.output)
    else:
        unittest.main()
