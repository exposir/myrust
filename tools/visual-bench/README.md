# Visual benchmark

Compares each demo page against MyGo's own demo of the same widget and
scores it 0–9 by pixel equality. This is how the port was checked against
the reference during development; it is not part of CI, because the
reference frames come from a live third-party site and would drift.

## Data

`demos.json` is the list of compared demos with the preview height each was
captured at, and the MyGo demo id where it differs from our slug. Keep it
and the docs previews in sync when adding a widget.

Reference frames are **not** stored in this repo. Capture them again when
you need a comparison; the same viewport on both sides matters more than
the exact pixel dimensions, as long as the two match.

## Capturing frames

Both sides use the same viewport: width 752, DPR 2, height from
`demos.json`. Three frames per demo, 1.4 s apart, so an animation is not
scored at the wrong moment. Run this on the Mac through ego-browser:

```js
// ego-browser nodejs < capture.mjs  — edit SITE and OUT first
const SITE = "https://mygo.run/docs-demo?demo=";  // or our /docs-demo/?demo=
const OUT = "/tmp/mygo-frames";
const demos = [
  ["button", 140], ["link", 160], /* … from demos.json (slug, h) */
];
const task = await taskSpace("visual-bench");
const page = task.page("p1");
for (const [slug, h] of demos) {
  await page.cdp("Emulation.setDeviceMetricsOverride",
    { width: 752, height: h, deviceScaleFactor: 2, mobile: false });
  await page.goto(SITE + slug, { waitUntil: "commit" }).catch(() => {});
  await page.waitForLoadState("networkidle", { timeout: 20000 }).catch(() => {});
  await page.waitForTimeout(600);
  await page.screenshot({ path: `${OUT}/${slug}.png` });
  await page.waitForTimeout(1400);
  await page.screenshot({ path: `${OUT}/${slug}-b.png` });
  await page.waitForTimeout(1400);
  await page.screenshot({ path: `${OUT}/${slug}-c.png` });
}
await task.finish({ keep: [] });
```

Our side only needs the single frame (`<slug>.png`) — the scorer picks the
closest reference frame.

## Scoring

```sh
python3 tools/visual-bench/score.py MINE_DIR MYGO_DIR          # whole page
python3 tools/visual-bench/score.py MINE_DIR MYGO_DIR --tile 32  # plus tiles
```

Pixels differing by more than 30 of 765 (summed RGB) count as different.
A tile is only reported bad when every reference frame disagrees there, so
mid-animation captures do not show phantom defects.

Bands: ≥99.5% equal → 9, ≥99% → 8, ≥98% → 7, ≥95% → 6, ≥90% → 5, ≥80% → 4,
≥60% → 3, ≥30% → 2, else 1.
