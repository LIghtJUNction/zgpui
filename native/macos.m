#include "zgpui.h"
#import <AppKit/AppKit.h>
#import <OpenGL/gl.h>
#import <CoreText/CoreText.h>
#include <stdlib.h>
#include <stdio.h>
#include <string.h>
#include <math.h>
struct ZgpuiWindow{NSWindow *window;NSOpenGLContext *context;int width,height;};
static NSString *font_family=nil;
static NSFont *font_for(NSString *family,int size,int bold){
    if(!family||![family length])return bold?[NSFont boldSystemFontOfSize:size]:[NSFont systemFontOfSize:size];
    if(![family isEqualToString:@"monospace"]){
        NSFont *font=[[NSFontManager sharedFontManager]fontWithFamily:family traits:bold?NSBoldFontMask:0 weight:bold?9:5 size:size];
        if(font)return font;
    }
    return [NSFont monospacedSystemFontOfSize:size weight:bold?NSFontWeightSemibold:NSFontWeightRegular];
}
int zgpui_set_font_family(ZgpuiWindow *w,const char *family,char *error,size_t capacity){
    @autoreleasepool{
        if(!w||!w->context||!family||strlen(family)>255){snprintf(error,capacity,"Invalid font family.");return 0;}
        for(const unsigned char *p=(const unsigned char*)family;*p;p++)if(*p<32||*p==127){snprintf(error,capacity,"Invalid font family.");return 0;}
        NSString *next=[[NSString alloc]initWithUTF8String:family];
        if(!next||!font_for(next,16,0)||!font_for(next,16,1)){
            [next release];snprintf(error,capacity,"Cannot prepare the requested font family.");return 0;
        }
        [w->context makeCurrentContext];
        if([NSOpenGLContext currentContext]!=w->context){[next release];snprintf(error,capacity,"Cannot activate the font renderer.");return 0;}
        zgpui_clear_text_cache();NSString *previous=font_family;font_family=next;[previous release];return 1;
    }
}
ZgpuiWindow *zgpui_open(const char *title,int width,int height,char *error,size_t capacity){
    @autoreleasepool{
        [NSApplication sharedApplication];[NSApp setActivationPolicy:NSApplicationActivationPolicyRegular];
        NSMenu *menu=[[[NSMenu alloc]init]autorelease];NSMenuItem *root=[[[NSMenuItem alloc]init]autorelease];[menu addItem:root];
        NSMenu *app=[[[NSMenu alloc]init]autorelease];[app addItemWithTitle:@"Quit" action:@selector(terminate:) keyEquivalent:@"q"];[root setSubmenu:app];[NSApp setMainMenu:menu];
        ZgpuiWindow *w=calloc(1,sizeof(*w));if(!w)return NULL;
        w->window=[[NSWindow alloc]initWithContentRect:NSMakeRect(0,0,width,height) styleMask:NSWindowStyleMaskTitled|NSWindowStyleMaskClosable|NSWindowStyleMaskMiniaturizable|NSWindowStyleMaskResizable backing:NSBackingStoreBuffered defer:NO];
        [w->window setReleasedWhenClosed:NO];[w->window setTitle:[NSString stringWithUTF8String:title]];[w->window setContentMinSize:NSMakeSize(width,height)];[w->window setAcceptsMouseMovedEvents:YES];
        // Prefer acceleration while allowing the system software renderer in virtual sessions.
        // https://developer.apple.com/documentation/appkit/opengl-pixel-format-attributes
        NSOpenGLPixelFormatAttribute attrs[]={NSOpenGLPFADoubleBuffer,NSOpenGLPFAColorSize,24,NSOpenGLPFAAlphaSize,8,0};
        NSOpenGLPixelFormat *format=[[NSOpenGLPixelFormat alloc]initWithAttributes:attrs];
        w->context=[[NSOpenGLContext alloc]initWithFormat:format shareContext:nil];[format release];
        if(!w->context){snprintf(error,capacity,"macOS OpenGL context creation failed.");zgpui_close(w);return NULL;}
        [w->context setView:[w->window contentView]];[w->context makeCurrentContext];GLint interval=1;[w->context setValues:&interval forParameter:NSOpenGLCPSwapInterval];
        w->width=width;w->height=height;[w->window center];[w->window makeKeyAndOrderFront:nil];[NSApp finishLaunching];[NSApp activateIgnoringOtherApps:YES];return w;
    }
}
void zgpui_close(ZgpuiWindow *w){if(!w)return;@autoreleasepool{if(w->context){[w->context makeCurrentContext];zgpui_gpu_destroy();[NSOpenGLContext clearCurrentContext];[w->context clearDrawable];[w->context release];}if(w->window){[w->window close];[w->window release];}free(w);zgpui_font_shutdown();}}
void zgpui_set_min_size(ZgpuiWindow *w,int width,int height){
    @autoreleasepool { [w->window setContentMinSize:NSMakeSize(width,height)]; }
}
void zgpui_size(ZgpuiWindow *w,int *width,int *height){*width=w->width;*height=w->height;}
void zgpui_swap(ZgpuiWindow *w){[w->context flushBuffer];}
static int key(unsigned short code){switch(code){case 48:return 9;case 36:case 76:return 13;case 53:return 27;case 51:return 8;case 117:return 127;case 123:return 1001;case 124:return 1002;case 126:return 1003;case 125:return 1004;case 115:return 1005;case 119:return 1006;default:return 0;}}
static int resized(ZgpuiWindow *w,ZgpuiEvent *out){
    NSRect bounds=[[w->window contentView]bounds];
    if((int)bounds.size.width==w->width && (int)bounds.size.height==w->height)return 0;
    w->width=(int)bounds.size.width;w->height=(int)bounds.size.height;
    [w->context update];out->kind=6;return 1;
}
int zgpui_event(ZgpuiWindow *w,ZgpuiEvent *out,int timeout){
    @autoreleasepool{
        memset(out,0,sizeof(*out));if(![w->window isVisible]){out->kind=7;return 1;}
        // A resize gets its own frame. Leave the next pointer or key event in the queue.
        if(resized(w,out))return 1;
        NSEvent *event=[NSApp nextEventMatchingMask:NSEventMaskAny untilDate:[NSDate dateWithTimeIntervalSinceNow:timeout/1000.0] inMode:NSDefaultRunLoopMode dequeue:NO];
        if(resized(w,out))return 1;
        if(event)event=[NSApp nextEventMatchingMask:NSEventMaskAny untilDate:[NSDate distantPast] inMode:NSDefaultRunLoopMode dequeue:YES];
        if(!event)return 0;
        NSPoint point=[[w->window contentView]convertPoint:[event locationInWindow] fromView:nil];out->x=(int)point.x;out->y=w->height-(int)point.y;
        switch([event type]){
            case NSEventTypeMouseMoved:case NSEventTypeLeftMouseDragged:out->kind=1;break;
            case NSEventTypeLeftMouseDown:out->kind=2;break;case NSEventTypeLeftMouseUp:out->kind=3;break;
            case NSEventTypeScrollWheel:out->kind=8;out->key=[event scrollingDeltaY]>0?-1:1;break;
            case NSEventTypeKeyDown:{
                out->kind=4;NSEventModifierFlags flags=[event modifierFlags];out->modifiers=(flags&(NSEventModifierFlagControl|NSEventModifierFlagCommand)?1:0)|(flags&NSEventModifierFlagShift?2:0);out->key=key([event keyCode]);
                const char *text=[[event characters]UTF8String];const char *plain=[[event charactersIgnoringModifiers]UTF8String];
                if(!out->key&&plain)out->key=(unsigned char)*plain;
                if((out->modifiers&1)&&(out->key=='v'||out->key=='V')){NSString *paste=[[NSPasteboard generalPasteboard]stringForType:NSPasteboardTypeString];if(paste){strncpy(out->text,[paste UTF8String],sizeof(out->text)-1);out->kind=5;}}
                else if(text&&!(out->modifiers&1)){strncpy(out->text,text,sizeof(out->text)-1);}
                if((flags&NSEventModifierFlagCommand)&&(out->key=='q'||out->key=='Q'))out->kind=7;
                break;
            }
            default:break;
        }
        if(!out->kind||[event type]!=NSEventTypeKeyDown)[NSApp sendEvent:event];[NSApp updateWindows];return out->kind?1:0;
    }
}
void zgpui_clipboard(ZgpuiWindow *w,const char *text){(void)w;@autoreleasepool{NSPasteboard *p=[NSPasteboard generalPasteboard];[p clearContents];[p setString:[NSString stringWithUTF8String:text] forType:NSPasteboardTypeString];}}
float zgpui_text_advance(const char *text,int size,int bold){
    if(!text||!*text||size<1||size>96)return 0;
    @autoreleasepool{
        NSString *string=[NSString stringWithUTF8String:text];if(!string)return 0;
        NSFont *font=font_for(font_family,size,bold);if(!font)return 0;
        NSAttributedString *attributed=[[[NSAttributedString alloc]initWithString:string
            attributes:@{NSFontAttributeName:font}]autorelease];
        CTLineRef line=CTLineCreateWithAttributedString((CFAttributedStringRef)attributed);
        if(!line)return 0;
        float advance=(float)CTLineGetTypographicBounds(line,NULL,NULL,NULL);
        CFRelease(line);return advance;
    }
}
ZgpuiBitmap zgpui_rasterize(const char *text,int size,int bold){
    ZgpuiBitmap out={0};@autoreleasepool{
        NSString *string=[NSString stringWithUTF8String:text];if(!string)return out;
        NSFont *font=font_for(font_family,size,bold);if(!font)return out;
        NSDictionary *attrs=@{NSFontAttributeName:font,NSForegroundColorAttributeName:[NSColor whiteColor]};
        NSAttributedString *attributed=[[[NSAttributedString alloc]initWithString:string attributes:attrs]autorelease];
        CTLineRef line=CTLineCreateWithAttributedString((CFAttributedStringRef)attributed);CGFloat ascent=0,descent=0,leading=0;double width=CTLineGetTypographicBounds(line,&ascent,&descent,&leading);
        out.width=(int)ceil(width)+2;out.height=(int)ceil(ascent+descent)+2;out.advance=(float)width;
        if(out.width>8192||out.height>256){CFRelease(line);return out;}
        size_t count=(size_t)out.width*out.height;unsigned char *rgba=calloc(count,4);if(!rgba){CFRelease(line);return out;}
        CGColorSpaceRef space=CGColorSpaceCreateDeviceRGB();CGContextRef context=CGBitmapContextCreate(rgba,out.width,out.height,8,(size_t)out.width*4,space,kCGImageAlphaPremultipliedLast);
        CGColorSpaceRelease(space);
        if(context){CGContextSetTextPosition(context,1,descent+1);CTLineDraw(line,context);out.pixels=malloc(count);if(out.pixels)for(int y=0;y<out.height;y++)for(int x=0;x<out.width;x++)out.pixels[(size_t)y*out.width+x]=rgba[((size_t)y*out.width+x)*4+3];CGContextRelease(context);}
        free(rgba);CFRelease(line);return out;
    }
}
void zgpui_font_shutdown(void){[font_family release];font_family=nil;}
