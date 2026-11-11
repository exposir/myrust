/* Site search over the Native UI documentation index. Shared by the homepage
   and the docs page. Requires window.__DOC_ENTRIES__ from docs-entries.js. */
(function () {
  var overlay = document.getElementById("search-overlay");
  var input = document.getElementById("search-input");
  var results = document.getElementById("search-results");
  if (!overlay || !input || !results) return;

  var docsPage = document.body.getAttribute("data-docs-page") === "true";
  var data = window.__DOC_ENTRIES__ || [];
  var active = 0;
  var current = [];

  function lang() {
    if (window.__docLang === "zh" || window.__docLang === "en") return window.__docLang;
    try {
      var stored = localStorage.getItem("myrust-lang");
      if (stored === "zh" || stored === "en") return stored;
    } catch (e) {}
    return "en";
  }

  function field(entry, name) {
    return lang() === "zh" ? entry[name + "Zh"] || entry[name] : entry[name];
  }

  function statusLabel(status) {
    if (lang() === "zh") {
      return status === "done" ? "已实现" : status === "partial" ? "部分实现" : "未实现";
    }
    return status === "done" ? "Implemented" : status === "partial" ? "Partial" : "Not implemented";
  }

  function open() {
    overlay.setAttribute("data-open", "");
    input.value = "";
    render("");
    setTimeout(function () { input.focus(); }, 20);
  }

  function close() {
    overlay.removeAttribute("data-open");
  }

  function go(entry) {
    if (entry.url) {
      location.href = entry.url;
    } else if (docsPage) {
      location.hash = "#" + entry.sectionId;
    } else {
      location.href = "/docs/ui/#" + entry.sectionId;
    }
    close();
  }

  function render(query) {
    var q = query.trim().toLowerCase();
    current = data.filter(function (entry) {
      if (!q) return true;
      var hay = [
        entry.title, entry.titleZh || "",
        entry.section, entry.sectionZh || "",
        entry.text, entry.textZh || "",
      ].join(" ").toLowerCase();
      return hay.indexOf(q) !== -1;
    });
    active = 0;
    results.replaceChildren();
    if (!current.length) {
      var empty = document.createElement("div");
      empty.className = "search-empty";
      empty.textContent = lang() === "zh" ? "没有匹配结果。" : "No matches.";
      results.append(empty);
      return;
    }
    current.forEach(function (entry, index) {
      var button = document.createElement("button");
      button.type = "button";
      button.className = "search-result";
      if (index === active) button.setAttribute("data-active", "");
      var title = document.createElement("span");
      title.className = "search-title";
      title.textContent = field(entry, "title");
      var meta = document.createElement("span");
      meta.className = "search-section";
      meta.textContent = field(entry, "section") + " · " + statusLabel(entry.status);
      button.append(title, meta);
      button.addEventListener("click", function () { go(entry); });
      results.append(button);
    });
  }

  function move(delta) {
    if (!current.length) return;
    active = (active + delta + current.length) % current.length;
    Array.prototype.forEach.call(results.children, function (child, index) {
      if (child.classList && child.classList.contains("search-result")) {
        if (index === active) {
          child.setAttribute("data-active", "");
          child.scrollIntoView({ block: "nearest" });
        } else {
          child.removeAttribute("data-active");
        }
      }
    });
  }

  document.querySelectorAll("[data-search-open]").forEach(function (button) {
    button.addEventListener("click", open);
  });
  input.addEventListener("input", function () { render(input.value); });
  input.addEventListener("keydown", function (event) {
    if (event.key === "ArrowDown") { event.preventDefault(); move(1); }
    else if (event.key === "ArrowUp") { event.preventDefault(); move(-1); }
    else if (event.key === "Enter") {
      event.preventDefault();
      if (current[active]) go(current[active]);
    }
  });
  overlay.addEventListener("click", function (event) { if (event.target === overlay) close(); });
  document.addEventListener("keydown", function (event) {
    var key = event.key.toLowerCase();
    if ((event.metaKey || event.ctrlKey) && key === "k") {
      event.preventDefault();
      open();
    } else if (event.key === "Escape") {
      close();
    }
  });
  window.__openDocsSearch = open;
})();
