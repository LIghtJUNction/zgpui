#ifndef ZGPUI_EFFECT_SHADERS_H
#define ZGPUI_EFFECT_SHADERS_H

static const char fx_projective_vertex[] =
    "#version 120\n"
    "uniform vec4 u_rect; uniform vec2 u_view;\n"
    "uniform vec4 u_fold; uniform float u_anchor; uniform int u_axis;\n"
    "varying vec2 v_local;\n"
    "void main(){\n"
    " v_local=gl_MultiTexCoord0.xy;\n"
    " vec2 p=u_rect.xy+v_local*u_rect.zw;\n"
    " vec2 hinge=u_rect.xy+u_rect.zw*0.5;\n"
    " float angle=u_fold.x*u_fold.y; float z=0.0;\n"
    " if(u_axis==1){hinge.x=u_rect.x+u_rect.z*u_anchor;\n"
    "  float d=p.x-hinge.x; p.x=hinge.x+d*cos(angle); z=d*sin(angle);}\n"
    " if(u_axis==2){hinge.y=u_rect.y+u_rect.w*u_anchor;\n"
    "  float d=p.y-hinge.y; p.y=hinge.y+d*cos(angle); z=-d*sin(angle);}\n"
    " float q=max(0.20,1.0-z/u_fold.z);\n"
    " vec2 projected=hinge+(p-hinge)/q;\n"
    " vec2 ndc=vec2(projected.x*2.0/u_view.x-1.0,1.0-projected.y*2.0/u_view.y);\n"
    " gl_Position=vec4(ndc*q,0.0,q);\n"
    "}\n";

static const char fx_blur_vertex[] =
    "#version 120\n"
    "varying vec2 v_local;\n"
    "void main(){v_local=gl_MultiTexCoord0.xy;gl_Position=ftransform();}\n";

static const char fx_blur_fragment[] =
    "#version 120\n"
    "uniform sampler2D u_source; uniform vec2 u_span;\n"
    "uniform vec2 u_step; uniform vec4 u_bounds; varying vec2 v_local;\n"
    "vec4 sample_at(vec2 uv){return texture2D(u_source,clamp(uv,u_bounds.xy,u_bounds.zw));}\n"
    "void main(){vec2 uv=v_local*u_span;\n"
    " vec4 c=sample_at(uv)*0.227027027;\n"
    " c+=(sample_at(uv+u_step*1.384615385)+sample_at(uv-u_step*1.384615385))*0.316216216;\n"
    " c+=(sample_at(uv+u_step*3.230769231)+sample_at(uv-u_step*3.230769231))*0.070270270;\n"
    " gl_FragColor=c;}\n";

static const char fx_material_head[] =
    "#version 120\n"
    "uniform sampler2D u_raw; uniform sampler2D u_blurred;\n"
    "uniform vec4 u_rect; uniform vec2 u_view; uniform vec4 u_source_rect;\n"
    "uniform vec4 u_capture; uniform vec2 u_raw_size; uniform vec2 u_blur_size;\n"
    "uniform vec4 u_tint; uniform vec4 u_material; uniform vec4 u_surface;\n"
    "uniform vec4 u_fold; uniform vec2 u_blur_content; uniform int u_preset;\n"
    "uniform float u_anchor; uniform int u_axis;\n"
    "uniform int u_layer; uniform float u_progressive; uniform float u_highlight;\n"
    "uniform float u_params[8]; varying vec2 v_local;\n"
    "float hash21(vec2 p){return fract(sin(dot(p,vec2(127.1,311.7)))*43758.5453123);}\n"
    "vec4 sample_raw(vec2 pixel){vec2 p=clamp(pixel,vec2(0.5),u_capture.zw-vec2(0.5));\n"
    " return texture2D(u_raw,p/u_raw_size);}\n"
    "vec4 sample_blurred(vec2 pixel){vec2 uv=pixel/u_capture.zw*u_blur_content;\n"
    " uv=clamp(uv,vec2(0.5),u_blur_content-vec2(0.5));\n"
    " return texture2D(u_blurred,uv/u_blur_size);}\n"
    "float rounded_distance(vec2 local){float r=min(u_material.x,min(u_rect.z,u_rect.w)*0.5);\n"
    " vec2 q=abs(local*u_rect.zw-u_rect.zw*0.5)-(u_rect.zw*0.5-vec2(r));\n"
    " return length(max(q,vec2(0.0)))+min(max(q.x,q.y),0.0)-r;}\n";

static const char fx_default_effect[] =
    "vec4 effect(vec2 local_uv,vec2 screen_uv,vec4 base_color){return base_color;}\n";

static const char fx_material_main[] =
    "void main(){\n"
    " float d=rounded_distance(v_local);float aa=max(fwidth(d),0.65);\n"
    " float coverage=1.0-smoothstep(-aa*0.5,aa*0.5,d);if(coverage<=0.0)discard;\n"
    " vec2 screen_pixel=vec2(gl_FragCoord.x,u_view.y-gl_FragCoord.y);\n"
    " vec2 sample_pixel=screen_pixel;\n"
    " if(u_layer!=0)sample_pixel=u_source_rect.xy+v_local*u_source_rect.zw;\n"
    " vec2 normal=normalize((v_local-vec2(0.5))*u_rect.zw+vec2(0.0001));\n"
    " float edge_lens=1.0-smoothstep(0.0,max(4.0,u_material.x+8.0),abs(d));\n"
    " if(u_preset==2)sample_pixel+=normal*u_surface.z*edge_lens;\n"
    " vec2 pixel=vec2(sample_pixel.x-u_capture.x,u_view.y-sample_pixel.y-u_capture.y);\n"
    " vec4 sharp=sample_raw(pixel);vec4 blurred=sample_blurred(pixel);\n"
    " float amount=u_material.y;\n"
    " bool folded=u_axis!=0 && abs(u_fold.x*u_fold.y)>0.000001;\n"
    " float distance_from_hinge=clamp(abs((u_axis==1?v_local.x:v_local.y)-u_anchor)/max(u_anchor,1.0-u_anchor),0.0,1.0);\n"
    " float gradient=folded?distance_from_hinge:v_local.y;\n"
    " if(u_progressive<0.0)gradient=1.0-gradient;\n"
    " amount*=mix(1.0,smoothstep(0.0,1.0,gradient),abs(u_progressive));\n"
    " vec4 c=mix(sharp,blurred,amount);\n"
    " if(u_preset==2 && u_surface.z>0.0){\n"
    "  vec2 chroma=normal*u_surface.z*0.075*edge_lens;\n"
    "  c.r=mix(c.r,sample_raw(pixel+chroma).r,0.28);\n"
    "  c.b=mix(c.b,sample_raw(pixel-chroma).b,0.28);}\n"
    " float grey=dot(c.rgb,vec3(0.2126,0.7152,0.0722));\n"
    " c.rgb=mix(vec3(grey),c.rgb,u_material.z);\n"
    " c.rgb=mix(c.rgb,u_tint.rgb,u_tint.a);\n"
    " float grain=hash21(floor(screen_pixel))-0.5;\n"
    " if(u_preset==1)c.rgb*=0.97+0.03*v_local.y;\n"
    " if(u_preset==3){float fiber=sin(screen_pixel.y*2.1+sin(screen_pixel.x*0.13))*0.5;\n"
    "  c.rgb+=(grain*0.65+fiber*0.35)*u_surface.y*0.07;}\n"
    " else c.rgb+=grain*u_surface.y*0.055;\n"
    " float edge=1.0-smoothstep(0.0,u_surface.w+0.75,abs(d));\n"
    " float light=0.35+0.65*max(dot(normal,normalize(vec2(-0.5,-0.85))),0.0);\n"
    " c.rgb=mix(c.rgb,vec3(1.0),edge*u_highlight*light);\n"
    " float dim_scale=folded?smoothstep(0.0,1.0,distance_from_hinge):1.0;\n"
    " c.rgb*=1.0-u_fold.w*dim_scale;\n"
    " c=effect(v_local,screen_pixel/u_view,c);\n"
    " gl_FragColor=vec4(clamp(c.rgb,0.0,1.0),clamp(c.a,0.0,1.0)*u_material.w*coverage);\n"
    "}\n";
#endif
