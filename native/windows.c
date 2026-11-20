#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#include <windowsx.h>
#include <GL/gl.h>
#include <stdlib.h>
#include <string.h>
#include <stdio.h>
#include "zgpui.h"
struct ZgpuiWindow{HWND window;HDC dc;HGLRC context;int width,height,closed;ZgpuiEvent pending;};
static LRESULT CALLBACK procedure(HWND h,UINT message,WPARAM wp,LPARAM lp){
    ZgpuiWindow *w=(ZgpuiWindow*)GetWindowLongPtrW(h,GWLP_USERDATA);
    if(message==WM_NCCREATE){w=(ZgpuiWindow*)((CREATESTRUCTW*)lp)->lpCreateParams;SetWindowLongPtrW(h,GWLP_USERDATA,(LONG_PTR)w);}
    if(w){
        if(message==WM_CLOSE){w->closed=1;return 0;}
        if(message==WM_SIZE){w->width=LOWORD(lp);w->height=HIWORD(lp);w->pending.kind=6;return 0;}
        if(message==WM_PAINT){PAINTSTRUCT p;BeginPaint(h,&p);EndPaint(h,&p);w->pending.kind=6;return 0;}
        if(message==WM_MOUSEMOVE||message==WM_LBUTTONDOWN||message==WM_LBUTTONUP){w->pending.kind=message==WM_MOUSEMOVE?1:message==WM_LBUTTONDOWN?2:3;w->pending.x=GET_X_LPARAM(lp);w->pending.y=GET_Y_LPARAM(lp);if(message==WM_LBUTTONDOWN)SetCapture(h);if(message==WM_LBUTTONUP)ReleaseCapture();return 0;}
        if(message==WM_MOUSEWHEEL){w->pending.kind=8;w->pending.key=GET_WHEEL_DELTA_WPARAM(wp)>0?-1:1;return 0;}
        if(message==WM_KEYDOWN){
            int ctrl=(GetKeyState(VK_CONTROL)&0x8000)!=0;
            memset(&w->pending,0,sizeof(w->pending));w->pending.kind=4;w->pending.modifiers=(ctrl?1:0)|((GetKeyState(VK_SHIFT)&0x8000)?2:0);
            switch(wp){case VK_LEFT:w->pending.key=1001;break;case VK_RIGHT:w->pending.key=1002;break;case VK_UP:w->pending.key=1003;break;case VK_DOWN:w->pending.key=1004;break;case VK_HOME:w->pending.key=1005;break;case VK_END:w->pending.key=1006;break;case VK_DELETE:w->pending.key=127;break;default:w->pending.key=(int)wp;}
            if(ctrl&&wp=='V'&&OpenClipboard(h)){HANDLE data=GetClipboardData(CF_UNICODETEXT);if(data){const wchar_t *text=GlobalLock(data);if(text){WideCharToMultiByte(CP_UTF8,0,text,-1,w->pending.text,sizeof(w->pending.text)-1,NULL,NULL);w->pending.text[sizeof(w->pending.text)-1]=0;w->pending.kind=5;GlobalUnlock(data);}}CloseClipboard();}
            return 0;
        }
        if(message==WM_CHAR&&!(GetKeyState(VK_CONTROL)&0x8000)&&wp>=32){
            static wchar_t high=0;wchar_t chars[3]={0};if(wp>=0xD800&&wp<=0xDBFF){high=(wchar_t)wp;return 0;}if(high){chars[0]=high;chars[1]=(wchar_t)wp;high=0;}else chars[0]=(wchar_t)wp;
            memset(&w->pending,0,sizeof(w->pending));w->pending.kind=5;WideCharToMultiByte(CP_UTF8,0,chars,-1,w->pending.text,sizeof(w->pending.text)-1,NULL,NULL);return 0;
        }
    }return DefWindowProcW(h,message,wp,lp);
}
ZgpuiWindow *zgpui_open(const char *title,int width,int height,char *error,size_t capacity){
    SetProcessDPIAware();ZgpuiWindow *w=calloc(1,sizeof(*w));if(!w)return NULL;
    HINSTANCE instance=GetModuleHandleW(NULL);WNDCLASSW cls={0};cls.style=CS_OWNDC;cls.lpfnWndProc=procedure;cls.hInstance=instance;cls.hIcon=LoadIconW(instance,MAKEINTRESOURCEW(1));if(!cls.hIcon)cls.hIcon=LoadIconW(NULL,IDI_APPLICATION);cls.hCursor=LoadCursorW(NULL,IDC_ARROW);cls.lpszClassName=L"ProjectZNativeUI";RegisterClassW(&cls);
    wchar_t wide[256];MultiByteToWideChar(CP_UTF8,0,title,-1,wide,256);RECT rect={0,0,width,height};AdjustWindowRect(&rect,WS_OVERLAPPEDWINDOW,FALSE);
    w->window=CreateWindowExW(0,cls.lpszClassName,wide,WS_OVERLAPPEDWINDOW,CW_USEDEFAULT,CW_USEDEFAULT,rect.right-rect.left,rect.bottom-rect.top,NULL,NULL,instance,w);
    if(!w->window){snprintf(error,capacity,"Win32 window creation failed (%lu).",GetLastError());free(w);return NULL;}
    w->dc=GetDC(w->window);PIXELFORMATDESCRIPTOR format={0};format.nSize=sizeof(format);format.nVersion=1;format.dwFlags=PFD_DRAW_TO_WINDOW|PFD_SUPPORT_OPENGL|PFD_DOUBLEBUFFER;format.iPixelType=PFD_TYPE_RGBA;format.cColorBits=24;format.cAlphaBits=8;
    int selected=ChoosePixelFormat(w->dc,&format);
    if(!selected||!SetPixelFormat(w->dc,selected,&format)||(w->context=wglCreateContext(w->dc))==NULL||!wglMakeCurrent(w->dc,w->context)){snprintf(error,capacity,"OpenGL context creation failed.");zgpui_close(w);return NULL;}
    w->width=width;w->height=height;ShowWindow(w->window,SW_SHOW);UpdateWindow(w->window);return w;
}
void zgpui_close(ZgpuiWindow *w){if(!w)return;if(w->context){wglMakeCurrent(w->dc,w->context);zgpui_gpu_destroy();wglMakeCurrent(NULL,NULL);wglDeleteContext(w->context);}if(w->dc)ReleaseDC(w->window,w->dc);if(w->window)DestroyWindow(w->window);free(w);}
void zgpui_size(ZgpuiWindow *w,int *width,int *height){*width=w->width;*height=w->height;}
void zgpui_swap(ZgpuiWindow *w){SwapBuffers(w->dc);}
int zgpui_event(ZgpuiWindow *w,ZgpuiEvent *out,int timeout){
    memset(out,0,sizeof(*out));if(w->closed){out->kind=7;return 1;}if(w->pending.kind){*out=w->pending;memset(&w->pending,0,sizeof(w->pending));return 1;}
    MsgWaitForMultipleObjects(0,NULL,FALSE,timeout,QS_ALLINPUT);MSG message;
    while(PeekMessageW(&message,NULL,0,0,PM_REMOVE)){if(message.message==WM_QUIT){out->kind=7;return 1;}TranslateMessage(&message);DispatchMessageW(&message);if(w->closed){out->kind=7;return 1;}if(w->pending.kind){*out=w->pending;memset(&w->pending,0,sizeof(w->pending));return 1;}}
    return 0;
}
void zgpui_clipboard(ZgpuiWindow *w,const char *text){
    int count=MultiByteToWideChar(CP_UTF8,0,text,-1,NULL,0);HGLOBAL data=GlobalAlloc(GMEM_MOVEABLE,(SIZE_T)count*sizeof(wchar_t));if(!data)return;
    wchar_t *p=GlobalLock(data);if(!p){GlobalFree(data);return;}MultiByteToWideChar(CP_UTF8,0,text,-1,p,count);GlobalUnlock(data);
    if(OpenClipboard(w->window)){EmptyClipboard();if(!SetClipboardData(CF_UNICODETEXT,data))GlobalFree(data);CloseClipboard();}else GlobalFree(data);
}
ZgpuiBitmap zgpui_rasterize(const char *text,int size,int bold){
    ZgpuiBitmap out={0};int count=MultiByteToWideChar(CP_UTF8,0,text,-1,NULL,0);if(count<=0||count>8192)return out;wchar_t *wide=calloc(count,sizeof(wchar_t));if(!wide)return out;MultiByteToWideChar(CP_UTF8,0,text,-1,wide,count);
    HDC dc=CreateCompatibleDC(NULL);HFONT font=CreateFontW(-size,0,0,0,bold?FW_SEMIBOLD:FW_NORMAL,FALSE,FALSE,FALSE,DEFAULT_CHARSET,OUT_DEFAULT_PRECIS,CLIP_DEFAULT_PRECIS,ANTIALIASED_QUALITY,DEFAULT_PITCH,L"Segoe UI");HGDIOBJ previous=SelectObject(dc,font);
    SIZE extent={0};GetTextExtentPoint32W(dc,wide,count-1,&extent);out.width=extent.cx+2;out.height=extent.cy+2;out.advance=(float)extent.cx;
    if(out.width>8192||out.height>256)goto done;
    BITMAPINFO info={0};info.bmiHeader.biSize=sizeof(BITMAPINFOHEADER);info.bmiHeader.biWidth=out.width;info.bmiHeader.biHeight=-out.height;info.bmiHeader.biPlanes=1;info.bmiHeader.biBitCount=32;info.bmiHeader.biCompression=BI_RGB;
    unsigned char *pixels=NULL;HBITMAP bitmap=CreateDIBSection(dc,&info,DIB_RGB_COLORS,(void**)&pixels,NULL,0);if(!bitmap)goto done;
    HGDIOBJ old_bitmap=SelectObject(dc,bitmap);memset(pixels,0,(size_t)out.width*out.height*4);SetBkColor(dc,RGB(0,0,0));SetTextColor(dc,RGB(255,255,255));SetBkMode(dc,OPAQUE);TextOutW(dc,1,1,wide,count-1);GdiFlush();
    out.pixels=malloc((size_t)out.width*out.height);if(out.pixels)for(int i=0;i<out.width*out.height;i++)out.pixels[i]=pixels[i*4];
    SelectObject(dc,old_bitmap);DeleteObject(bitmap);
done:SelectObject(dc,previous);DeleteObject(font);DeleteDC(dc);free(wide);return out;
}
void zgpui_font_shutdown(void){}
