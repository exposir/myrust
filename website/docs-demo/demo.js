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
  // MyGo rasterizes its canvas with full 8-bit coverage; Chrome's GPU
  // canvas answers with four levels (0, 64, 128, 191, 255) at the edge of a
  // curve, which shows as a stair on every rounded corner. A CPU-backed
  // context takes the analytic path instead and matches MyGo's edges.
  var ctx = canvas.getContext("2d", { willReadFrequently: true });
  var slug = new URLSearchParams(location.search).get("demo") || "tabs";
  var demoIds = { tabs: 0, markdown: 1, views: 2, layout: 3, text: 4, styling: 5, input: 6, "drag-and-drop": 7, "pages-and-navigation": 8, overlays: 9, accessibility: 10, "right-to-left": 11, languages: 12, "drawing-and-animation": 13, transitions: 14, motion: 15, transforms: 16, "custom-widgets": 17, "windows-with-native-ui": 18, testing: 19, inspector: 20, rendering: 21, migration: 22, performance: 23, button: 24, link: 25, "menu-button": 26, "context-menu": 27, toggle: 28, "segmented-control": 29, toolbar: 30, checkbox: 31, switch: 32, radio: 33, select: 34, slider: 35, "range-slider": 36, stepper: 37, rating: 38, "text-input": 39, "text-input-primitives": 40, "number-input": 41, "decimal-input": 42, "search-field": 43, combobox: 44, autocomplete: 45, "token-field": 46, "editable-text": 47, "find-bar": 48, "rich-text-editor": 49, "code-editor": 50, "diff-viewer": 51, calendar: 52, "date-input": 53, "time-input": 54, "color-picker": 55, list: 56, table: 57, tree: 58, outline: 59, "grid-view": 60, "scroll-view": 61, grid: 62, "split-view": 63, collapsible: 64, accordion: 65, form: 66, sidebar: 67, breadcrumbs: 68, "back-and-forward-buttons": 69, dialog: 70, "alert-dialog": 71, popover: 72, tooltip: 73, toast: 74, "progress-bar": 75, spinner: 76, meter: 77, badge: 78, charts: 79, image: 80, icon: 81, avatar: 82, showcase: 83 };
  var slugId = demoIds[slug] !== undefined ? demoIds[slug] : 0;
  var labels = { tabs: "Tabs", markdown: "Markdown", views: "Views", layout: "Layout", text: "Text", styling: "Styling and themes", input: "Input", "drag-and-drop": "Drag and drop", "pages-and-navigation": "Pages and navigation", overlays: "Overlays", accessibility: "Accessibility", "right-to-left": "Right to left", languages: "Languages", "drawing-and-animation": "Drawing and animation", transitions: "Transitions", motion: "Motion", transforms: "Transforms", "custom-widgets": "Custom widgets", "windows-with-native-ui": "Windows with native UI", testing: "Testing", inspector: "Inspector", rendering: "Rendering", migration: "Migration", performance: "Performance", button: "Button", link: "Link", "menu-button": "Menu button", "context-menu": "Context menu", toggle: "Toggle", "segmented-control": "Segmented control", toolbar: "Toolbar", checkbox: "Checkbox", switch: "Switch", radio: "Radio", select: "Select", slider: "Slider", "range-slider": "Range slider", stepper: "Stepper", rating: "Rating", "text-input": "Text input", "text-input-primitives": "Text-input primitives", "number-input": "Number input", "decimal-input": "Decimal input", "search-field": "Search field", combobox: "Combobox", autocomplete: "Autocomplete", "token-field": "Token field", "editable-text": "Editable text", "find-bar": "Find bar", "rich-text-editor": "Rich text editor", "code-editor": "Code editor", "diff-viewer": "Diff viewer", calendar: "Calendar", "date-input": "Date input", "time-input": "Time input", "color-picker": "Color picker", list: "List", table: "Table", tree: "Tree", outline: "Outline", "grid-view": "Grid view", "scroll-view": "Scroll view", grid: "Grid", "split-view": "Split view", collapsible: "Collapsible", accordion: "Accordion", form: "Form", sidebar: "Sidebar", breadcrumbs: "Breadcrumbs", "back-and-forward-buttons": "Back and forward buttons", dialog: "Dialog", "alert-dialog": "Alert dialog", popover: "Popover", tooltip: "Tooltip", toast: "Toast", "progress-bar": "Progress bar", spinner: "Spinner", meter: "Meter", badge: "Badge", charts: "Charts", image: "Image", icon: "Icon", avatar: "Avatar", showcase: "Showcase" };
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
  function textPens(text) {
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
        w[1] = ctx.measureText(cs[0]).width;
      } else {
        w[n] = ctx.measureText(pieces[p]).width;
        if (n > 33) {
          for (var k = 1; k < n; k++) {
            w[k] = w[k - 1] + ctx.measureText(cs[k - 1]).width;
          }
        } else {
          var through = cs[0];
          for (var k = 1; k < n; k++) {
            through += cs[k];
            var prefix = ctx.measureText(through).width;
            var alone = ctx.measureText(cs[k]).width;
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
  // fillTextClustered draws a run cluster by cluster, each at the quarter
  // of a device pixel MyGo's placement picks. It returns false where the
  // run must be drawn whole, with color glyphs or a rotated transform.
  function fillTextClustered(text, x, baseline, color, ctm) {
    if (!(ctm[0] > 0) || !(ctm[3] > 0) || ctm[1] !== 0 || ctm[2] !== 0) return false;
    if (hasColorGlyphs(text)) return false;
    var clusters = textPens(text);
    var sx = ctm[0], offset = ctm[4];
    for (var i = 0; i < clusters.length; i++) {
      var cluster = clusters[i];
      if (!cluster.text.trim()) continue;
      var device = (x + cluster.x) * sx + offset;
      var whole = Math.floor(device);
      var q = Math.floor((device - whole) * 4 + 0.5);
      var origin = whole + (q === 4 ? 1 : q / 4);
      if (!fillTextShaded(cluster.text, (origin - offset) / sx, baseline, color, ctm)) {
        return false;
      }
    }
    return true;
  }
  // fillTextShaded draws text as MyGo's canvas engine does: the glyphs
  // rasterized in the gray of the text's shade, the coverage tinted with
  // the color. It returns false where the text must fill itself, under a
  // rotated transform or with color glyphs.
  function fillTextShaded(text, x, baseline, color, ctm) {
    if (!(ctm[0] > 0) || !(ctm[3] > 0) || ctm[1] !== 0 || ctm[2] !== 0) return false;
    if (hasColorGlyphs(text)) return false;
    var m = ctx.measureText(text);
    var left = x - (m.actualBoundingBoxLeft || 0);
    var right = x + (m.actualBoundingBoxRight || 0);
    var top = baseline - (m.actualBoundingBoxAscent || 0);
    var bottom = baseline + (m.actualBoundingBoxDescent || 0);
    if (!(right > left) || !(bottom > top)) return false;
    // The mask's own grid must land on whole device pixels, or drawing it
    // back resamples the coverage it carries.
    var sx = ctm[0], sy = ctm[3], pad = 2;
    var x0 = Math.floor(left * sx + ctm[4]) - pad;
    var y0 = Math.floor(top * sy + ctm[5]) - pad;
    var w = Math.ceil(right * sx + ctm[4]) + pad - x0;
    var h = Math.ceil(bottom * sy + ctm[5]) + pad - y0;
    if (w > 4096 || h > 4096) return false;
    var off = shadeCanvas;
    if (!off) off = shadeCanvas = document.createElement("canvas");
    if (off.width < w || off.height < h) {
      off.width = Math.max(off.width, w);
      off.height = Math.max(off.height, h);
    }
    // MyGo's engine keeps its scratch canvas in software, for the readback
    // it makes (`internal/text/canvas_js.go` newCanvas: willReadFrequently),
    // whose glyph coverage the hardware canvas rounds differently.
    var octx = off.getContext("2d", { willReadFrequently: true });
    octx.setTransform(1, 0, 0, 1, 0, 0);
    octx.clearRect(0, 0, w, h);
    octx.setTransform(sx, 0, 0, sy, ctm[4] - x0, ctm[5] - y0);
    octx.font = ctx.font;
    octx.textBaseline = "alphabetic";
    octx.textAlign = "start";
    var gray = shadeOf(color);
    octx.fillStyle = "rgb(" + gray + "," + gray + "," + gray + ")";
    octx.fillText(text, x, baseline);
    // Where the gray covers, the color shows: the coverage is the alpha.
    octx.setTransform(1, 0, 0, 1, 0, 0);
    octx.globalCompositeOperation = "source-in";
    octx.fillStyle = rgba(color);
    octx.fillRect(0, 0, w, h);
    octx.globalCompositeOperation = "source-over";
    ctx.drawImage(off, 0, 0, w, h,
      (x0 - ctm[4]) / sx, (y0 - ctm[5]) / sy, w / sx, h / sy);
    return true;
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

  function roundedSubpath(x, y, w, h, radii) {
    var limit = Math.min(w, h) / 2;
    var r = radii.map(function (value) { return Math.max(0, Math.min(value, limit)); });
    if (ctx.roundRect) {
      ctx.roundRect(x, y, w, h, r);
      return;
    }
    ctx.moveTo(x + r[0], y);
    ctx.lineTo(x + w - r[1], y);
    ctx.arcTo(x + w, y, x + w, y + r[1], r[1]);
    ctx.lineTo(x + w, y + h - r[2]);
    ctx.arcTo(x + w, y + h, x + w - r[2], y + h, r[2]);
    ctx.lineTo(x + r[3], y + h);
    ctx.arcTo(x, y + h, x, y + h - r[3], r[3]);
    ctx.lineTo(x, y + r[0]);
    ctx.arcTo(x, y, x + r[0], y, r[0]);
    ctx.closePath();
  }

  function roundedPath(x, y, w, h, radii) {
    ctx.beginPath();
    roundedSubpath(x, y, w, h, radii);
  }

  function render() {
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
          roundedPath(box[0], box[1], box[2], box[3], [tl, tr, br, bl]);
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
          roundedPath(circleBox[0], circleBox[1], circleBox[2], circleBox[3], [circleR, circleR, circleR, circleR]);
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
            if (!window.__plainText && fillTextClustered(lines[lineIndex], tx, lineBaseline, color, ctm)) {
              // Drawn through the shade's coverage, as MyGo does.
            } else {
              ctx.fillStyle = rgba(color);
              ctx.fillText(lines[lineIndex], tx, lineBaseline);
            }
          }
        } else if (!window.__plainText && fillTextClustered(str, tx, baseline, color, ctm)) {
          // Drawn through the shade's coverage, as MyGo does.
        } else {
          ctx.fillStyle = rgba(color);
          ctx.fillText(str, tx, baseline);
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
          roundedPath(gsnap[0], gsnap[1], gsnap[2], gsnap[3], [rtl, rtr, rbr, rbl]);
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
          roundedSubpath(sbox[0], sbox[1], sbox[2], sbox[3], [sr, sr, sr, sr]);
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

  function schedule() {
    var now = performance.now();
    if (now - lastDraw >= 80) { lastDraw = now; render(); }
    requestAnimationFrame(schedule);
  }

  function point(event) {
    var rect = canvas.getBoundingClientRect();
    return { x: event.clientX - rect.left, y: event.clientY - rect.top };
  }

  canvas.addEventListener("pointerdown", function (event) {
    event.preventDefault();
    canvas.focus();
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
  canvas.addEventListener("keydown", function (event) {
    if (keys[event.key] !== undefined) {
      event.preventDefault();
      wasm.demo_key(keys[event.key]);
      render();
      return;
    }
    if (event.key.length === 1 && !event.ctrlKey && !event.metaKey && !event.altKey) {
      event.preventDefault();
      wasm.demo_text(event.key.codePointAt(0));
      render();
    }
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
  WebAssembly.instantiateStreaming(fetch("./demo.wasm?v=2961c99948"), {
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
      setInterval(render, 100);
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
