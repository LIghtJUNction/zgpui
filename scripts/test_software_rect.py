#!/usr/bin/env python3
"""Pixel-exact CPU rectangle regression and an isolated drawing CPU benchmark.

The reference is the pre-optimization signed-distance implementation. The default
test uses ASan/UBSan. --benchmark builds without sanitizers and alternates order;
its CPU timings describe only the fixed 1120x760 scene, not whole GUI throughput.
"""
import argparse
import json
import os
from pathlib import Path
import statistics
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
SOURCE = r"""
#define _POSIX_C_SOURCE 200809L
#define _DEFAULT_SOURCE 1
#include "zgpui.h"
#include <assert.h>
#include <math.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>

/* Rectangle tests never render text. Also support the metrics-only text API. */
ZgpuiBitmap zgpui_rasterize(const char *text,int size,int bold) {
    (void)text;(void)size;(void)bold;return (ZgpuiBitmap){0};
}
float zgpui_text_advance(const char *text,int size,int bold) {
    (void)text;(void)size;(void)bold;return 0;
}
#include "draw_software.c"

static void reference_rect(float x,float y,float width,float height,float radius,uint32_t color) {
    int b[4];
    if (!(color & 255u) || !isfinite(radius) || !bounds(x,y,x+width,y+height,b)) return;
    radius=clamp(radius,0,fminf(width,height)*0.5f);
    float cx=x+width*0.5f,cy=y+height*0.5f;
    float hx=width*0.5f-radius,hy=height*0.5f-radius;
    for(int row=b[1];row<b[3];++row) {
        for(int col=b[0];col<b[2];++col) {
            float qx=fabsf(col+0.5f-cx)-hx,qy=fabsf(row+0.5f-cy)-hy;
            float ox=fmaxf(qx,0),oy=fmaxf(qy,0);
            float distance=sqrtf(ox*ox+oy*oy)+fminf(fmaxf(qx,qy),0)-radius;
            blend(col,row,color,(unsigned int)(clamp(0.5f-distance,0,1)*255.0f+0.5f));
        }
    }
}
typedef struct {float x,y,w,h,r;uint32_t color;} Shape;
static uint32_t random_state=0x173f567bu;
static uint32_t random_u32(void) {
    random_state^=random_state<<13;random_state^=random_state>>17;random_state^=random_state<<5;
    return random_state;
}
static void compare(Shape s,int clip_x,int clip_y,int clip_w,int clip_h) {
    const size_t count=(size_t)screen_width*(size_t)screen_height;
    uint32_t *expected=malloc(count*sizeof(uint32_t));assert(expected);
    for(size_t i=0;i<count;++i) pixels[i]=((uint32_t)i*2654435761u)&0xffffffu;
    zgpui_clip(clip_x,clip_y,clip_w,clip_h);
    reference_rect(s.x,s.y,s.w,s.h,s.r,s.color);
    memcpy(expected,pixels,count*sizeof(uint32_t));
    for(size_t i=0;i<count;++i) pixels[i]=((uint32_t)i*2654435761u)&0xffffffu;
    zgpui_rect(s.x,s.y,s.w,s.h,s.r,s.color);
    for(size_t i=0;i<count;++i) {
        if(pixels[i]!=expected[i]) {
            fprintf(stderr,"pixel %zu,%zu: %06x != %06x; shape %.9g %.9g %.9g %.9g %.9g %08x; clip %d %d %d %d\n",
                i%(size_t)screen_width,i/(size_t)screen_width,pixels[i],expected[i],
                s.x,s.y,s.w,s.h,s.r,s.color,clip_x,clip_y,clip_w,clip_h);
            abort();
        }
    }
    free(expected);
}
static void verify(void) {
    zgpui_begin(96,80,0);
    const Shape cases[]={
        {0,0,96,80,0,0x123456ff},{1,2,91,75,12,0x8ab4f8ff},
        {1.25f,2.75f,88.5f,72.125f,13.1f,0xa2b3c47f},
        {-18.25f,-7.75f,107.5f,82.625f,20.3f,0x12345655},
        {0.5f,0.5f,90,76,0.25f,0xffffffff},
        {1,2,90,74,0.49999f,0x123456fe},{1,2,90,74,0.50001f,0x12345601},
        {0.125f,0.875f,95.5f,79.75f,38.5f,0xff224488},
        {20.25f,10.5f,35.75f,35.75f,100,0x8040c0ff},
        {20.25f,10.5f,35.75f,35.75f,-1,0x8040c0ff},
        {1.4f,3.2f,0.4f,0.3f,0,0xffffffff},{2,2,1,1,0.5f,0xffffffff},
        {-10,-10,500,500,12,0x12345680},{-50,-50,100,100,50,0xffffffff},
        {3,4,80,68,7,0},{3,4,0,68,7,0xffffffff},
        {3,4,80,-1,7,0xffffffff},{NAN,0,80,70,0,0xffffffff},
        {0,0,INFINITY,70,0,0xffffffff},{0,0,80,70,NAN,0xffffffff},
        {-1e10f,-1e10f,2e10f,2e10f,5,0x12345680}
    };
    const int clips[][4]={{0,0,96,80},{7,11,53,43},{-9,-7,46,58},{95,79,1,1},{100,100,50,50}};
    for(size_t c=0;c<sizeof(cases)/sizeof(cases[0]);++c)
        for(size_t k=0;k<sizeof(clips)/sizeof(clips[0]);++k)
            compare(cases[c],clips[k][0],clips[k][1],clips[k][2],clips[k][3]);
    for(int i=0;i<4096;++i) {
        Shape s={
            (float)(int)(random_u32()%18001)/128.0f-40,
            (float)(int)(random_u32()%15001)/128.0f-30,
            (float)(random_u32()%18001)/128.0f,
            (float)(random_u32()%15001)/128.0f,
            (float)(random_u32()%12001)/256.0f,
            random_u32()
        };
        compare(s,(int)(random_u32()%91)-12,(int)(random_u32()%79)-12,
            (int)(random_u32()%120),(int)(random_u32()%104));
    }
    zgpui_gpu_destroy();
    puts("{\"pixel_cases\":4201,\"mismatched_pixels\":0}");
}
typedef void (*RectFn)(float,float,float,float,float,uint32_t);
static volatile uint64_t benchmark_guard;
static void scene(RectFn rect) {
    rect(0,0,236,760,0,0x17191dff);
    rect(258,24,838,710,18,0x202329ff);
    for(int row=0;row<3;++row) {
        rect(282,56+row*180,790,158,12,0x2c3038ff);
        rect(306,80+row*180,118,110,10,0x8ab4f8ff);
        rect(914,155+row*180,132,36,12,0x8ab4f880);
    }
    rect(16.25f,24.75f,204.5f,48.25f,12,0x263650ff);
    rect(20,690,196,42,12,0x8ab4f8ff);
}
static double cpu_seconds(void) {
    struct timespec t;assert(!clock_gettime(CLOCK_PROCESS_CPUTIME_ID,&t));
    return (double)t.tv_sec+(double)t.tv_nsec/1e9;
}
static void benchmark(int optimized,int frames) {
    RectFn rect=optimized?zgpui_rect:reference_rect;
    zgpui_begin(1120,760,0x111315ff);scene(rect);
    double start=cpu_seconds();
    for(int i=0;i<frames;++i) {
        zgpui_begin(1120,760,0x111315ff);scene(rect);
        benchmark_guard+=pixels[(size_t)(i%760)*1120+(size_t)(i%1120)];
    }
    double elapsed=cpu_seconds()-start;
    printf("{\"cpu_seconds\":%.9f,\"guard\":%llu}\n",elapsed,(unsigned long long)benchmark_guard);
    zgpui_gpu_destroy();
}
int main(int argc,char **argv) {
    if(argc==2&&!strcmp(argv[1],"test")){verify();return 0;}
    if(argc==3&&(!strcmp(argv[1],"optimized")||!strcmp(argv[1],"reference"))) {
        int frames=atoi(argv[2]);assert(frames>0&&frames<=10000);
        benchmark(!strcmp(argv[1],"optimized"),frames);return 0;
    }
    return 2;
}
"""

def build(directory, sanitizer):
    source = directory / "rect_fixture.c"
    source.write_text(SOURCE)
    binary = directory / "rect_fixture"
    flags = ["-O1", "-g", "-fsanitize=address,undefined", "-fno-omit-frame-pointer"] if sanitizer else ["-O2"]
    subprocess.run([
        os.environ.get("CC", "cc"), "-std=c11", "-Wall", "-Wextra", "-Werror",
        *flags, "-DZGPUI_SOFTWARE", "-I", str(ROOT / "native"),
        str(source), "-lm", "-o", str(binary),
    ], check=True)
    return binary

class SoftwareRectangleTests(unittest.TestCase):
    def test_matches_reference_at_every_pixel(self):
        with tempfile.TemporaryDirectory(prefix="zgpui-rect-test-") as temporary:
            binary = build(Path(temporary), True)
            subprocess.run([str(binary), "test"], check=True, env={
                **os.environ, "ASAN_OPTIONS": "detect_leaks=1:halt_on_error=1",
                "UBSAN_OPTIONS": "halt_on_error=1",
            })

def benchmark(frames, rounds):
    with tempfile.TemporaryDirectory(prefix="zgpui-rect-bench-") as temporary:
        binary = build(Path(temporary), False)
        measurements = {"reference": [], "optimized": []}
        guards = set()
        for round_index in range(rounds):
            order = ("reference", "optimized") if round_index % 2 == 0 else ("optimized", "reference")
            for mode in order:
                sample = json.loads(subprocess.check_output([str(binary), mode, str(frames)], text=True))
                measurements[mode].append(sample["cpu_seconds"])
                guards.add(sample["guard"])
        if len(guards) != 1:
            raise RuntimeError("Benchmark checksums differ")
        baseline = statistics.median(measurements["reference"])
        optimized = statistics.median(measurements["optimized"])
        print(json.dumps({
            "scope": "fixed CPU rectangle scene; includes framebuffer clear; no text, X11 or event loop",
            "size": [1120, 760], "frames_per_sample": frames, "rounds": rounds,
            "clock": "CLOCK_PROCESS_CPUTIME_ID", "compiler_flags": "-O2 -std=c11",
            "order": "alternating", "samples_cpu_seconds": measurements,
            "median_cpu_seconds": {"reference": baseline, "optimized": optimized},
            "reference_over_optimized": baseline / optimized, "guard": next(iter(guards)),
        }, indent=2))

if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--benchmark", action="store_true")
    parser.add_argument("--frames", type=int, default=80)
    parser.add_argument("--rounds", type=int, default=5)
    args = parser.parse_args()
    if args.frames < 1 or args.frames > 10000 or args.rounds < 1 or args.rounds > 100:
        parser.error("frames must be 1..10000 and rounds must be 1..100")
    if args.benchmark:
        benchmark(args.frames, args.rounds)
    else:
        unittest.main(argv=[__file__])
