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
  var wasm = null;
  var lastDraw = 0;
  var pixelWidth = 0, pixelHeight = 0;

  function render() {
    if (!wasm) return;
    var rect = canvas.getBoundingClientRect();
    if (rect.width < 1 || rect.height < 1) return;
    var dpr = Math.max(1, Math.min(3, window.devicePixelRatio || 1));
    var w = Math.round(rect.width * dpr), h = Math.round(rect.height * dpr);
    if (canvas.width !== w || canvas.height !== h) { canvas.width = w; canvas.height = h; }
    var len = wasm.demo_render(slugId, rect.width, rect.height, dpr);
    if (!len) return;
    var ptr = wasm.demo_buffer_ptr();
    var bytes = new Uint8ClampedArray(wasm.memory.buffer, ptr, len);
    var image = new ImageData(bytes, w, h);
    ctx.putImageData(image, 0, 0);
    pixelWidth = w; pixelHeight = h;
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
    canvas.setAttribute("data-ready", "true");
    render();
    setInterval(render, 100);
    requestAnimationFrame(schedule);
  }).catch(function (error) {
    console.error("MyRust demo failed:", error);
    ctx.fillStyle = "#111827"; ctx.fillRect(0, 0, canvas.width, canvas.height);
    ctx.fillStyle = "#fca5a5"; ctx.font = "14px monospace";
    ctx.fillText("demo failed: " + error.message, 16, 32);
  });
})();
