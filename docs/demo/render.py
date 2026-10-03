import sys, os
sys.path.insert(0, os.path.dirname(__file__))
from PIL import Image, ImageDraw, ImageFont
import schedule as S

SCALE = 2                      # draw at 2x then downscale for anti-aliasing
W, H = 1280, 720
WS, HS = W * SCALE, H * SCALE
FONT = "/System/Library/Fonts/Menlo.ttc"
F  = lambda px: ImageFont.truetype(FONT, px * SCALE)
FB = lambda px: ImageFont.truetype(FONT, px * SCALE, index=1)  # bold

def clamp(x, a=0.0, b=1.0): return max(a, min(b, x))
def P(t, start, dur): return clamp((t - start) / dur) if dur > 0 else (1.0 if t >= start else 0.0)
def eo(p):  return 1 - (1 - p) ** 3                    # ease-out
def eb(p):                                             # ease-out-back (pop)
    c1, c3 = 1.70158, 2.70158
    return 1 + c3 * (p - 1) ** 3 + c1 * (p - 1) ** 2
def mix(a, b, t):
    t = clamp(t)
    return tuple(round(a[i] + (b[i] - a[i]) * t) for i in range(3))

# Background built once (dark fill + a faint dot grid), then copied each frame.
def make_bg():
    img = Image.new("RGB", (WS, HS), S.BG)
    d = ImageDraw.Draw(img)
    step = 34 * SCALE
    for y in range(0, HS, step):
        for x in range(0, WS, step):
            d.ellipse([x, y, x + 1 * SCALE, y + 1 * SCALE], fill=(16, 18, 22))
    return img
BG_IMG = make_bg()

def rrect(d, box, r, **kw):
    d.rounded_rectangle(box, radius=r * SCALE, **kw)

def render(i):
    t = i / S.FPS
    img = BG_IMG.copy()
    d = ImageDraw.Draw(img)
    M = 60 * SCALE                       # outer margin

    # Window chrome: a titled terminal panel.
    rrect(d, [M, 44 * SCALE, WS - M, HS - 44 * SCALE], 12, fill=S.PANEL, outline=S.LINE, width=1 * SCALE)
    d.text((M + 22 * SCALE, 58 * SCALE), "enx", font=FB(15), fill=S.RED)
    d.text((M + 62 * SCALE, 60 * SCALE), "· my-api · session", font=F(12), fill=S.FAINT)
    d.text((WS - M - 150 * SCALE, 60 * SCALE), "glm-5.3", font=F(12), fill=S.DIM)

    x0 = M + 30 * SCALE
    y  = 112 * SCALE
    line_h = 30 * SCALE

    # The prompt line, typed out.
    typed = ""
    if t >= S.TYPE_START:
        n = int((t - S.TYPE_START) * S.TYPE_CPS)
        typed = S.PROMPT[:min(n, len(S.PROMPT))]
    d.text((x0, y), "› ", font=FB(16), fill=S.ACCENT)
    d.text((x0 + 26 * SCALE, y), typed, font=F(16), fill=S.TEXT)
    # Blinking caret while typing, before send.
    if t < S.SEND and (int(t * 2) % 2 == 0 or t < S.TYPE_END):
        cx = x0 + 26 * SCALE + d.textlength(typed, font=F(16))
        d.rectangle([cx + 2 * SCALE, y, cx + 10 * SCALE, y + 22 * SCALE], fill=S.ACCENT)

    y += line_h + 14 * SCALE

    # After send: the delegation rows fan out, each with a state dot, agent,
    # its model, and its task. Positions are recomputed from time.
    if t >= S.SEND:
        d.text((x0, y), "ORCHESTRATION", font=F(11), fill=S.FAINT)
        y += 26 * SCALE
        for (agent, task, model, start, done) in S.AGENTS:
            app = P(t, start, 0.4)
            if app <= 0:
                continue
            # Slide + fade the row in with a slight pop.
            dx = (1 - eb(app)) * 24 * SCALE
            fade = eo(app)
            running = t < done
            col = S.YELLOW if running else S.GREEN
            marker = "◆" if running else "✓"
            rowx = x0 + dx
            d.text((rowx, y), marker, font=FB(14), fill=mix(S.BG, col, fade))
            d.text((rowx + 26 * SCALE, y), agent, font=FB(14), fill=mix(S.BG, S.TEXT, fade))
            d.text((rowx + 26 * SCALE + d.textlength(agent, font=FB(14)) + 12 * SCALE, y),
                   model, font=F(12), fill=mix(S.BG, S.BLUE, fade * 0.9))
            d.text((rowx + 26 * SCALE, y + 20 * SCALE), task, font=F(12), fill=mix(S.BG, S.DIM, fade))
            y += 50 * SCALE

    # The final report card pops in.
    if t >= S.REPORT_AT:
        rp = eb(P(t, S.REPORT_AT, 0.5))
        fade = eo(P(t, S.REPORT_AT, 0.5))
        cy = y + 6 * SCALE
        cw = WS - 2 * M - 60 * SCALE
        ch = 118 * SCALE
        off = (1 - rp) * 20 * SCALE
        box = [x0, cy + off, x0 + cw, cy + ch + off]
        rrect(d, box, 10, fill=mix(S.PANEL, (22, 26, 30), fade), outline=mix(S.BG, S.GREEN, fade * 0.6), width=1 * SCALE)
        bx, by = x0 + 22 * SCALE, cy + off + 18 * SCALE
        d.text((bx, by), "✓ Done", font=FB(15), fill=mix(S.BG, S.GREEN, fade))
        for k, ln in enumerate([
            "landing page + pricing built, API wired, reviewed",
            "4 files changed · tests: pass · reviewed: yes",
            "what changed and how it was checked, in the report",
        ]):
            d.text((bx, by + (26 + k * 22) * SCALE), ln, font=F(12), fill=mix(S.BG, S.DIM if k else S.TEXT, fade))

    # Caption at the bottom, steady.
    cap = "enx · a team of AI agents in your terminal"
    cfade = eo(P(t, 0.4, 0.8))
    d.text((M + 22 * SCALE, HS - 74 * SCALE), cap, font=F(12), fill=mix(S.BG, S.FAINT, cfade))

    return img.resize((W, H), Image.LANCZOS)

if __name__ == "__main__":
    # Contact sheet: 8 sample frames across the timeline, for a look before full render.
    sheet = Image.new("RGB", (W // 2 * 4, H // 2 * 2), (0, 0, 0))
    for k in range(8):
        fr = int(S.TOTAL * (k + 0.5) / 8)
        im = render(fr).resize((W // 2, H // 2), Image.LANCZOS)
        sheet.paste(im, ((k % 4) * (W // 2), (k // 4) * (H // 2)))
    sheet.save("./contact.png")
    print("contact sheet written")
