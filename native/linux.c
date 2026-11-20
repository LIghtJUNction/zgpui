#include "zgpui.h"
#include <X11/Xlib.h>
#include <X11/Xutil.h>
#include <X11/keysym.h>
#include <X11/Xatom.h>
#ifndef ZGPUI_SOFTWARE
#include <GL/glx.h>
#endif
#include <fontconfig/fontconfig.h>
#ifdef __GLIBC__
#include <malloc.h>
#endif
#include <ft2build.h>
#include FT_FREETYPE_H
#include <poll.h>
#include <locale.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
struct ZgpuiWindow {Display *display;Window window;Colormap colormap;
#ifdef ZGPUI_SOFTWARE
    GC gc; Visual *visual; int depth;
#else
    GLXContext context;
#endif
    XIM im;XIC ic;Atom close,clipboard,utf8,paste,targets;char *copied;int width,height;};
ZgpuiWindow *zgpui_open(const char *title,int width,int height,char *error,size_t capacity){
    setlocale(LC_CTYPE,"");
    ZgpuiWindow *w=calloc(1,sizeof(*w));if(!w)return NULL;
    w->display=XOpenDisplay(NULL);if(!w->display){snprintf(error,capacity,"Cannot open X11 display. On Wayland, install/enable XWayland.");free(w);return NULL;}
#ifdef ZGPUI_SOFTWARE
    XVisualInfo software_visual;
    if (!XMatchVisualInfo(w->display,DefaultScreen(w->display),DefaultDepth(w->display,DefaultScreen(w->display)),TrueColor,&software_visual) ||
        software_visual.red_mask != 0xff0000 || software_visual.green_mask != 0xff00 || software_visual.blue_mask != 0xff) {
        snprintf(error,capacity,"The CPU renderer requires a 24/32-bit RGB X11 visual.");
        XCloseDisplay(w->display);free(w);return NULL;
    }
    XVisualInfo *visual=&software_visual;
    w->visual=visual->visual;w->depth=visual->depth;
#else
    int attrs[]={GLX_RGBA,GLX_DOUBLEBUFFER,GLX_RED_SIZE,8,GLX_GREEN_SIZE,8,GLX_BLUE_SIZE,8,None};
    XVisualInfo *visual=glXChooseVisual(w->display,DefaultScreen(w->display),attrs);
    if(!visual){snprintf(error,capacity,"No OpenGL-compatible X11 visual.");XCloseDisplay(w->display);free(w);return NULL;}
#endif
    XSetWindowAttributes a={0};a.colormap=XCreateColormap(w->display,RootWindow(w->display,visual->screen),visual->visual,AllocNone);
    a.event_mask=ExposureMask|StructureNotifyMask|PointerMotionMask|ButtonPressMask|ButtonReleaseMask|KeyPressMask|FocusChangeMask;
    w->colormap=a.colormap;
    w->window=XCreateWindow(w->display,RootWindow(w->display,visual->screen),80,80,width,height,0,visual->depth,InputOutput,visual->visual,CWColormap|CWEventMask,&a);
#ifdef ZGPUI_SOFTWARE
    w->gc=XCreateGC(w->display,w->window,0,NULL);
    if(!w->gc || !zgpui_software_resize(width,height)){snprintf(error,capacity,"Cannot allocate the CPU framebuffer.");zgpui_close(w);return NULL;}
    XImage *probe=XCreateImage(w->display,w->visual,(unsigned int)w->depth,ZPixmap,0,NULL,(unsigned int)width,(unsigned int)height,32,0);
    int valid_image=probe && probe->bits_per_pixel==32 && probe->bytes_per_line==width*4;
    if(probe)XDestroyImage(probe);
    if(!valid_image){snprintf(error,capacity,"Unsupported X11 image format for the CPU renderer.");zgpui_close(w);return NULL;}
#else
    w->context=glXCreateContext(w->display,visual,NULL,True);XFree(visual);
    if(!w->context||!glXMakeCurrent(w->display,w->window,w->context)){snprintf(error,capacity,"OpenGL context creation failed.");zgpui_close(w);return NULL;}
#endif
    w->width=width;w->height=height;XStoreName(w->display,w->window,title);
    // The caller owns window identity. Keep the leading title identifier stable
    // when a caller appends a translated description or document name.
    char instance[128],class_name[128];size_t length=0;
    while(length<sizeof(instance)-1){
        unsigned char c=(unsigned char)title[length];
        if(!((c>='a'&&c<='z')||(c>='A'&&c<='Z')||(c>='0'&&c<='9')||c=='-'||c=='_'||c=='.'))break;
        instance[length]=(char)((c>='A'&&c<='Z')?c-'A'+'a':c);length++;
    }
    if(!length){static const char fallback[]="zgpui";length=sizeof(fallback)-1;memcpy(instance,fallback,length);}instance[length]=0;
    memcpy(class_name,instance,length+1);if(class_name[0]>='a'&&class_name[0]<='z')class_name[0]=(char)(class_name[0]-'a'+'A');
    XClassHint class_hint={instance,class_name};XSetClassHint(w->display,w->window,&class_hint);
    XSizeHints hints={0};hints.flags=PMinSize;hints.min_width=width;hints.min_height=height;XSetWMNormalHints(w->display,w->window,&hints);
    w->close=XInternAtom(w->display,"WM_DELETE_WINDOW",False);XSetWMProtocols(w->display,w->window,&w->close,1);
    w->clipboard=XInternAtom(w->display,"CLIPBOARD",False);w->utf8=XInternAtom(w->display,"UTF8_STRING",False);w->paste=XInternAtom(w->display,"ZGPUI_PASTE",False);w->targets=XInternAtom(w->display,"TARGETS",False);
    Atom net_name=XInternAtom(w->display,"_NET_WM_NAME",False);XChangeProperty(w->display,w->window,net_name,w->utf8,8,PropModeReplace,(const unsigned char*)title,(int)strlen(title));
    w->im=XOpenIM(w->display,NULL,NULL,NULL);
    if(w->im)w->ic=XCreateIC(w->im,XNInputStyle,XIMPreeditNothing|XIMStatusNothing,XNClientWindow,w->window,XNFocusWindow,w->window,NULL);
    if(w->ic)XSetICFocus(w->ic);
    XMapWindow(w->display,w->window);XFlush(w->display);return w;
}
void zgpui_close(ZgpuiWindow *w){
    if(!w)return;
    if(w->display){
#ifdef ZGPUI_SOFTWARE
        if(w->gc)XFreeGC(w->display,w->gc);
        zgpui_gpu_destroy();
#else
        if(w->context){glXMakeCurrent(w->display,w->window,w->context);zgpui_gpu_destroy();glXMakeCurrent(w->display,None,NULL);glXDestroyContext(w->display,w->context);}
#endif
        if(w->ic)XDestroyIC(w->ic);
        if(w->im)XCloseIM(w->im);
        if(w->window)XDestroyWindow(w->display,w->window);
        if(w->colormap)XFreeColormap(w->display,w->colormap);
        XCloseDisplay(w->display);
    }
    free(w->copied);free(w);zgpui_font_shutdown();
}
void zgpui_size(ZgpuiWindow *w,int *width,int *height){*width=w->width;*height=w->height;}
void zgpui_swap(ZgpuiWindow *w){
#ifdef ZGPUI_SOFTWARE
    int width=0,height=0;
    uint32_t *pixels=zgpui_software_pixels(&width,&height);
    if(!pixels || width!=w->width || height!=w->height)return;
    XImage *image=XCreateImage(w->display,w->visual,(unsigned int)w->depth,ZPixmap,0,(char *)pixels,(unsigned int)width,(unsigned int)height,32,0);
    if(!image)return;
    const unsigned int endian=1;
    image->byte_order=(*(const unsigned char *)&endian)?LSBFirst:MSBFirst;
    XPutImage(w->display,w->window,w->gc,image,0,0,0,0,(unsigned int)width,(unsigned int)height);
    XFlush(w->display);
    /* draw_software owns the single framebuffer; XDestroyImage owns only metadata. */
    image->data=NULL;XDestroyImage(image);
    /* Ordinary XPutImage copies the bytes before returning (unlike XShm).
       XFlush drains Xlib's copy; the server no longer needs our client pages. */
    zgpui_software_release();
#else
    glXSwapBuffers(w->display,w->window);
#endif
}
void zgpui_clipboard(ZgpuiWindow *w,const char *text){free(w->copied);w->copied=strdup(text);XSetSelectionOwner(w->display,w->clipboard,w->window,CurrentTime);XFlush(w->display);}
static int key(KeySym k){switch(k){case XK_Tab:case XK_ISO_Left_Tab:return 9;case XK_Return:case XK_KP_Enter:return 13;case XK_Escape:return 27;case XK_BackSpace:return 8;case XK_Delete:return 127;case XK_Left:return 1001;case XK_Right:return 1002;case XK_Up:return 1003;case XK_Down:return 1004;case XK_Home:return 1005;case XK_End:return 1006;default:return k<128?(int)k:0;}}
int zgpui_event(ZgpuiWindow *w,ZgpuiEvent *out,int timeout){
    memset(out,0,sizeof(*out));
    if(!XPending(w->display)){struct pollfd fd={ConnectionNumber(w->display),POLLIN,0};if(poll(&fd,1,timeout)<=0)return 0;}
    while(XPending(w->display)){
        XEvent e;XNextEvent(w->display,&e);if(XFilterEvent(&e,None))continue;
        if(e.type==ClientMessage&&(Atom)e.xclient.data.l[0]==w->close){out->kind=7;return 1;}
        if(e.type==Expose){if(e.xexpose.count)continue;out->kind=6;out->width=w->width;out->height=w->height;return 1;}
        if(e.type==ConfigureNotify){if(w->width==e.xconfigure.width&&w->height==e.xconfigure.height)continue;
#ifdef ZGPUI_SOFTWARE
            if(!zgpui_software_resize(e.xconfigure.width,e.xconfigure.height)){XResizeWindow(w->display,w->window,(unsigned int)w->width,(unsigned int)w->height);continue;}
#endif
            w->width=e.xconfigure.width;w->height=e.xconfigure.height;out->kind=6;out->width=w->width;out->height=w->height;return 1;}
        if(e.type==MotionNotify){out->kind=1;out->x=e.xmotion.x;out->y=e.xmotion.y;return 1;}
        if(e.type==ButtonPress||e.type==ButtonRelease){out->x=e.xbutton.x;out->y=e.xbutton.y;if(e.xbutton.button==4||e.xbutton.button==5){if(e.type==ButtonRelease)continue;out->kind=8;out->key=e.xbutton.button==4?-1:1;}else{if(e.xbutton.button!=1)continue;out->kind=e.type==ButtonPress?2:3;}return 1;}
        if(e.type==FocusIn){if(w->ic)XSetICFocus(w->ic);continue;}if(e.type==FocusOut){if(w->ic)XUnsetICFocus(w->ic);continue;}
        if(e.type==KeyPress){
            KeySym sym=0;Status status=0;int count=w->ic?Xutf8LookupString(w->ic,&e.xkey,out->text,sizeof(out->text)-1,&sym,&status):XLookupString(&e.xkey,out->text,sizeof(out->text)-1,&sym,NULL);
            if(status==XBufferOverflow)count=0;if(count<0)count=0;out->text[count]=0;
            out->kind=4;out->key=key(sym);out->modifiers=(e.xkey.state&ControlMask?1:0)|(e.xkey.state&ShiftMask?2:0)|(e.xkey.state&Mod1Mask?4:0);
            if((out->modifiers&1)&&(sym==XK_v||sym==XK_V)){XConvertSelection(w->display,w->clipboard,w->utf8,w->paste,w->window,CurrentTime);XFlush(w->display);out->text[0]=0;}
            return 1;
        }
        if(e.type==SelectionNotify&&e.xselection.property!=None){
            Atom type;int format;unsigned long count,remaining;unsigned char *data=NULL;
            if(XGetWindowProperty(w->display,w->window,w->paste,0,1023,True,AnyPropertyType,&type,&format,&count,&remaining,&data)==Success&&data){
                if(format==8&&count<sizeof(out->text)){memcpy(out->text,data,count);out->text[count]=0;out->kind=5;}XFree(data);
            }if(out->kind)return 1;
        }
        if(e.type==SelectionRequest){
            XSelectionRequestEvent *q=&e.xselectionrequest;XEvent reply={0};reply.xselection.type=SelectionNotify;reply.xselection.display=q->display;reply.xselection.requestor=q->requestor;reply.xselection.selection=q->selection;reply.xselection.target=q->target;reply.xselection.time=q->time;reply.xselection.property=None;
            Atom property=q->property?q->property:q->target;
            if(q->target==w->targets){Atom supported[]={w->targets,w->utf8,XA_STRING};XChangeProperty(w->display,q->requestor,property,XA_ATOM,32,PropModeReplace,(unsigned char*)supported,3);reply.xselection.property=property;}
            else if((q->target==w->utf8||q->target==XA_STRING)&&w->copied){XChangeProperty(w->display,q->requestor,property,q->target,8,PropModeReplace,(unsigned char*)w->copied,(int)strlen(w->copied));reply.xselection.property=property;}
            XSendEvent(w->display,q->requestor,False,0,&reply);XFlush(w->display);
        }
    }return 0;
}
static FT_Library library;
static FT_Face faces[12];
static char *paths[12];
static int face_count;
static unsigned int face_weights[12];
static int face_indices[12];
static FT_Face face_for(uint32_t cp,int size,int bold){
    if(!library&&FT_Init_FreeType(&library))return NULL;
    bold=!!bold;
    /* Reuse an already opened face before asking fontconfig to enumerate the
       installed fonts. A face owns its file independently of the match config. */
    for(int i=0;i<face_count;i++){
        if((face_weights[i]&(1u<<bold))&&FT_Get_Char_Index(faces[i],cp)){
            FT_Set_Pixel_Sizes(faces[i],0,size);
            return faces[i];
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
    FcPatternAddString(pattern,FC_FAMILY,(const FcChar8*)"sans");
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
            for(int i=0;i<face_count;i++){
                if(face_indices[i]==index&&strcmp(paths[i],(const char*)path)==0){
                    face_weights[i]|=1u<<bold;
                    face=faces[i];break;
                }
            }
            if(!face&&face_count<12&&!FT_New_Face(library,(const char*)path,index,&face)){
                char *owned=strdup((const char*)path);
                if(owned){
                    paths[face_count]=owned;
                    face_indices[face_count]=index;
                    face_weights[face_count]=1u<<bold;
                    faces[face_count++]=face;
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
static uint32_t codepoint(const unsigned char **p){
    uint32_t c=*(*p)++;if(c<0x80)return c;unsigned int count=c<0xe0?1:c<0xf0?2:3;c&=(1u<<(6-count))-1;
    while(count--&&**p){c=(c<<6)|(*(*p)++&63);}return c;
}
ZgpuiBitmap zgpui_rasterize(const char *text,int size,int bold){
    ZgpuiBitmap out={0};if(size<1||size>96)return out;FT_Face regular=face_for('A',size,bold);if(!regular)return out;
    int ascent=(int)(regular->size->metrics.ascender>>6),descent=(int)(-regular->size->metrics.descender>>6);
    int width=2,height=ascent+descent+2;const unsigned char *p=(const unsigned char*)text;
    while(*p){uint32_t cp=codepoint(&p);FT_Face f=FT_Get_Char_Index(regular,cp)?regular:face_for(cp,size,bold);if(f&&!FT_Load_Char(f,cp,FT_LOAD_RENDER))width+=(int)(f->glyph->advance.x>>6);}
    if(width<1||width>8192||height>256)return out;out.pixels=calloc((size_t)width,height);if(!out.pixels)return out;out.width=width;out.height=height;out.advance=(float)(width-2);
    int x=1;p=(const unsigned char*)text;
    while(*p){uint32_t cp=codepoint(&p);FT_Face f=FT_Get_Char_Index(regular,cp)?regular:face_for(cp,size,bold);if(!f||FT_Load_Char(f,cp,FT_LOAD_RENDER))continue;FT_GlyphSlot g=f->glyph;
        for(unsigned int row=0;row<g->bitmap.rows;row++)for(unsigned int col=0;col<g->bitmap.width;col++){int dx=x+g->bitmap_left+(int)col,dy=ascent-g->bitmap_top+(int)row+1;if(dx>=0&&dx<width&&dy>=0&&dy<height)out.pixels[(size_t)dy*width+dx]=g->bitmap.buffer[row*(unsigned int)g->bitmap.pitch+col];}
        x+=(int)(g->advance.x>>6);
    }return out;
}
void zgpui_font_shutdown(void){for(int i=0;i<face_count;i++){FT_Done_Face(faces[i]);free(paths[i]);}face_count=0;if(library){FT_Done_FreeType(library);library=NULL;}}
