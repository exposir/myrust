# docs-demo tests

`functional.mjs` drives every docs demo the way a reader would — clicking
labels, typing, pressing keys, dropping files — and asserts the wasm's own
state afterwards. It is not part of CI: it needs a browser and a live
server, and ego-browser only runs on macOS.

## Running

Serve `website/` (the demo loads `./demo.wasm` relatively), then point the
harness at it:

```sh
cd website && python3 -m http.server 8080     # in the container
DEMO_BASE=http://localhost:8080 ego-browser nodejs < website/docs-demo/tests/functional.mjs
```

`ego-browser` runs on the Mac. OrbStack forwards container ports to the
Mac's localhost, so `http://localhost:8080` works from both sides. Without
`DEMO_BASE` the harness runs against `https://myrust.pages.dev` — use that
to check a deployment (`DEMO_BASE=https://…` for preview deployments).

It exits after printing one line per step and `SUMMARY passed=N failed=M`.

After a deploy, run it once against production (no `DEMO_BASE`) — that is
the only check that the shipped wasm still behaves like the one tested
locally.

## How it works

The demos draw to a canvas, so there is nothing in the DOM to click. The
harness reads the draw-command trace and computes a point from the label's
own transform:

- `page.evaluate` runs `findPoint` inside the page; a `Text` command carries
  its raw position, font size and the CTM it was drawn under, so the click
  point is `(ctm · p) / dpr` in CSS pixels.
- `xy` / `click2` targets are absolute page pixels at the docs preview
  width (670); `bar` targets find a 280×N track and click a fraction along
  it.
- Expectations are the wasm probe counter (`e.probe`), a visible string
  (`e.text`), or `e.unchanged` for interactions that should leave state
  alone.

Waits are 250 ms after a click — the render loop runs every 80–100 ms, and
shorter waits read a stale frame.

## Test hooks

`demo.js` exposes these on `window` for the harness. They are debug hooks,
**not a public API** — they exist so the canvas can be inspected without
screen-scraping pixels:

| hook | what it returns |
|------|-----------------|
| `__demoRender()` | redraws the current demo |
| `__demoCommands()` | the last frame's draw commands (needs `__traceCommands = true`) |
| `__demoProbe()` | the demo's interaction counter (`demo_probe` in the wasm) |
| `__demoSelected()` | the demo's selection index (`demo_selected`) |
| `__demoTick()` | the demo's tick counter (`demo_tick`) |
| `canvas[data-ready]` | `"true"` once the wasm has rendered its first frame |

If a demo changes how it reports state, update the wasm export and the
harness together.

## Coverage gaps

- `diff-viewer` cannot be edited yet: its status line promises editing, but
  `myrust-ui/src/diff.rs` has no text input. The harness only checks the
  static render.
- `icon` is a static showcase with nothing to assert.
