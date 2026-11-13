(function () {
  "use strict";
  var canvas = document.getElementById("canvas");
  var title = document.getElementById("title");
  var ctx = canvas.getContext("2d");
  var slug = new URLSearchParams(location.search).get("demo") || "tabs";
  var demoIds = { tabs: 0, markdown: 1, views: 2, layout: 3, text: 4, styling: 5, input: 6, "drag-and-drop": 7, "pages-and-navigation": 8, overlays: 9, accessibility: 10, "right-to-left": 11, languages: 12, "drawing-and-animation": 13, transitions: 14, motion: 15, transforms: 16, "custom-widgets": 17, "windows-with-native-ui": 18, testing: 19, inspector: 20, rendering: 21, migration: 22, performance: 23, button: 24, link: 25, "menu-button": 26, "context-menu": 27, toggle: 28, "segmented-control": 29, toolbar: 30, checkbox: 31, switch: 32, radio: 33, select: 34, slider: 35, "range-slider": 36, stepper: 37, rating: 38, "text-input": 39, "text-input-primitives": 40, "number-input": 41, "decimal-input": 42, "search-field": 43, combobox: 44, autocomplete: 45, "token-field": 46, "editable-text": 47, "find-bar": 48, "rich-text-editor": 49, "code-editor": 50, "diff-viewer": 51, calendar: 52, "date-input": 53, "time-input": 54, "color-picker": 55, list: 56, table: 57, tree: 58, outline: 59, "grid-view": 60, "scroll-view": 61, grid: 62, "split-view": 63, collapsible: 64, accordion: 65, form: 66, sidebar: 67, breadcrumbs: 68, "back-and-forward-buttons": 69, dialog: 70, "alert-dialog": 71, popover: 72, tooltip: 73, toast: 74, "progress-bar": 75, spinner: 76, meter: 77, badge: 78, charts: 79, image: 80, icon: 81, avatar: 82 };
  var slugId = demoIds[slug] !== undefined ? demoIds[slug] : 0;
  var labels = { tabs: "Tabs", markdown: "Markdown", views: "Views", layout: "Layout", text: "Text", styling: "Styling and themes", input: "Input", "drag-and-drop": "Drag and drop", "pages-and-navigation": "Pages and navigation", overlays: "Overlays", accessibility: "Accessibility", "right-to-left": "Right to left", languages: "Languages", "drawing-and-animation": "Drawing and animation", transitions: "Transitions", motion: "Motion", transforms: "Transforms", "custom-widgets": "Custom widgets", "windows-with-native-ui": "Windows with native UI", testing: "Testing", inspector: "Inspector", rendering: "Rendering", migration: "Migration", performance: "Performance", button: "Button", link: "Link", "menu-button": "Menu button", "context-menu": "Context menu", toggle: "Toggle", "segmented-control": "Segmented control", toolbar: "Toolbar", checkbox: "Checkbox", switch: "Switch", radio: "Radio", select: "Select", slider: "Slider", "range-slider": "Range slider", stepper: "Stepper", rating: "Rating", "text-input": "Text input", "text-input-primitives": "Text-input primitives", "number-input": "Number input", "decimal-input": "Decimal input", "search-field": "Search field", combobox: "Combobox", autocomplete: "Autocomplete", "token-field": "Token field", "editable-text": "Editable text", "find-bar": "Find bar", "rich-text-editor": "Rich text editor", "code-editor": "Code editor", "diff-viewer": "Diff viewer", calendar: "Calendar", "date-input": "Date input", "time-input": "Time input", "color-picker": "Color picker", list: "List", table: "Table", tree: "Tree", outline: "Outline", "grid-view": "Grid view", "scroll-view": "Scroll view", grid: "Grid", "split-view": "Split view", collapsible: "Collapsible", accordion: "Accordion", form: "Form", sidebar: "Sidebar", breadcrumbs: "Breadcrumbs", "back-and-forward-buttons": "Back and forward buttons", dialog: "Dialog", "alert-dialog": "Alert dialog", popover: "Popover", tooltip: "Tooltip", toast: "Toast", "progress-bar": "Progress bar", spinner: "Spinner", meter: "Meter", badge: "Badge", charts: "Charts", image: "Image", icon: "Icon", avatar: "Avatar" };
  var name = labels[slug] || labels.tabs;
  if (window.self !== window.top) {
    document.querySelector(".titlebar").style.display = "none";
    document.body.style.padding = "0";
    document.querySelector(".window").style.border = "0";
    document.querySelector(".window").style.borderRadius = "0";
    document.querySelector(".window").style.boxShadow = "none";
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

  function roundedPath(x, y, w, h, radii) {
    var limit = Math.min(w, h) / 2;
    var r = radii.map(function (value) { return Math.max(0, Math.min(value, limit)); });
    ctx.beginPath();
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
    ctx.lineCap = "butt";
    // The wasm display list has no background command; use the same backdrop
    // color the CPU path used before this renderer took over.
    ctx.fillStyle = "#18181b";
    ctx.fillRect(0, 0, rect.width, rect.height);

    while (state.off < commandLen) {
      var tag = bytes[state.off++];
      if (tag === 0) { // Rect
        var x = view.getFloat32(state.off, true); state.off += 4;
        var y = view.getFloat32(state.off, true); state.off += 4;
        var rw = view.getFloat32(state.off, true); state.off += 4;
        var rh = view.getFloat32(state.off, true); state.off += 4;
        var color = readColor(bytes, state);
        var x0 = Math.round(x * dpr) / dpr;
        var y0 = Math.round(y * dpr) / dpr;
        var x1 = Math.round((x + rw) * dpr) / dpr;
        var y1 = Math.round((y + rh) * dpr) / dpr;
        ctx.fillStyle = rgba(color);
        ctx.fillRect(x0, y0, x1 - x0, y1 - y0);
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
          var x0 = Math.round(x * dpr) / dpr;
          var y0 = Math.round(y * dpr) / dpr;
          var x1 = Math.round((x + rw) * dpr) / dpr;
          var y1 = Math.round((y + rh) * dpr) / dpr;
          roundedPath(x0, y0, x1 - x0, y1 - y0, [tl, tr, br, bl]);
          ctx.fillStyle = rgba(color);
          ctx.fill();
        }
      } else if (tag === 2) { // Circle
        var cx = view.getFloat32(state.off, true); state.off += 4;
        var cy = view.getFloat32(state.off, true); state.off += 4;
        var radius = view.getFloat32(state.off, true); state.off += 4;
        var color = readColor(bytes, state);
        if (radius > 0) {
          ctx.beginPath();
          ctx.arc(cx, cy, radius, 0, Math.PI * 2);
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
          ctx.beginPath();
          ctx.moveTo(x1, y1);
          ctx.lineTo(x2, y2);
          ctx.lineWidth = width;
          ctx.strokeStyle = rgba(color);
          ctx.stroke();
        }
      } else if (tag === 4) { // Text
        var tx = view.getFloat32(state.off, true); state.off += 4;
        var ty = view.getFloat32(state.off, true); state.off += 4;
        var fontSize = view.getFloat32(state.off, true); state.off += 4;
        var weight = view.getUint16(state.off, true); state.off += 2;
        var color = readColor(bytes, state);
        /* max_width = */ state.off += 4;
        /* alignment = */ state.off += 1;
        var tlen = view.getUint32(state.off, true); state.off += 4;
        var str = new TextDecoder().decode(bytes.subarray(state.off, state.off + tlen));
        state.off += tlen;
        // x already contains the alignment offset; textAlign stays start.
        ctx.textBaseline = "top";
        ctx.textAlign = "start";
        ctx.font = (weight >= 600 ? "bold " : "") + fontSize + 'px "Geist Variable", "Geist", sans-serif';
        ctx.fillStyle = rgba(color);
        // Snap the top-left text origin to a physical pixel. At dpr=1 this
        // rounds to whole CSS pixels; at dpr=2 it may use a half-CSS-pixel,
        // which is still an exact device pixel.
        var textX = textHalf
          ? Math.round(tx * dpr) / dpr + 0.5 / dpr
          : Math.round(tx * dpr) / dpr;
        var textY = textHalf
          ? Math.round(ty * dpr) / dpr + 0.5 / dpr
          : Math.round(ty * dpr) / dpr;
        ctx.fillText(str, textX, textY);
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
        // Path data is in the icon view box. The TransformPush command already
        // maps that view box into the widget box, so no extra translate/scale.
        var path = new Path2D(pathData);
        if (fill) { ctx.fillStyle = rgba(fill); ctx.fill(path); }
        if (stroke) {
          ctx.lineWidth = strokeWidth;
          ctx.strokeStyle = rgba(stroke);
          ctx.stroke(path);
        }
      } else if (tag === 6) { // LinearGradient
        var x = view.getFloat32(state.off, true); state.off += 4;
        var y = view.getFloat32(state.off, true); state.off += 4;
        var rw = view.getFloat32(state.off, true); state.off += 4;
        var rh = view.getFloat32(state.off, true); state.off += 4;
        var sx = view.getFloat32(state.off, true); state.off += 4;
        var sy = view.getFloat32(state.off, true); state.off += 4;
        var ex = view.getFloat32(state.off, true); state.off += 4;
        var ey = view.getFloat32(state.off, true); state.off += 4;
        var stopCount = view.getUint16(state.off, true); state.off += 2;
        var gradient = ctx.createLinearGradient(x + sx * rw, y + sy * rh, x + ex * rw, y + ey * rh);
        for (var i = 0; i < stopCount; i++) {
          var offset = view.getFloat32(state.off, true); state.off += 4;
          var stopColor = readColor(bytes, state);
          gradient.addColorStop(Math.max(0, Math.min(1, offset)), rgba(stopColor));
        }
        ctx.fillStyle = gradient;
        ctx.fillRect(x, y, rw, rh);
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

  WebAssembly.instantiateStreaming(fetch("./demo.wasm"), {
    env: { myrust_now_ms: function () { return performance.now(); } }
  }).then(function (result) {
    wasm = result.instance.exports;
    window.__demoCanvas = canvas;
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
      document.fonts.load('16px "Geist Variable"').then(start, start);
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
