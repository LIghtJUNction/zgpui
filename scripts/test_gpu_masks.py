#!/usr/bin/env python3
"""Exercise real legacy OpenGL uploads, tile pixels and owned texture limits."""
import os
from pathlib import Path
import shlex
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
SOURCE = r"""
#include <assert.h>
#include <GL/gl.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include "zgpui.h"

typedef struct {GLuint id;size_t bytes;} Allocation;
static Allocation allocations[16384];
static int forced_limit,actual_limit,fail_after=-1,live_count,fail_next_gen,image_calls;
static size_t live_bytes,peak_bytes;
static GLuint bound_texture;
static Allocation *allocation(GLuint id){
    for(size_t i=0;i<sizeof(allocations)/sizeof(allocations[0]);i++)
        if(allocations[i].id==id)return &allocations[i];
    assert(!"unowned texture");return NULL;
}
static void tracked_gen(GLsizei count,GLuint *ids){
    int failure=fail_next_gen;fail_next_gen=0;
    if(failure==1){memset(ids,0,(size_t)count*sizeof(*ids));return;}
    glGenTextures(count,ids);
    for(int j=0;j<count;j++){
        assert(ids[j]);
        size_t i=0;while(i<sizeof(allocations)/sizeof(allocations[0]) && allocations[i].id)i++;
        assert(i<sizeof(allocations)/sizeof(allocations[0]));
        allocations[i]=(Allocation){ids[j],0};live_count++;
    }
    if(failure==2){GLuint unused=0;glGenTextures(-1,&unused);}
}
static void tracked_delete(GLsizei count,const GLuint *ids){
    for(int j=0;j<count;j++)if(ids[j]){
        Allocation *item=allocation(ids[j]);
        live_bytes-=item->bytes;*item=(Allocation){0};live_count--;
    }
    glDeleteTextures(count,ids);
}
static void tracked_bind(GLenum target,GLuint id){
    bound_texture=id;glBindTexture(target,id);
}
static void tracked_integer(GLenum name,GLint *value){
    glGetIntegerv(name,value);
    if(name==GL_MAX_TEXTURE_SIZE && forced_limit)*value=forced_limit;
}
static void tracked_image(GLenum target,GLint level,GLint internal,GLsizei width,
                          GLsizei height,GLint border,GLenum format,GLenum type,const void *pixels){
    image_calls++;
    int maximum=forced_limit?forced_limit:actual_limit;
    assert(width>0 && height>0 && width<=maximum && height<=maximum);
    assert(!(width&(width-1)) && !(height&(height-1)));
    assert(internal==GL_ALPHA && format==GL_ALPHA && type==GL_UNSIGNED_BYTE && !border);
    if(fail_after==0){
        fail_after=-1;
        /* A real failed upload: no texture storage is created and GL sets its error. */
        glTexImage2D(target,level,internal,-1,height,border,format,type,pixels);
        return;
    }
    if(fail_after>0)fail_after--;
    Allocation *item=allocation(bound_texture);assert(!item->bytes);
    item->bytes=(size_t)width*height;live_bytes+=item->bytes;
    if(live_bytes>peak_bytes)peak_bytes=live_bytes;
    glTexImage2D(target,level,internal,width,height,border,format,type,pixels);
}
#define glGetIntegerv tracked_integer
#define glGenTextures tracked_gen
#define glDeleteTextures tracked_delete
#define glBindTexture tracked_bind
#define glTexImage2D tracked_image
#include "draw.c"
#undef glGetIntegerv
#undef glGenTextures
#undef glDeleteTextures
#undef glBindTexture
#undef glTexImage2D

#define WIDTH 640
#define HEIGHT 180
#define PIXELS (WIDTH*HEIGHT*4)
static void read_pixels(unsigned char *out){
    glFinish();glReadBuffer(GL_BACK);
    glReadPixels(0,0,WIDTH,HEIGHT,GL_RGBA,GL_UNSIGNED_BYTE,out);
    assert(glGetError()==GL_NO_ERROR);
}
static float frame(int limit,int size,int count,float scale,unsigned char *out){
    zgpui_gpu_destroy();assert(!live_bytes && !live_count);forced_limit=limit;
    char text[512];assert(count<(int)sizeof(text));memset(text,'M',count);text[count]=0;
    zgpui_begin(WIDTH,HEIGHT,0x161412ff);
    glScalef(scale,scale,1);
    float width=zgpui_text(text,7.25f,10.5f,size,0,0xe8ded2ff);
    assert(width>WIDTH);read_pixels(out);return width;
}
static void tiles(void){
    unsigned char *baseline=malloc(PIXELS),*tiled=malloc(PIXELS);assert(baseline && tiled);
    const int sizes[]={31,96},counts[]={300,60},limits[]={256,64};
    const float scales[]={1,1.25f};int maximum_delta=0;
    for(size_t s=0;s<2;s++)for(size_t scale=0;scale<2;scale++){
        float width=frame(0,sizes[s],counts[s],scales[scale],baseline);
        for(size_t l=0;l<2;l++){
            assert(frame(limits[l],sizes[s],counts[s],scales[scale],tiled)==width);
            assert(live_count>1);
            for(size_t i=0;i<PIXELS;i++){
                int delta=abs((int)tiled[i]-baseline[i]);assert(delta<=2);
                if(delta>maximum_delta)maximum_delta=delta;
            }
        }
    }
    printf("{\"tile_comparison_max_channel_delta\":%d,\"tolerance\":2,\"fractional_positions\":true,\"scales\":[1,1.25]}\n",maximum_delta);
    free(tiled);free(baseline);
}
static void budget(void){
    forced_limit=256;
    for(int cycle=0;cycle<2;cycle++){
        for(int i=0;i<120;i++){
            char text[128];memset(text,'M',80);snprintf(text+80,sizeof(text)-80," %d-%d",cycle,i);
            assert(zgpui_measure(text,42,0)>1000);
            assert(live_bytes<=2097152 && texture_bytes==live_bytes);
        }
        assert(peak_bytes>1048576 && peak_bytes<=2097152 && live_count>1);
        zgpui_clear_text_cache();assert(!live_bytes && !live_count);
    }
}
static void failed_upload(void){
    forced_limit=64;fail_after=1;
    char text[301];memset(text,'M',300);text[300]=0;
    assert(zgpui_measure(text,20,0)==0);
    assert(!live_bytes && !live_count && !texture_bytes);
    assert(zgpui_measure(text,20,0)>WIDTH && live_count>1);
    assert(glGetError()==GL_NO_ERROR);
    zgpui_clear_text_cache();assert(!live_bytes && !live_count);
}
static void failed_generation(void){
    for(int failure=1;failure<=2;failure++){
        int uploads=image_calls;fail_next_gen=failure;
        assert(zgpui_measure("A generated texture must be owned",20,0)==0);
        assert(image_calls==uploads && !live_bytes && !live_count && !texture_bytes);
        assert(glGetError()==GL_NO_ERROR);
        assert(zgpui_measure("A generated texture must be owned",20,0)>20);
        assert(image_calls>uploads && live_count>0);
        zgpui_clear_text_cache();assert(!live_bytes && !live_count);
    }
}
static void legacy(void){
    const char *version=(const char*)glGetString(GL_VERSION);
    const char *extensions=(const char*)glGetString(GL_EXTENSIONS);
    assert(version && !strncmp(version,"1.1 ",4));
    assert(extensions && !strstr(extensions,"GL_ARB_texture_non_power_of_two"));
    zgpui_begin(WIDTH,HEIGHT,0x161412ff);
    zgpui_rect(20,20,140,44,9,0xd4a373ff);
    assert(zgpui_text("Glyphs 0123",20,80,23,0,0xe8ded2ff)>20);
    unsigned char frame[PIXELS];read_pixels(frame);
    size_t corner=((HEIGHT-1-21)*WIDTH+21)*4;
    assert(frame[corner]==0x16 && frame[corner+1]==0x14 && frame[corner+2]==0x12);
    int ink=0,background=0;
    for(int y=80;y<110;y++)for(int x=20;x<160;x++){
        size_t i=((HEIGHT-1-y)*WIDTH+x)*4;
        if(frame[i]>80)ink++;
        else if(frame[i]==0x16 && frame[i+1]==0x14 && frame[i+2]==0x12)background++;
    }
    assert(ink>100 && background>100);
}
int main(int argc,char **argv){
    assert(argc==2);char error[256]={0};
    ZgpuiWindow *window=zgpui_open("zgpui-mask-test",WIDTH,HEIGHT,error,sizeof(error));
    if(!window){fprintf(stderr,"window failed: %s\n",error);return 1;}
    glGetIntegerv(GL_MAX_TEXTURE_SIZE,&actual_limit);assert(actual_limit>=64);
    assert(zgpui_set_font_family(window,"monospace",error,sizeof(error)));
    if(!strcmp(argv[1],"tiles"))tiles();
    else if(!strcmp(argv[1],"budget"))budget();
    else if(!strcmp(argv[1],"failure"))failed_upload();
    else if(!strcmp(argv[1],"generation-failure"))failed_generation();
    else if(!strcmp(argv[1],"legacy"))legacy();
    else assert(!"unknown fixture");
    zgpui_close(window);assert(!live_count && !live_bytes);
    printf("{\"case\":\"%s\",\"passed\":true,\"owned_texture_bytes_after_close\":0}\n",argv[1]);
    return 0;
}
"""


class GpuMaskTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.tmp = tempfile.TemporaryDirectory(prefix="zgpui-gpu-masks-")
        cls.addClassCleanup(cls.tmp.cleanup)
        source = Path(cls.tmp.name) / "masks.c"
        source.write_text(SOURCE)
        cls.binary = source.with_suffix("")
        flags = shlex.split(subprocess.check_output(
            ["pkg-config", "--cflags", "--libs", "x11", "fontconfig", "freetype2", "gl"],
            text=True,
        ))
        subprocess.run([
            "cc", "-std=c11", "-D_DEFAULT_SOURCE", "-O1", "-g", "-Wall", "-Wextra",
            "-Werror", "-Wno-misleading-indentation", "-fsanitize=address,undefined",
            "-fno-omit-frame-pointer", "-I", str(ROOT / "native"),
            str(source), str(ROOT / "native/linux.c"), *flags, "-lm", "-o", str(cls.binary),
        ], check=True, timeout=120)

    def run_case(self, name, legacy=False):
        env = {**os.environ, "LIBGL_ALWAYS_SOFTWARE": "1",
               "ASAN_OPTIONS": "detect_leaks=1:halt_on_error=1",
               "UBSAN_OPTIONS": "halt_on_error=1"}
        if legacy:
            env.update({"MESA_GL_VERSION_OVERRIDE": "1.1",
                        "MESA_EXTENSION_OVERRIDE": "-GL_ARB_texture_non_power_of_two"})
        subprocess.run([str(self.binary), name], env=env, check=True, timeout=60)

    def test_fractional_text_pixels_and_metrics_survive_small_texture_limits(self):
        self.run_case("tiles")

    def test_actual_padded_storage_obeys_cache_budget_and_clears(self):
        self.run_case("budget")

    def test_partial_upload_failure_releases_all_tiles_and_retries_cleanly(self):
        self.run_case("failure")

    def test_texture_generation_failure_never_uploads_default_object(self):
        self.run_case("generation-failure")

    def test_real_opengl_11_without_npot_renders_glyphs_and_round_corners(self):
        self.run_case("legacy", legacy=True)


if __name__ == "__main__":
    unittest.main()
