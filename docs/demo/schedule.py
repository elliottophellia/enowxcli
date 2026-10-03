# One source of truth for timing. Both render.py and audio.py import this,
# so every sound lands on the exact frame its event is drawn.
FPS = 30
SECONDS = 24
TOTAL = FPS * SECONDS

# Colours (the enx / enowx dark palette).
BG      = (8, 9, 10)
PANEL   = (16, 18, 21)
LINE    = (38, 41, 47)
TEXT    = (228, 230, 234)
DIM     = (138, 145, 158)
FAINT   = (90, 96, 107)
ACCENT  = (240, 241, 242)   # near-white, the enx accent
GREEN   = (120, 200, 150)
YELLOW  = (210, 180, 120)
RED     = (232, 80, 90)
BLUE    = (120, 170, 220)

# The typed prompt.
PROMPT = "build a landing page for my API, with pricing"
TYPE_START = 1.2
TYPE_CPS   = 24.0   # chars per second
TYPE_END   = TYPE_START + len(PROMPT) / TYPE_CPS
SEND = TYPE_END + 0.5

# The delegation fan-out: (agent, task, model, start, done).
AGENTS = [
    ("orchestrator", "reads the request, plans the work", "glm-5.3",        SEND + 0.3, SEND + 2.0),
    ("fe",           "builds the page and the pricing UI", "deepseek-flash", SEND + 2.2, SEND + 6.0),
    ("be",           "wires the pricing API",              "deepseek-flash", SEND + 2.6, SEND + 6.4),
    ("review",       "reviews both, sends back fixes",     "glm-5.3",        SEND + 6.6, SEND + 9.0),
]
REPORT_AT = SEND + 9.4
