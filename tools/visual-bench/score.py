#!/usr/bin/env python3
"""Score demo screenshots against MyGo's frames, whole page and per tile.

Reference frames are captured from mygo.run (see README.md); they are not
stored in this repo. A demo's score comes from the fraction of pixels that
differ by more than 30 of 765 (summed RGB), compared against the closest of
MyGo's own frames, so a frame captured at another moment of an animation is
not counted as a defect.

    score.py MINE_DIR MYGO_DIR [--tile 32]

Both directories hold `<slug>.png`; the reference side may also carry
`<slug>-b.png` / `<slug>-c.png`, frames taken a moment apart.
"""
import argparse
import os
import sys

import numpy as np
from PIL import Image


def grade(equal_pct):
    for score, cut in ((9, 99.5), (8, 99.0), (7, 98.0), (6, 95.0),
                       (5, 90.0), (4, 80.0), (3, 60.0), (2, 30.0)):
        if equal_pct >= cut:
            return score
    return 1


def load(path):
    return np.array(Image.open(path).convert("RGB")).astype(np.int16)


def reference_frames(mygo_dir, slug, mine):
    """Frames that match `mine`'s shape, as boolean bad-pixel masks."""
    frames = []
    for suffix in ("", "-b", "-c"):
        path = f"{mygo_dir}/{slug}{suffix}.png"
        if not os.path.exists(path):
            continue
        ref = load(path)
        if ref.shape != mine.shape:
            continue
        frames.append(np.abs(ref - mine).sum(axis=2) > 30)
    return frames


def main():
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("mine_dir")
    ap.add_argument("mygo_dir")
    ap.add_argument("--tile", type=int, default=0,
                    help="also report NxN tiles below 9 (0: whole page only)")
    args = ap.parse_args()

    rows = []
    for name in sorted(os.listdir(args.mine_dir)):
        if not name.endswith(".png"):
            continue
        slug = name[:-4]
        mine = load(f"{args.mine_dir}/{name}")
        frames = reference_frames(args.mygo_dir, slug, mine)
        if not frames:
            rows.append((slug, None, None))
            continue
        # Whole-page score uses the closest single frame.
        bad = min(float(f.mean()) for f in frames)
        rows.append((slug, grade((1.0 - bad) * 100.0), (1.0 - bad) * 100.0))
        if args.tile:
            # A tile is bad only when every frame disagrees there, so a
            # mid-animation capture does not report phantom defects.
            all_bad = np.logical_and.reduce(frames)
            h, w = all_bad.shape
            for y in range(0, h, args.tile):
                for x in range(0, w, args.tile):
                    block = all_bad[y:y + args.tile, x:x + args.tile]
                    if block.size and block.any():
                        eq = 100.0 * (1.0 - block.mean())
                        s = grade(eq)
                        if s < 9:
                            rows.append((f"{slug}@{x},{y}", s, eq))
    for slug, score, equal in rows:
        if score is None:
            print(f"{slug:34s}  ?  NO-MATCHING-FRAME")
        else:
            print(f"{slug:34s}  {score}  {equal:7.3f}%")
    scored = [r for r in rows if r[1] is not None]
    nines = sum(1 for r in scored if r[1] == 9)
    print("---- entries:", len(scored), " nines:", nines,
          " mean score: %.2f" % (sum(r[1] for r in scored) / max(len(scored), 1)))
    return 0


if __name__ == "__main__":
    sys.exit(main())
