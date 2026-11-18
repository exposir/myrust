// ascentOf returns the alphabetic baseline offset of the current canvas
// font, as MyGo's browser engine lays out text (Canvas fontBoundingBoxAscent).
var ascentCache = {};
function ascentOf(ctx) {
  var key = ctx.font;
  var m = ascentCache[key];
  if (m === undefined) {
    var tm = ctx.measureText("Hg");
    m = tm.fontBoundingBoxAscent;
    if (!(m > 0)) {
      var size = parseFloat(key) || 14;
      m = size * 0.93;
    }
    ascentCache[key] = m;
  }
  return m;
}


// lineHeightOf returns the line box of the current canvas font, as the
// engine's measurer does (fontBoundingBox ascent + descent + 1; `font.rs`
// line_height).
var lineHeightCache = {};
function lineHeightOf(ctx) {
  var key = ctx.font;
  var m = lineHeightCache[key];
  if (m === undefined) {
    var tm = ctx.measureText("Hg");
    m = tm.fontBoundingBoxAscent > 0 && tm.fontBoundingBoxDescent > 0
      ? tm.fontBoundingBoxAscent + tm.fontBoundingBoxDescent + 1
      : (parseFloat(key) || 14) * 1.3;
    lineHeightCache[key] = m;
  }
  return m;
}

// wrapCanvasText breaks a run at `maxWidth` the way the engine's
// `layout_text` does: Latin words at whitespace, an oversized word between
// characters, and CJK one character at a time (no spaces to break at).
function wrapCanvasText(ctx, text, maxWidth) {
  if (ctx.measureText(text).width <= maxWidth) return [text];
  var lines = [];
  var line = "";
  var tokens = text.split(/(\s+)/);
  for (var i = 0; i < tokens.length; i++) {
    var token = tokens[i];
    if (!token) continue;
    if (/^\s+$/.test(token)) {
      if (line) line += " ";
      continue;
    }
    var candidate = line ? line + token : token;
    if (ctx.measureText(candidate).width <= maxWidth) {
      line = candidate;
      continue;
    }
    if (line) {
      lines.push(line);
      line = "";
    }
    // A word wider than the line: cut it between characters.
    var rest = token;
    while (rest) {
      var cut = rest.length;
      while (cut > 1 && ctx.measureText(rest.slice(0, cut)).width > maxWidth) cut--;
      if (cut >= rest.length) {
        line = rest;
        break;
      }
      lines.push(rest.slice(0, cut));
      rest = rest.slice(cut);
    }
  }
  if (line) lines.push(line);
  return lines.length ? lines : [""];
}

// cssFamilyOf maps a MyGo font family name to the CSS stack its browser
// canvas engine uses (`internal/text/canvas_js.go` cssFamily). The generic
// names take the system's own stacks; anything else is quoted verbatim.
function cssFamilyOf(name) {
  if (!name) {
    return 'system-ui, -apple-system, "Segoe UI", Roboto, "Helvetica Neue", Arial, "Noto Sans", sans-serif';
  }
  var lower = String(name).toLowerCase();
  if (lower === "system-ui") {
    return 'system-ui, -apple-system, "Segoe UI", Roboto, "Helvetica Neue", Arial, "Noto Sans", sans-serif, "Apple Color Emoji", "Segoe UI Emoji"';
  }
  if (lower === "sans-serif") return "sans-serif";
  if (lower === "serif") return "serif";
  if (lower === "monospace") {
    return 'ui-monospace, SFMono-Regular, Menlo, Consolas, "Liberation Mono", monospace';
  }
  return '"' + String(name).replace(/"/g, '\\"') + '"';
}

(function () {
  "use strict";
  var canvas = document.getElementById("canvas");
  var title = document.getElementById("title");
  // MyRust's web surface draws with WebGL 2, as MyGo's web backend does
  // (`internal/web/surface.go`, `internal/gpu/gl`): one instanced quad per
  // paint op, signed distances for rounded coverage, and Canvas 2D only for
  // the text, SVG and shadow coverage masks that WebGL cannot rasterize --
  // the same split MyGo makes (`internal/text/canvas_js.go`). A browser
  // without WebGL 2 falls back to the Canvas 2D renderer below, and
  // `?renderer=canvas` forces that path.
  // Instance stride and batch cap for the WebGL 2 presenter below.
  var GFLOAT = 36;
  var GCAP = 4096;
  var forcedCanvas = new URLSearchParams(location.search).get("renderer") === "canvas";
  var glr = forcedCanvas ? null : makeGlRenderer(canvas);
  window.__demoGl = !!glr;

  // textCtx measures and rasterizes text in both modes: WebGL has no text
  // rasterizer, so glyph coverage always comes from Canvas 2D. In canvas
  // mode it is the visible context itself.
  // MyGo rasterizes its canvas with full 8-bit coverage; Chrome's GPU
  // canvas answers with four levels (0, 64, 128, 191, 255) at the edge of a
  // curve, which shows as a stair on every rounded corner. A CPU-backed
  // context takes the analytic path instead and matches MyGo's edges.
  var ctx = glr
    ? document.createElement("canvas").getContext("2d", { willReadFrequently: true })
    : canvas.getContext("2d", { willReadFrequently: true });

  // A hidden textarea takes the keyboard, as it does in MyGo's web surface
  // (`internal/web/surface.go`): browsers show input methods, compose text
  // and place candidates in an editable element, not in a canvas. Nothing
  // here is drawn; the wasm UI draws its own caret.
  var input = document.createElement("textarea");
  ["autocomplete|off", "autocorrect|off", "autocapitalize|off",
   "spellcheck|false", "inputmode|none"].forEach(function (pair) {
    var parts = pair.split("|");
    input.setAttribute(parts[0], parts[1]);
  });
  input.setAttribute("aria-label", "MyRust UI text input");
  input.style.cssText =
    "position:fixed;left:0;top:0;width:1px;height:16px;padding:0;margin:0;" +
    "border:0;outline:0;background:transparent;color:transparent;" +
    "caret-color:transparent;resize:none;overflow:hidden;white-space:pre;" +
    "z-index:10;opacity:0;font:16px/1 system-ui,sans-serif;";
  canvas.parentNode.appendChild(input);

  function focusDemo() {
    if (document.activeElement === input) return;
    try { input.focus({ preventScroll: true }); } catch (error) { input.focus(); }
  }
  window.__demoFocus = focusDemo;

  input.addEventListener("focus", function () { canvas.setAttribute("data-focus", "true"); });
  input.addEventListener("blur", function () { canvas.setAttribute("data-focus", "false"); });
  var slug = new URLSearchParams(location.search).get("demo") || "tabs";
  var demoIds = { tabs: 0, markdown: 1, views: 2, layout: 3, text: 4, styling: 5, input: 6, "drag-and-drop": 7, "pages-and-navigation": 8, overlays: 9, accessibility: 10, "right-to-left": 11, languages: 12, "drawing-and-animation": 13, transitions: 14, motion: 15, transforms: 16, "custom-widgets": 17, "windows-with-native-ui": 18, testing: 19, inspector: 20, rendering: 21, migration: 22, performance: 23, button: 24, link: 25, "menu-button": 26, "context-menu": 27, toggle: 28, "segmented-control": 29, toolbar: 30, checkbox: 31, switch: 32, radio: 33, select: 34, slider: 35, "range-slider": 36, stepper: 37, rating: 38, "text-input": 39, "text-input-primitives": 40, "number-input": 41, "decimal-input": 42, "search-field": 43, combobox: 44, autocomplete: 45, "token-field": 46, "editable-text": 47, "find-bar": 48, "rich-text-editor": 49, "code-editor": 50, "diff-viewer": 51, calendar: 52, "date-input": 53, "time-input": 54, "color-picker": 55, list: 56, table: 57, tree: 58, outline: 59, "grid-view": 60, "scroll-view": 61, grid: 62, "split-view": 63, collapsible: 64, accordion: 65, form: 66, sidebar: 67, breadcrumbs: 68, "back-and-forward-buttons": 69, dialog: 70, "alert-dialog": 71, popover: 72, tooltip: 73, toast: 74, "progress-bar": 75, spinner: 76, meter: 77, badge: 78, charts: 79, image: 80, icon: 81, avatar: 82, showcase: 83, 2048: 84, snake: 85, tetris: 86 };
  var slugId = demoIds[slug] !== undefined ? demoIds[slug] : 0;
  var labels = { tabs: "Tabs", markdown: "Markdown", views: "Views", layout: "Layout", text: "Text", styling: "Styling and themes", input: "Input", "drag-and-drop": "Drag and drop", "pages-and-navigation": "Pages and navigation", overlays: "Overlays", accessibility: "Accessibility", "right-to-left": "Right to left", languages: "Languages", "drawing-and-animation": "Drawing and animation", transitions: "Transitions", motion: "Motion", transforms: "Transforms", "custom-widgets": "Custom widgets", "windows-with-native-ui": "Windows with native UI", testing: "Testing", inspector: "Inspector", rendering: "Rendering", migration: "Migration", performance: "Performance", button: "Button", link: "Link", "menu-button": "Menu button", "context-menu": "Context menu", toggle: "Toggle", "segmented-control": "Segmented control", toolbar: "Toolbar", checkbox: "Checkbox", switch: "Switch", radio: "Radio", select: "Select", slider: "Slider", "range-slider": "Range slider", stepper: "Stepper", rating: "Rating", "text-input": "Text input", "text-input-primitives": "Text-input primitives", "number-input": "Number input", "decimal-input": "Decimal input", "search-field": "Search field", combobox: "Combobox", autocomplete: "Autocomplete", "token-field": "Token field", "editable-text": "Editable text", "find-bar": "Find bar", "rich-text-editor": "Rich text editor", "code-editor": "Code editor", "diff-viewer": "Diff viewer", calendar: "Calendar", "date-input": "Date input", "time-input": "Time input", "color-picker": "Color picker", list: "List", table: "Table", tree: "Tree", outline: "Outline", "grid-view": "Grid view", "scroll-view": "Scroll view", grid: "Grid", "split-view": "Split view", collapsible: "Collapsible", accordion: "Accordion", form: "Form", sidebar: "Sidebar", breadcrumbs: "Breadcrumbs", "back-and-forward-buttons": "Back and forward buttons", dialog: "Dialog", "alert-dialog": "Alert dialog", popover: "Popover", tooltip: "Tooltip", toast: "Toast", "progress-bar": "Progress bar", spinner: "Spinner", meter: "Meter", badge: "Badge", charts: "Charts", image: "Image", icon: "Icon", avatar: "Avatar", showcase: "Showcase", 2048: "2048", snake: "Snake", tetris: "Tetris" };
  var name = labels[slug] || labels.tabs;
  if (true) {
    document.querySelector(".titlebar").style.display = "none";
    document.body.style.padding = "0";
    document.querySelector(".window").style.border = "0";
    document.querySelector(".window").style.borderRadius = "0";
    document.querySelector(".window").style.boxShadow = "none";
    document.body.style.background = "#ffffff";
    document.documentElement.style.background = "#ffffff";
    document.querySelector(".window").style.background = "#ffffff";
  }
  title.textContent = name + " · MyRust UI";
  document.title = name + " · MyRust UI demo";
  var textHalf = new URLSearchParams(location.search).get("text") === "half";
  var disabledDemo = new URLSearchParams(location.search).get("disabled") === "1";
  var wasm = null;
  var lastDraw = 0;
  var pixelWidth = 0, pixelHeight = 0;

  function rgba(color) {
    return "rgba(" + color[0] + "," + color[1] + "," + color[2] + "," + (color[3] / 255) + ")";
  }

  function readColor(bytes, state) {
    var color = [bytes[state.off], bytes[state.off + 1], bytes[state.off + 2], bytes[state.off + 3]];
    state.off += 4;
    return color;
  }

  function applyCtm(ctm) {
    ctx.setTransform(ctm[0], ctm[1], ctm[2], ctm[3], ctm[4], ctm[5]);
  }

  // MyGo snaps every painted edge to the device pixel grid in absolute
  // coordinates (Painter.snap): round(X*scale). Layout positions are
  // fractional, and a node's own translation may be too, so snapping in
  // local space leaves the edge half a pixel off once the translation
  // lands between device pixels. Snap in device space and convert back.
  function snapAxis(value, offset, scale) {
    if (!(scale > 0)) return value;
    return (Math.round(value * scale + offset) - offset) / scale;
  }
  function axisAligned(ctm) {
    return Math.abs(ctm[1]) < 1e-4 && Math.abs(ctm[2]) < 1e-4;
  }
  // snapRect returns the local box whose edges fall on whole device
  // pixels, as MyGo's Painter.snap does: round(X*scale). At a device scale
  // of 2 an edge on an odd device pixel covers half a CSS pixel once the
  // frame is composited down, which is the soft edge MyGo's own page shows.
  function snapRect(ctm, x, y, w, h, dpr) {
    if (!axisAligned(ctm)) return [x, y, w, h];
    var x0 = snapDevice(x, ctm[4], ctm[0]);
    var y0 = snapDevice(y, ctm[5], ctm[3]);
    var x1 = snapDevice(x + w, ctm[4], ctm[0]);
    var y1 = snapDevice(y + h, ctm[5], ctm[3]);
    return [x0, y0, x1 - x0, y1 - y0];
  }
  function snapDevice(value, offset, scale) {
    if (!(scale > 0)) return value;
    return (Math.round(value * scale + offset) - offset) / scale;
  }
  // snapRule puts a rule's covered span on whole CSS pixels: its device
  // start lands on a CSS pixel boundary, so the rule fills one row (or
  // column) of the composited image instead of half of two.
  function snapRule(value, offset, scale, width, dpr) {
    if (!(scale > 0)) return value;
    var device = value * scale + offset;
    var deviceWidth = width * scale;
    var start = Math.round(device - deviceWidth / 2);
    return (start + deviceWidth / 2 - offset) / scale;
  }
  // snapEdge puts an edge (a border's outer side) on a CSS pixel boundary,
  // for the same reason: half-covered pixels read as a lighter rule.
  function snapEdge(value, offset, scale, dpr) {
    return snapDevice(value, offset, scale);
  }

  // MyGo draws text through a gray of its shade: a canvas rasterizes text
  // of each luminance with its own coverage (Skia corrects coverage by the
  // text's color), so its engine fills each glyph with the gray of the
  // text's shade, reads the coverage back and tints it with the color
  // (internal/text/canvas_js.go glyph/shadeGray; ui/glyphs.go ShadeOf).
  // Filling the color itself skips that correction, and the text lands a
  // shade lighter than MyGo's.
  var SHADE_GRAY = [0, 137, 188, 225];
  var shadeLinear = null;
  function shadeOf(color) {
    if (!shadeLinear) {
      shadeLinear = new Float32Array(256);
      for (var i = 0; i < 256; i++) {
        var v = i / 255;
        shadeLinear[i] = v <= 0.04045 ? v / 12.92 : Math.pow((v + 0.055) / 1.055, 2.4);
      }
    }
    // ShadeOf rounds the relative luminance to a quarter, at most 3/4.
    var y = 0.2126 * shadeLinear[color[0]] + 0.7152 * shadeLinear[color[1]] +
      0.0722 * shadeLinear[color[2]];
    return SHADE_GRAY[Math.min(Math.floor(y * 4 + 0.5), 3)];
  }
  // hasColorGlyphs reports text the canvas paints in its own colors, which
  // MyGo leaves untinted (internal/text/canvas_js.go isEmoji).
  function hasColorGlyphs(text) {
    for (var i = 0; i < text.length; i++) {
      var c = text.charCodeAt(i);
      if (c >= 0xd800 && c <= 0xdbff && i + 1 < text.length) {
        var d = text.charCodeAt(i + 1);
        if ((d & 0xfc00) === 0xdc00) {
          var cp = 0x10000 + ((c - 0xd800) << 10) + (d - 0xdc00);
          if (cp >= 0x1f000 && cp <= 0x1faff) return true;
          i++;
          continue;
        }
      }
      if (c >= 0x2600 && c <= 0x27bf) return true;
      if (c === 0xfe0f) return true;
    }
    return false;
  }
  var shadeCanvas = null;
  var iconCanvas = null;
  // MyGo's canvas engine lays a word out cluster by cluster and draws each
  // alone at the pen the browser gives it in the whole word, so that
  // kerning with the cluster before it moves it, and not the next one
  // (`internal/text/canvas_js.go` measureClusters). Its rasterizer then
  // places each glyph at a quarter of a device pixel (`internal/text/
  // raster.go` place: four positions). These find those clusters and pens.
  var clusterSegmenter = null;
  if (typeof Intl !== "undefined" && Intl.Segmenter) {
    clusterSegmenter = new Intl.Segmenter(undefined, { granularity: "grapheme" });
  }
  function clustersOf(text) {
    if (clusterSegmenter) {
      var out = [];
      var steps = clusterSegmenter.segment(text);
      for (var step of steps) out.push(step.segment);
      return out;
    }
    return Array.from(text);
  }
  function textPens(mctx, text) {
    var clusters = [];
    var pen = 0;
    var pieces = text.split(/(\s+)/);
    for (var p = 0; p < pieces.length; p++) {
      if (!pieces[p]) continue;
      var cs = clustersOf(pieces[p]);
      var n = cs.length;
      if (n === 0) continue;
      var w = new Array(n + 1);
      w[0] = 0;
      if (n === 1) {
        w[1] = mctx.measureText(cs[0]).width;
      } else {
        w[n] = mctx.measureText(pieces[p]).width;
        if (n > 33) {
          for (var k = 1; k < n; k++) {
            w[k] = w[k - 1] + mctx.measureText(cs[k - 1]).width;
          }
        } else {
          var through = cs[0];
          for (var k = 1; k < n; k++) {
            through += cs[k];
            var prefix = mctx.measureText(through).width;
            var alone = mctx.measureText(cs[k]).width;
            w[k] = Math.max(prefix - alone, w[k - 1]);
          }
        }
      }
      for (var k = 0; k < n; k++) {
        clusters.push({ text: cs[k], x: pen });
        pen += Math.max(w[k + 1] - w[k], 0);
      }
    }
    return clusters;
  }
  // buildTextRun rasterizes a run cluster by cluster, each at the quarter
  // of a device pixel MyGo's placement picks, into a Canvas 2D coverage
  // mask (`internal/text/canvas_js.go`). It returns the mask and where it
  // belongs, or null where the run must be drawn whole with plain fillText
  // (color glyphs, a rotated transform, an oversized mask).
  //
  // The clusters of one run never overlap, so they share a single mask: one
  // clear, one pass of fills, one tint. Rasterizing each cluster on its own
  // meant four canvas operations and one texture upload per glyph, which is
  // what made a paragraph of text cost milliseconds.
  function buildTextRun(mctx, text, x, baseline, color, ctm, off) {
    if (!(ctm[0] > 0) || !(ctm[3] > 0) || ctm[1] !== 0 || ctm[2] !== 0) return null;
    if (hasColorGlyphs(text)) return null;
    var clusters = textPens(mctx, text);
    var sx = ctm[0], sy = ctm[3], pad = 2;
    // Place every cluster first: the mask's box has to hold all of them.
    var placed = [];
    var left = Infinity, top = Infinity, right = -Infinity, bottom = -Infinity;
    for (var i = 0; i < clusters.length; i++) {
      var cluster = clusters[i];
      if (!cluster.text.trim()) continue;
      var device = (x + cluster.x) * sx + ctm[4];
      var whole = Math.floor(device);
      var q = Math.floor((device - whole) * 4 + 0.5);
      var origin = whole + (q === 4 ? 1 : q / 4);
      var cx = (origin - ctm[4]) / sx;
      var m = mctx.measureText(cluster.text);
      var l = cx - (m.actualBoundingBoxLeft || 0);
      var r = cx + (m.actualBoundingBoxRight || 0);
      var t = baseline - (m.actualBoundingBoxAscent || 0);
      var b = baseline + (m.actualBoundingBoxDescent || 0);
      if (!(r > l) || !(b > t)) continue;
      placed.push({ text: cluster.text, x: cx });
      if (l < left) left = l;
      if (t < top) top = t;
      if (r > right) right = r;
      if (b > bottom) bottom = b;
    }
    if (placed.length === 0) return null;
    var x0 = Math.floor(left * sx + ctm[4]) - pad;
    var y0 = Math.floor(top * sy + ctm[5]) - pad;
    var w = Math.ceil(right * sx + ctm[4]) + pad - x0;
    var h = Math.ceil(bottom * sy + ctm[5]) + pad - y0;
    if (w > 4096 || h > 4096) return null;
    var target = off || document.createElement("canvas");
    if (!off) {
      target.width = w;
      target.height = h;
    } else if (target.width < w || target.height < h) {
      target.width = Math.max(target.width, w);
      target.height = Math.max(target.height, h);
    }
    var octx = target.getContext("2d", { willReadFrequently: true });
    octx.setTransform(1, 0, 0, 1, 0, 0);
    octx.clearRect(0, 0, w, h);
    octx.setTransform(sx, 0, 0, sy, ctm[4] - x0, ctm[5] - y0);
    octx.font = mctx.font;
    octx.textBaseline = "alphabetic";
    octx.textAlign = "start";
    var gray = shadeOf(color);
    octx.fillStyle = "rgb(" + gray + "," + gray + "," + gray + ")";
    for (var k = 0; k < placed.length; k++) octx.fillText(placed[k].text, placed[k].x, baseline);
    // Where the gray covers, the color shows: the coverage is the alpha.
    octx.setTransform(1, 0, 0, 1, 0, 0);
    octx.globalCompositeOperation = "source-in";
    octx.fillStyle = rgba(color);
    octx.fillRect(0, 0, w, h);
    octx.globalCompositeOperation = "source-over";
    return { canvas: target, x0: x0, y0: y0, w: w, h: h, sx: sx, sy: sy, tx: ctm[4], ty: ctm[5] };
  }

  // paintTextRun2d blits a run's mask where it belongs, in local
  // coordinates, so the visible Canvas 2D frame gets the same pixels.
  function paintTextRun2d(run) {
    ctx.drawImage(run.canvas, 0, 0, run.w, run.h,
      (run.x0 - run.tx) / run.sx, (run.y0 - run.ty) / run.sy, run.w / run.sx, run.h / run.sy);
  }

  // buildPlainRun rasterizes a run whole with fillText under its own
  // transform, in device pixels, for the runs the clustered mask cannot
  // take: color glyphs, rotated transforms, a forced plain draw. The GPU
  // presenter draws the result as one textured quad.
  function buildPlainRun(mctx, text, x, baseline, color, ctm) {
    if (!text) return null;
    var m = mctx.measureText(text);
    var ascent = ascentOf(mctx);
    var descent = m.fontBoundingBoxDescent > 0 ? m.fontBoundingBoxDescent : ascent * 0.3;
    var l = x - Math.max(m.actualBoundingBoxLeft || 0, 0) - 1;
    var r = x + Math.max(m.actualBoundingBoxRight || 0, 0) + 1;
    var t = baseline - Math.max(m.actualBoundingBoxAscent || 0, ascent) - 1;
    var b = baseline + Math.max(m.actualBoundingBoxDescent || 0, descent) + 1;
    if (!(r > l) || !(b > t)) return null;
    var minX = Infinity, minY = Infinity, maxX = -Infinity, maxY = -Infinity;
    for (var i = 0; i < 4; i++) {
      var px = (i & 1) ? r : l;
      var py = (i & 2) ? b : t;
      var dx = ctm[0] * px + ctm[2] * py + ctm[4];
      var dy = ctm[1] * px + ctm[3] * py + ctm[5];
      if (dx < minX) minX = dx;
      if (dy < minY) minY = dy;
      if (dx > maxX) maxX = dx;
      if (dy > maxY) maxY = dy;
    }
    var x0 = Math.floor(minX), y0 = Math.floor(minY);
    var w = Math.ceil(maxX) - x0, h = Math.ceil(maxY) - y0;
    if (w <= 0 || h <= 0 || w > 4096 || h > 4096) return null;
    var target = document.createElement("canvas");
    target.width = w;
    target.height = h;
    var ictx = target.getContext("2d");
    ictx.setTransform(1, 0, 0, 1, 0, 0);
    ictx.setTransform(ctm[0], ctm[1], ctm[2], ctm[3], ctm[4] - x0, ctm[5] - y0);
    ictx.font = mctx.font;
    ictx.textBaseline = "alphabetic";
    ictx.textAlign = "start";
    ictx.fillStyle = rgba(color);
    ictx.fillText(text, x, baseline);
    return { canvas: target, x0: x0, y0: y0, w: w, h: h };
  }

  function multiplyCtm(a, b) {
    return [
      a[0] * b[0] + a[2] * b[1],
      a[1] * b[0] + a[3] * b[1],
      a[0] * b[2] + a[2] * b[3],
      a[1] * b[2] + a[3] * b[3],
      a[0] * b[4] + a[2] * b[5] + a[4],
      a[1] * b[4] + a[3] * b[5] + a[5]
    ];
  }

  function roundedSubpath(c2d, x, y, w, h, radii) {
    var limit = Math.min(w, h) / 2;
    var r = radii.map(function (value) { return Math.max(0, Math.min(value, limit)); });
    if (c2d.roundRect) {
      c2d.roundRect(x, y, w, h, r);
      return;
    }
    c2d.moveTo(x + r[0], y);
    c2d.lineTo(x + w - r[1], y);
    c2d.arcTo(x + w, y, x + w, y + r[1], r[1]);
    c2d.lineTo(x + w, y + h - r[2]);
    c2d.arcTo(x + w, y + h, x + w - r[2], y + h, r[2]);
    c2d.lineTo(x + r[3], y + h);
    c2d.arcTo(x, y + h, x, y + h - r[3], r[3]);
    c2d.lineTo(x, y + r[0]);
    c2d.arcTo(x, y, x + r[0], y, r[0]);
    c2d.closePath();
  }

  function roundedPath(c2d, x, y, w, h, radii) {
    c2d.beginPath();
    roundedSubpath(c2d, x, y, w, h, radii);
  }

  function render() {
    if (glr) renderGl();
    else if (ctx) renderCanvas();
  }


  // ---- WebGL 2 presenter --------------------------------------------------
  //
  // MyGo's web backend draws every op as one instanced quad through a
  // signed-distance shader (`internal/gpu/gl`): fills, borders, lines and
  // gradients from distances, text/SVG/shadow coverage as Canvas 2D-built
  // textures (`internal/text/canvas_js.go`). The functions below mirror
  // that arrangement for MyRust's display list; when WebGL 2 is missing or
  // the shader fails to build, the page stays on the Canvas 2D renderer.
  function makeGlRenderer(canvas) {
    var gl = null;
    try {
      gl = canvas.getContext("webgl2", {
        alpha: false, premultipliedAlpha: false, antialias: false,
        depth: false, stencil: false, preserveDrawingBuffer: true,
        powerPreference: "default"
      });
    } catch (error) { gl = null; }
    if (!gl || (gl.isContextLost && gl.isContextLost())) return null;

    var vertex = [
      "#version 300 es",
      "layout(location = 0) in vec4 aRect;",
      "layout(location = 1) in vec4 aRadii;",
      "layout(location = 2) in vec4 aColor;",
      "layout(location = 3) in vec4 aGrad;",
      "layout(location = 4) in vec4 aDev;",
      "layout(location = 5) in vec4 aInv;",
      "layout(location = 6) in vec4 aInvT;",
      "layout(location = 7) in vec4 aSeg;",
      "layout(location = 8) in vec4 aHalf;",
      "uniform vec2 uSize;",
      "out vec2 vLocal;",
      "out vec2 vDev;",
      "flat out vec4 vRect;",
      "flat out vec4 vRadii;",
      "flat out vec4 vColor;",
      "flat out vec4 vGrad;",
      "flat out vec4 vSeg;",
      "flat out vec4 vHalf;",
      "flat out vec4 vParams;",
      "void main() {",
      "  vec2 corner = vec2(float(gl_VertexID & 1), float(gl_VertexID >> 1));",
      "  vec2 dev = aDev.xy + corner * aDev.zw;",
      "  vLocal = vec2(aInv.x * dev.x + aInv.y * dev.y + aInvT.x,",
      "                aInv.z * dev.x + aInv.w * dev.y + aInvT.y);",
      "  vDev = dev;",
      "  vRect = aRect; vRadii = aRadii; vColor = aColor;",
      "  vGrad = aGrad; vSeg = aSeg; vHalf = aHalf;",
      "  vParams = vec4(aInvT.z, aInvT.w, 0.0, 0.0);",
      "  gl_Position = vec4(dev.x / uSize.x * 2.0 - 1.0,",
      "                     1.0 - dev.y / uSize.y * 2.0, 0.0, 1.0);",
      "}"
    ].join("\n");

    var fragment = [
      "#version 300 es",
      "precision highp float;",
      "in vec2 vLocal;",
      "in vec2 vDev;",
      "flat in vec4 vRect;",
      "flat in vec4 vRadii;",
      "flat in vec4 vColor;",
      "flat in vec4 vGrad;",
      "flat in vec4 vSeg;",
      "flat in vec4 vHalf;",
      "flat in vec4 vParams;",
      "uniform sampler2D uTex;",
      "uniform sampler2D uLut;",
      "out vec4 fragColor;",
      "float sdRoundBox(vec2 p, vec2 b, vec4 radii) {",
      "  vec2 rr = (p.x > 0.0) ? vec2(radii.y, radii.z) : vec2(radii.x, radii.w);",
      "  float r = (p.y > 0.0) ? rr.y : rr.x;",
      "  vec2 q = abs(p) - b + r;",
      "  return min(max(q.x, q.y), 0.0) + length(max(q, 0.0)) - r;",
      "}",
      "float segDistance(vec2 p, vec2 a, vec2 b) {",
      "  vec2 pa = p - a, ba = b - a;",
      "  float h = clamp(dot(pa, ba) / max(dot(ba, ba), 1e-12), 0.0, 1.0);",
      "  return length(pa - ba * h);",
      "}",
      "void main() {",
      "  float kind = vParams.x;",
      "  float scale = vParams.y;",
      "  vec4 color = vColor;",
      "  float cover = 1.0;",
      "  if (kind < 0.5) {",
      "    vec2 b = vRect.zw * 0.5;",
      "    cover = clamp(0.5 - sdRoundBox(vLocal - vRect.xy - b, b, vRadii) * scale, 0.0, 1.0);",
      "  } else if (kind < 1.5) {",
      "    float d = segDistance(vLocal, vSeg.xy, vSeg.zw) - vHalf.x;",
      "    cover = clamp(0.5 - d * scale, 0.0, 1.0);",
      "  } else if (kind < 2.5) {",
      "    vec2 b = vRect.zw * 0.5;",
      "    cover = clamp(0.5 - sdRoundBox(vLocal - vRect.xy - b, b, vRadii) * scale, 0.0, 1.0);",
      "    vec2 grad = vGrad.zw - vGrad.xy;",
      "    float dd = dot(grad, grad);",
      "    float t = dd > 0.0 ? dot(vLocal - vGrad.xy, grad) / dd : 0.0;",
      "    color = texture(uLut, vec2(clamp(t, 0.0, 1.0), 0.5));",
      "  } else {",
      "    vec2 uv = (vDev - vGrad.xy) / vGrad.zw;",
      "    vec4 texel = texture(uTex, uv);",
      "    if (kind < 3.5) {",
      "      color = texel;",
      "    } else {",
      "      color = vec4(vColor.rgb, texel.a * vColor.a);",
      "    }",
      "  }",
      "  fragColor = vec4(color.rgb, color.a * cover);",
      "  if (fragColor.a <= 0.0) discard;",
      "}"
    ].join("\n");

    function compile(type, source) {
      var shader = gl.createShader(type);
      gl.shaderSource(shader, source);
      gl.compileShader(shader);
      if (!gl.getShaderParameter(shader, gl.COMPILE_STATUS)) {
        console.error("MyRust WebGL shader:", gl.getShaderInfoLog(shader));
        gl.deleteShader(shader);
        return null;
      }
      return shader;
    }
    var vs = compile(gl.VERTEX_SHADER, vertex);
    var fs = compile(gl.FRAGMENT_SHADER, fragment);
    if (!vs || !fs) { if (vs) gl.deleteShader(vs); if (fs) gl.deleteShader(fs); return null; }
    var program = gl.createProgram();
    gl.attachShader(program, vs);
    gl.attachShader(program, fs);
    gl.linkProgram(program);
    gl.deleteShader(vs);
    gl.deleteShader(fs);
    if (!gl.getProgramParameter(program, gl.LINK_STATUS)) {
      console.error("MyRust WebGL program:", gl.getProgramInfoLog(program));
      gl.deleteProgram(program);
      return null;
    }
    gl.useProgram(program);
    var uSize = gl.getUniformLocation(program, "uSize");
    gl.uniform1i(gl.getUniformLocation(program, "uTex"), 0);
    gl.uniform1i(gl.getUniformLocation(program, "uLut"), 1);
    gl.enable(gl.BLEND);
    gl.blendFuncSeparate(gl.SRC_ALPHA, gl.ONE_MINUS_SRC_ALPHA, gl.ONE, gl.ONE_MINUS_SRC_ALPHA);
    gl.disable(gl.DEPTH_TEST);
    gl.disable(gl.STENCIL_TEST);
    gl.disable(gl.CULL_FACE);

    var vao = gl.createVertexArray();
    var buffer = gl.createBuffer();
    var stride = GFLOAT * 4;
    gl.bindVertexArray(vao);
    gl.bindBuffer(gl.ARRAY_BUFFER, buffer);
    var offsets = [0, 4, 8, 12, 16, 20, 24, 28, 32];
    for (var a = 0; a < offsets.length; a++) {
      gl.enableVertexAttribArray(a);
      gl.vertexAttribPointer(a, 4, gl.FLOAT, false, stride, offsets[a] * 4);
      gl.vertexAttribDivisor(a, 1);
    }
    gl.bindVertexArray(null);

    var instances = new Float32Array(GFLOAT * 512);
    var count = 0;
    var bufferBytes = 0;
    var frame = 0;
    var frameW = 1, frameH = 1;
    var clip = [];
    var maskCache = new Map();
    var maskPixels = 0;
    var lutCache = new Map();

    function color4(color) {
      return [color[0] / 255, color[1] / 255, color[2] / 255, color[3] / 255];
    }

    function transformOf(ctm) {
      var a = ctm[0], b = ctm[2], c = ctm[1], d = ctm[3];
      var det = a * d - b * c;
      if (!isFinite(det) || Math.abs(det) < 1e-9) return null;
      var A = d / det, B = -b / det, C = -c / det, D = a / det;
      return {
        A: A, B: B, C: C, D: D,
        bx: -(A * ctm[4] + B * ctm[5]),
        by: -(C * ctm[4] + D * ctm[5]),
        scale: Math.sqrt(Math.abs(det))
      };
    }

    function deviceBox(x, y, w, h, ctm, margin) {
      var minX = Infinity, minY = Infinity, maxX = -Infinity, maxY = -Infinity;
      for (var i = 0; i < 4; i++) {
        var px = (i & 1) ? x + w : x;
        var py = (i & 2) ? y + h : y;
        var dx = ctm[0] * px + ctm[2] * py + ctm[4];
        var dy = ctm[1] * px + ctm[3] * py + ctm[5];
        if (dx < minX) minX = dx;
        if (dy < minY) minY = dy;
        if (dx > maxX) maxX = dx;
        if (dy > maxY) maxY = dy;
      }
      minX -= margin; minY -= margin; maxX += margin; maxY += margin;
      return [minX, minY, maxX - minX, maxY - minY];
    }

    function addInstance(rect, radii, color, grad, dev, inv, kind, scale, seg, half) {
      if ((count + 1) * GFLOAT > instances.length) {
        var grown = new Float32Array(instances.length * 2);
        grown.set(instances.subarray(0, count * GFLOAT));
        instances = grown;
      }
      var o = count * GFLOAT;
      var data = instances;
      data[o] = rect[0]; data[o + 1] = rect[1]; data[o + 2] = rect[2]; data[o + 3] = rect[3];
      data[o + 4] = radii[0]; data[o + 5] = radii[1]; data[o + 6] = radii[2]; data[o + 7] = radii[3];
      data[o + 8] = color[0]; data[o + 9] = color[1]; data[o + 10] = color[2]; data[o + 11] = color[3];
      data[o + 12] = grad[0]; data[o + 13] = grad[1]; data[o + 14] = grad[2]; data[o + 15] = grad[3];
      data[o + 16] = dev[0]; data[o + 17] = dev[1]; data[o + 18] = dev[2]; data[o + 19] = dev[3];
      data[o + 20] = inv.A; data[o + 21] = inv.B; data[o + 22] = inv.C; data[o + 23] = inv.D;
      data[o + 24] = inv.bx; data[o + 25] = inv.by; data[o + 26] = kind; data[o + 27] = scale;
      data[o + 28] = seg[0]; data[o + 29] = seg[1]; data[o + 30] = seg[2]; data[o + 31] = seg[3];
      data[o + 32] = half;
      count += 1;
      if (count >= GCAP) flush();
    }

    function flush() {
      if (!count) return;
      gl.bindVertexArray(vao);
      gl.bindBuffer(gl.ARRAY_BUFFER, buffer);
      var needed = count * GFLOAT * 4;
      if (needed > bufferBytes) {
        gl.bufferData(gl.ARRAY_BUFFER, needed, gl.DYNAMIC_DRAW);
        bufferBytes = needed;
      }
      gl.bufferSubData(gl.ARRAY_BUFFER, 0, instances.subarray(0, count * GFLOAT));
      gl.drawArraysInstanced(gl.TRIANGLE_STRIP, 0, 4, count);
      count = 0;
    }

    function upload(canvas) {
      var tex = gl.createTexture();
      gl.activeTexture(gl.TEXTURE0);
      gl.bindTexture(gl.TEXTURE_2D, tex);
      gl.pixelStorei(gl.UNPACK_PREMULTIPLY_ALPHA_WEBGL, false);
      gl.pixelStorei(gl.UNPACK_FLIP_Y_WEBGL, false);
      gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA, gl.RGBA, gl.UNSIGNED_BYTE, canvas);
      gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.LINEAR);
      gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.LINEAR);
      gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE);
      gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);
      return tex;
    }

    function cachedTexture(key, build) {
      var hit = maskCache.get(key);
      if (hit) { hit.used = frame; return hit; }
      var run = build();
      if (!run) return null;
      var tex = upload(run.canvas);
      var entry = { tex: tex, x0: run.x0, y0: run.y0, w: run.w, h: run.h, used: frame,
        pixels: run.w * run.h };
      maskCache.set(key, entry);
      maskPixels += entry.pixels;
      while (maskCache.size > 128 || maskPixels > 24000000) {
        var oldest = maskCache.keys().next().value;
        var evicted = maskCache.get(oldest);
        maskCache.delete(oldest);
        maskPixels -= evicted.pixels;
        gl.deleteTexture(evicted.tex);
      }
      return entry;
    }

    function gradientLut(stops) {
      var key = stops.map(function (stop) {
        return stop.offset + ":" + stop.color.join(",");
      }).join("|");
      var hit = lutCache.get(key);
      if (hit) return hit;
      var canvas = document.createElement("canvas");
      canvas.width = 1024;
      canvas.height = 1;
      var lctx = canvas.getContext("2d", { willReadFrequently: true });
      var grad = lctx.createLinearGradient(0, 0, 1024, 0);
      for (var i = 0; i < stops.length; i++) {
        var c = stops[i].color;
        grad.addColorStop(Math.max(0, Math.min(1, stops[i].offset)),
          "rgba(" + c[0] + "," + c[1] + "," + c[2] + "," + (c[3] / 255) + ")");
      }
      lctx.fillStyle = grad;
      lctx.fillRect(0, 0, 1024, 1);
      var entry = { tex: upload(canvas) };
      lutCache.set(key, entry);
      if (lutCache.size > 64) {
        var oldest = lutCache.keys().next().value;
        var evicted = lutCache.get(oldest);
        lutCache.delete(oldest);
        gl.deleteTexture(evicted.tex);
      }
      return entry;
    }

    function applyClip() {
      flush();
      var r = clip.length ? clip[clip.length - 1] : [0, 0, frameW, frameH];
      var x0 = Math.max(0, Math.round(r[0]));
      var y0 = Math.max(0, Math.round(r[1]));
      var x1 = Math.min(frameW, Math.max(x0, Math.round(r[2])));
      var y1 = Math.min(frameH, Math.max(y0, Math.round(r[3])));
      gl.enable(gl.SCISSOR_TEST);
      gl.scissor(x0, frameH - y1, x1 - x0, y1 - y0);
    }

    return {
      begin: function (w, h) {
        frame += 1;
        frameW = w; frameH = h;
        gl.viewport(0, 0, w, h);
        gl.uniform2f(uSize, w, h);
        gl.disable(gl.SCISSOR_TEST);
        gl.clearColor(1, 1, 1, 1);
        gl.clear(gl.COLOR_BUFFER_BIT);
        clip.length = 0;
        count = 0;
      },
      end: function () { flush(); },
      box: function (rect, radii, color, ctm) {
        var t = transformOf(ctm);
        if (!t) return;
        var limit = Math.min(rect[2], rect[3]) / 2;
        var r = radii.map(function (value) { return Math.max(0, Math.min(value, limit)); });
        addInstance(rect, r, color4(color), [0, 0, 0, 0],
          deviceBox(rect[0], rect[1], rect[2], rect[3], ctm, 1.5), t, 0, t.scale,
          [0, 0, 0, 0], 0);
      },
      line: function (x1, y1, x2, y2, width, color, ctm) {
        var t = transformOf(ctm);
        if (!t) return;
        var d1x = ctm[0] * x1 + ctm[2] * y1 + ctm[4];
        var d1y = ctm[1] * x1 + ctm[3] * y1 + ctm[5];
        var d2x = ctm[0] * x2 + ctm[2] * y2 + ctm[4];
        var d2y = ctm[1] * x2 + ctm[3] * y2 + ctm[5];
        var pad = width / 2 * t.scale + 1.5;
        var minX = Math.min(d1x, d2x) - pad, minY = Math.min(d1y, d2y) - pad;
        var maxX = Math.max(d1x, d2x) + pad, maxY = Math.max(d1y, d2y) + pad;
        addInstance([0, 0, 0, 0], [0, 0, 0, 0], color4(color), [0, 0, 0, 0],
          [minX, minY, maxX - minX, maxY - minY], t, 1, t.scale,
          [x1, y1, x2, y2], width / 2);
      },
      gradient: function (rect, radii, start, end, lut, ctm) {
        var t = transformOf(ctm);
        if (!t) return;
        if (start[0] === end[0] && start[1] === end[1]) return;
        flush();
        gl.activeTexture(gl.TEXTURE1);
        gl.bindTexture(gl.TEXTURE_2D, lut.tex);
        var limit = Math.min(rect[2], rect[3]) / 2;
        var r = radii.map(function (value) { return Math.max(0, Math.min(value, limit)); });
        addInstance(rect, r, [1, 1, 1, 1], start.concat(end),
          deviceBox(rect[0], rect[1], rect[2], rect[3], ctm, 1.5), t, 2, t.scale,
          [0, 0, 0, 0], 0);
        flush();
      },
      mask: function (entry, tint) {
        flush();
        gl.activeTexture(gl.TEXTURE0);
        gl.bindTexture(gl.TEXTURE_2D, entry.tex);
        var inv = { A: 1, B: 0, C: 0, D: 1, bx: -entry.x0, by: -entry.y0, scale: 1 };
        addInstance([0, 0, entry.w, entry.h], [0, 0, 0, 0],
          tint ? color4(tint) : [1, 1, 1, 1],
          [entry.x0, entry.y0, entry.w, entry.h],
          [entry.x0, entry.y0, entry.w, entry.h], inv,
          tint ? 4 : 3, 1, [0, 0, 0, 0], 0);
        flush();
      },
      cachedTexture: cachedTexture,
      gradientLut: gradientLut,
      pushClip: function (x, y, w, h) {
        var r = [x, y, x + w, y + h];
        if (clip.length) {
          var p = clip[clip.length - 1];
          r = [Math.max(p[0], r[0]), Math.max(p[1], r[1]),
               Math.min(p[2], r[2]), Math.min(p[3], r[3])];
        }
        clip.push(r);
        applyClip();
      },
      popClip: function () {
        if (clip.length) {
          clip.pop();
          applyClip();
        }
      }
    };
  }

  function renderCanvas() {
    if (!wasm) return;
    var rect = canvas.getBoundingClientRect();
    if (rect.width < 1 || rect.height < 1) return;
    var dpr = Math.max(1, Math.min(3, window.devicePixelRatio || 1));
    var w = Math.round(rect.width * dpr), h = Math.round(rect.height * dpr);
    if (canvas.width !== w || canvas.height !== h) { canvas.width = w; canvas.height = h; }
    window.__r10dbg = {disabled: disabledDemo, has: !!wasm.demo_set_disabled};
    if (wasm.demo_set_disabled) wasm.demo_set_disabled(disabledDemo ? 1 : 0);
    if (wasm.demo_get_disabled) window.__r10disabled = wasm.demo_get_disabled();
    if (!wasm.demo_render(slugId, rect.width, rect.height, dpr)) return;
    wantsFrame = !!(wasm.demo_needs_frame && wasm.demo_needs_frame());
    syncTextInput();
    var commandLen = wasm.demo_command_len();
    if (!commandLen) return;
    var commandPtr = wasm.demo_command_ptr();
    var bytes = new Uint8Array(wasm.memory.buffer, commandPtr, commandLen);
    var view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
    var state = { off: 0 };
    var ctm = [dpr, 0, 0, dpr, 0, 0];
    if (window.__traceCommands) { window.__lastCommands = []; }
    var ctmStack = [];
    var scissorDepth = 0;
    pixelWidth = w; pixelHeight = h;

    // The frame is composited by canvas2D in display-list order. This keeps
    // panels, text, scissor and transforms in the same z-order as the CPU path.
    ctx.setTransform(1, 0, 0, 1, 0, 0);
    ctx.clearRect(0, 0, w, h);
    applyCtm(ctm);
    ctx.globalAlpha = 1;
    ctx.globalCompositeOperation = "source-over";
    // MyGo's icon SVGs stroke with round caps and joins (helpers.go icon()).
    ctx.lineCap = "round";
    ctx.lineJoin = "round";
    // The wasm display list has no background command; use the same backdrop
    // color the CPU path used before this renderer took over.
    ctx.fillStyle = "#ffffff";
    ctx.fillRect(0, 0, rect.width, rect.height);

    while (state.off < commandLen) {
      var tag = bytes[state.off++];
      if (tag === 0) { // Rect
        var x = view.getFloat32(state.off, true); state.off += 4;
        var y = view.getFloat32(state.off, true); state.off += 4;
        var rw = view.getFloat32(state.off, true); state.off += 4;
        var rh = view.getFloat32(state.off, true); state.off += 4;
        var color = readColor(bytes, state);
        var box = snapRect(ctm, x, y, rw, rh, dpr);
        if (window.__lastCommands) window.__lastCommands.push({ t: "Rect", raw: [x, y, rw, rh], box: box, ctm: ctm.slice(), color: color });
        ctx.fillStyle = rgba(color);
        ctx.fillRect(box[0], box[1], box[2], box[3]);
      } else if (tag === 1) { // RoundedRect
        var x = view.getFloat32(state.off, true); state.off += 4;
        var y = view.getFloat32(state.off, true); state.off += 4;
        var rw = view.getFloat32(state.off, true); state.off += 4;
        var rh = view.getFloat32(state.off, true); state.off += 4;
        var tl = view.getFloat32(state.off, true); state.off += 4;
        var tr = view.getFloat32(state.off, true); state.off += 4;
        var br = view.getFloat32(state.off, true); state.off += 4;
        var bl = view.getFloat32(state.off, true); state.off += 4;
        var color = readColor(bytes, state);
        if (rw > 0 && rh > 0) {
          var box = snapRect(ctm, x, y, rw, rh, dpr);
          if (window.__lastCommands) window.__lastCommands.push({ t: "RR", raw: [x, y, rw, rh], box: box, ctm: ctm.slice(), color: color });
          roundedPath(ctx, box[0], box[1], box[2], box[3], [tl, tr, br, bl]);
          ctx.fillStyle = rgba(color);
          ctx.fill();
        }
      } else if (tag === 2) { // Circle
        var cx = view.getFloat32(state.off, true); state.off += 4;
        var cy = view.getFloat32(state.off, true); state.off += 4;
        var radius = view.getFloat32(state.off, true); state.off += 4;
        var color = readColor(bytes, state);
        if (radius > 0) {
          // A circle is MyGo's fill of its box with every corner at half its
          // short side: the box snaps to whole device pixels before the
          // radii apply, so an unsnapped arc lands half a pixel off the
          // grid its ring partners sit on.
          var circleBox = snapRect(ctm, cx - radius, cy - radius, radius * 2, radius * 2, dpr);
          var circleR = Math.min(circleBox[2], circleBox[3]) / 2;
          if (window.__lastCommands) window.__lastCommands.push({ t: "Circle", raw: [cx, cy, radius], box: circleBox, ctm: ctm.slice(), color: color });
          roundedPath(ctx, circleBox[0], circleBox[1], circleBox[2], circleBox[3], [circleR, circleR, circleR, circleR]);
          ctx.fillStyle = rgba(color);
          ctx.fill();
        }
      } else if (tag === 3) { // Line
        var x1 = view.getFloat32(state.off, true); state.off += 4;
        var y1 = view.getFloat32(state.off, true); state.off += 4;
        var x2 = view.getFloat32(state.off, true); state.off += 4;
        var y2 = view.getFloat32(state.off, true); state.off += 4;
        var width = view.getFloat32(state.off, true); state.off += 4;
        var color = readColor(bytes, state);
        if (width > 0) {
          // Axis-aligned odd-width lines sit half a pixel off the grid, so
          // shift them onto whole pixels as MyGo's filled borders do.
          // A stroke centers on its path, so the rule covers whole device
          // pixels only when that center lands on a pixel's middle (an odd
          // device width) or on its boundary (an even one). Snap the center
          // in device space, as MyGo's filled borders land.
          var horizontal = Math.abs(y2 - y1) < 0.01;
          var vertical = Math.abs(x2 - x1) < 0.01;
          var sx1 = x1, sy1 = y1, sx2 = x2, sy2 = y2;
          if (axisAligned(ctm)) {
            if (horizontal) {
              sy1 = sy2 = snapRule(y1, ctm[5], ctm[3], width, dpr);
              sx1 = snapAxis(x1, ctm[4], ctm[0]);
              sx2 = snapAxis(x2, ctm[4], ctm[0]);
            } else if (vertical) {
              sx1 = sx2 = snapRule(x1, ctm[4], ctm[0], width, dpr);
              sy1 = snapAxis(y1, ctm[5], ctm[3]);
              sy2 = snapAxis(y2, ctm[5], ctm[3]);
            }
          }
          if (window.__lastCommands) window.__lastCommands.push({ t: "Line", raw: [x1, y1, x2, y2, width], snapped: [sx1, sy1, sx2, sy2], ctm: ctm.slice(), color: color });
          ctx.beginPath();
          ctx.moveTo(sx1, sy1);
          ctx.lineTo(sx2, sy2);
          ctx.lineWidth = width;
          ctx.strokeStyle = rgba(color);
          ctx.stroke();
        }
      } else if (tag === 4) { // Text
        var tx = view.getFloat32(state.off, true); state.off += 4;
        var ty = view.getFloat32(state.off, true); state.off += 4;
        var fontSize = view.getFloat32(state.off, true); state.off += 4;
        var weight = view.getUint16(state.off, true); state.off += 2;
        var italic = bytes[state.off++] === 1;
        var mono = bytes[state.off++] === 1;
        var color = readColor(bytes, state);
        var maxWidth = view.getFloat32(state.off, true); state.off += 4;
        /* alignment = */ state.off += 1;
        var hasFamily = bytes[state.off++];
        var family = null;
        if (hasFamily) {
          var flen = view.getUint32(state.off, true); state.off += 4;
          family = new TextDecoder().decode(bytes.subarray(state.off, state.off + flen));
          state.off += flen;
        }
        var tlen = view.getUint32(state.off, true); state.off += 4;
        var str = new TextDecoder().decode(bytes.subarray(state.off, state.off + tlen));
        state.off += tlen;
        // x already contains the alignment offset; textAlign stays start.
        // MyGo draws text with its font's ascent inside the line box, so
        // the baseline sits 0.93 of the size below the top; alphabetic
        // baseline keeps the glyph feet on the same row the CPU path and
        // the source page put them.
        ctx.textBaseline = "alphabetic";
        ctx.textAlign = "start";
        // The same family stack MyGo's browser canvas uses
        // (internal/text/canvas_js.go cssFamily), so glyphs match the source.
        ctx.font = (italic ? "italic " : "") + (weight > 0 ? weight : 400) + " " + fontSize +
          "px " + (family ? cssFamilyOf(family) : cssFamilyOf(mono ? "monospace" : null));
        // The transform is part of the record: a widget's text can sit under
        // a translated parent, so its raw coordinates are not page pixels.
        if (window.__lastCommands) window.__lastCommands.push({ t: "Text", str: str, raw: [tx, ty], size: fontSize, weight: weight, ctm: ctm.slice() });
        ctx.fillStyle = rgba(color);
        // MyGo keeps each run's subpixel phase and places its glyph at
        // Floor(pen) (`ui/paint.go`), and its canvas engine rounds the
        // baseline to a whole device pixel (`internal/text/canvas_js.go`
        // baseline). Canvas2D at the exact pen does the same subpixel work
        // horizontally; only the baseline needs the rounding, so glyph rows
        // land where its rasterizer puts them instead of straddling two.
        var baseline = snapDevice(ty + ascentOf(ctx), ctm[5], ctm[3]);
        var lines = maxWidth > 0 && isFinite(maxWidth)
          ? wrapCanvasText(ctx, str, maxWidth)
          : [str];
        if (lines.length > 1) {
          // A bounded run wraps inside its max width; each line advances by
          // the font's line box on the same baseline rounding.
          var lineHeight = lineHeightOf(ctx);
          for (var lineIndex = 0; lineIndex < lines.length; lineIndex++) {
            if (!lines[lineIndex]) continue;
            var lineBaseline = snapDevice(ty + lineIndex * lineHeight + ascentOf(ctx), ctm[5], ctm[3]);
            if (!shadeCanvas) shadeCanvas = document.createElement("canvas");
            var run = window.__plainText ? null
              : buildTextRun(ctx, lines[lineIndex], tx, lineBaseline, color, ctm, shadeCanvas);
            if (run) {
              // Drawn through the shade's coverage, as MyGo does.
              paintTextRun2d(run);
            } else {
              ctx.fillStyle = rgba(color);
              ctx.fillText(lines[lineIndex], tx, lineBaseline);
            }
          }
        } else {
          if (!shadeCanvas) shadeCanvas = document.createElement("canvas");
          var single = window.__plainText ? null
            : buildTextRun(ctx, str, tx, baseline, color, ctm, shadeCanvas);
          if (single) {
            // Drawn through the shade's coverage, as MyGo does.
            paintTextRun2d(single);
          } else {
            ctx.fillStyle = rgba(color);
            ctx.fillText(str, tx, baseline);
          }
        }
      } else if (tag === 5) { // SvgPath
        var x = view.getFloat32(state.off, true); state.off += 4;
        var y = view.getFloat32(state.off, true); state.off += 4;
        var rw = view.getFloat32(state.off, true); state.off += 4;
        var rh = view.getFloat32(state.off, true); state.off += 4;
        var hasFill = bytes[state.off++];
        var fill = hasFill ? readColor(bytes, state) : null;
        var hasStroke = bytes[state.off++];
        var stroke = hasStroke ? readColor(bytes, state) : null;
        var strokeWidth = view.getFloat32(state.off, true); state.off += 4;
        var plen = view.getUint32(state.off, true); state.off += 4;
        var pathData = new TextDecoder().decode(bytes.subarray(state.off, state.off + plen));
        state.off += plen;
        // Path data is in the command's own box; move it into place first.
        // Icons carry their mapping in a TransformPush (their x/y are 0),
        // while framework-painted borders rely on this origin. A path that
        // MyGo draws with round caps and joins says so before its data:
        // its SVG renderer takes each path's own stroke-linecap, while a
        // hand-built Path is round unless it sets them.
        var roundCaps = pathData.indexOf("!round!") === 0;
        if (roundCaps) pathData = pathData.slice(7);
        var path = new Path2D(pathData);
        // An icon is placed by a transform and drawn the way MyGo's
        // drawIcon draws it (`ui/svg.go`): its box snapped to device
        // pixels, the SVG rasterized into a mask of that whole size, and
        // the mask blitted at a whole position. A path that is not an icon
        // keeps its fractional position, as MyGo's FillPath and StrokePath
        // do, so it is left to the plain draw below.
        var icon = axisAligned(ctm) && Math.abs(ctm[0] - dpr) > 1e-4;
        if (icon && rw > 0 && rh > 0) {
          var dx0 = Math.round(x * ctm[0] + ctm[4]);
          var dy0 = Math.round(y * ctm[3] + ctm[5]);
          var mw = Math.round((x + rw) * ctm[0] + ctm[4]) - dx0;
          var mh = Math.round((y + rh) * ctm[3] + ctm[5]) - dy0;
          if (mw > 0 && mh > 0 && mw <= 512 && mh <= 512) {
            if (!iconCanvas) iconCanvas = document.createElement("canvas");
            if (iconCanvas.width < mw || iconCanvas.height < mh) {
              iconCanvas.width = Math.max(iconCanvas.width, mw);
              iconCanvas.height = Math.max(iconCanvas.height, mh);
            }
            var ictx = iconCanvas.getContext("2d");
            ictx.setTransform(1, 0, 0, 1, 0, 0);
            ictx.clearRect(0, 0, mw, mh);
            ictx.setTransform(mw / rw, 0, 0, mh / rh, 0, 0);
            // MyGo's SVG renderer takes each path's own stroke-linecap and
            // stroke-linejoin, which default to butt and miter; the few
            // icons that ask for round carry an explicit request.
            ictx.lineCap = roundCaps ? "round" : "butt";
            ictx.lineJoin = roundCaps ? "round" : "miter";
            if (fill) { ictx.fillStyle = rgba(fill); ictx.fill(path); }
            if (stroke) {
              ictx.lineWidth = strokeWidth;
              ictx.strokeStyle = rgba(stroke);
              ictx.stroke(path);
            }
            if (window.__lastCommands) window.__lastCommands.push({ t: "Svg", raw: [x, y, rw, rh], ctm: ctm.slice(), stroke: stroke, fill: fill, path: pathData.slice(0, 80), mask: [dx0, dy0, mw, mh] });
            ctx.save();
            ctx.setTransform(1, 0, 0, 1, 0, 0);
            ctx.drawImage(iconCanvas, 0, 0, mw, mh, dx0, dy0, mw, mh);
            ctx.restore();
            continue;
          }
        }
        ctx.save();
        // A path's origin is placed as given: a framework border's path is
        // already inset by half a pixel so its one-pixel stroke fills whole
        // pixel cells, and snapping it again would straddle two of them.
        ctx.translate(x, y);
        if (window.__lastCommands) window.__lastCommands.push({ t: "Svg", raw: [x, y, rw, rh], ctm: ctm.slice(), stroke: stroke, fill: fill, path: pathData.slice(0, 80) });
        if (fill) { ctx.fillStyle = rgba(fill); ctx.fill(path); }
        if (stroke) {
          ctx.lineWidth = strokeWidth;
          ctx.strokeStyle = rgba(stroke);
          ctx.stroke(path);
        }
        ctx.restore();
      } else if (tag === 6) { // LinearGradient
        var x = view.getFloat32(state.off, true); state.off += 4;
        var y = view.getFloat32(state.off, true); state.off += 4;
        var rw = view.getFloat32(state.off, true); state.off += 4;
        var rh = view.getFloat32(state.off, true); state.off += 4;
        var sx = view.getFloat32(state.off, true); state.off += 4;
        var sy = view.getFloat32(state.off, true); state.off += 4;
        var ex = view.getFloat32(state.off, true); state.off += 4;
        var ey = view.getFloat32(state.off, true); state.off += 4;
        var rtl = view.getFloat32(state.off, true); state.off += 4;
        var rtr = view.getFloat32(state.off, true); state.off += 4;
        var rbr = view.getFloat32(state.off, true); state.off += 4;
        var rbl = view.getFloat32(state.off, true); state.off += 4;
        var stopCount = view.getUint16(state.off, true); state.off += 2;
        var gradient = ctx.createLinearGradient(x + sx * rw, y + sy * rh, x + ex * rw, y + ey * rh);
        for (var i = 0; i < stopCount; i++) {
          var offset = view.getFloat32(state.off, true); state.off += 4;
          var stopColor = readColor(bytes, state);
          gradient.addColorStop(Math.max(0, Math.min(1, offset)), rgba(stopColor));
        }
        if (window.__lastCommands) window.__lastCommands.push({ t: "Grad", raw: [x, y, rw, rh], rad: [rtl, rtr, rbr, rbl], ctm: ctm.slice() });
        // MyGo's Painter.snap rounds every painted box's edges to whole
        // device pixels, the gradient's included; without it the card's
        // right and bottom edges cover half a pixel and read as a lighter
        // rim (`ui/paint.go` gradient, `Painter.snap`).
        var gsnap = snapRect(ctm, x, y, rw, rh, dpr);
        ctx.fillStyle = gradient;
        if (rtl > 0 || rtr > 0 || rbr > 0 || rbl > 0) {
          // MyGo's FillGradient rounds its box with the theme radius.
          roundedPath(ctx, gsnap[0], gsnap[1], gsnap[2], gsnap[3], [rtl, rtr, rbr, rbl]);
          ctx.fill();
        } else {
          ctx.fillRect(gsnap[0], gsnap[1], gsnap[2], gsnap[3]);
        }
      } else if (tag === 7) { // ScissorPush, physical AABB
        var x = view.getFloat32(state.off, true); state.off += 4;
        var y = view.getFloat32(state.off, true); state.off += 4;
        var rw = view.getFloat32(state.off, true); state.off += 4;
        var rh = view.getFloat32(state.off, true); state.off += 4;
        ctx.save();
        ctx.setTransform(1, 0, 0, 1, 0, 0);
        ctx.beginPath();
        ctx.rect(x, y, Math.max(0, rw), Math.max(0, rh));
        ctx.clip();
        if (window.__lastCommands) window.__lastCommands.push({ t: "Scissor", raw: [x, y, rw, rh], ctm: ctm.slice() });
        applyCtm(ctm);
        scissorDepth += 1;
      } else if (tag === 8) { // ScissorPop
        if (scissorDepth > 0) {
          ctx.restore();
          scissorDepth -= 1;
          applyCtm(ctm);
        }
      } else if (tag === 9) { // TransformPush
        var m = [
          view.getFloat32(state.off, true), view.getFloat32(state.off + 4, true),
          view.getFloat32(state.off + 8, true), view.getFloat32(state.off + 12, true),
          view.getFloat32(state.off + 16, true), view.getFloat32(state.off + 20, true)
        ];
        state.off += 24;
        ctmStack.push(ctm.slice());
        ctm = multiplyCtm(ctm, m);
        ctx.transform(m[0], m[1], m[2], m[3], m[4], m[5]);
      } else if (tag === 10) { // TransformPop
        if (ctmStack.length) {
          ctm = ctmStack.pop();
          applyCtm(ctm);
        }
      } else if (tag === 11) { // Shadow
        var sx = view.getFloat32(state.off, true); state.off += 4;
        var sy = view.getFloat32(state.off, true); state.off += 4;
        var sw = view.getFloat32(state.off, true); state.off += 4;
        var sh = view.getFloat32(state.off, true); state.off += 4;
        var sr = view.getFloat32(state.off, true); state.off += 4;
        var sdx = view.getFloat32(state.off, true); state.off += 4;
        var sdy = view.getFloat32(state.off, true); state.off += 4;
        var sblur = view.getFloat32(state.off, true); state.off += 4;
        var scol = readColor(bytes, state);
        if (window.__lastCommands) window.__lastCommands.push({ t: "Shadow", raw: [sx, sy, sw, sh, sr, sdx, sdy, sblur], color: scol });
        // Canvas2D casts the shadow of the shape it fills, and casts none
        // for a shape the clip removes entirely, so the casting shape is
        // filled here rather than clipped out. The opaque face drawn next
        // covers it; white leaves no dark rim where that face's own edge
        // antialiases, and an OpShadow shows only outside the casting box
        // (`internal/scene/scene.go`).
        if (sw > 0 && sh > 0 && sblur > 0 && scol[3] > 0) {
          ctx.save();
          ctx.shadowColor = rgba(scol);
          // MyGo's blur is the Gaussian's standard deviation in device
          // pixels (`ui/paint.go` shadow: `Blur: sh.blur * p.scale`,
          // `internal/raster/raster.go` shadow: `sigma := op.Blur / 2`),
          // while Canvas2D's shadowBlur is twice its sigma. The transform
          // does not scale it, so scale by dpr here.
          ctx.shadowBlur = sblur * dpr;
          ctx.shadowOffsetX = sdx * dpr;
          ctx.shadowOffsetY = sdy * dpr;
          ctx.beginPath();
          // MyGo snaps the casting box like every other painted box
          // (`ui/paint.go` shadow), so its edge lands on the same device
          // pixels as the face that covers it.
          var sbox = snapRect(ctm, sx, sy, sw, sh, dpr);
          roundedSubpath(ctx, sbox[0], sbox[1], sbox[2], sbox[3], [sr, sr, sr, sr]);
          ctx.fillStyle = "#ffffff";
          ctx.fill();
          ctx.restore();
        }
      } else {
        throw new Error("unknown canvas2D command tag " + tag);
      }
    }
    while (scissorDepth > 0) {
      ctx.restore();
      scissorDepth -= 1;
    }
  }


  // deviceBoxOf returns the device-space AABB of a local box under ctm,
  // grown by margin in device pixels.
  function deviceBoxOf(x, y, w, h, ctm, margin) {
    var minX = Infinity, minY = Infinity, maxX = -Infinity, maxY = -Infinity;
    for (var i = 0; i < 4; i++) {
      var px = (i & 1) ? x + w : x;
      var py = (i & 2) ? y + h : y;
      var dx = ctm[0] * px + ctm[2] * py + ctm[4];
      var dy = ctm[1] * px + ctm[3] * py + ctm[5];
      if (dx < minX) minX = dx;
      if (dy < minY) minY = dy;
      if (dx > maxX) maxX = dx;
      if (dy > maxY) maxY = dy;
    }
    minX -= margin; minY -= margin; maxX += margin; maxY += margin;
    return [minX, minY, maxX - minX, maxY - minY];
  }

  // drawGlIcon blits one SVG icon through the presenter, as MyGo's
  // drawIcon does: the box snapped to device pixels, the SVG rasterized
  // into a mask of that whole size, the mask blitted at a whole position.
  function drawGlIcon(presenter, pathData, roundCaps, fill, stroke, strokeWidth,
                      rw, rh, dx0, dy0, mw, mh) {
    var key = "I|" + pathData + "|" + roundCaps + "|" + (fill ? fill.join(",") : "-") + "|" +
      (stroke ? stroke.join(",") : "-") + "|" + strokeWidth + "|" + rw + "|" + rh + "|" +
      dx0 + "|" + dy0 + "|" + mw + "|" + mh;
    var entry = presenter.cachedTexture(key, function () {
      var c = document.createElement("canvas");
      c.width = mw;
      c.height = mh;
      var ictx = c.getContext("2d");
      ictx.setTransform(1, 0, 0, 1, 0, 0);
      ictx.setTransform(mw / rw, 0, 0, mh / rh, 0, 0);
      ictx.lineCap = roundCaps ? "round" : "butt";
      ictx.lineJoin = roundCaps ? "round" : "miter";
      var p = new Path2D(pathData);
      if (fill) { ictx.fillStyle = rgba(fill); ictx.fill(p); }
      if (stroke) { ictx.lineWidth = strokeWidth; ictx.strokeStyle = rgba(stroke); ictx.stroke(p); }
      return { canvas: c, x0: dx0, y0: dy0, w: mw, h: mh };
    });
    if (entry) presenter.mask(entry, null);
  }

  // drawGlText paints one run through the GPU presenter: the clustered
  // coverage mask when it applies, otherwise a whole-run fillText mask for
  // color glyphs and rotated transforms.
  function drawGlText(mctx, presenter, str, x, baseline, color, ctm) {
    if (!str) return;
    var plain = !!window.__plainText;
    var key = (plain ? "TP|" : "TC|") + mctx.font + "|" + rgba(color) + "|" + str + "|" +
      x + "|" + baseline + "|" + ctm.join(",");
    var entry = presenter.cachedTexture(key, function () {
      var run = plain ? null : buildTextRun(mctx, str, x, baseline, color, ctm, null);
      if (!run) run = buildPlainRun(mctx, str, x, baseline, color, ctm);
      return run;
    });
    if (entry) presenter.mask(entry, null);
  }

  // pathBounds returns the geometry bounds of SVG path data, through the
  // browser's own path parser: charts and hand-built paths carry no
  // viewport box, so the mask has to come from the path itself.
  var bboxCache = new Map();
  var bboxSvg = null;
  var bboxPath = null;
  function pathBounds(pathData) {
    var hit = bboxCache.get(pathData);
    if (hit) return hit;
    if (!bboxSvg) {
      var ns = "http://www.w3.org/2000/svg";
      bboxSvg = document.createElementNS(ns, "svg");
      bboxSvg.setAttribute("width", "0");
      bboxSvg.setAttribute("height", "0");
      bboxSvg.style.cssText = "position:absolute;left:0;top:0;width:0;height:0;overflow:hidden";
      bboxPath = document.createElementNS(ns, "path");
      bboxSvg.appendChild(bboxPath);
      document.body.appendChild(bboxSvg);
    }
    bboxPath.setAttribute("d", pathData);
    var box;
    try { box = bboxPath.getBBox(); } catch (error) { return null; }
    var bounds = { x: box.x, y: box.y, width: box.width, height: box.height };
    if (bboxCache.size > 512) bboxCache.clear();
    bboxCache.set(pathData, bounds);
    return bounds;
  }

  // renderGl paints the display list through the WebGL 2 presenter. It
  // walks the same commands, with the same snapping and tracing, as the
  // Canvas 2D renderer above; text, SVG and shadow coverage masks are
  // cached by content across frames.
  function renderGl() {
    if (!wasm) return;
    var rect = canvas.getBoundingClientRect();
    if (rect.width < 1 || rect.height < 1) return;
    var dpr = Math.max(1, Math.min(3, window.devicePixelRatio || 1));
    var w = Math.round(rect.width * dpr), h = Math.round(rect.height * dpr);
    if (canvas.width !== w || canvas.height !== h) { canvas.width = w; canvas.height = h; }
    window.__r10dbg = {disabled: disabledDemo, has: !!wasm.demo_set_disabled};
    if (wasm.demo_set_disabled) wasm.demo_set_disabled(disabledDemo ? 1 : 0);
    if (wasm.demo_get_disabled) window.__r10disabled = wasm.demo_get_disabled();
    if (!wasm.demo_render(slugId, rect.width, rect.height, dpr)) return;
    wantsFrame = !!(wasm.demo_needs_frame && wasm.demo_needs_frame());
    syncTextInput();
    var commandLen = wasm.demo_command_len();
    if (!commandLen) return;
    var commandPtr = wasm.demo_command_ptr();
    var bytes = new Uint8Array(wasm.memory.buffer, commandPtr, commandLen);
    var view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
    var state = { off: 0 };
    var ctm = [dpr, 0, 0, dpr, 0, 0];
    if (window.__traceCommands) { window.__lastCommands = []; }
    var ctmStack = [];
    var scissorDepth = 0;
    pixelWidth = w; pixelHeight = h;
    glr.begin(w, h);

    while (state.off < commandLen) {
      var tag = bytes[state.off++];
      if (tag === 0) { // Rect
        var x = view.getFloat32(state.off, true); state.off += 4;
        var y = view.getFloat32(state.off, true); state.off += 4;
        var rw = view.getFloat32(state.off, true); state.off += 4;
        var rh = view.getFloat32(state.off, true); state.off += 4;
        var color = readColor(bytes, state);
        var box = snapRect(ctm, x, y, rw, rh, dpr);
        if (window.__lastCommands) window.__lastCommands.push({ t: "Rect", raw: [x, y, rw, rh], box: box, ctm: ctm.slice(), color: color });
        glr.box(box, [0, 0, 0, 0], color, ctm);
      } else if (tag === 1) { // RoundedRect
        var x = view.getFloat32(state.off, true); state.off += 4;
        var y = view.getFloat32(state.off, true); state.off += 4;
        var rw = view.getFloat32(state.off, true); state.off += 4;
        var rh = view.getFloat32(state.off, true); state.off += 4;
        var tl = view.getFloat32(state.off, true); state.off += 4;
        var tr = view.getFloat32(state.off, true); state.off += 4;
        var br = view.getFloat32(state.off, true); state.off += 4;
        var bl = view.getFloat32(state.off, true); state.off += 4;
        var color = readColor(bytes, state);
        if (rw > 0 && rh > 0) {
          var box = snapRect(ctm, x, y, rw, rh, dpr);
          if (window.__lastCommands) window.__lastCommands.push({ t: "RR", raw: [x, y, rw, rh], box: box, ctm: ctm.slice(), color: color });
          glr.box(box, [tl, tr, br, bl], color, ctm);
        }
      } else if (tag === 2) { // Circle
        var cx = view.getFloat32(state.off, true); state.off += 4;
        var cy = view.getFloat32(state.off, true); state.off += 4;
        var radius = view.getFloat32(state.off, true); state.off += 4;
        var color = readColor(bytes, state);
        if (radius > 0) {
          var circleBox = snapRect(ctm, cx - radius, cy - radius, radius * 2, radius * 2, dpr);
          var circleR = Math.min(circleBox[2], circleBox[3]) / 2;
          if (window.__lastCommands) window.__lastCommands.push({ t: "Circle", raw: [cx, cy, radius], box: circleBox, ctm: ctm.slice(), color: color });
          glr.box(circleBox, [circleR, circleR, circleR, circleR], color, ctm);
        }
      } else if (tag === 3) { // Line
        var x1 = view.getFloat32(state.off, true); state.off += 4;
        var y1 = view.getFloat32(state.off, true); state.off += 4;
        var x2 = view.getFloat32(state.off, true); state.off += 4;
        var y2 = view.getFloat32(state.off, true); state.off += 4;
        var width = view.getFloat32(state.off, true); state.off += 4;
        var color = readColor(bytes, state);
        if (width > 0) {
          var horizontal = Math.abs(y2 - y1) < 0.01;
          var vertical = Math.abs(x2 - x1) < 0.01;
          var sx1 = x1, sy1 = y1, sx2 = x2, sy2 = y2;
          if (axisAligned(ctm)) {
            if (horizontal) {
              sy1 = sy2 = snapRule(y1, ctm[5], ctm[3], width, dpr);
              sx1 = snapAxis(x1, ctm[4], ctm[0]);
              sx2 = snapAxis(x2, ctm[4], ctm[0]);
            } else if (vertical) {
              sx1 = sx2 = snapRule(x1, ctm[4], ctm[0], width, dpr);
              sy1 = snapAxis(y1, ctm[5], ctm[3]);
              sy2 = snapAxis(y2, ctm[5], ctm[3]);
            }
          }
          if (window.__lastCommands) window.__lastCommands.push({ t: "Line", raw: [x1, y1, x2, y2, width], snapped: [sx1, sy1, sx2, sy2], ctm: ctm.slice(), color: color });
          glr.line(sx1, sy1, sx2, sy2, width, color, ctm);
        }
      } else if (tag === 4) { // Text
        var tx = view.getFloat32(state.off, true); state.off += 4;
        var ty = view.getFloat32(state.off, true); state.off += 4;
        var fontSize = view.getFloat32(state.off, true); state.off += 4;
        var weight = view.getUint16(state.off, true); state.off += 2;
        var italic = bytes[state.off++] === 1;
        var mono = bytes[state.off++] === 1;
        var color = readColor(bytes, state);
        var maxWidth = view.getFloat32(state.off, true); state.off += 4;
        /* alignment = */ state.off += 1;
        var hasFamily = bytes[state.off++];
        var family = null;
        if (hasFamily) {
          var flen = view.getUint32(state.off, true); state.off += 4;
          family = new TextDecoder().decode(bytes.subarray(state.off, state.off + flen));
          state.off += flen;
        }
        var tlen = view.getUint32(state.off, true); state.off += 4;
        var str = new TextDecoder().decode(bytes.subarray(state.off, state.off + tlen));
        state.off += tlen;
        ctx.textBaseline = "alphabetic";
        ctx.textAlign = "start";
        ctx.font = (italic ? "italic " : "") + (weight > 0 ? weight : 400) + " " + fontSize +
          "px " + (family ? cssFamilyOf(family) : cssFamilyOf(mono ? "monospace" : null));
        if (window.__lastCommands) window.__lastCommands.push({ t: "Text", str: str, raw: [tx, ty], size: fontSize, weight: weight, ctm: ctm.slice() });
        var baseline = snapDevice(ty + ascentOf(ctx), ctm[5], ctm[3]);
        var lines = maxWidth > 0 && isFinite(maxWidth)
          ? wrapCanvasText(ctx, str, maxWidth)
          : [str];
        if (lines.length > 1) {
          var lineHeight = lineHeightOf(ctx);
          for (var lineIndex = 0; lineIndex < lines.length; lineIndex++) {
            if (!lines[lineIndex]) continue;
            var lineBaseline = snapDevice(ty + lineIndex * lineHeight + ascentOf(ctx), ctm[5], ctm[3]);
            drawGlText(ctx, glr, lines[lineIndex], tx, lineBaseline, color, ctm);
          }
        } else {
          drawGlText(ctx, glr, str, tx, baseline, color, ctm);
        }
      } else if (tag === 5) { // SvgPath
        var x = view.getFloat32(state.off, true); state.off += 4;
        var y = view.getFloat32(state.off, true); state.off += 4;
        var rw = view.getFloat32(state.off, true); state.off += 4;
        var rh = view.getFloat32(state.off, true); state.off += 4;
        var hasFill = bytes[state.off++];
        var fill = hasFill ? readColor(bytes, state) : null;
        var hasStroke = bytes[state.off++];
        var stroke = hasStroke ? readColor(bytes, state) : null;
        var strokeWidth = view.getFloat32(state.off, true); state.off += 4;
        var plen = view.getUint32(state.off, true); state.off += 4;
        var pathData = new TextDecoder().decode(bytes.subarray(state.off, state.off + plen));
        state.off += plen;
        var roundCaps = pathData.indexOf("!round!") === 0;
        if (roundCaps) pathData = pathData.slice(7);
        var icon = axisAligned(ctm) && Math.abs(ctm[0] - dpr) > 1e-4;
        if (icon && rw > 0 && rh > 0) {
          var dx0 = Math.round(x * ctm[0] + ctm[4]);
          var dy0 = Math.round(y * ctm[3] + ctm[5]);
          var mw = Math.round((x + rw) * ctm[0] + ctm[4]) - dx0;
          var mh = Math.round((y + rh) * ctm[3] + ctm[5]) - dy0;
          if (mw > 0 && mh > 0 && mw <= 512 && mh <= 512) {
            if (window.__lastCommands) window.__lastCommands.push({ t: "Svg", raw: [x, y, rw, rh], ctm: ctm.slice(), stroke: stroke, fill: fill, path: pathData.slice(0, 80), mask: [dx0, dy0, mw, mh] });
            drawGlIcon(glr, pathData, roundCaps, fill, stroke, strokeWidth, rw, rh, dx0, dy0, mw, mh);
            continue;
          }
        }
        // A path that is not an icon keeps its fractional position and its
        // own stroke caps: rasterize it into a device-space mask.
        if (window.__lastCommands) window.__lastCommands.push({ t: "Svg", raw: [x, y, rw, rh], ctm: ctm.slice(), stroke: stroke, fill: fill, path: pathData.slice(0, 80) });
        var mat = multiplyCtm(ctm, [1, 0, 0, 1, x, y]);
        var bounds = pathBounds(pathData);
        if (bounds && (bounds.width > 0 || bounds.height > 0)) {
          var pad = (stroke ? strokeWidth / 2 : 0) + 1;
          var dev = deviceBoxOf(bounds.x, bounds.y, bounds.width, bounds.height, mat, pad);
          var px0 = Math.floor(dev[0]);
          var py0 = Math.floor(dev[1]);
          var pw = Math.ceil(dev[0] + dev[2]) - px0;
          var ph = Math.ceil(dev[1] + dev[3]) - py0;
          if (pw > 0 && ph > 0 && pw <= 4096 && ph <= 4096) {
            var key = "S|" + pathData + "|" + (fill ? fill.join(",") : "-") + "|" +
              (stroke ? stroke.join(",") : "-") + "|" + strokeWidth + "|" + roundCaps + "|" +
              mat.join(",") + "|" + px0 + "," + py0 + "," + pw + "," + ph;
            var entry = glr.cachedTexture(key, function () {
              var c = document.createElement("canvas");
              c.width = pw;
              c.height = ph;
              var ictx = c.getContext("2d");
              ictx.setTransform(1, 0, 0, 1, 0, 0);
              ictx.setTransform(mat[0], mat[1], mat[2], mat[3], mat[4] - px0, mat[5] - py0);
              ictx.lineCap = "round";
              ictx.lineJoin = "round";
              var p = new Path2D(pathData);
              if (fill) { ictx.fillStyle = rgba(fill); ictx.fill(p); }
              if (stroke) { ictx.lineWidth = strokeWidth; ictx.strokeStyle = rgba(stroke); ictx.stroke(p); }
              return { canvas: c, x0: px0, y0: py0, w: pw, h: ph };
            });
            if (entry) glr.mask(entry, null);
          }
        }
      } else if (tag === 6) { // LinearGradient
        var x = view.getFloat32(state.off, true); state.off += 4;
        var y = view.getFloat32(state.off, true); state.off += 4;
        var rw = view.getFloat32(state.off, true); state.off += 4;
        var rh = view.getFloat32(state.off, true); state.off += 4;
        var gsx = view.getFloat32(state.off, true); state.off += 4;
        var gsy = view.getFloat32(state.off, true); state.off += 4;
        var gex = view.getFloat32(state.off, true); state.off += 4;
        var gey = view.getFloat32(state.off, true); state.off += 4;
        var rtl = view.getFloat32(state.off, true); state.off += 4;
        var rtr = view.getFloat32(state.off, true); state.off += 4;
        var rbr = view.getFloat32(state.off, true); state.off += 4;
        var rbl = view.getFloat32(state.off, true); state.off += 4;
        var stopCount = view.getUint16(state.off, true); state.off += 2;
        var stops = [];
        for (var i = 0; i < stopCount; i++) {
          var offset = view.getFloat32(state.off, true); state.off += 4;
          var stopColor = readColor(bytes, state);
          stops.push({ offset: Math.max(0, Math.min(1, offset)), color: stopColor });
        }
        if (window.__lastCommands) window.__lastCommands.push({ t: "Grad", raw: [x, y, rw, rh], rad: [rtl, rtr, rbr, rbl], ctm: ctm.slice() });
        var gsnap = snapRect(ctm, x, y, rw, rh, dpr);
        var lut = glr.gradientLut(stops);
        glr.gradient(gsnap, [rtl, rtr, rbr, rbl],
          [x + gsx * rw, y + gsy * rh], [x + gex * rw, y + gey * rh], lut, ctm);
      } else if (tag === 7) { // ScissorPush, physical AABB
        var x = view.getFloat32(state.off, true); state.off += 4;
        var y = view.getFloat32(state.off, true); state.off += 4;
        var rw = view.getFloat32(state.off, true); state.off += 4;
        var rh = view.getFloat32(state.off, true); state.off += 4;
        if (window.__lastCommands) window.__lastCommands.push({ t: "Scissor", raw: [x, y, rw, rh], ctm: ctm.slice() });
        glr.pushClip(x, y, Math.max(0, rw), Math.max(0, rh));
        scissorDepth += 1;
      } else if (tag === 8) { // ScissorPop
        if (scissorDepth > 0) {
          glr.popClip();
          scissorDepth -= 1;
        }
      } else if (tag === 9) { // TransformPush
        var m = [
          view.getFloat32(state.off, true), view.getFloat32(state.off + 4, true),
          view.getFloat32(state.off + 8, true), view.getFloat32(state.off + 12, true),
          view.getFloat32(state.off + 16, true), view.getFloat32(state.off + 20, true)
        ];
        state.off += 24;
        ctmStack.push(ctm.slice());
        ctm = multiplyCtm(ctm, m);
      } else if (tag === 10) { // TransformPop
        if (ctmStack.length) ctm = ctmStack.pop();
      } else if (tag === 11) { // Shadow
        var sx = view.getFloat32(state.off, true); state.off += 4;
        var sy = view.getFloat32(state.off, true); state.off += 4;
        var sw = view.getFloat32(state.off, true); state.off += 4;
        var sh = view.getFloat32(state.off, true); state.off += 4;
        var sr = view.getFloat32(state.off, true); state.off += 4;
        var sdx = view.getFloat32(state.off, true); state.off += 4;
        var sdy = view.getFloat32(state.off, true); state.off += 4;
        var sblur = view.getFloat32(state.off, true); state.off += 4;
        var scol = readColor(bytes, state);
        if (window.__lastCommands) window.__lastCommands.push({ t: "Shadow", raw: [sx, sy, sw, sh, sr, sdx, sdy, sblur], color: scol });
        if (sw > 0 && sh > 0 && sblur > 0 && scol[3] > 0) {
          var sbox = snapRect(ctm, sx, sy, sw, sh, dpr);
          var margin = sblur * dpr * 2 + Math.max(Math.abs(sdx * dpr), Math.abs(sdy * dpr)) + 3;
          var dev = deviceBoxOf(sbox[0], sbox[1], sbox[2], sbox[3], ctm, margin);
          var hx0 = Math.floor(dev[0]);
          var hy0 = Math.floor(dev[1]);
          var hw = Math.ceil(dev[0] + dev[2]) - hx0;
          var hh = Math.ceil(dev[1] + dev[3]) - hy0;
          if (hw > 0 && hh > 0 && hw <= 4096 && hh <= 4096) {
            var key = "H|" + sbox.join(",") + "|" + sdx + "|" + sdy + "|" + sblur + "|" +
              scol.join(",") + "|" + ctm.join(",") + "|" + hx0 + "," + hy0 + "," + hw + "," + hh;
            var entry = glr.cachedTexture(key, function () {
              var c = document.createElement("canvas");
              c.width = hw;
              c.height = hh;
              var ictx = c.getContext("2d");
              ictx.setTransform(1, 0, 0, 1, 0, 0);
              ictx.setTransform(ctm[0], ctm[1], ctm[2], ctm[3], ctm[4] - hx0, ctm[5] - hy0);
              ictx.shadowColor = "rgba(255,255,255,1)";
              ictx.shadowBlur = sblur * dpr;
              ictx.shadowOffsetX = sdx * dpr;
              ictx.shadowOffsetY = sdy * dpr;
              ictx.beginPath();
              roundedSubpath(ictx, sbox[0], sbox[1], sbox[2], sbox[3], [sr, sr, sr, sr]);
              ictx.fillStyle = "#ffffff";
              ictx.fill();
              return { canvas: c, x0: hx0, y0: hy0, w: hw, h: hh };
            });
            if (entry) glr.mask(entry, scol);
          }
        }
      } else {
        throw new Error("unknown WebGL command tag " + tag);
      }
    }
    while (scissorDepth > 0) {
      glr.popClip();
      scissorDepth -= 1;
    }
    glr.end();
  }

  // MyGo moves its textarea to the caret so input methods show candidates
  // where the field is, and switches inputmode so a touch keyboard appears
  // only while a field takes text.
  var lastTextActive = false;
  function syncTextInput() {
    if (!wasm.demo_text_input) return;
    var active = wasm.demo_text_input() === 1;
    if (active !== lastTextActive) {
      lastTextActive = active;
      input.setAttribute("inputmode", active ? "text" : "none");
    }
    if (!active) return;
    input.style.left = wasm.demo_caret_x() + "px";
    input.style.top = wasm.demo_caret_y() + "px";
    input.style.height = Math.max(wasm.demo_caret_height(), 1) + "px";
  }

  // The wasm sets this when the frame it just produced is mid-animation. A
  // static demo is polled every 80 ms to save power; an animation is drawn
  // every frame, or a 100 ms slide would show one intermediate pose and look
  // like a jump.
  var wantsFrame = false;

  function schedule() {
    var now = performance.now();
    if (wantsFrame || now - lastDraw >= 80) { lastDraw = now; render(); }
    requestAnimationFrame(schedule);
  }

  function point(event) {
    var rect = canvas.getBoundingClientRect();
    return { x: event.clientX - rect.left, y: event.clientY - rect.top };
  }

  canvas.addEventListener("pointerdown", function (event) {
    event.preventDefault();
    focusDemo();
    var p = point(event);
    wasm.demo_pointer(1, p.x, p.y);
    render();
  });
  canvas.addEventListener("pointermove", function (event) {
    var p = point(event);
    wasm.demo_pointer(0, p.x, p.y);
  });
  canvas.addEventListener("pointerup", function (event) {
    var p = point(event);
    wasm.demo_pointer(2, p.x, p.y);
    render();
  });
  canvas.addEventListener("pointerleave", function () {
    wasm.demo_pointer(0, -1, -1);
  });
  canvas.addEventListener("contextmenu", function (event) {
    event.preventDefault();
    var p = point(event);
    wasm.demo_secondary(p.x, p.y);
    render();
  });
  canvas.addEventListener("wheel", function (event) {
    event.preventDefault();
    wasm.demo_wheel(event.deltaX, event.deltaY);
    render();
  }, { passive: false });

  // A page can take files dropped on it, but only their names, sizes and
  // bytes — not the path a desktop build reads with os.ReadFile. The demo's
  // drop zone lists what a drop delivered (`demo_drop_files`).
  function firstLine(file) {
    if (!file.size) return Promise.resolve("");
    if (file.type && file.type.indexOf("text/") !== 0 && file.type !== "application/json") {
      return Promise.resolve("");
    }
    return file.slice(0, 4096).text().then(function (text) {
      var line = text.split("\n")[0].replace(/^\s+|\s+$/g, "");
      line = line.replace(/[\u0000-\u0008\u000b-\u001f\u007f]/g, "");
      return line.slice(0, 80);
    }).catch(function () { return ""; });
  }

  function hostDrop(files) {
    Promise.all(files.slice(0, 8).map(function (file) {
      return firstLine(file).then(function (line) {
        return file.name.replace(/[\t\r\n]/g, " ") + "\t" + file.size + "\t" + line;
      });
    })).then(function (rows) {
      var bytes = new TextEncoder().encode(rows.join("\n"));
      if (!wasm.demo_host_buffer || bytes.length > 65536) return;
      var pointer = wasm.demo_host_buffer();
      new Uint8Array(wasm.memory.buffer, pointer, bytes.length).set(bytes);
      wasm.demo_drop_files(bytes.length);
      render();
    });
  }

  canvas.addEventListener("dragover", function (event) {
    event.preventDefault();
    if (event.dataTransfer) event.dataTransfer.dropEffect = "copy";
    if (wasm.demo_drag_hover) wasm.demo_drag_hover(1);
  });
  canvas.addEventListener("dragleave", function () {
    if (wasm.demo_drag_hover) wasm.demo_drag_hover(0);
  });
  canvas.addEventListener("drop", function (event) {
    event.preventDefault();
    if (wasm.demo_drag_hover) wasm.demo_drag_hover(0);
    var files = event.dataTransfer ? Array.prototype.slice.call(event.dataTransfer.files || []) : [];
    if (files.length) hostDrop(files);
  });

  var keys = {
    Tab: 1, Enter: 2, " ": 3, ArrowLeft: 4, ArrowRight: 5, ArrowUp: 6,
    ArrowDown: 7, Escape: 8, Backspace: 9, Delete: 10, Home: 11, End: 12,
    PageUp: 13, PageDown: 14
  };
  function textFieldFocused() {
    return !!(wasm && wasm.demo_text_input && wasm.demo_text_input() === 1);
  }

  function sendText(text) {
    for (var index = 0; index < text.length; index++) {
      var code = text.codePointAt(index);
      if (code > 0xffff) index++;
      wasm.demo_text(code);
    }
    render();
  }

  // Keys go to the textarea so an input method can compose in it; printable
  // characters arrive through `beforeinput`, with the browser keeping the
  // caret and selection the way an editable element does.
  var composing = false;
  input.addEventListener("keydown", function (event) {
    // While an input method owns the keys, leave them to it: arrows move
    // through its candidates, not through the UI.
    if (event.isComposing || event.keyCode === 229) return;
    var code = keys[event.key];
    if (code !== undefined) {
      // Inside a text field a space is text; elsewhere it is the Space key,
      // which pauses Snake and drops a Tetris piece.
      if (event.key === " " && textFieldFocused()) return;
      event.preventDefault();
      wasm.demo_key(code);
      render();
      return;
    }
    if (event.ctrlKey || event.metaKey || event.altKey) return;
    // A printable key is inserted into the textarea; `beforeinput` sends it.
  });

  input.addEventListener("beforeinput", function (event) {
    if (event.isComposing || composing) return;
    if (event.inputType === "insertText" || event.inputType === "insertFromPaste") {
      if (event.data) {
        event.preventDefault();
        sendText(event.data);
      }
    }
  });

  input.addEventListener("compositionstart", function () {
    composing = true;
  });

  input.addEventListener("compositionend", function (event) {
    composing = false;
    input.value = "";
    if (event.data) sendText(event.data);
  });

  function resize() {
    render();
  }
  if (window.ResizeObserver) new ResizeObserver(resize).observe(canvas);
  window.addEventListener("resize", resize);

  // Text advances come from this canvas, the same one that draws the
  // text, so layout and glyphs share one font and one measurement.
  var measureCanvas = document.createElement("canvas");
  var measureCtx = measureCanvas.getContext("2d");
  var measureDecoder = new TextDecoder();
  WebAssembly.instantiateStreaming(fetch("./demo.wasm?v=829908d0dd"), {
    env: {
      myrust_now_ms: function () { return performance.now(); },
      myrust_measure_text: function (ptr, len, size) {
        if (!wasm || !wasm.memory) return 0;
        var text = measureDecoder.decode(new Uint8Array(wasm.memory.buffer, ptr, len));
        measureCtx.font = (size > 0 ? size : 14) +
          'px system-ui, -apple-system, "Segoe UI", Roboto, "Helvetica Neue", Arial, "Noto Sans", sans-serif';
        return measureCtx.measureText(text).width;
      },
      myrust_text_ascent: function (size, flags) {
        var style = (flags & 1 ? "italic " : "") + (flags & 4 ? "bold " : "");
        var family = flags & 2
          ? 'ui-monospace, SFMono-Regular, Menlo, Consolas, "Liberation Mono", monospace'
          : 'system-ui, -apple-system, "Segoe UI", Roboto, "Helvetica Neue", Arial, "Noto Sans", sans-serif';
        measureCtx.font = style + (size > 0 ? size : 14) + 'px ' + family;
        return ascentOf(measureCtx);
      },
      myrust_text_descent: function (size, flags) {
        var style = (flags & 1 ? "italic " : "") + (flags & 4 ? "bold " : "");
        var family = flags & 2
          ? 'ui-monospace, SFMono-Regular, Menlo, Consolas, "Liberation Mono", monospace'
          : 'system-ui, -apple-system, "Segoe UI", Roboto, "Helvetica Neue", Arial, "Noto Sans", sans-serif';
        measureCtx.font = style + (size > 0 ? size : 14) + 'px ' + family;
        var tm = measureCtx.measureText("Hg");
        var descent = tm.fontBoundingBoxDescent;
        if (!(descent > 0)) descent = (size > 0 ? size : 14) * 0.25;
        return descent;
      },
      myrust_text_x_height: function (size, flags) {
        var style = (flags & 1 ? "italic " : "") + (flags & 4 ? "bold " : "");
        var family = flags & 2
          ? 'ui-monospace, SFMono-Regular, Menlo, Consolas, "Liberation Mono", monospace'
          : 'system-ui, -apple-system, "Segoe UI", Roboto, "Helvetica Neue", Arial, "Noto Sans", sans-serif';
        measureCtx.font = style + (size > 0 ? size : 14) + 'px ' + family;
        // MyGo's canvas engine strikes text half an x high, the middle of
        // its lowercase (`internal/text/canvas_js.go` font: strikeTop is
        // half the ascent of an "x").
        return measureCtx.measureText("x").actualBoundingBoxAscent;
      },
      myrust_measure_text_family: function (ptr, len, size, italic, mono, weight, familyPtr, familyLen) {
        if (!wasm || !wasm.memory) return 0;
        var text = measureDecoder.decode(new Uint8Array(wasm.memory.buffer, ptr, len));
        var family = familyLen
          ? measureDecoder.decode(new Uint8Array(wasm.memory.buffer, familyPtr, familyLen))
          : null;
        var style = (italic ? "italic " : "") + (weight > 0 ? weight : 400) + " ";
        measureCtx.font = style + (size > 0 ? size : 14) + "px " + cssFamilyOf(family);
        return measureCtx.measureText(text).width;
      },
      myrust_measure_text_weighted: function (ptr, len, size, italic, mono, weight) {
        if (!wasm || !wasm.memory) return 0;
        var text = measureDecoder.decode(new Uint8Array(wasm.memory.buffer, ptr, len));
        var style = (italic ? "italic " : "") + (weight > 0 ? weight : 400) + " ";
        var family = mono
          ? 'ui-monospace, SFMono-Regular, Menlo, Consolas, "Liberation Mono", monospace'
          : 'system-ui, -apple-system, "Segoe UI", Roboto, "Helvetica Neue", Arial, "Noto Sans", sans-serif';
        measureCtx.font = style + (size > 0 ? size : 14) + 'px ' + family;
        return measureCtx.measureText(text).width;
      },
      myrust_measure_text_styled: function (ptr, len, size, flags) {
        if (!wasm || !wasm.memory) return 0;
        var text = measureDecoder.decode(new Uint8Array(wasm.memory.buffer, ptr, len));
        var style = (flags & 1 ? "italic " : "") + (flags & 4 ? "bold " : "");
        var family = flags & 2
          ? 'ui-monospace, SFMono-Regular, Menlo, Consolas, "Liberation Mono", monospace'
          : 'system-ui, -apple-system, "Segoe UI", Roboto, "Helvetica Neue", Arial, "Noto Sans", sans-serif';
        measureCtx.font = style + (size > 0 ? size : 14) + 'px ' + family;
        return measureCtx.measureText(text).width;
      }
    }
  }).then(function (result) {
    wasm = result.instance.exports;
    window.__demoCanvas = canvas;
    // Debug hook: the commands of the last frame, for pixel-level checks.
    window.__demoCommands = function () { return window.__lastCommands || []; };
    window.__demoSelected = function () { return wasm.demo_selected(); };
    window.__demoTick = function () { return wasm.demo_tick(); };
    window.__demoProbe = function () { return wasm.demo_probe(); };
    window.__demoRender = render;
    var start = function () {
      canvas.setAttribute("data-ready", "true");
      render();
      // One loop: schedule() polls at 80 ms when idle and draws every frame
      // while the wasm is mid-animation. A separate interval would render
      // the same frame twice.
      requestAnimationFrame(schedule);
    };
    if (document.fonts && document.fonts.load) {
      start();
    } else {
      start();
    }
  }).catch(function (error) {
    console.error("MyRust demo failed:", error);
    ctx.fillStyle = "#111827"; ctx.fillRect(0, 0, canvas.width, canvas.height);
    ctx.fillStyle = "#fca5a5"; ctx.font = "14px monospace";
    ctx.fillText("demo failed: " + error.message, 16, 32);
  });
})();
