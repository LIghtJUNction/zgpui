#!/usr/bin/env python3
"""Check native RGBA8 pixels and resource lifetime; --gpu requires a private X display."""
import argparse
import os
from pathlib import Path
import shlex
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
GPU = False
LEAK_SCAN = True
SOURCE = r"""
#define _DEFAULT_SOURCE 1
#include "zgpui.h"
#include <assert.h>
#include <float.h>
#include <math.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#ifdef TEST_GPU
#include <GL/gl.h>
typedef struct {GLuint id;size_t bytes;} Allocation;
static Allocation allocations[128];
static GLuint bound_texture;
static int live_count,generated,forced_limit,fail_generation,fail_storage,fail_upload;
static size_t live_bytes,peak_bytes;
static Allocation *allocation(GLuint id){
    for(size_t i=0;i<128;i++)if(allocations[i].id==id)return &allocations[i];
    assert(!"unowned texture");return NULL;
}
static void tracked_gen(GLsizei count,GLuint *ids){
    if(fail_generation){fail_generation=0;memset(ids,0,(size_t)count*sizeof(*ids));return;}
    glGenTextures(count,ids);
    for(int i=0;i<count;i++){
        assert(ids[i]);size_t slot=0;while(slot<128&&allocations[slot].id)slot++;
        assert(slot<128);allocations[slot]=(Allocation){ids[i],0};live_count++;generated++;
    }
}
static void tracked_delete(GLsizei count,const GLuint *ids){
    for(int i=0;i<count;i++)if(ids[i]){
        Allocation *item=allocation(ids[i]);live_bytes-=item->bytes;
        *item=(Allocation){0};live_count--;
    }
    glDeleteTextures(count,ids);
}
static void tracked_bind(GLenum target,GLuint id){bound_texture=id;glBindTexture(target,id);}
static void tracked_integer(GLenum name,GLint *value){
    glGetIntegerv(name,value);
    if(name==GL_MAX_TEXTURE_SIZE&&forced_limit)*value=forced_limit;
}
static void tracked_image(GLenum target,GLint level,GLint internal,GLsizei width,
                          GLsizei height,GLint border,GLenum format,GLenum type,const void *data){
    assert(width>0&&height>0&&!(width&(width-1))&&!(height&(height-1)));
    assert(width<=1024&&height<=1024&&internal==GL_RGBA&&format==GL_RGBA);
    assert(!border&&type==GL_UNSIGNED_BYTE&&data==NULL);
    if(forced_limit)assert(width<=forced_limit&&height<=forced_limit);
    if(fail_storage){fail_storage=0;glTexImage2D(target,level,internal,-1,height,border,format,type,data);return;}
    Allocation *item=allocation(bound_texture);live_bytes-=item->bytes;
    item->bytes=(size_t)width*height*4;live_bytes+=item->bytes;
    if(live_bytes>peak_bytes)peak_bytes=live_bytes;
    glTexImage2D(target,level,internal,width,height,border,format,type,data);
}
static void tracked_upload(GLenum target,GLint level,GLint x,GLint y,GLsizei width,
                           GLsizei height,GLenum format,GLenum type,const void *data){
    if(fail_upload){fail_upload=0;glTexSubImage2D(target,level,x,y,-1,height,format,type,data);return;}
    glTexSubImage2D(target,level,x,y,width,height,format,type,data);
}
#define glGenTextures tracked_gen
#define glDeleteTextures tracked_delete
#define glBindTexture tracked_bind
#define glGetIntegerv tracked_integer
#define glTexImage2D tracked_image
#define glTexSubImage2D tracked_upload
#include "draw.c"
#undef glGenTextures
#undef glDeleteTextures
#undef glBindTexture
#undef glGetIntegerv
#undef glTexImage2D
#undef glTexSubImage2D
#else
float zgpui_text_advance(const char *text,int size,int bold){(void)text;(void)size;(void)bold;return 0;}
ZgpuiBitmap zgpui_rasterize(const char *text,int size,int bold){
    (void)text;(void)size;(void)bold;return (ZgpuiBitmap){0};
}
#include "draw_software.c"
#endif

#define WIDTH 160
#define HEIGHT 100
static uint32_t frame[WIDTH*HEIGHT];
static int sample_x,sample_y,sample_case;
static void read_frame(void){
#ifdef TEST_GPU
    unsigned char raw[WIDTH*HEIGHT*4];
    glFinish();glReadBuffer(GL_BACK);
    glReadPixels(0,0,WIDTH,HEIGHT,GL_RGBA,GL_UNSIGNED_BYTE,raw);
    assert(glGetError()==GL_NO_ERROR);
    for(int y=0;y<HEIGHT;y++)for(int x=0;x<WIDTH;x++){
        const unsigned char *p=raw+((HEIGHT-1-y)*WIDTH+x)*4;
        frame[y*WIDTH+x]=((uint32_t)p[0]<<16)|((uint32_t)p[1]<<8)|p[2];
    }
#else
    int width,height;uint32_t *p=zgpui_software_pixels(&width,&height);
    assert(p&&width==WIDTH&&height==HEIGHT);memcpy(frame,p,sizeof(frame));
#endif
}
static void near(uint32_t actual,uint32_t expected){
    for(int shift=0;shift<=16;shift+=8)if(abs((int)((actual>>shift)&255)-(int)((expected>>shift)&255))>1){
        fprintf(stderr,"pixel mismatch case=%d x=%d y=%d actual=%06x expected=%06x\n",
                sample_case,sample_x,sample_y,actual,expected);abort();
    }
}
static void orientation_alpha(void){
    /* Explicit independent expected colors catch RGBA/BGRA, flip and premultiplication errors. */
    const unsigned char source[]={255,0,0,255, 0,255,0,128, 0,0,255,0, 255,255,255,255};
    zgpui_begin(WIDTH,HEIGHT,0x204060ff);
    assert(!zgpui_image(4,6,8,8,2,2,source,sizeof(source)));
    read_frame();
    near(frame[7*WIDTH+5],0xff0000);near(frame[7*WIDTH+10],0x10a030);
    near(frame[12*WIDTH+5],0x204060);near(frame[12*WIDTH+10],0xffffff);
    assert(frame[5*WIDTH+5]==0x204060&&frame[6*WIDTH+12]==0x204060);
    /* Reusing and mutating the same caller buffer must submit its current pixels. */
    unsigned char changing[]={0x1a,0x73,0xe8,255};
    assert(!zgpui_image(20,20,1,1,1,1,changing,4));
    changing[0]=240;changing[1]=20;changing[2]=100;
    assert(!zgpui_image(21,20,1,1,1,1,changing,4));
    read_frame();near(frame[20*WIDTH+20],0x1a73e8);near(frame[20*WIDTH+21],0xf01464);
}
static void clipping(void){
    const unsigned char source[]={255,0,0,255,0,255,0,255,0,0,255,255,255,255,0,255};
    zgpui_begin(WIDTH,HEIGHT,0x204060ff);
    zgpui_clip(1,1,2,2);
    assert(!zgpui_image(-2,-2,8,8,2,2,source,sizeof(source)));
    read_frame();
    assert(frame[WIDTH+1]==0xff0000&&frame[WIDTH+2]==0x00ff00);
    assert(frame[2*WIDTH+1]==0x0000ff&&frame[2*WIDTH+2]==0xffff00);
    assert(frame[0]==0x204060&&frame[3*WIDTH+3]==0x204060);
    zgpui_unclip();
    assert(!zgpui_image(-1,-1,2,2,2,2,source,sizeof(source)));
    read_frame();assert(frame[0]==0xffff00);
    zgpui_rect(10,10,1,1,0,0x7a4c2dff);
    read_frame();assert(frame[10*WIDTH+10]==0x7a4c2d);
}
static void invalid(void){
    const unsigned char pixel[]={1,2,3,255};
    zgpui_begin(WIDTH,HEIGHT,0x204060ff);
    assert(zgpui_image(0,0,1,1,0,1,pixel,0)==-1);
    assert(zgpui_image(0,0,1,1,1,0,pixel,0)==-1);
    assert(zgpui_image(0,0,1,1,1,1,pixel,3)==-1);
    assert(zgpui_image(0,0,1,1,1,1,pixel,5)==-1);
    assert(zgpui_image(0,0,1,1,1,1,NULL,4)==-1);
    assert(zgpui_image(0,0,1,1,UINT32_MAX,UINT32_MAX,pixel,SIZE_MAX)==-1);
    assert(zgpui_image(0,0,1,1,16385,1,pixel,16385*4)==-1);
    assert(zgpui_image(0,0,1,1,16384,4097,pixel,(size_t)16384*4097*4)==-1);
    assert(zgpui_image(NAN,0,1,1,1,1,pixel,4)==-1);
    assert(zgpui_image(0,0,INFINITY,1,1,1,pixel,4)==-1);
    assert(zgpui_image(0,0,-1,1,1,1,pixel,4)==-1);
    assert(zgpui_image(0,0,1,0,1,1,pixel,4)==-1);
    assert(zgpui_image(FLT_MAX,0,FLT_MAX,1,1,1,pixel,4)==-1);
    read_frame();for(size_t i=0;i<WIDTH*HEIGHT;i++)assert(frame[i]==0x204060);
    assert(!zgpui_image(-100,-100,1,1,1,1,pixel,4));
    zgpui_clip(2,2,0,0);assert(!zgpui_image(0,0,WIDTH,HEIGHT,1,1,pixel,4));
    read_frame();for(size_t i=0;i<WIDTH*HEIGHT;i++)assert(frame[i]==0x204060);
#ifndef TEST_GPU
    zgpui_software_release();assert(zgpui_image(0,0,1,1,1,1,pixel,4)==-2);
#endif
}
static void scaled(void){
    enum {SW=257,SH=129};unsigned char *source=malloc(SW*SH*4);assert(source);
    for(int y=0;y<SH;y++)for(int x=0;x<SW;x++){
        unsigned char *p=source+(y*SW+x)*4;
        p[0]=(unsigned char)x;p[1]=(unsigned char)y;p[2]=(unsigned char)(3*x+5*y);p[3]=(unsigned char)(127+y);
    }
#ifdef TEST_GPU
    forced_limit=64;
#endif
    const float cases[][4]={{3.25f,5.25f,129,65},{-11.25f,-8.25f,300,180},{7.25f,9.25f,67,31}};
    for(size_t c=0;c<sizeof(cases)/sizeof(cases[0]);c++){
        float x=cases[c][0],y=cases[c][1],w=cases[c][2],h=cases[c][3];
        zgpui_begin(WIDTH,HEIGHT,0x204060ff);
        assert(!zgpui_image(x,y,w,h,SW,SH,source,SW*SH*4));read_frame();
        for(int row=0;row<HEIGHT;row++)for(int col=0;col<WIDTH;col++){
            uint32_t expected=0x204060;
            if(col+0.5>=x&&col+0.5<x+w&&row+0.5>=y&&row+0.5<y+h){
                int sx=(int)floor((col+0.5-x)*SW/w),sy=(int)floor((row+0.5-y)*SH/h);
                const unsigned char *p=source+(sy*SW+sx)*4;
                unsigned int a=p[3];
                unsigned int r=(p[0]*a+32*(255-a)+127)/255;
                unsigned int g=(p[1]*a+64*(255-a)+127)/255;
                unsigned int b=(p[2]*a+96*(255-a)+127)/255;
                expected=(r<<16)|(g<<8)|b;
            }
            sample_case=(int)c;sample_x=col;sample_y=row;near(frame[row*WIDTH+col],expected);
        }
    }
    free(source);
}
#ifdef TEST_GPU
static void lifetime(void){
    size_t length=1537u*1025u*4u;unsigned char *source=malloc(length);assert(source);
    memset(source,255,length);int before=generated;
    for(int i=0;i<300;i++){
        uint32_t w=i%2?3:1537,h=i%2?2:1025;
        source[0]=(unsigned char)i;
        zgpui_begin(WIDTH,HEIGHT,0x204060ff);
        assert(!zgpui_image(0,0,WIDTH,HEIGHT,w,h,source,(size_t)w*h*4));
        assert(live_count==1&&live_bytes<=4u*1024u*1024u&&generated==before+1);
    }
    assert(peak_bytes==4u*1024u*1024u);
    zgpui_gpu_destroy();assert(!live_count&&!live_bytes);
    zgpui_begin(WIDTH,HEIGHT,0x204060ff);
    assert(!zgpui_image(0,0,1,1,1,1,source,4));assert(live_count==1);
    free(source);
}
static void failures(void){
    const unsigned char pixel[]={255,0,0,255};
    for(int i=0;i<3;i++){
        zgpui_begin(WIDTH,HEIGHT,0x204060ff);
        if(i==0)fail_generation=1;
        if(i==1)fail_storage=1;
        if(i==2)fail_upload=1;
        assert(zgpui_image(0,0,1,1,1,1,pixel,4)==-3);
        assert(!live_count&&!live_bytes&&glGetError()==GL_NO_ERROR);
        assert(!zgpui_image(0,0,1,1,1,1,pixel,4));
        read_frame();near(frame[0],0xff0000);
        zgpui_gpu_destroy();assert(!live_count&&!live_bytes);
    }
}
static void pixel_store(void){
    const unsigned char source[]={255,0,0,255,0,255,0,255,0,0,255,255,255,255,255,255};
    zgpui_begin(WIDTH,HEIGHT,0x204060ff);
    glPixelStorei(GL_UNPACK_ALIGNMENT,8);glPixelStorei(GL_UNPACK_ROW_LENGTH,43);
    glPixelStorei(GL_UNPACK_SKIP_ROWS,7);glPixelStorei(GL_UNPACK_SKIP_PIXELS,3);
    assert(!zgpui_image(0,0,2,2,2,2,source,sizeof(source)));
    GLint value;glGetIntegerv(GL_UNPACK_ALIGNMENT,&value);assert(value==8);
    glGetIntegerv(GL_UNPACK_ROW_LENGTH,&value);assert(value==43);
    glGetIntegerv(GL_UNPACK_SKIP_ROWS,&value);assert(value==7);
    glGetIntegerv(GL_UNPACK_SKIP_PIXELS,&value);assert(value==3);
    read_frame();near(frame[0],0xff0000);near(frame[WIDTH],0x0000ff);
}
#endif
int main(int argc,char **argv){
    assert(argc==2);
#ifdef TEST_GPU
    char error[256]={0};
    ZgpuiWindow *window=zgpui_open("zgpui-image-test",WIDTH,HEIGHT,error,sizeof(error));
    if(!window){fprintf(stderr,"window failed: %s\n",error);return 1;}
    printf("OpenGL: %s; renderer: %s\n",glGetString(GL_VERSION),glGetString(GL_RENDERER));
#endif
    if(!strcmp(argv[1],"orientation-alpha"))orientation_alpha();
    else if(!strcmp(argv[1],"clip"))clipping();
    else if(!strcmp(argv[1],"invalid"))invalid();
    else if(!strcmp(argv[1],"scale-tiles"))scaled();
#ifdef TEST_GPU
    else if(!strcmp(argv[1],"lifetime"))lifetime();
    else if(!strcmp(argv[1],"failure"))failures();
    else if(!strcmp(argv[1],"pixel-store"))pixel_store();
#endif
    else assert(!"unknown fixture");
#ifdef TEST_GPU
    zgpui_close(window);assert(!live_count&&!live_bytes);
    window=zgpui_open("zgpui-image-reopen",WIDTH,HEIGHT,error,sizeof(error));assert(window);
    orientation_alpha();zgpui_close(window);assert(!live_count&&!live_bytes);
#else
    zgpui_gpu_destroy();zgpui_gpu_destroy();
    orientation_alpha();zgpui_gpu_destroy();
#endif
    printf("{\"case\":\"%s\",\"passed\":true,\"closed_and_reopened\":true}\n",argv[1]);
    return 0;
}
"""


class ImageTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.tmp = tempfile.TemporaryDirectory(prefix="zgpui-images-")
        cls.addClassCleanup(cls.tmp.cleanup)
        source = Path(cls.tmp.name) / "images.c"
        source.write_text(SOURCE)
        cls.binary = source.with_suffix("")
        command = [os.environ.get("CC", "cc"), "-std=c11", "-D_DEFAULT_SOURCE", "-O1", "-g",
                   "-Wall", "-Wextra", "-Werror", "-Wno-misleading-indentation",
                   "-fsanitize=address,undefined", "-fno-omit-frame-pointer",
                   "-I", str(ROOT / "native"), str(source)]
        if GPU:
            flags = shlex.split(subprocess.check_output(
                ["pkg-config", "--cflags", "--libs", "fontconfig", "freetype2"],
                text=True,
            ))
            command.extend(["-DTEST_GPU", str(ROOT / "native/linux.c"), *flags, "-lX11", "-lGL"])
        else:
            command.append("-DZGPUI_SOFTWARE")
        command.extend(["-lm", "-o", str(cls.binary)])
        subprocess.run(command, check=True, timeout=120)

    def run_case(self, name, legacy=False):
        env = {**os.environ, "LIBGL_ALWAYS_SOFTWARE": "1",
               "ASAN_OPTIONS": f"detect_leaks={int(LEAK_SCAN)}:halt_on_error=1",
               "UBSAN_OPTIONS": "halt_on_error=1"}
        if legacy:
            env.update({"MESA_GL_VERSION_OVERRIDE": "1.1",
                        "MESA_EXTENSION_OVERRIDE": "-GL_ARB_texture_non_power_of_two"})
        subprocess.run([str(self.binary), name], env=env, check=True, timeout=60)

    def test_rgba_orientation_and_straight_alpha(self):
        self.run_case("orientation-alpha")

    def test_clip_and_window_crop_preserve_source_coordinates(self):
        self.run_case("clip")

    def test_invalid_inputs_draw_nothing(self):
        self.run_case("invalid")

    def test_fractional_scale_and_legacy_tiles_match_independent_pixels(self):
        self.run_case("scale-tiles")

    def test_gpu_streaming_uses_one_bounded_texture(self):
        if not GPU:
            self.skipTest("GPU resource contract; run with --gpu")
        self.run_case("lifetime")

    def test_gpu_failed_allocation_and_upload_clean_up_and_recover(self):
        if not GPU:
            self.skipTest("GPU failure injection; run with --gpu")
        self.run_case("failure")

    def test_gpu_pixel_store_is_restored(self):
        if not GPU:
            self.skipTest("GPU pixel store contract; run with --gpu")
        self.run_case("pixel-store")

    def test_real_gl11_without_npot(self):
        if not GPU:
            self.skipTest("GL 1.1 context; run with --gpu")
        self.run_case("scale-tiles", legacy=True)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__, add_help=False)
    parser.add_argument("--gpu", action="store_true")
    parser.add_argument("--no-leak-scan", action="store_true",
                        help="Explicitly disable LSan where ptrace restrictions prevent it; ASan/UBSan remain enabled")
    args, remaining = parser.parse_known_args()
    GPU = args.gpu
    LEAK_SCAN = not args.no_leak_scan
    if not LEAK_SCAN:
        print("LeakSanitizer disabled explicitly; address/undefined sanitizers remain enabled", flush=True)
    unittest.main(argv=[__file__, *remaining])
