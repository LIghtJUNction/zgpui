#!/usr/bin/env python3
"""Inspect actual X11 identity, size hints and client geometry on a private display."""
import argparse,json,os,pathlib,re,subprocess,tempfile,time

parser=argparse.ArgumentParser()
parser.add_argument("--binary",default="target/release/examples/window_probe")
parser.add_argument("--output",default="_validation/window-properties.json")
args=parser.parse_args()
if not os.environ.get("DISPLAY") or os.environ["DISPLAY"]==":0":
    raise SystemExit("Run on a private X server, for example xvfb-run -a")
binary=str(pathlib.Path(args.binary).resolve())
def xd(*arguments):
    return subprocess.check_output(["xdotool",*map(str,arguments)],text=True).strip()
results=[]
for title,width,height,instance,cls in [
    ("ztype",660,264,"ztype","Ztype"),
    ("zcall-gui · window check",1080,740,"zcall-gui","Zcall-gui"),
    ("Other.App - identity check",512,320,"other.app","Other.app"),
]:
    proc=subprocess.Popen([binary,title,str(width),str(height)],stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True)
    try:
        window=None
        for _ in range(100):
            if proc.poll()is not None:raise AssertionError(proc.stderr.read())
            found=subprocess.run(["xdotool","search","--onlyvisible","--class","^"+re.escape(cls)+"$"],capture_output=True,text=True)
            if found.returncode==0:
                window=found.stdout.splitlines()[-1];break
            time.sleep(.05)
        assert window,f"Native probe window did not appear: {title}; {found.stderr}"
        properties=subprocess.check_output(["xprop","-id",window,"WM_CLASS","WM_NORMAL_HINTS","_NET_WM_NAME"],text=True)
        geometry=dict(line.split("=",1)for line in xd("getwindowgeometry","--shell",window).splitlines() if "="in line)
        assert xd("getwindowname",window)==title,properties
        assert f'"{instance}", "{cls}"' in properties,properties
        assert f"minimum size: {width} by {height}"in properties,properties
        assert (int(geometry["WIDTH"]),int(geometry["HEIGHT"]))==(width,height),geometry
        results.append({"title":title,"instance":instance,"class":cls,"requested":[width,height],
                        "actual":[int(geometry["WIDTH"]),int(geometry["HEIGHT"])],
                        "xprop":properties.strip()})
    finally:
        if proc.poll()is None:proc.terminate()
        try:proc.wait(timeout=5)
        except subprocess.TimeoutExpired:proc.kill();proc.wait()
output=pathlib.Path(args.output);output.parent.mkdir(parents=True,exist_ok=True)
report={"scope":"real X11 properties and native window geometry on a private X server","windows":results}
output.write_text(json.dumps(report,ensure_ascii=False,indent=2)+"\n")
print(json.dumps(report,ensure_ascii=False,indent=2))
