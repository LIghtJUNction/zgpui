#!/usr/bin/env python3
"""Exercise effect ownership and fallback without a GPU; --real adds GL pixels.

Default: compile C stub and CPU cases under ASan/UBSan, no graphics packages.
Real: xvfb-run -a python3 scripts/test_effects_backend.py --real
The pixel test reads back only for verification; the production path never does.
"""
from __future__ import annotations

import argparse
import os
from pathlib import Path
import shlex
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]

FAKE_GL = r"""
#ifndef TEST_GL_H
#define TEST_GL_H
#include <stddef.h>
typedef unsigned int GLenum, GLuint, GLbitfield;
typedef int GLint, GLsizei;
typedef float GLfloat;
typedef unsigned char GLboolean, GLubyte;
#define GL_VERSION 0x1F02
#define GL_EXTENSIONS 0x1F03
#define GL_MAX_TEXTURE_SIZE 0x0D33
#define GL_NO_ERROR 0
#define GL_OUT_OF_MEMORY 0x0505
#define GL_TEXTURE_2D 0x0DE1
#define GL_TEXTURE_MIN_FILTER 0x2801
#define GL_TEXTURE_MAG_FILTER 0x2800
#define GL_TEXTURE_WRAP_S 0x2802
#define GL_TEXTURE_WRAP_T 0x2803
#define GL_LINEAR 0x2601
#define GL_RGBA 0x1908
#define GL_UNSIGNED_BYTE 0x1401
#define GL_MATRIX_MODE 0x0BA0
#define GL_SCISSOR_BOX 0x0C10
#define GL_SCISSOR_TEST 0x0C11
#define GL_ALL_ATTRIB_BITS 0xFFFFFFFFu
#define GL_PROJECTION 0x1701
#define GL_MODELVIEW 0x1700
#define GL_DEPTH_TEST 0x0B71
#define GL_CULL_FACE 0x0B44
#define GL_BLEND 0x0BE2
#define GL_QUADS 7
#define GL_BACK 0x0405
#define GL_SRC_ALPHA 0x0302
#define GL_ONE_MINUS_SRC_ALPHA 0x0303
#endif
"""

STUB_SOURCE = r"""
#include <assert.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include "GL/gl.h"
#include "zgpui.h"
#define ZGPUI_EFFECTS_TEST_LOADER 1

typedef struct {size_t bytes;int alive,w,h;} Texture;
static Texture textures[4096];
static GLuint next_texture=1,next_shader=1,next_program=1;
static unsigned int live_textures,live_shaders,live_programs,live_fbos;
static size_t allocated_bytes;
static int image_calls,copy_calls,finish_calls,fallback_calls,load_calls;
static int fail_image=-1,legacy_context,omit_fbo,fail_compile,fail_link;
static GLenum pending_error;
static int shader_alive[4096],program_alive[4096];
static GLuint attachment;
typedef struct {GLint active,bindings[8],matrix,scissor[4],fbo,program,viewport[4];int clipping;} State;
static State state,saved;
static int attrib_depth,matrix_depth;
static const GLubyte *glGetString(GLenum name){
 if(name==GL_VERSION)return (const GLubyte*)(legacy_context?"1.1 fake":"2.1 fake");
 if(name==GL_EXTENSIONS)return (const GLubyte*)(omit_fbo?"":"GL_EXT_framebuffer_object");
 return (const GLubyte*)"1.20 fake";
}
static void glGetIntegerv(GLenum name,GLint *out){
 switch(name){case GL_MAX_TEXTURE_SIZE:*out=2048;break;
 case 0x8B8D:*out=state.program;break;case 0x8CA6:*out=state.fbo;break;
 case 0x84E0:*out=state.active;break;case GL_MATRIX_MODE:*out=state.matrix;break;
 case GL_SCISSOR_BOX:memcpy(out,state.scissor,sizeof(state.scissor));break;
 default:assert(!"unexpected GL state query");}
}
static GLenum glGetError(void){GLenum error=pending_error;pending_error=0;return error;}
static void glGenTextures(GLsizei n,GLuint *out){
 for(int i=0;i<n;i++){assert(next_texture<4096);out[i]=next_texture++;textures[out[i]].alive=1;live_textures++;}
}
static void glDeleteTextures(GLsizei n,const GLuint *ids){
 for(int i=0;i<n;i++)if(ids[i]){assert(ids[i]<4096&&textures[ids[i]].alive);assert(attachment!=ids[i]);
 allocated_bytes-=textures[ids[i]].bytes;memset(&textures[ids[i]],0,sizeof(Texture));live_textures--;
 for(int j=0;j<8;j++)if(state.bindings[j]==(GLint)ids[i])state.bindings[j]=0;}
}
static void glBindTexture(GLenum target,GLuint id){assert(target==GL_TEXTURE_2D);state.bindings[state.active-0x84C0]=(GLint)id;}
static void glTexParameteri(GLenum target,GLenum parameter,GLint value){(void)target;(void)parameter;(void)value;}
static void glTexImage2D(GLenum target,GLint level,GLint internal,GLsizei w,GLsizei h,GLint border,GLenum format,GLenum type,const void *data){
 (void)target;(void)level;assert(internal==0x8058&&format==GL_RGBA&&type==GL_UNSIGNED_BYTE&&!border&&!data);
 image_calls++;if(fail_image==0){fail_image=-1;pending_error=GL_OUT_OF_MEMORY;return;}if(fail_image>0)fail_image--;
 GLuint id=(GLuint)state.bindings[state.active-0x84C0];assert(id&&textures[id].alive&&!textures[id].bytes);
 textures[id].w=w;textures[id].h=h;textures[id].bytes=(size_t)w*h*4;allocated_bytes+=textures[id].bytes;
}
static void glCopyTexSubImage2D(GLenum target,GLint level,GLint dx,GLint dy,GLint x,GLint y,GLsizei w,GLsizei h){
 (void)target;(void)level;assert(!dx&&!dy&&x>=0&&y>=0&&x+w<=640&&y+h<=480);
 GLuint id=(GLuint)state.bindings[state.active-0x84C0];assert(id&&textures[id].alive&&w<=textures[id].w&&h<=textures[id].h);copy_calls++;
}
static GLboolean glIsEnabled(GLenum name){return (GLboolean)(name==GL_SCISSOR_TEST&&state.clipping);}
static void glPushAttrib(GLbitfield bits){(void)bits;assert(!attrib_depth);saved=state;attrib_depth++;}
static void glPopAttrib(void){assert(attrib_depth==1);GLint program=state.program,fbo=state.fbo;state=saved;state.program=program;state.fbo=fbo;attrib_depth--;}
static void glMatrixMode(GLenum mode){state.matrix=(GLint)mode;}
static void glPushMatrix(void){matrix_depth++;}
static void glPopMatrix(void){assert(matrix_depth>0);matrix_depth--;}
static void glEnable(GLenum name){if(name==GL_SCISSOR_TEST)state.clipping=1;}
static void glDisable(GLenum name){if(name==GL_SCISSOR_TEST)state.clipping=0;}
static void glScissor(GLint x,GLint y,GLsizei w,GLsizei h){state.scissor[0]=x;state.scissor[1]=y;state.scissor[2]=w;state.scissor[3]=h;}
static void glDrawBuffer(GLenum buffer){(void)buffer;}
static void glViewport(GLint x,GLint y,GLsizei w,GLsizei h){state.viewport[0]=x;state.viewport[1]=y;state.viewport[2]=w;state.viewport[3]=h;}
static void glLoadIdentity(void){}
static void glOrtho(double l,double r,double b,double t,double n,double f){(void)l;(void)r;(void)b;(void)t;(void)n;(void)f;}
static void glBlendFunc(GLenum source,GLenum dest){(void)source;(void)dest;}
static void glBegin(GLenum mode){assert(mode==GL_QUADS);}
static void glTexCoord2f(GLfloat x,GLfloat y){(void)x;(void)y;}
static void glVertex2f(GLfloat x,GLfloat y){(void)x;(void)y;}
static void glEnd(void){}
static void glFinish(void){finish_calls++;}
void zgpui_rect(float x,float y,float w,float h,float radius,uint32_t color){(void)x;(void)y;(void)w;(void)h;(void)radius;(void)color;fallback_calls++;}

static GLuint mockCreateShader(GLenum type){(void)type;assert(next_shader<4096);shader_alive[next_shader]=1;live_shaders++;return next_shader++;}
static void mockShaderSource(GLuint id,GLsizei count,const char *const *parts,const GLint *lengths){
 assert(shader_alive[id]);for(int i=0;i<count;i++){assert(parts[i]);if(lengths)assert(lengths[i]>0&&lengths[i]<20000);}}
static void mockCompileShader(GLuint id){assert(shader_alive[id]);}
static void mockGetShaderiv(GLuint id,GLenum property,GLint *out){(void)property;assert(shader_alive[id]);*out=!fail_compile;}
static void mockGetShaderInfoLog(GLuint id,GLsizei capacity,GLsizei *length,char *out){(void)id;int n=snprintf(out,(size_t)capacity,"injected shader compilation error");*length=n;}
static void mockDeleteShader(GLuint id){assert(shader_alive[id]);shader_alive[id]=0;live_shaders--;}
static GLuint mockCreateProgram(void){assert(next_program<4096);program_alive[next_program]=1;live_programs++;return next_program++;}
static void mockAttachShader(GLuint p,GLuint s){assert(program_alive[p]&&shader_alive[s]);}
static void mockLinkProgram(GLuint id){assert(program_alive[id]);}
static void mockGetProgramiv(GLuint id,GLenum property,GLint *out){(void)property;assert(program_alive[id]);*out=!fail_link;}
static void mockGetProgramInfoLog(GLuint id,GLsizei capacity,GLsizei *length,char *out){(void)id;int n=snprintf(out,(size_t)capacity,"injected program link error");*length=n;}
static void mockDeleteProgram(GLuint id){assert(program_alive[id]);program_alive[id]=0;live_programs--;}
static void mockUseProgram(GLuint id){state.program=(GLint)id;}
static GLint mockGetUniformLocation(GLuint id,const char *name){(void)name;assert(program_alive[id]);return 1;}
static void mockUniform1i(GLint location,GLint value){(void)location;(void)value;}
static void mockUniform1f(GLint location,GLfloat a){(void)location;assert(a==a);}
static void mockUniform2f(GLint location,GLfloat a,GLfloat b){(void)location;assert(a==a&&b==b);}
static void mockUniform4f(GLint location,GLfloat a,GLfloat b,GLfloat c,GLfloat d){(void)location;assert(a==a&&b==b&&c==c&&d==d);}
static void mockActiveTexture(GLenum unit){assert(unit>=0x84C0&&unit<0x84C8);state.active=(GLint)unit;}
static void mockGenFramebuffers(GLsizei count,GLuint *ids){assert(count==1);ids[0]=9;live_fbos++;}
static void mockDeleteFramebuffers(GLsizei count,const GLuint *ids){assert(count==1&&ids[0]==9&&!attachment);live_fbos--;}
static void mockBindFramebuffer(GLenum target,GLuint id){(void)target;state.fbo=(GLint)id;}
static void mockFramebufferTexture2D(GLenum target,GLenum position,GLenum textarget,GLuint texture,GLint level){
 (void)target;(void)position;(void)textarget;(void)level;assert(state.fbo==9);attachment=texture;}
static GLenum mockCheckFramebufferStatus(GLenum target){(void)target;return attachment?0x8CD5:0;}
static void *zgpui_test_gl_proc(const char *name){
 load_calls++;
#define PROC(n) if(!strcmp(name,"gl" #n))return (void*)mock##n
 PROC(CreateShader);PROC(ShaderSource);PROC(CompileShader);PROC(GetShaderiv);PROC(GetShaderInfoLog);PROC(DeleteShader);
 PROC(CreateProgram);PROC(AttachShader);PROC(LinkProgram);PROC(GetProgramiv);PROC(GetProgramInfoLog);PROC(DeleteProgram);
 PROC(UseProgram);PROC(GetUniformLocation);PROC(Uniform1i);PROC(Uniform1f);PROC(Uniform2f);PROC(Uniform4f);PROC(ActiveTexture);
 PROC(GenFramebuffers);PROC(DeleteFramebuffers);PROC(BindFramebuffer);PROC(FramebufferTexture2D);PROC(CheckFramebufferStatus);
#undef PROC
#define EXT(n) if(!strcmp(name,"gl" #n "EXT"))return (void*)mock##n
 EXT(GenFramebuffers);EXT(DeleteFramebuffers);EXT(BindFramebuffer);EXT(FramebufferTexture2D);EXT(CheckFramebufferStatus);
#undef EXT
 return NULL;
}
#include "effects_gpu.h"

static ZgpuiWindow *window=(ZgpuiWindow *)(uintptr_t)1;
static ZgpuiEffect material(void){
 ZgpuiEffect e={0};e.source=(ZgpuiEffectCapture){1,1,80,80,200,160};
 e.x=80;e.y=80;e.width=200;e.height=160;e.radius=12;e.blur_radius=14;e.saturation=1;e.opacity=1;
 e.tint_rgba=0xe0e8f040;e.tint_mix=1;e.fold_progress=1;e.fold_perspective=900;e.fold_anchor=0.5f;return e;
}
static void reset(void){
 fx_destroy();assert(!allocated_bytes&&!live_textures&&!live_shaders&&!live_programs&&!live_fbos);
 memset(&state,0,sizeof(state));state.active=0x84C3;state.matrix=GL_MODELVIEW;
 state.clipping=1;state.scissor[0]=7;state.scissor[1]=11;state.scissor[2]=211;state.scissor[3]=191;
 state.viewport[2]=640;state.viewport[3]=480;legacy_context=omit_fbo=fail_compile=fail_link=0;
 fail_image=-1;fx_frame_begin(640,480);
}
static void invariants(void){
 assert(fx.stats.texture_bytes==allocated_bytes);assert(fx.stats.live_programs==live_programs);
 assert(fx.stats.texture_bytes<=fx.config.max_texture_bytes);assert(!live_shaders&&!attachment);
 assert(!attrib_depth&&!matrix_depth);assert(state.active==0x84C3&&state.matrix==GL_MODELVIEW);
 assert(state.clipping&&state.scissor[0]==7&&state.scissor[1]==11&&state.scissor[2]==211&&state.scissor[3]==191);
}
int main(void){
 assert(sizeof(ZgpuiEffectConfig)==24&&sizeof(ZgpuiEffectCapabilities)==32&&sizeof(ZgpuiEffectStats)==96);
 assert(sizeof(ZgpuiEffectCapture)==32&&sizeof(ZgpuiEffect)==160);
 reset();int loads=load_calls;fx_frame_begin(640,480);fx_frame_begin(640,480);
 assert(load_calls==loads&&!live_textures&&!live_programs);
 ZgpuiEffectCapabilities caps;assert(zgpui_effect_capabilities(window,&caps)==0);
 assert((caps.flags&31u)==31u&&!allocated_bytes&&!live_programs);
 ZgpuiEffect e=material();State prior=state;
 assert(zgpui_draw_effect(window,&e)==0);invariants();assert(!memcmp(&state,&prior,sizeof(state)));
 assert(fx.stats.captures==1&&fx.stats.blur_passes==2&&live_textures==3&&live_programs==2);
 int images=image_calls,copies=copy_calls;uint64_t bytes=fx.stats.texture_bytes;
 for(int i=0;i<100;i++){e.x=80+(float)i*0.1f;e.fold_axis=1;e.fold_radians=(float)i*0.009f;e.dim=(float)i*0.002f;
  assert(zgpui_draw_effect(window,&e)==0);invariants();}
 assert(copy_calls==copies&&image_calls==images&&fx.stats.texture_bytes==bytes&&fx.stats.blur_passes==2);
 e.blur_radius=24;assert(zgpui_draw_effect(window,&e)==0);assert(copy_calls==copies&&fx.stats.blur_passes==4&&image_calls==images);
 e.source.revision++;assert(zgpui_draw_effect(window,&e)==0);assert(copy_calls==copies+1&&image_calls==images&&fx.stats.blur_passes==6);
 e.flags=ZGPUI_EFFECT_CACHE_ONLY|ZGPUI_EFFECT_LAYER;e.source.width=e.source.height=0;
 assert(zgpui_draw_effect(window,&e)==0);assert(copy_calls==copies+1);invariants();
 e.source.revision++;assert(zgpui_draw_effect(window,&e)==ZGPUI_EFFECT_CACHE_MISS);
 assert(zgpui_effect_invalidate(window,1)==0);assert(!allocated_bytes&&!live_textures);
 assert(zgpui_draw_effect(window,&e)==ZGPUI_EFFECT_CACHE_MISS);
 e=material();e.blur_radius=0;e.flags=ZGPUI_EFFECT_CAPTURE_ONLY;
 assert(zgpui_draw_effect(window,&e)==0);uint64_t captures=fx.stats.captures;
 fx_frame_begin(640,480);e.flags=ZGPUI_EFFECT_LAYER|ZGPUI_EFFECT_CACHE_ONLY;
 assert(zgpui_draw_effect(window,&e)==0&&fx.stats.captures==captures);invariants();
 assert(!finish_calls);assert(zgpui_effect_finish(window)==0&&finish_calls==1);
 const char *shader="vec4 effect(vec2 p,vec2 s,vec4 c){return c;}";uint32_t slot=0;char diagnostic[128];
 assert(zgpui_effect_shader_register(window,shader,strlen(shader),&slot,diagnostic,sizeof(diagnostic))==0&&slot==1);
 unsigned programs=live_programs;assert(zgpui_effect_clear(window)==0&&!allocated_bytes&&live_programs==programs);
 ZgpuiEffectConfig config=fx_default_config();config.max_shader_slots=0;
 assert(zgpui_effect_configure(window,&config)==ZGPUI_EFFECT_BUDGET&&live_programs==programs);
 assert(zgpui_effect_shader_remove(window,slot)==0&&live_programs==programs-1);
 fail_compile=1;assert(zgpui_effect_shader_register(window,shader,strlen(shader),&slot,diagnostic,sizeof(diagnostic))==ZGPUI_EFFECT_SHADER_ERROR);
 assert(strstr(diagnostic,"compilation error")&&!live_shaders&&live_programs==programs-1);fail_compile=0;
 fail_link=1;assert(zgpui_effect_shader_register(window,shader,strlen(shader),&slot,diagnostic,sizeof(diagnostic))==ZGPUI_EFFECT_SHADER_ERROR);
 assert(strstr(diagnostic,"link error")&&!live_shaders&&live_programs==programs-1);fail_link=0;
 config=fx_default_config();config.max_entries=1;config.max_texture_bytes=2u*1024u*1024u;
 assert(zgpui_effect_configure(window,&config)==0);e=material();e.blur_radius=0;
 for(int i=1;i<=40;i++){e.source.id=(uint64_t)i;assert(zgpui_draw_effect(window,&e)==0);assert(fx.stats.live_entries==1);invariants();}
 assert(fx.stats.evictions>=39);config.max_texture_bytes=1;
 assert(zgpui_effect_configure(window,&config)==0&&!allocated_bytes);
 assert(zgpui_draw_effect(window,&e)==ZGPUI_EFFECT_BUDGET&&!allocated_bytes);
 reset();e=material();fail_image=2;assert(zgpui_draw_effect(window,&e)==ZGPUI_EFFECT_ALLOCATION);invariants();
 assert(live_textures==1);assert(zgpui_draw_effect(window,&e)==0);invariants();
 fx_frame_begin(639,480);assert(!allocated_bytes&&!live_textures);reset();
 legacy_context=1;assert(zgpui_effect_capabilities(window,&caps)==0&&caps.flags==ZGPUI_EFFECT_GPU);
 e=material();assert(zgpui_draw_effect(window,&e)==ZGPUI_EFFECT_FALLBACK&&!allocated_bytes&&!live_programs);
 assert(zgpui_effect_capture(window,&e.source)==ZGPUI_EFFECT_UNSUPPORTED);
 assert(zgpui_effect_finish(window)==ZGPUI_EFFECT_OK);
 reset();omit_fbo=1;assert(zgpui_effect_capabilities(window,&caps)==0&&!(caps.flags&ZGPUI_EFFECT_BLUR));
 e=material();assert(zgpui_draw_effect(window,&e)==ZGPUI_EFFECT_UNSUPPORTED);
 reset();e=material();e.source.x=NAN;assert(zgpui_draw_effect(window,&e)==ZGPUI_EFFECT_INVALID&&!allocated_bytes);
 fx_destroy();assert(!allocated_bytes&&!live_textures&&!live_shaders&&!live_programs&&!live_fbos);
 printf("{\"case\":\"stub\",\"passed\":true,\"cache_animation_recaptures\":0,\"owned_resources_after_destroy\":0}\n");
 return 0;
}
"""

SOFTWARE_SOURCE = r"""
#include <assert.h>
#include <math.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include "zgpui.h"
struct ZgpuiWindow {int unused;};
float zgpui_text_advance(const char *s,int size,int bold){(void)s;(void)size;(void)bold;return 0;}
ZgpuiBitmap zgpui_rasterize(const char *s,int size,int bold){(void)s;(void)size;(void)bold;ZgpuiBitmap result={0};return result;}
int main(void){
 struct ZgpuiWindow storage={0};ZgpuiWindow *window=&storage;ZgpuiEffectCapabilities caps;ZgpuiEffectStats stats;
 zgpui_begin(64,64,0x202020ff);assert(zgpui_effect_capabilities(window,&caps)==0&&caps.flags==ZGPUI_EFFECT_SOFTWARE);
 ZgpuiEffect e={0};e.source=(ZgpuiEffectCapture){1,1,4,4,48,48};e.x=4;e.y=4;e.width=48;e.height=48;
 e.saturation=e.opacity=e.tint_mix=1;e.tint_rgba=0xe0e0e0ff;e.fold_perspective=900;
 assert(zgpui_draw_effect(window,&e)==ZGPUI_EFFECT_FALLBACK);
 assert(zgpui_effect_stats(window,&stats)==0&&stats.fallbacks==1&&!stats.texture_bytes&&!stats.live_programs);
 assert(zgpui_effect_finish(window)==ZGPUI_EFFECT_OK);
 assert(zgpui_effect_capture(window,&e.source)==ZGPUI_EFFECT_UNSUPPORTED);
 e.flags=ZGPUI_EFFECT_LAYER|ZGPUI_EFFECT_CACHE_ONLY;assert(zgpui_draw_effect(window,&e)==ZGPUI_EFFECT_UNSUPPORTED);
 e.flags=0;e.opacity=NAN;assert(zgpui_draw_effect(window,&e)==ZGPUI_EFFECT_INVALID);
 uint32_t slot=7;char error[128];assert(zgpui_effect_shader_register(window,"x",1,&slot,error,sizeof(error))==ZGPUI_EFFECT_UNSUPPORTED&&slot==0&&error[0]);
 zgpui_gpu_destroy();printf("{\"case\":\"software\",\"passed\":true,\"explicit_fallback\":true}\n");return 0;
}
"""

REAL_SOURCE = r"""
#include <assert.h>
#include <GL/gl.h>
#include <math.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include "zgpui.h"
#define WIDTH 320
#define HEIGHT 240
static unsigned char before[WIDTH*HEIGHT*4],after[WIDTH*HEIGHT*4];
static void pixels(unsigned char *out){glFinish();glReadBuffer(GL_BACK);glReadPixels(0,0,WIDTH,HEIGHT,GL_RGBA,GL_UNSIGNED_BYTE,out);assert(glGetError()==GL_NO_ERROR);}
static void pattern(void){zgpui_begin(WIDTH,HEIGHT,0x202020ff);for(int x=0;x<WIDTH;x+=4)zgpui_rect((float)x,0,4,HEIGHT,0,((x/4)&1)?0xffffffff:0x000000ff);}
static double region_variance(const unsigned char *data,int x0,int y0,int x1,int y1){double sum=0,sum2=0;int n=0;
 for(int y=y0;y<y1;y++)for(int x=x0;x<x1;x++){double v=data[((HEIGHT-1-y)*WIDTH+x)*4];sum+=v;sum2+=v*v;n++;}
 return sum2/n-(sum/n)*(sum/n);}
static double variance(const unsigned char *data){return region_variance(data,110,95,185,145);}
static int red_at(const unsigned char *data,int x,int y){return data[((HEIGHT-1-y)*WIDTH+x)*4];}
static ZgpuiEffect effect(void){ZgpuiEffect e={0};e.source=(ZgpuiEffectCapture){1,1,80,60,140,120};
 e.x=80;e.y=60;e.width=140;e.height=120;e.saturation=e.opacity=e.tint_mix=e.fold_progress=1;e.fold_perspective=900;e.fold_anchor=0;return e;}
int main(void){char error[4096]={0};ZgpuiWindow *window=zgpui_open("zgpui-effects-check",WIDTH,HEIGHT,error,sizeof(error));
 if(!window){fprintf(stderr,"%s\n",error);return 1;}ZgpuiEffectCapabilities caps;ZgpuiEffectStats a,b;
 assert(zgpui_effect_capabilities(window,&caps)==0);assert(zgpui_effect_stats(window,&a)==0&&!a.texture_bytes&&!a.live_programs);
 if(!(caps.flags&ZGPUI_EFFECT_BLUR)){ZgpuiEffect e=effect();e.blur_radius=8;pattern();int status=zgpui_draw_effect(window,&e);
  assert(status==((caps.flags&ZGPUI_EFFECT_CAPTURE)?ZGPUI_EFFECT_UNSUPPORTED:ZGPUI_EFFECT_FALLBACK));
  printf("{\"case\":\"real-limited\",\"passed\":true,\"capability_flags\":%u,\"status\":%d}\n",caps.flags,status);zgpui_close(window);return 0;}
 pattern();pixels(before);ZgpuiEffect e=effect();e.blur_radius=16;
 assert(zgpui_draw_effect(window,&e)==0);pixels(after);double original=variance(before),blurred=variance(after);assert(original>12000&&blurred<original*0.4);
 assert(zgpui_effect_stats(window,&a)==0);for(int i=0;i<30;i++){pattern();e.fold_axis=1;e.fold_radians=(float)i*0.02f;assert(zgpui_draw_effect(window,&e)==0);}
 assert(zgpui_effect_finish(window)==0&&zgpui_effect_stats(window,&b)==0);assert(b.captures==a.captures&&b.blur_passes==a.blur_passes&&b.texture_bytes==a.texture_bytes);
 pattern();pixels(before);e.blur_radius=0;e.fold_radians=0.8f;assert(zgpui_draw_effect(window,&e)==0);pixels(after);
 int maximum=0;for(size_t i=0;i<sizeof(before);i++)if(i%4!=3){int delta=abs((int)before[i]-after[i]);if(delta>maximum)maximum=delta;}assert(maximum<=2);
 /* The blur gradient follows the active hinge, including right/top anchors;
  * changing only the fold/material continues to reuse the same capture. */
 e=effect();e.blur_radius=16;e.progressive_blur=1;e.fold_axis=1;e.fold_radians=0.18f;
 pattern();assert(zgpui_draw_effect(window,&e)==0);pixels(after);
 assert(region_variance(after,90,90,110,150)>region_variance(after,180,90,200,150)*2.0);
 e.fold_anchor=1;pattern();assert(zgpui_draw_effect(window,&e)==0);pixels(after);
 assert(region_variance(after,190,90,210,150)>region_variance(after,90,90,110,150)*2.0);
 e.fold_axis=2;e.fold_anchor=0;pattern();assert(zgpui_draw_effect(window,&e)==0);pixels(after);
 assert(region_variance(after,110,70,185,90)>region_variance(after,110,150,185,170)*2.0);
 e.fold_axis=1;e.progressive_blur=-1;pattern();assert(zgpui_draw_effect(window,&e)==0);pixels(after);
 assert(region_variance(after,180,90,200,150)>region_variance(after,90,90,110,150)*2.0);
 assert(zgpui_effect_clear(window)==0);zgpui_begin(WIDTH,HEIGHT,0xffffffff);
 e=effect();e.source.id=50;e.fold_axis=1;e.fold_radians=0.18f;e.dim=0.75f;
 assert(zgpui_draw_effect(window,&e)==0);pixels(after);assert(red_at(after,90,120)>220&&red_at(after,205,120)<110);
 zgpui_begin(WIDTH,HEIGHT,0xffffffff);e.fold_axis=0;e.fold_radians=0;
 assert(zgpui_draw_effect(window,&e)==0);pixels(after);assert(red_at(after,150,120)>=62&&red_at(after,150,120)<=66);
 assert(zgpui_effect_clear(window)==0);pattern();zgpui_rect(102,90,22,20,0,0xff3030ff);zgpui_text("layer",100,120,18,0,0x30ff30ff);
 ZgpuiEffectCapture capture={7,1,80,60,140,120};assert(zgpui_effect_capture(window,&capture)==0);
 zgpui_begin(WIDTH,HEIGHT,0x202020ff);e=effect();e.source=capture;e.source.width=e.source.height=0;e.flags=ZGPUI_EFFECT_LAYER|ZGPUI_EFFECT_CACHE_ONLY;e.x=120;e.y=75;
 assert(zgpui_draw_effect(window,&e)==0);pixels(after);size_t red=((HEIGHT-1-112)*WIDTH+151)*4;assert(after[red]>210&&after[red+1]<80);
 const char *shader="vec4 effect(vec2 p,vec2 s,vec4 c){return vec4(mix(c.rgb,vec3(1.0)-c.rgb,u_params[0]),c.a);}";uint32_t slot=0;
 assert(zgpui_effect_shader_register(window,shader,strlen(shader),&slot,error,sizeof(error))==0&&slot);
 e.shader_slot=slot;e.preset=ZGPUI_MATERIAL_CUSTOM;e.shader_params[0]=1;assert(zgpui_draw_effect(window,&e)==0);pixels(before);
 assert(before[red]<60&&before[red+1]>170);
 uint32_t bad=0;const char *invalid="this is not valid GLSL";assert(zgpui_effect_shader_register(window,invalid,strlen(invalid),&bad,error,sizeof(error))==ZGPUI_EFFECT_SHADER_ERROR&&error[0]&&!bad);
 assert(zgpui_effect_shader_remove(window,slot)==0);assert(zgpui_effect_clear(window)==0&&zgpui_effect_stats(window,&b)==0&&!b.texture_bytes&&!b.live_entries);
 printf("{\"case\":\"real\",\"passed\":true,\"stripe_variance_before\":%.3f,\"stripe_variance_after\":%.3f,\"screen_uv_max_channel_delta\":%d,\"hinge_blur_and_dim\":true,\"cache_animation_recaptures\":0}\n",original,blurred,maximum);
 zgpui_close(window);return 0;
}
"""


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--real", action="store_true", help="also compile and run actual desktop OpenGL pixel checks")
    parser.add_argument("--no-sanitize", action="store_true", help="disable ASan/UBSan when unsupported by the compiler")
    parser.add_argument("--no-leak-check", action="store_true", help="retain ASan/UBSan but disable LeakSanitizer in ptraced/restricted runners")
    parser.add_argument("--cc", default=os.environ.get("CC", "cc"))
    args = parser.parse_args()
    with tempfile.TemporaryDirectory(prefix="zgpui-effects-") as temporary:
        work = Path(temporary)
        (work / "GL").mkdir()
        (work / "GL/gl.h").write_text(FAKE_GL)
        (work / "GL/glx.h").write_text("/* Test loader replaces GLX. */\n")
        flags = ["-std=c11", "-O1", "-g", "-Wall", "-Wextra", "-Werror", "-I", str(ROOT / "native")]
        if not args.no_sanitize:
            flags += ["-fsanitize=address,undefined", "-fno-omit-frame-pointer"]
        env = {**os.environ, "ASAN_OPTIONS": f"detect_leaks={0 if args.no_leak_check else 1}:halt_on_error=1", "UBSAN_OPTIONS": "halt_on_error=1"}
        for name, content in (("stub", STUB_SOURCE), ("software", SOFTWARE_SOURCE)):
            source, binary = work / f"{name}.c", work / name
            source.write_text(content)
            command = [args.cc, *flags, str(source)]
            if name == "stub":
                command += ["-I", str(work)]
            else:
                command += ["-DZGPUI_SOFTWARE", str(ROOT / "native/draw_software.c")]
            subprocess.run([*command, "-lm", "-o", str(binary)], check=True, timeout=60)
            subprocess.run([str(binary)], check=True, timeout=30, env=env)
        if args.real:
            source, binary = work / "real.c", work / "real"
            source.write_text(REAL_SOURCE)
            libraries = shlex.split(subprocess.check_output(
                ["pkg-config", "--cflags", "--libs", "x11", "freetype2", "fontconfig", "gl"], text=True))
            # Existing native sources intentionally retain their historical C dialect.
            real_flags = [flag for flag in flags if flag not in ("-std=c11", "-Werror")]
            subprocess.run([args.cc, *real_flags, str(source), str(ROOT / "native/draw.c"),
                            str(ROOT / "native/linux.c"), *libraries, "-lm", "-o", str(binary)], check=True, timeout=120)
            subprocess.run([str(binary)], check=True, timeout=60, env=env)


if __name__ == "__main__":
    main()
