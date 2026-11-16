// Functional self-test for the docs demo pages.
//
// Runs under ego-browser's Node host on the Mac (the browser that owns the
// page lives there):
//
//   ego-browser nodejs < website/docs-demo/tests/functional.mjs
//   DEMO_BASE=https://<deployment>.myrust.pages.dev ego-browser nodejs < …
//
// Each scenario drives the canvas demo the way a reader would — clicking
// labels and icons, typing, pressing keys, dragging — and then asserts the
// wasm's own state. `__demoProbe`/`__demoSelected` are counters the demos
// set on interaction (`demo-wasm/src/lib.rs`), and the visible status line
// is checked as text.
//
// Coordinates come from the draw-command trace (`__traceCommands`): every
// Text command carries the transform it was drawn under, so a widget inside
// a translated row is still clickable by its label. `xy` steps are page
// pixels at the width the docs preview gives the frame (670).
//
// Not covered: `icon` and `drag-and-drop` have no state to assert (a static
// showcase and a drop zone), and `diff-viewer` cannot be edited yet — its
// status line promises it, but `myrust-ui/src/diff.rs` has no text input.
const BASE = (typeof process !== "undefined" && process.env.DEMO_BASE) ||
  "https://myrust.pages.dev";

const demos = [
  { slug: "button", h: 140, steps: [
    { c: { text: "Save" }, e: { probe: 1 } },
    { c: { text: "Cancel" }, e: { probe: 0 } },
    { c: { text: "Save" }, e: { probe: 2 } }] },
  { slug: "link", h: 160, steps: [
    { c: { text: "Privacy policy" }, e: { probe: 1 } },
    { c: { text: "the guide" }, e: { probe: 2 } },
    { c: { text: "Show details" }, e: { probe: 3 } },
    { c: { text: "Hide details" }, e: { probe: 3 } }] },
  { slug: "menu-button", h: 120, steps: [
    { c: { text: "Sort by" }, e: { unchanged: true } },
    { c: { text: "Date" }, e: { probe: 2 } },
    { c: { svg: "M5 11a1" }, e: { unchanged: true } },
    { c: { text: "Duplicate" }, e: { probe: 10 } }] },
  { slug: "context-menu", h: 180, steps: [
    { right: { text: "Groceries", dx: 60 } },
    { c: { text: "Rename" }, e: { probe: 10 } }] },
  { slug: "toggle", h: 150, steps: [
    { c: { text: "Inspector" }, e: { probe: 4 } },
    { c: { svg: "M7 5h6a3.5" }, e: { probe: 1 } },
    { c: { svg: "M19 4h-9" }, e: { probe: 2 } }] },
  { slug: "segmented-control", h: 200, steps: [
    { c: { text: "Grid" }, e: { probe: 2 } },
    { c: { text: "List" }, e: { probe: 1 } }] },
  { slug: "toolbar", h: 190, steps: [
    { c: { text: "Bold" }, e: { probe: 1 } },
    { c: { text: "Share" }, e: { probe: 3 } },
    { c: { text: "Next chapter" }, e: { probe: 99 } }] },
  { slug: "checkbox", h: 220, steps: [
    { c: { text: "Show hidden files" }, e: { probe: 2 } },
    { c: { text: "Notifications" }, e: { probe: 10 } },
    { c: { text: "Mail" }, e: { probe: 11 } }] },
  { slug: "switch", h: 220, steps: [
    { c: { text: "Wi-Fi", dx: 202 }, e: { text: "Wi-Fi off, Bluetooth off" } },
    { c: { text: "Wi-Fi", dx: 202 }, e: { text: "Wi-Fi on, Bluetooth off" } },
    { c: { text: "Bluetooth", dx: 202 }, e: { probe: 3 } }] },
  { slug: "radio", h: 160, steps: [
    { c: { text: "Medium" }, e: { probe: 2 } },
    { c: { text: "Center" }, e: { probe: 12 } }] },
  { slug: "select", h: 230, steps: [
    { c: { text: "Pro" }, e: { unchanged: true } },
    { c: { text: "Team" }, e: { probe: 3 } }] },

  { slug: "slider", h: 180, steps: [
    { c: { bar: [280, 8, 0, 0.75] }, e: { probeRange: [65, 85] } }] },
  { slug: "range-slider", h: 120, steps: [
    { c: { bar: [280, 8, 0, 0.08] }, e: { probeRange: [600, 1000] } }] },
  { slug: "stepper", h: 100, steps: [
    { c: { xy: [365, 40] }, e: { probe: 3 } },
    { c: { xy: [365, 60] }, e: { probe: 2 } }] },
  { slug: "rating", h: 110, steps: [{ c: { xy: [370, 40] }, e: { probe: 5 } }] },
  { slug: "text-input", h: 220, steps: [
    { c: { text: "Name" } }, { type: "Rust", e: { probe: 4 } },
    { c: { text: "Notes, on as many lines as you like" } },
    { type: "ab" }, { key: "Enter" }, { type: "cd", e: { probe: 2, text: "Enter in the name submits it. Notes: 5 characters" } }] },
  { slug: "text-input-primitives", h: 200, steps: [
    { c: { xy: [250, 80] } },
    { type: "abc", e: { text: "4 lines, 48 characters, version 0" } }] },
  { slug: "number-input", h: 160, steps: [
    { c: { text: "+", nth: "first" }, e: { text: "3 copies at 50% opacity" } },
    { c: { text: "+", nth: "first" }, e: { text: "4 copies at 50% opacity" } }] },
  { slug: "decimal-input", h: 160, steps: [
    { c: { text: "+", nth: "first" }, e: { text: "Price 1234.51, 0.00% off" } }] },
  { slug: "search-field", h: 200, steps: [
    { c: { text: "Search" } },
    { type: "an", e: { probe: 2, text: "3 of 12 fruits" } },
    { key: "Backspace", e: { probe: 1, text: "8 of 12 fruits" } }] },
  { slug: "combobox", h: 280, steps: [
    { c: { xy: [250, 42] }, e: { probe: 10 } },
    { key: "End" },
    { key: "Backspace" }, { key: "Backspace" }, { key: "Backspace" },
    { key: "Backspace" }, { key: "Backspace" }, { key: "Backspace" }, { key: "Backspace" },
    { type: "Cou", e: { text: "Courier New" } },
    { c: { text: "Courier New" }, e: { probe: 2 } }] },
  { slug: "autocomplete", h: 260, steps: [
    { c: { xy: [250, 42] } },
    { type: "Par", e: { probe: 10 } },
    { c: { text: "Paris" }, e: { probe: 6, text: "Shipping to Paris" } }] },
  { slug: "token-field", h: 260, steps: [
    { c: { xy: [300, 28] } }, { type: "rust" },
    { key: "Enter", e: { probe: 3, text: "3 tags: go, native, rust" } }] },
  { slug: "editable-text", h: 120, steps: [
    { c: { click2: [290, 40] }, e: { probe: 1 } }] },
  { slug: "find-bar", h: 200, steps: [
    { c: { text: "Done" }, e: { text: "Find…" } },
    { c: { text: "Find…" }, e: { text: "1 of 2" } }] },
  { slug: "code-editor", h: 320, steps: [
    { c: { xy: [200, 118] }, e: { probe: 5, text: "Ln 5, Col 1 · 11 lines · 1 TODO" } }] },
  { slug: "calendar", h: 340, steps: [
    { c: { text: "15" }, e: { probe: 15, text: "Due Thursday, October 15, 2026" } }] },
  { slug: "date-input", h: 320, steps: [
    { c: { text: "4/12/1990" }, e: { probe: 2 } },
    { c: { text: "15" }, e: { probe: 1 } }] },
  { slug: "time-input", h: 120, steps: [
    { c: { text: "07" } },
    { key: "ArrowUp", e: { probe: 510, text: "The alarm rings at 8:30 AM" } }] },
  { slug: "color-picker", h: 430, steps: [{ c: { xy: [300, 100] }, e: { probe: 1 } }] },

  { slug: "tabs", h: 160, steps: [
    { c: { text: "Appearance" }, e: { probe: 1, text: "Dark sidebar" } }] },
  { slug: "list", h: 290, steps: [
    { c: { text: "Beach.jpg" }, e: { probe: 3 } },
    { c: { text: "Delete 1" }, e: { probe: 99 } }] },
  { slug: "table", h: 310, steps: [
    { c: { text: "Name" }, e: { probe: 1 } },
    { c: { xy: [300, 97] }, e: { probe: 2 } }] },
  { slug: "tree", h: 280, steps: [
    { c: { text: "ui" }, e: { probe: 2 } },
    { c: { text: "docs" }, e: { probe: 2 } }] },
  { slug: "outline", h: 320, steps: [
    { c: { text: "In columns" }, e: { probe: 1 } },
    { c: { text: "button.go" }, e: { probe: 2 } }] },
  { slug: "grid-view", h: 310, steps: [
    { c: { text: "Canyon.jpg" }, e: { probe: 2 } }] },
  { slug: "scroll-view", h: 280, steps: [
    { c: { text: "Add a line" }, e: { probe: 1 } },
    { c: { text: "Top" }, e: { probe: -1 } },
    { c: { text: "End" }, e: { probe: -2 } }] },
  { slug: "grid", h: 260, steps: [
    { c: { text: "Add" }, e: { probe: 7 } },
    { c: { text: "Remove" }, e: { probe: 6 } }] },
  { slug: "split-view", h: 270, steps: [
    { c: { text: "app.go", nth: "first" }, e: { probe: 180 } }] },
  { slug: "collapsible", h: 220, steps: [
    { c: { text: "Advanced" }, e: { probe: 0 } },
    { c: { text: "Details" }, e: { probe: 1 } }] },
  { slug: "accordion", h: 300, steps: [
    { c: { text: "Privacy" }, e: { probe: 2 } }] },
  { slug: "form", h: 340, steps: [
    { c: { text: "Ada Lovelace" } }, { type: "!", e: { probe: 13 } }] },
  { slug: "sidebar", h: 280, steps: [
    { c: { text: "Drafts" }, e: { probe: 6, text: "No messages" } },
    { c: { text: "Inbox", nth: "first" }, e: { probe: 5 } },
    { c: { text: "Mark all as read" }, e: { probe: 99 } }] },
  { slug: "breadcrumbs", h: 180, steps: [
    { c: { text: "Users" }, e: { probe: 1 } },
    { c: { text: "Notes", nth: "first" }, e: { probe: 2 } }] },
  { slug: "back-and-forward-buttons", h: 240, steps: [
    { c: { text: "Notes" }, e: { probe: 2 } }] },
  { slug: "dialog", h: 260, steps: [
    { c: { text: "Rename…" }, e: { probe: 10 } },
    { c: { text: "Rename" }, e: { probe: 1 } }] },
  { slug: "alert-dialog", h: 240, steps: [
    { c: { text: "Delete", nth: "first" }, e: { probe: 10 } },
    { c: { text: "Delete", nth: "last" }, e: { probe: 1 } }] },
  { slug: "alert-dialog", h: 240, steps: [
    { c: { text: "Delete", nth: "first" }, e: { probe: 10 } },
    { c: { text: "Cancel", nth: "last" }, e: { probe: 2, text: "Canceled" } }] },
  { slug: "popover", h: 240, steps: [
    { c: { text: "Dimensions", nth: "first" }, e: { probe: 10 } }] },
  { slug: "tooltip", h: 160, steps: [
    { hover: { svg: "M15 5a3" }, e: { probe: 10, text: "Share with others" } }] },
  { slug: "toast", h: 300, steps: [{ c: { text: "Save" }, e: { probe: 1 } }] },
  { slug: "progress-bar", h: 180, steps: [
    { c: { text: "Upload a file" }, e: { probe: 3 } }] },
  { slug: "spinner", h: 120, steps: [
    { c: { text: "Stop" }, e: { probe: 1, text: "Reload" } }] },
  { slug: "meter", h: 200, steps: [
    { c: { xy: [335, 82] }, e: { probe: 250 } },
    { c: { xy: [335, 149] }, e: { probe: 50 } }] },
  { slug: "badge", h: 180, steps: [
    { c: { text: "Receive a message" }, e: { probe: 4 } }] },
  { slug: "charts", h: 300, steps: [
    { c: { text: "Line" }, e: { probe: 1 } },
    { c: { text: "Shuffle" }, e: { probe: 11 } }] },
  { slug: "image", h: 240, steps: [
    { c: { text: "Cover" }, e: { probe: 1 } },
    { c: { text: "Grayscale" }, e: { probe: 101 } }] },
  { slug: "avatar", h: 150, steps: [
    { c: { text: "Ada Lovelace" } }, { type: "!", e: { probe: 13 } }] },
  { slug: "rich-text-editor", h: 300, steps: [
    { c: { text: "B" }, e: { probe: 1 } }] },
  { slug: "markdown", h: 440, steps: [
    { c: { text: "Stream it" }, e: { probe: 1 } },
    { c: { text: "Sentence" }, e: { probe: 3 } }] },
];

const task = await taskSpace("docs-demo-functional");
const page = task.page("p1");
const sleep = ms => new Promise(r => setTimeout(r, ms));

// findPoint runs in the page: the label and the transform it is drawn
// under become a click point in CSS pixels.
function findPoint(t) {
  const canvas = document.getElementById("canvas");
  const dpr = canvas.width / canvas.clientWidth;
  const commands = window.__demoCommands();
  const place = (m, x, y) => ({
    x: (m[0] * x + m[2] * y + m[4]) / dpr,
    y: (m[1] * x + m[3] * y + m[5]) / dpr,
  });
  if (t.text !== undefined) {
    const hits = commands.filter(c => c.t === "Text" && c.str === t.text);
    if (!hits.length) return null;
    const hit = t.nth === "first" ? hits[0] : hits[hits.length - 1];
    return place(hit.ctm, hit.raw[0] + 4 + (t.dx || 0), hit.raw[1] + hit.size * 0.6);
  }
  if (t.svg !== undefined) {
    const hits = commands.filter(c => c.t === "Svg" && c.path &&
      c.path.replace("!round!", "").startsWith(t.svg));
    if (!hits.length) return null;
    return place(hits[hits.length - 1].ctm, 12, 12);
  }
  if (t.bar) {
    const [width, maxHeight, index, fraction] = t.bar;
    const hits = commands.filter(c => c.t === "RR" &&
      Math.abs(c.raw[2] - width) < 2 && c.raw[3] <= maxHeight);
    if (hits.length <= index) return null;
    const hit = hits[index];
    return place(hit.ctm, hit.raw[0] + width * fraction, hit.raw[1] + hit.raw[3] / 2);
  }
  if (t.click2) return { x: t.click2[0], y: t.click2[1] };
  if (t.xy) return { x: t.xy[0], y: t.xy[1] };
  return null;
}

let passed = 0, failed = 0;
for (const demo of demos) {
  await page.cdp("Emulation.setDeviceMetricsOverride",
    { width: 670, height: demo.h, deviceScaleFactor: 2, mobile: false });
  await page.goto(BASE + "/docs-demo/?demo=" + demo.slug + "&t=" + Date.now(),
    { waitUntil: "domcontentloaded" });
  await page.evaluate(() => new Promise(resolve => {
    const timer = setInterval(() => {
      const canvas = document.getElementById("canvas");
      if (canvas && canvas.getAttribute("data-ready") === "true") {
        clearInterval(timer); resolve();
      }
    }, 80);
  }));
  await page.evaluate(() => { window.__traceCommands = true; window.__demoRender(); });

  for (const step of demo.steps) {
    const want = step.e || {};
    const before = await page.evaluate(() => window.__demoProbe());
    const target = step.c || step.right || step.hover;
    let point = null;
    if (target) point = await page.evaluate(findPoint, target);
    if (target && !point) {
      console.log("F " + demo.slug + " " + JSON.stringify(target) + " label-missing");
      failed++; continue;
    }
    if (point && step.hover) {
      await page.mouse.move(point.x, point.y);
      await sleep(600);
    } else if (point && step.right) {
      await page.evaluate(({ x, y }) => {
        const canvas = document.getElementById("canvas");
        canvas.dispatchEvent(new MouseEvent("contextmenu",
          { bubbles: true, cancelable: true, clientX: x, clientY: y }));
      }, point);
      await sleep(250);
    } else if (point && step.c && step.c.click2) {
      await page.mouse.click(point.x, point.y);
      await sleep(70);
      await page.mouse.click(point.x, point.y);
      await sleep(250);
    } else if (point) {
      await page.mouse.click(point.x, point.y);
      await sleep(280);
    }
    if (step.type !== undefined) { await page.keyboard.type(step.type); await sleep(200); }
    if (step.key) { await page.keyboard.press(step.key); await sleep(140); }

    const after = await page.evaluate(() => ({
      probe: window.__demoProbe(),
      texts: window.__demoCommands().filter(c => c.t === "Text").map(c => c.str),
    }));
    let ok = true, note = "";
    if (want.probe !== undefined) { ok = ok && after.probe === want.probe; note += " probe=" + after.probe + " want " + want.probe; }
    if (want.probeRange) { ok = ok && after.probe >= want.probeRange[0] && after.probe <= want.probeRange[1]; note += " probe=" + after.probe + " want " + want.probeRange; }
    if (want.unchanged) { ok = ok && after.probe === before; note += " probe=" + after.probe + " (unchanged)"; }
    if (want.text) {
      const seen = after.texts.includes(want.text);
      ok = ok && seen;
      note += " text=" + (seen ? "ok" : "missing");
    }
    if (ok) passed++; else failed++;
    console.log((ok ? "P " : "F ") + demo.slug + " " + JSON.stringify(target || {}) +
      (step.type ? " type=" + step.type : "") + (step.key ? " key=" + step.key : "") +
      " ->" + note);
  }
}
console.log("SUMMARY passed=" + passed + " failed=" + failed);
await task.finish({ keep: [] });
