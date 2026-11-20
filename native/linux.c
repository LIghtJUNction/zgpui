#include "zgpui.h"
#include <X11/Xlib.h>
#include <X11/Xutil.h>
#include <X11/keysym.h>
#include <X11/Xatom.h>
#ifndef ZGPUI_SOFTWARE
#include <GL/glx.h>
#endif
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
#include "font_linux.h"
static FontState fonts;
int zgpui_set_font_family(ZgpuiWindow *w,const char *family,char *error,size_t capacity){
    if(!w || !w->display || !family){snprintf(error,capacity,"No active font window.");return 0;}
    size_t length=0;
    while(length<256 && family[length]){
        unsigned char c=(unsigned char)family[length++];
        if(c<32 || c==127){snprintf(error,capacity,"Invalid font family.");return 0;}
    }
    if(length>255){snprintf(error,capacity,"Font family is too long.");return 0;}
    FontState next={0};memcpy(next.family,family,length+1);
    /* Prepare independent faces first: a failed match never invalidates live text. */
    if(!face_for_state(&next,'A',16,0) || !face_for_state(&next,'A',16,1)){
        font_state_destroy(&next);snprintf(error,capacity,"Cannot prepare the requested font family.");return 0;
    }
#ifndef ZGPUI_SOFTWARE
    if(!w->context || !glXMakeCurrent(w->display,w->window,w->context)){
        font_state_destroy(&next);snprintf(error,capacity,"Cannot activate the font renderer.");return 0;
    }
#endif
    zgpui_clear_text_cache();
    FontState previous=fonts;fonts=next;
    font_state_destroy(&previous);
    return 1;
}
float zgpui_text_advance(const char *text,int size,int bold){
    return font_text_advance(&fonts,text,size,bold);
}
ZgpuiBitmap zgpui_rasterize(const char *text,int size,int bold){
    return font_rasterize(&fonts,text,size,bold);
}
void zgpui_font_shutdown(void){font_state_destroy(&fonts);}
