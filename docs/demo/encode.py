import sys, os, subprocess
sys.path.insert(0, '.')
import schedule as S
from render import render, W, H
from multiprocessing import Pool

OUT = "./enx-demo.mp4"

def frame_bytes(i):
    return render(i).tobytes()

if __name__ == "__main__":
    ff = subprocess.Popen([
        "ffmpeg", "-y", "-f", "rawvideo", "-pix_fmt", "rgb24",
        "-s", f"{W}x{H}", "-r", str(S.FPS), "-i", "-",
        "-c:v", "libx264", "-crf", "20", "-pix_fmt", "yuv420p",
        "-movflags", "+faststart", OUT,
    ], stdin=subprocess.PIPE)
    with Pool(8) as pool:
        for data in pool.imap(frame_bytes, range(S.TOTAL), chunksize=4):
            ff.stdin.write(data)
    ff.stdin.close()
    ff.wait()
    print("wrote", OUT)
