# Demo video, rendered from code

`docs/demo.gif` and `docs/demo.mp4` are generated here, not recorded. Every
frame and every sound is computed, then ffmpeg joins them. This is the
programmatic-motion-graphics approach the `motion-video` skill teaches.

Needs only Python with Pillow and NumPy, and ffmpeg.

```sh
cd docs/demo
python3 encode.py          # renders the 24s MP4 (no audio) to enowx-demo.mp4
python3 audio.py           # synthesises audio.wav from formulas
ffmpeg -i enowx-demo.mp4 -i audio.wav -c:v copy -af loudnorm=I=-16:TP=-1.5 \
  -c:a aac -shortest ../demo.mp4
# GIF for the README:
ffmpeg -i enowx-demo.mp4 -vf "fps=15,scale=900:-1:flags=lanczos,split[s0][s1];\
  [s0]palettegen=max_colors=64[p];[s1][p]paletteuse" ../demo.gif
```

`schedule.py` is the one timeline both the picture and the sound import, so
every click and whoosh lands on the exact frame its event is drawn.
