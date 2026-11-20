#!/usr/bin/env python3
"""Exercise the CPU rasterizer without an X server or graphics driver."""
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
SOURCE = r"""
#define _DEFAULT_SOURCE 1
#include "zgpui.h"
#include <assert.h>
#include <limits.h>
#include <math.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <errno.h>
#include <sys/mman.h>
#include <unistd.h>
static int fail_next_mapping;
void *__real_mmap(void *,size_t,int,int,int,off_t);
void *__wrap_mmap(void *at,size_t bytes,int prot,int flags,int fd,off_t offset) {
    if(fail_next_mapping) { --fail_next_mapping; errno=ENOMEM; return MAP_FAILED; }
    return __real_mmap(at,bytes,prot,flags,fd,offset);
}
static int raster_calls;
ZgpuiBitmap zgpui_rasterize(const char *text,int size,int bold) {
    (void)text;(void)size;(void)bold;
    ++raster_calls;
    unsigned char *p=malloc(4);assert(p);
    p[0]=255;p[1]=128;p[2]=0;p[3]=255;
    return (ZgpuiBitmap){p,2,2,1.5f};
}
static uint32_t pixel(int x,int y) {
    int w,h;uint32_t *p=zgpui_software_pixels(&w,&h);
    assert(p&&x>=0&&x<w&&y>=0&&y<h);return p[y*w+x];
}
static void geometry(void) {
    zgpui_begin(8,8,0x000000ff);
    zgpui_rect(2,2,4,4,0,0xff0000ff);
    assert(pixel(2,2)==0xff0000&&pixel(5,5)==0xff0000);
    assert(pixel(1,2)==0&&pixel(6,5)==0);
    zgpui_begin(8,8,0x000000ff);
    zgpui_rect(2,2,4,4,2,0xff0000ff);
    assert(pixel(2,2)>0&&pixel(2,2)<0xff0000);
    assert(pixel(3,3)==0xff0000&&pixel(1,1)==0);
    zgpui_begin(8,8,0x000000ff);
    zgpui_line(2.5f,3.5f,5.5f,3.5f,1,0x00ff00ff);
    assert(pixel(3,3)==0x00ff00&&pixel(3,1)==0);
    const float triangle[]={1,1,7,1,1,7};
    zgpui_begin(8,8,0x000000ff);
    zgpui_polygon(triangle,3,0x0000ffff);
    assert(pixel(2,2)==255&&pixel(6,6)==0);
}
static void clipping(void) {
    zgpui_begin(8,8,0x000000ff);
    zgpui_clip(3,3,1,1);
    zgpui_rect(-100,-100,200,200,0,0xff000080);
    assert(pixel(3,3)==0x800000&&pixel(4,3)==0&&pixel(2,3)==0);
    zgpui_clip(INT_MAX,INT_MAX,INT_MAX,INT_MAX);
    zgpui_rect(0,0,8,8,0,0xffffffff);
    assert(pixel(3,3)==0x800000&&pixel(0,0)==0);
    zgpui_unclip();
    zgpui_rect(NAN,0,8,8,0,0xffffffff);
    zgpui_rect(0,0,INFINITY,8,0,0xffffffff);
    assert(pixel(0,0)==0);
    zgpui_clip(-2,-2,3,3);
    zgpui_rect(0,0,8,8,0,0xffffffff);
    assert(pixel(0,0)==0xffffff&&pixel(1,0)==0);
}
static void text(void) {
    zgpui_begin(8,8,0x000000ff);
    assert(zgpui_text("same",1,1,15,0,0x0000ffff)==1.5f);
    assert(pixel(1,1)==255&&pixel(2,1)==128&&pixel(1,2)==0&&pixel(2,2)==255);
    assert(raster_calls==1&&zgpui_measure("same",15,0)==1.5f&&raster_calls==1);
    int w,h;uint32_t saved[64];memcpy(saved,zgpui_software_pixels(&w,&h),sizeof(saved));
    zgpui_software_release();
    assert(!zgpui_software_pixels(&w,&h)&&w==8&&h==8);
    zgpui_begin(8,8,0x000000ff);
    assert(zgpui_text("same",1,1,15,0,0x0000ffff)==1.5f&&raster_calls==1);
    assert(!memcmp(saved,zgpui_software_pixels(&w,&h),sizeof(saved)));
    assert(zgpui_measure("same",16,0)==1.5f&&raster_calls==2);
    for(int i=0;i<200;i++){char key[32];snprintf(key,sizeof(key),"text-%d",i);zgpui_measure(key,15,0);}
    int before=raster_calls;zgpui_measure("same",15,0);assert(raster_calls==before+1);
    zgpui_gpu_destroy();zgpui_gpu_destroy();
    zgpui_begin(8,8,0x000000ff);
    before=raster_calls;zgpui_measure("same",15,0);assert(raster_calls==before+1);
}
static void resize(void) {
    zgpui_begin(8,8,0x000000ff);
    int w,h;uint32_t *before=zgpui_software_pixels(&w,&h);
    assert(!zgpui_software_resize(0,8)&&!zgpui_software_resize(INT_MAX,8));
    assert(!zgpui_software_resize(16384,16384));
    assert(zgpui_software_pixels(&w,&h)==before&&w==8&&h==8);
    assert(zgpui_software_resize(9,7));zgpui_begin(9,7,0x123456ff);
    assert(pixel(8,6)==0x123456);
    zgpui_gpu_destroy();assert(!zgpui_software_pixels(&w,&h)&&w==0&&h==0);
}
static size_t resident_pages(uint32_t *p,size_t bytes) {
    long page=sysconf(_SC_PAGESIZE);assert(page>0);
    size_t count=(bytes+(size_t)page-1)/(size_t)page;
    unsigned char *present=calloc(count,1);assert(present);
    assert(mincore(p,bytes,present)==0);
    size_t resident=0;for(size_t i=0;i<count;i++)resident+=present[i]&1u;
    free(present);return resident;
}
static void release_frame(void) {
    const int width=1120,height=760;
    const size_t bytes=(size_t)width*height*sizeof(uint32_t);
    zgpui_begin(width,height,0x123456ff);
    int w,h;uint32_t *before=zgpui_software_pixels(&w,&h);assert(before);
    long page=sysconf(_SC_PAGESIZE);assert(page>0);
    assert(resident_pages(before,bytes)==(bytes+(size_t)page-1)/(size_t)page);
    zgpui_clip(1,1,1,1);
    zgpui_software_release();
    assert(!zgpui_software_pixels(&w,&h)&&w==width&&h==height);
    assert(resident_pages(before,bytes)==0);
    zgpui_software_release();
    zgpui_rect(0,0,width,height,0,0xffffffff);
    assert(resident_pages(before,bytes)==0);
    zgpui_begin(width,height,0x654321ff);
    assert(zgpui_software_pixels(&w,&h)==before);
    for(size_t i=0;i<(size_t)width*height;i++)assert(before[i]==0x654321);
    zgpui_rect(width-1,height-1,1,1,0,0xffffffff);
    assert(pixel(width-1,height-1)==0xffffff);
}
static void allocation_failure(void) {
    zgpui_begin(8,8,0x123456ff);
    int w,h;uint32_t *before=zgpui_software_pixels(&w,&h);
    fail_next_mapping=1;
    assert(!zgpui_software_resize(9,7));
    assert(zgpui_software_pixels(&w,&h)==before&&w==8&&h==8);
    assert(pixel(7,7)==0x123456);
    fail_next_mapping=1;
    zgpui_begin(9,7,0xffffffff);
    assert(!zgpui_software_pixels(&w,&h)&&w==8&&h==8);
    zgpui_rect(0,0,8,8,0,0xff0000ff);
    assert(before[0]==0x123456);
    zgpui_begin(8,8,0x654321ff);
    assert(zgpui_software_pixels(&w,&h)==before&&pixel(7,7)==0x654321);
}
int main(int argc,char **argv) {
    assert(argc==2);
    if(!strcmp(argv[1],"geometry"))geometry();
    else if(!strcmp(argv[1],"clipping"))clipping();
    else if(!strcmp(argv[1],"text"))text();
    else if(!strcmp(argv[1],"resize"))resize();
    else if(!strcmp(argv[1],"release"))release_frame();
    else if(!strcmp(argv[1],"allocation"))allocation_failure();
    else return 2;
    zgpui_gpu_destroy();
    return 0;
}
"""

class SoftwareRasterTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.tmp = tempfile.TemporaryDirectory(prefix="zgpui-software-test-")
        cls.addClassCleanup(cls.tmp.cleanup)
        root = Path(cls.tmp.name)
        fixture = root / "fixture.c"
        fixture.write_text(SOURCE)
        cls.binary = root / "raster-test"
        subprocess.run([
            os.environ.get("CC", "cc"), "-std=c11", "-O1", "-g",
            "-Wall", "-Wextra", "-Werror", "-fsanitize=address,undefined",
            "-fno-omit-frame-pointer", "-DZGPUI_SOFTWARE", "-I", str(ROOT / "native"),
            str(fixture), str(ROOT / "native/draw_software.c"),
            "-Wl,--wrap=mmap", "-lm", "-o", str(cls.binary),
        ], check=True)

    def run_case(self, case):
        subprocess.run([str(self.binary), case], check=True, env={
            **os.environ, "ASAN_OPTIONS": "detect_leaks=1:halt_on_error=1",
            "UBSAN_OPTIONS": "halt_on_error=1",
        })

    def test_shapes_and_antialiasing(self):
        self.run_case("geometry")

    def test_clip_alpha_and_invalid_geometry(self):
        self.run_case("clipping")

    def test_text_mask_cache_and_cleanup(self):
        self.run_case("text")

    def test_resize_limits_and_recovery(self):
        self.run_case("resize")

    def test_present_release_drops_residency_and_begin_clears_all(self):
        self.run_case("release")

    def test_allocation_failure_preserves_storage_and_recovers(self):
        self.run_case("allocation")

if __name__ == "__main__":
    unittest.main()
