import sys, os
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from PIL import Image, ImageDraw, ImageFont
import schedule as S

SCALE = 2
W, H = 1280, 720
WS, HS = W * SCALE, H * SCALE
FONT = "/System/Library/Fonts/Menlo.ttc"
F  = lambda px: ImageFont.truetype(FONT, px * SCALE)
FB = lambda px: ImageFont.truetype(FONT, px * SCALE, index=1)

def clamp(x,a=0.0,b=1.0): return max(a,min(b,x))
def P(t,s,d): return clamp((t-s)/d) if d>0 else (1.0 if t>=s else 0.0)
def eo(p): return 1-(1-p)**3
def eb(p):
    c1,c3=1.70158,2.70158
    return 1+c3*(p-1)**3+c1*(p-1)**2
def mix(a,b,t):
    t=clamp(t); return tuple(round(a[i]+(b[i]-a[i])*t) for i in range(3))

BG,PANEL,LINE,TEXT,DIM,FAINT = S.BG,S.PANEL,S.LINE,S.TEXT,S.DIM,S.FAINT
ACCENT,GREEN,YELLOW,RED,BLUE = S.ACCENT,S.GREEN,S.YELLOW,S.RED,S.BLUE

def make_bg():
    img=Image.new("RGB",(WS,HS),BG)
    d=ImageDraw.Draw(img); step=34*SCALE
    for y in range(0,HS,step):
        for x in range(0,WS,step):
            d.ellipse([x,y,x+1*SCALE,y+1*SCALE],fill=(15,17,21))
    return img
BG_IMG=make_bg()

# A box drawn in box-drawing characters, with a title set into the top edge,
# exactly like the enowx chrome: ╭─ title ─────╮ ... ╰─ keys ─╯
def box(d, x, y, w, h, title=None, keys=None, col=LINE, fill=None):
    fx = F(14); cw = d.textlength("M", font=fx); ch = 24*SCALE
    def put(cx, cy, s, c=col, f=fx): d.text((cx, cy), s, font=f, fill=c)
    cols = int(w/cw); rows = int(h/ch)
    if fill: d.rectangle([x, y, x+w, y+h], fill=fill)
    # top edge with title
    top = "╭─ "
    if title: top += title + " "
    top += "─"*max(0, cols-len(top)-1) + "╮"
    put(x, y, top, col)
    if title:
        put(x + 3*cw, y, title+" ", TEXT)
    # sides
    for r in range(1, rows-1):
        put(x, y+r*ch, "│", col); put(x+(cols-1)*cw, y+r*ch, "│", col)
    # bottom edge with keys
    bot = "╰─"
    if keys: bot += " " + keys + " "
    bot += "─"*max(0, cols-len(bot)-1) + "╯"
    put(x, y+(rows-1)*ch, bot, col)
    if keys:
        put(x + 3*cw, y+(rows-1)*ch, keys, FAINT)
    return cw, ch, cols, rows

def render(i):
    t=i/S.FPS
    img=BG_IMG.copy(); d=ImageDraw.Draw(img)
    fx=F(14); cw=d.textlength("M",font=fx); ch=24*SCALE
    M=24*SCALE
    side_w = int(42*cw)
    gap = int(1*cw)
    chat_x, chat_y = M, M
    chat_w = WS - 2*M - side_w - gap
    chat_h = HS - 2*M - ch - 6*SCALE

    # --- chat box ---
    box(d, chat_x, chat_y, chat_w, chat_h, title="my-api · session")
    ix = chat_x + 2*cw
    iy = chat_y + int(1.4*ch)

    # typed prompt
    typed=""
    if t>=S.TYPE_START:
        n=int((t-S.TYPE_START)*S.TYPE_CPS); typed=S.PROMPT[:min(n,len(S.PROMPT))]
    d.text((ix,iy),"› ",font=FB(14),fill=ACCENT)
    d.text((ix+2*cw,iy),typed,font=F(14),fill=TEXT)
    if t<S.SEND and (int(t*2)%2==0 or t<S.TYPE_END):
        cx=ix+2*cw+d.textlength(typed,font=F(14)); d.rectangle([cx+2*SCALE,iy,cx+int(cw)-2*SCALE,iy+18*SCALE],fill=ACCENT)
    y=iy+int(1.8*ch)

    if t>=S.SEND:
        d.text((ix,y),"Orchestration",font=F(12),fill=FAINT); y+=int(1.3*ch)
        for (agent,task,model,start,done) in S.AGENTS:
            app=P(t,start,0.4)
            if app<=0: continue
            dx=(1-eb(app))*16*SCALE; fade=eo(app)
            running=t<done; col=YELLOW if running else GREEN
            marker="◆" if running else "✓"
            rx=ix+dx
            d.text((rx,y),marker,font=FB(14),fill=mix(BG,col,fade))
            d.text((rx+2*cw,y),agent,font=FB(14),fill=mix(BG,TEXT,fade))
            d.text((rx+2*cw+d.textlength(agent,font=FB(14))+cw,y),model,font=F(12),fill=mix(BG,BLUE,fade*0.9))
            d.text((rx+2*cw,y+int(0.8*ch)),task,font=F(12),fill=mix(BG,DIM,fade))
            y+=int(1.9*ch)

    if t>=S.REPORT_AT:
        rp=eb(P(t,S.REPORT_AT,0.5)); fade=eo(P(t,S.REPORT_AT,0.5))
        off=(1-rp)*14*SCALE
        bx,by=ix,y+off+4*SCALE
        d.text((bx,by),"✓ Done",font=FB(14),fill=mix(BG,GREEN,fade))
        for k,ln in enumerate(["landing page + pricing built, API wired, reviewed",
                               "4 files changed · tests: pass · reviewed: yes"]):
            d.text((bx,by+int((0.9+k*0.8)*ch)),ln,font=F(12),fill=mix(BG,DIM if k else TEXT,fade))

    # --- side column: SESSION card over a tabs edge (like the real detail card) ---
    sx=chat_x+chat_w+gap; sw=side_w
    sess_h=int(10*ch)
    box(d,sx,chat_y,sw,sess_h,title="SESSION")
    px=sx+2*cw; py=chat_y+int(1.4*ch)
    # context bar
    ctx=eo(P(t,S.SEND,3.0))*0.34
    barw=int(36*cw)
    d.rectangle([px,py,px+barw,py+6*SCALE],fill=LINE)
    d.rectangle([px,py,px+int(barw*ctx),py+6*SCALE],fill=mix(LINE,ACCENT,0.8))
    d.text((px,py+int(0.6*ch)),f"context  {int(ctx*100)}% of 200k",font=F(11),fill=DIM)
    rows=[("tokens in", f"{int(eo(P(t,S.SEND,4))*4120):,}"),
          ("tokens out", f"{int(eo(P(t,S.SEND,5))*1870):,}"),
          ("cost", f"${eo(P(t,S.SEND,5))*0.012:.3f}"),
          ("tool calls", str(int(eo(P(t,S.SEND,4))*9)))]
    ry=py+int(1.8*ch)
    for lab,val in rows:
        d.text((px,ry),lab,font=F(12),fill=DIM)
        d.text((px+barw-d.textlength(val,font=F(12)),ry),val,font=F(12),fill=TEXT)
        ry+=int(ch)
    # detail card with tabs on its edge
    det_y=chat_y+sess_h+int(0.3*ch)
    det_h=chat_h-sess_h-int(0.3*ch)
    box(d,sx,det_y,sw,det_h,title="Agents · Tools · Skills · Log")
    # mini agent list inside
    ay=det_y+int(1.4*ch)
    for (agent,task,model,start,done) in S.AGENTS:
        if t<start: continue
        running=t<done
        d.text((sx+2*cw,ay),("◆ " if running else "✓ ")+agent,font=F(12),
               fill=(YELLOW if running else GREEN) if t>=start else FAINT)
        ay+=int(ch)

    # --- status bar ---
    stx=M; sty=HS-M-ch
    d.text((stx,sty),"◆",font=FB(14),fill=YELLOW if t<S.REPORT_AT and t>=S.SEND else (GREEN if t>=S.REPORT_AT else DIM))
    d.text((stx+int(1.6*cw),sty),"Orchestrator",font=FB(14),fill=ACCENT)
    d.text((stx+int(1.6*cw)+d.textlength("Orchestrator",font=FB(14)),sty)," · glm-5.3",font=F(13),fill=DIM)
    keys="↑↓ move · Enter send · Ctrl+C quit"
    d.text((WS-M-d.textlength(keys,font=F(12)),sty+2*SCALE),keys,font=F(12),fill=FAINT)

    # brand mark bottom-left of chat edge area handled by title; add enowx wordmark subtly top
    d.text((chat_x+2*cw, chat_y-0), "", font=F(1))

    return img.resize((W,H),Image.LANCZOS)

if __name__=="__main__":
    sheet=Image.new("RGB",(W//2*4,H//2*2),(0,0,0))
    for k in range(8):
        fr=int(S.TOTAL*(k+0.5)/8)
        sheet.paste(render(fr).resize((W//2,H//2),Image.LANCZOS),((k%4)*(W//2),(k//4)*(H//2)))
    sheet.save(os.path.join(os.path.dirname(__file__),"contact.png")); print("contact written")
