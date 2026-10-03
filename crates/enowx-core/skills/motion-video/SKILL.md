---
name: motion-video
description: "Rendering a finished video file (an MP4) from code, when the user asks for a motion graphic, an explainer, an animated logo or a social clip as a video, not a web animation. Draw every frame with Pillow and NumPy, synthesise the sound from formulas, and pipe both to ffmpeg, using only what is already installed so the user installs and records nothing. Read before reaching for After Effects, a webview recording or a stock audio search."
---

# Rendering a video from code

A request for a video as a file ("make me a 30-second explainer", "an animated
logo as an MP4", "a motion graphic for this feature") does not need video
software, a browser to screen-record, or stock footage and sound. A video is a
run of still images played fast plus one audio track, and both can be computed.
Draw each frame, synthesise the sound, and let ffmpeg join them. The user
installs nothing and records nothing.

This is the default for a video **file**. For animation that lives in a web
page (scroll reveals, a component's states, a hero demo on a site), use the
`motion` family instead; this skill is for an exported `.mp4`.

## Decide the approach, then say it in one line

Check what is on the machine before choosing a tool, and never make the user
install or record anything:

- **Default: Python + ffmpeg.** `ffmpeg` on PATH, Python with Pillow (draw)
  and NumPy (numbers). This needs nothing else and is the path below. Confirm
  with `ffmpeg -version`, `python3 -c "import PIL, numpy"`.
- **Remotion** only when the project is already React/Node and the user wants
  the video defined there; it still needs Node and an install, so it is not the
  default.
- **Manim** for mathematical or diagram-heavy explainers, if it is installed.

If Pillow or NumPy is missing, install only that (`pip install pillow numpy`),
say so, and go on. Never answer a video request with "record it yourself",
"open After Effects", or "find a stock track".

## 1. The schedule is one source of truth

Before any drawing, write the timeline as numbers in one place that both the
picture and the sound import: fps (30), the total frames (`seconds * fps`), and
every scene's start and duration in seconds. Picture and sound read the same
schedule, so a sound lands on the exact frame its event is drawn. Never write
one timeline for the video and another for the audio.

## 2. Draw each frame as a function of time (render.py)

One function `render(i) -> Image` takes a frame number and returns one image:

1. `t = i / FPS`, so frame 450 at 30 fps is second 15.
2. Paste a background made once and copied into every frame (a dark fill, a
   vignette, a dot grid).
3. Pick the active scene from `t` and draw its elements: rounded rectangles,
   circles, polygons, text.
4. Draw the caption last, over everything.

Movement is recomputed, not stored. Nothing is a moving object; at each frame a
position is derived from the time:

- A progress `p` in `0..1` over a window: `p = clamp((t - start) / dur, 0, 1)`.
- Easing shapes it: ease-out (slows into the end), ease-out-back (overshoots
  then settles, the bouncy "pop"), a Bezier for a curved path (a packet arcing
  from one box to another).
- Fade by blending the element's colour toward the background colour at that
  spot, which is faster than a real alpha layer and leaves no dark halo.

Render each frame at 2x (for 1080p, draw at 4K) and downscale to the target
size, so edges are anti-aliased and smooth.

## 3. Synthesise the sound (audio.py)

Generate the audio from formulas as sample arrays (44100 per second); download
nothing. Use an external or open-source file only when the user supplies one or
asks for it by name, and say what you used.

- **Pop**: a sine whose pitch falls fast (about 950 to 260 Hz) with a decaying
  envelope.
- **Key click**: a very short (about 30 ms) burst of noise.
- **Ding**: a few harmonic sines, bell-like, decaying slowly.
- **Buzz / miss**: a filtered square wave around 100 Hz.
- **Whoosh**: white noise through a sweeping filter, volume up then down.
- **Boom**: a falling bass sine plus a little noise.
- **Music** (optional): a chord progression at a set tempo as a pad (slightly
  detuned left and right for stereo), bass, arpeggio, and a kick and hi-hat,
  faded in at the start and out at the end.

`audio.py` imports the schedule from `render.py`, so every on-screen event has
its sound at the same instant; one source, so they cannot drift. Place each
effect on the timeline, pan some left and right, sum with the music, soft-clip
(`tanh`) so it never distorts, normalise, and write `audio.wav`.

## 4. Render and encode

- Render frames in parallel across CPU cores (`multiprocessing.Pool`).
- Send each frame's raw pixels straight to ffmpeg over a pipe; do not write
  thousands of PNGs to disk.
- ffmpeg encodes H.264 (CRF 18 for high quality), audio to AAC 192k, and
  muxes them into the final `.mp4`.

A minute at 1080p renders in roughly a minute on a laptop.

## 5. Check it, because you cannot watch or hear it

- **Before the full render**, draw about 8 sample frames into one contact sheet
  and look at it. The real problems show up here: a dark halo where something
  faded (fade target did not match the background at that spot), a label hidden
  by a moving element, a wrong word in a caption.
- **After the render**: `ffprobe` confirms the duration, size, fps, and that
  both a video and an audio stream exist; measure loudness (EBU R128, aim near
  -16 LUFS for online); pull a few frames from the finished MP4 and look again.
- You cannot hear the result. Measure its level, and ask the user to play it
  once to confirm the sound, saying that is the one thing you cannot check.

## Why from code

- Precision: a sound and its frame are computed from the same number, so they
  are always in sync.
- Changeable: edit a caption or a duration, run again, and the video rebuilds
  in about a minute.
- No rights to clear: every asset (the logo, icons, sound, music) is made from
  scratch, so there is nothing to license and nothing for the user to find.

This is programmatic motion graphics; Remotion (React) and Manim (Python, used
by 3Blue1Brown) are the same idea in other tools.
