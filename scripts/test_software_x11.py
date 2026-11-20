#!/usr/bin/env python3
"""Compare real X11 framebuffer pixels before/after release, resize and expose.

Run on a private X server. The baseline defaults to the pre-release source commit.
Both C probes use ASan/UBSan. All build products stay in --output.
"""
import argparse
import json
import os
import shutil
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[1]
SOURCE = r"""
#define _DEFAULT_SOURCE 1
#include "zgpui.h"
#include <X11/Xlib.h>
#include <X11/Xutil.h>
#include <assert.h>
#include <errno.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/mman.h>
#include <unistd.h>
static int fail_next_mapping;
void *__real_mmap(void *,size_t,int,int,int,off_t);
void *__wrap_mmap(void *at,size_t bytes,int prot,int flags,int fd,off_t off) {
    if(fail_next_mapping){--fail_next_mapping;errno=ENOMEM;return MAP_FAILED;}
    return __real_mmap(at,bytes,prot,flags,fd,off);
}
static Window find_window(Display *d,const char *title) {
    for(int retry=0;retry<200;retry++){
        Window root,parent,*children=NULL;unsigned int count=0;
        assert(XQueryTree(d,DefaultRootWindow(d),&root,&parent,&children,&count));
        Window found=0;
        for(unsigned int i=0;i<count;i++){
            char *name=NULL;XFetchName(d,children[i],&name);
            if(name&&!strcmp(name,title))found=children[i];
            if(name)XFree(name);
        }
        if(children)XFree(children);
        if(found)return found;
        usleep(10000);
    }
    assert(!"window was not mapped");return 0;
}
static void draw_scene(int width,int height) {
    zgpui_begin(width,height,0x182028ff);
    zgpui_rect(19.25f,22.5f,260,110,19,0xd18e50cf);
    zgpui_rect(45,50,90,50,9,0x40a0f0a0);
    zgpui_line(16.5f,160.5f,425.75f,211.25f,3.5f,0x75e0b0df);
    const float triangle[]={310.25f,20.5f,460.75f,100.25f,330.5f,155.75f};
    zgpui_polygon(triangle,3,0xe0d060a0);
    zgpui_clip(24,235,380,58);
    zgpui_text("Framebuffer — 世界 / Aa",12,230,24,0,0xf4f0e8ff);
    zgpui_text("cached text",80,270,18,1,0x70b0e0d0);
    zgpui_unclip();
    zgpui_rect(width-17,height-19,9,11,2,0x998877ff);
}
static uint64_t pixel_hash(const uint32_t *p,size_t count) {
    uint64_t hash=1469598103934665603ULL;
    for(size_t i=0;i<count;i++){
        uint32_t value=p[i]&0xffffffu;
        for(int j=0;j<4;j++){hash^=(value>>(j*8))&255u;hash*=1099511628211ULL;}
    }
    return hash;
}
static int server_matches(Display *d,Window id,const uint32_t *want,int w,int h) {
    for(int retry=0;retry<100;retry++){
        XImage *image=XGetImage(d,id,0,0,(unsigned int)w,(unsigned int)h,AllPlanes,ZPixmap);
        assert(image);
        int same=1;
        for(int y=0;y<h&&same;y++)for(int x=0;x<w;x++){
            if((XGetPixel(image,x,y)&0xffffffu)!=(want[(size_t)y*w+x]&0xffffffu)){
                same=0;break;
            }
        }
        XDestroyImage(image);
        if(same)return 1;
        usleep(10000);
    }
    return 0;
}
static void frame(ZgpuiWindow *window,Display *d,Window id,const char *phase) {
    int w,h;zgpui_size(window,&w,&h);draw_scene(w,h);
    int sw,sh;uint32_t *pixels=zgpui_software_pixels(&sw,&sh);
    assert(pixels&&sw==w&&sh==h);
    size_t count=(size_t)w*h;
    uint32_t *copy=malloc(count*sizeof(*copy));assert(copy);
    memcpy(copy,pixels,count*sizeof(*copy));
    uint64_t hash=pixel_hash(copy,count);
    zgpui_swap(window);
#ifdef CHECK_RELEASE
    assert(!zgpui_software_pixels(&sw,&sh)&&sw==w&&sh==h);
#endif
    assert(server_matches(d,id,copy,w,h));
    zgpui_swap(window); /* No new begin: must not erase the server's pixels. */
    assert(server_matches(d,id,copy,w,h));
    free(copy);
    printf("{\"phase\":\"%s\",\"width\":%d,\"height\":%d,\"hash\":\"%016llx\",\"server_match\":true,\"repeated_present\":true}\n",
           phase,w,h,(unsigned long long)hash);
    fflush(stdout);
}
static void drain(ZgpuiWindow *window) {
    ZgpuiEvent event;while(zgpui_event(window,&event,0)){}
}
static void resize_to(ZgpuiWindow *window,Display *d,Window id,int width,int height) {
    drain(window);
    XResizeWindow(d,id,(unsigned int)width,(unsigned int)height);XFlush(d);
    for(int retry=0;retry<200;retry++){
        ZgpuiEvent event;zgpui_event(window,&event,10);
        int w,h;zgpui_size(window,&w,&h);
        if(w==width&&h==height)return;
    }
    assert(!"resize did not arrive");
}
static void expose(ZgpuiWindow *window,Display *d,Window id) {
    drain(window);XClearArea(d,id,0,0,0,0,True);XFlush(d);
    for(int retry=0;retry<200;retry++){
        ZgpuiEvent event;
        if(zgpui_event(window,&event,10)&&event.kind==6)return;
    }
    assert(!"expose did not arrive");
}
static void failed_resize(ZgpuiWindow *window,Display *d,Window id) {
#ifdef CHECK_RELEASE
    drain(window);
    int oldw,oldh;zgpui_size(window,&oldw,&oldh);fail_next_mapping=1;
    XResizeWindow(d,id,(unsigned int)(oldw+64),(unsigned int)(oldh+32));XFlush(d);
    int restored=0;
    for(int retry=0;retry<200;retry++){
        ZgpuiEvent event;zgpui_event(window,&event,10);
        XWindowAttributes attrs;assert(XGetWindowAttributes(d,id,&attrs));
        if(!fail_next_mapping&&attrs.width==oldw&&attrs.height==oldh){restored=1;break;}
    }
    int w,h;zgpui_size(window,&w,&h);
    assert(restored&&w==oldw&&h==oldh);
#else
    (void)window;(void)d;(void)id;
#endif
}
static void screenshot(Display *d,Window id,int w,int h,const char *path) {
    XImage *image=XGetImage(d,id,0,0,(unsigned int)w,(unsigned int)h,AllPlanes,ZPixmap);
    assert(image);FILE *out=fopen(path,"wb");assert(out);
    fprintf(out,"P6\n%d %d\n255\n",w,h);
    for(int y=0;y<h;y++)for(int x=0;x<w;x++){
        unsigned long value=XGetPixel(image,x,y);
        unsigned char rgb[3]={(value>>16)&255u,(value>>8)&255u,value&255u};
        assert(fwrite(rgb,1,3,out)==3);
    }
    assert(!fclose(out));XDestroyImage(image);
}
int main(int argc,char **argv) {
    assert(argc==2);char error[512]={0};
    ZgpuiWindow *window=zgpui_open("zgpui-framebuffer-check",1120,760,error,sizeof(error));
    if(!window){fprintf(stderr,"%s\n",error);return 1;}
    Display *d=XOpenDisplay(NULL);assert(d);
    Window id=find_window(d,"zgpui-framebuffer-check");drain(window);
    const int sizes[][2]={{1120,760},{1280,840},{640,520},{1120,760}};
    for(int i=0;i<12;i++){
        resize_to(window,d,id,sizes[i%4][0],sizes[i%4][1]);
        char phase[48];snprintf(phase,sizeof(phase),"resize-%d",i);
        frame(window,d,id,phase);
        expose(window,d,id);snprintf(phase,sizeof(phase),"expose-%d",i);
        frame(window,d,id,phase);
    }
    failed_resize(window,d,id);frame(window,d,id,"allocation-recovery");
    screenshot(d,id,1120,760,argv[1]);
    XCloseDisplay(d);zgpui_close(window);return 0;
}
"""

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--baseline", default="cb50f2fe554622fdc984ae7fec9bcd844f175d67")
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    if not os.environ.get("DISPLAY") or os.environ["DISPLAY"] == ":0":
        raise SystemExit("Run on an owned private X server, not the desktop.")
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    fixture = output / "present.c"
    fixture.write_text(SOURCE)
    baseline = output / "baseline-native"
    baseline.mkdir(exist_ok=True)
    for name in ("zgpui.h", "draw_software.c", "linux.c"):
        (baseline / name).write_bytes(subprocess.check_output(
            ["git", "show", args.baseline + ":native/" + name], cwd=ROOT))
    flags = subprocess.check_output(
        ["pkg-config", "--cflags", "--libs", "x11", "freetype2", "fontconfig"],
        text=True).split()
    rows = {}
    for name, source, defines in (
        ("baseline", baseline, []),
        ("release", ROOT / "native", ["-DCHECK_RELEASE"]),
    ):
        binary = output / (name + "-probe")
        command = [
            os.environ.get("CC", "cc"), "-std=c11", "-D_DEFAULT_SOURCE",
            # Match the native bridge build flags; its baseline emits indentation warnings.
            # test_software.py separately compiles the changed renderer with -Werror.
            "-O1", "-g", "-Wall", "-Wextra",
            "-fsanitize=address,undefined", "-fno-omit-frame-pointer",
            "-DZGPUI_SOFTWARE", *defines, "-I", str(source),
            str(fixture), str(source / "draw_software.c"), str(source / "linux.c"),
            "-Wl,--wrap=mmap", *flags, "-lm", "-o", str(binary),
        ]
        subprocess.run(command, check=True)
        result = subprocess.run(
            [str(binary), str(output / (name + ".ppm"))], check=True,
            capture_output=True, text=True, timeout=90,
            env={**os.environ, "ASAN_OPTIONS": "detect_leaks=1:halt_on_error=1",
                 "UBSAN_OPTIONS": "halt_on_error=1"},
        )
        (output / (name + ".log")).write_text(result.stdout + result.stderr)
        rows[name] = [json.loads(line) for line in result.stdout.splitlines()]
    assert rows["baseline"] == rows["release"], "Actual raster or server pixels differ"
    convert = shutil.which("magick") or shutil.which("convert")
    if not convert:
        raise RuntimeError("ImageMagick is required to preserve PNG window evidence")
    for name in rows:
        subprocess.run(
            [convert, str(output / (name + ".ppm")), str(output / (name + ".png"))],
            check=True, timeout=30,
        )
    result = {
        "status": "passed", "baseline": args.baseline,
        "frames_per_build": len(rows["release"]), "results": rows["release"],
        "scope": "Real X11 pixel readback, AA/text scene, twelve resizes and exposes, "
                 "repeated presents, failed mmap resize rollback; ASan/UBSan enabled",
        "screenshots": ["baseline.png", "release.png"],
    }
    (output / "result.json").write_text(json.dumps(result, indent=2) + "\n")
    print(json.dumps(result))

if __name__ == "__main__":
    main()
