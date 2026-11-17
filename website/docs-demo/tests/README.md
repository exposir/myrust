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
website/docs-demo/tests/run.sh http://localhost:8080
```

`tests/run.sh` takes the base URL as its argument and defaults to
`https://myrust.pages.dev`. It splices the URL into a temporary copy of
`functional.mjs` because `ego-browser nodejs` does not inherit the shell
environment — `DEMO_BASE=… ego-browser nodejs` silently reads nothing and
runs against production.

`ego-browser` runs on the Mac; `run.sh` SSHes there when it is not running
on macOS itself. OrbStack forwards container ports to the Mac's localhost,
so a server in the container is reachable at the same port from the Mac.

It exits after printing one line per step and `SUMMARY passed=N failed=M`.

After a deploy, run `tests/run.sh` with no argument — that is the only check
that the shipped wasm still behaves like the one tested locally.

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
