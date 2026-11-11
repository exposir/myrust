#!/usr/bin/env python3
"""Build the Native UI index links and the pilot detail pages.

The 87-entry metadata lives in entries.json. Detail pages are emitted only for
entries that have en/zh markdown sources; prev/next are computed across the
full list regardless.
"""

import html
import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
PLACEHOLDER = "place" + "holder"
DOCS = ROOT / "website" / "docs" / "ui"
SRC = ROOT / "website" / "docs-src"
INDEX = DOCS / "index.html"
DETAILS = {"tabs", "markdown", "views", "layout", "text", "styling-and-themes", "input", "drag-and-drop", "pages-and-navigation", "overlays", "accessibility", "right-to-left", "languages", "drawing-and-animation", "shaders", "transitions", "motion", "transforms", "custom-widgets", "windows-with-native-ui", "testing", "inspector", "rendering", "browsers", "migration", "performance", "button", "link", "menu-button", "context-menu", "toggle", "segmented-control", "toolbar", "checkbox", "switch", "radio", "select", "slider", "range-slider", "stepper", "rating", "text-input", "text-input-primitives", "number-input", "decimal-input", "search-field", "combobox", "autocomplete", "token-field", "editable-text", "find-bar", "rich-text-editor", "code-editor", "diff-viewer", "calendar", "date-input", "time-input", "color-picker", "list", "table", "tree", "outline", "grid-view", "scroll-view", "grid", "split-view", "collapsible", "accordion", "form", "web-view", "sidebar", "breadcrumbs", "back-and-forward-buttons", "dialog", "alert-dialog", "popover", "tooltip", "toast", "progress-bar", "spinner", "meter", "badge", "charts", "image", "video", "icon", "avatar"}
COLORS = {
    "comment": ("#6E7781", "#8B949E"),
    "string": ("#0A3069", "#A5D6FF"),
    "keyword": ("#CF222E", "#FF7B72"),
    "number": ("#0550AE", "#79C0FF"),
    "type": ("#953800", "#FFA657"),
    "function": ("#8250DF", "#D2A8FF"),
    "attribute": ("#116329", "#7EE787"),
    "plain": ("#1F2328", "#E6EDF3"),
}
KEYWORDS = {
    "as", "async", "await", "break", "const", "continue", "crate", "else", "enum",
    "extern", "false", "fn", "for", "if", "impl", "in", "let", "loop", "match",
    "mod", "move", "mut", "pub", "ref", "return", "self", "Self", "static",
    "struct", "super", "trait", "true", "type", "unsafe", "use", "where", "while",
    "dyn", "Some", "None", "Ok", "Err",
}
TOKEN = re.compile(
    r"(//[^\n]*)|(\"(?:[^\"\\]|\\.)*\")|(#\[[^\]]*\])|(\b\d+(?:\.\d+)?\b)"
    r"|([A-Za-z_][A-Za-z0-9_]*)|(\s+)|(.)"
)


def esc(text):
    return html.escape(text, quote=True)


def hl(code):
    out = []
    for line in code.split("\n"):
        spans = []
        pos = 0
        while pos < len(line):
            m = TOKEN.match(line, pos)
            text = m.group(0)
            kind = "plain"
            if m.group(1):
                kind = "comment"
            elif m.group(2):
                kind = "string"
            elif m.group(3):
                kind = "attribute"
            elif m.group(4):
                kind = "number"
            elif m.group(5):
                word = m.group(5)
                if word in KEYWORDS:
                    kind = "keyword"
                elif word[:1].isupper():
                    kind = "type"
                elif line[m.end():m.end() + 1] == "(":
                    kind = "function"
            pos = m.end()
            if kind == "plain" and text.isspace():
                spans.append(esc(text))
                continue
            light, dark = COLORS[kind]
            spans.append(
                '<span style="--shiki-light:%s;--shiki-dark:%s">%s</span>'
                % (light, dark, esc(text))
            )
        out.append('<span class="line">%s</span>' % "".join(spans))
    return "\n".join(out)


def inline(text):
    text = esc(text)
    text = re.sub(r"`([^`]+)`", r"<code>\1</code>", text)
    text = re.sub(r"\*\*([^*]+)\*\*", r"<strong>\1</strong>", text)
    text = re.sub(r"\[([^\]]+)\]\(([^)]+)\)", r'<a href="\2">\1</a>', text)
    return text


def markdown(source):
    lines = source.splitlines()
    out = []
    i = 0
    while i < len(lines):
        line = lines[i]
        if line.startswith("```"):
            i += 1
            code = []
            while i < len(lines) and not lines[i].startswith("```"):
                code.append(lines[i])
                i += 1
            i += 1
            out.append(
                '<div class="code-block"><pre class="shiki shiki-themes github-light-default github-dark-default"><code>'
                + hl("\n".join(code))
                + '</code></pre><button type="button" class="copy-button" aria-label="Copy code">'
                '<svg class="icon-copy" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" aria-hidden="true"><path d="M8 8h12a2 2 0 0 1 2 2v10a2 2 0 0 1-2 2H10a2 2 0 0 1-2-2z"></path><path d="M4 16a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h10a2 2 0 0 1 2 2"></path></svg>'
                '<svg class="icon-check" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" aria-hidden="true"><path d="M20 6 9 17l-5-5"></path></svg></button></div>'
            )
            continue
        if line.startswith("### "):
            title = line[4:].strip()
            anchor = re.sub(r"[^a-z0-9]+", "-", title.lower()).strip("-") or "section-%d" % len(out)
            out.append('<h3 id="%s">%s</h3>' % (anchor, inline(title)))
            i += 1
            continue
        if line.startswith("## "):
            title = line[3:].strip()
            anchor = re.sub(r"[^a-z0-9]+", "-", title.lower()).strip("-")
            if not anchor:
                anchor = "section-%d" % len(out)
            out.append('<h2 id="%s"><a href="#%s">%s</a></h2>' % (anchor, anchor, inline(title)))
            i += 1
            continue
        if line.startswith("- "):
            items = []
            while i < len(lines) and lines[i].startswith("- "):
                items.append("<li>%s</li>" % inline(lines[i][2:].strip()))
                i += 1
            out.append("<ul>%s</ul>" % "".join(items))
            continue
        if not line.strip():
            i += 1
            continue
        para = [line.strip()]
        i += 1
        while i < len(lines) and lines[i].strip() and not lines[i].startswith(("#", "### ", "- ", "```")):
            para.append(lines[i].strip())
            i += 1
        out.append("<p>%s</p>" % inline(" ".join(para)))
    return "\n".join(out)


def patch_anchor(chunk, by_key):
    m = re.search(r'data-i18n="t\.([^.]+)\.(\d+)"', chunk)
    if not m:
        return chunk
    entry = by_key.get((m.group(1), int(m.group(2))))
    if entry and entry["detail"]:
        chunk = re.sub(r'href="[^"]*"', 'href="/docs/ui/%s/"' % entry["slug"], chunk, count=1)
    return chunk


def patch_index(index, by_key):
    index = re.sub(
        r'<a href="[^"]*" class="doc-card">.*?</a>',
        lambda m: patch_anchor(m.group(0), by_key),
        index,
        flags=re.S,
    )
    index = re.sub(
        r'<a class="[^"]*" href="[^"]*"><span data-i18n="t\.[^.]+\.\d+">',
        lambda m: patch_anchor(m.group(0), by_key),
        index,
        flags=re.S,
    )
    def detail_disabled(m):
        cls, sid, idx, title = m.group(1), m.group(2), int(m.group(3)), m.group(4)
        entry = by_key.get((sid, idx))
        if not entry or not entry["detail"]:
            return m.group(0)
        cls = cls.replace(" side-disabled", "")
        return (
            '<a class="%s" href="/docs/ui/%s/"><span data-i18n="t.%s.%d">%s</span>'
            '<span class="side-dot" data-status="none"></span></a>'
            % (cls, entry["slug"], sid, idx, title)
        )

    index = re.sub(
        r'<span class="([^"]*side-disabled[^"]*)"[^>]*><span data-i18n="t\.([^.]+)\.(\d+)">([^<]+)</span></span>',
        detail_disabled,
        index,
    )

    # The original generator nested the section list inside the Overview anchor.
    index = re.sub(
        r'(<span data-i18n="aside.overview">Overview</span>)<ul',
        r'\1</a><ul',
        index,
    )
    # The text extraction above may already have added the closing tag.
    index = index.replace('Overview</span></a><ul</a>', 'Overview</span></a><ul')
    # Repair older generated attribute placeholders if present.
    index = index.replace("placeholder", PLACEHOLDER)
    return index


def extract(pattern, text, default=""):
    m = re.search(pattern, text, re.S)
    return m.group(0) if m else default


def shell(index):
    header = extract(r'<header class="frame sticky.*?</header>', index)
    aside = extract(r'<aside class="sticky.*?</aside>', index)
    footer = extract(r'<footer class="frame label.*?</footer>', index)
    drawer = extract(r'<div class="mobile-drawer".*?</div></div>', index)
    mobile = extract(r'<div class="sticky top-14 z-30.*?</div>', index)
    style = extract(r'<style>.*?</style>', index)
    m = re.search(r"window\.__DOC_I18N__ = (.*?);\s*</script>", index, re.S)
    base_i18n = json.loads(m.group(1)) if m else {"en": {}, "zh": {}}
    scripts = re.findall(r"<script>(.*?)</script>", index, re.S)
    js = max(scripts, key=lambda s: len(s) if "function applyLang" in s else 0)
    search = (
        '<div class="search-overlay" id="search-overlay"><div class="search-panel" role="dialog" aria-modal="true" aria-label="Search docs">'
        '<input id="search-input" type="text" %s="Search docs…" autocomplete="off">'
        '<div class="search-results" id="search-results"></div></div></div>'
    ) % PLACEHOLDER
    return header, aside, footer, drawer, mobile, style, base_i18n, js, search


def detail_page(entry, prev, next, meta, shell_parts):
    header, aside, footer, drawer, mobile, style, base_i18n, js, search = shell_parts
    aside = aside.replace('href="#', 'href="/docs/ui/#')
    en_body = markdown((SRC / (entry["slug"] + ".en.md")).read_text())
    zh_body = markdown((SRC / (entry["slug"] + ".zh.md")).read_text())
    i18n = json.loads(json.dumps(base_i18n))
    i18n["en"].update({
        "meta.title": entry["title"] + " · Native UI · MyRust",
        "meta.desc": entry["description"],
        "page.title": entry["title"],
        "entry.section": entry["sectionTitle"],
        "entry.title": entry["title"],
        "entry.desc": entry["description"],
        "entry.body": en_body,
        "preview.title": entry["title"],
        "preview.open": "Open demo in a new tab",
        "prev.label": "Previous",
        "prev.title": prev["title"] if prev else "",
        "next.title": next["title"] if next else "",
        "edit.label": "Edit this page on GitHub",
    })
    i18n["zh"].update({
        "meta.title": entry["titleZh"] + " · 原生界面 · MyRust",
        "meta.desc": entry["descriptionZh"],
        "page.title": entry["titleZh"],
        "entry.section": entry["sectionTitleZh"],
        "entry.title": entry["titleZh"],
        "entry.desc": entry["descriptionZh"],
        "entry.body": zh_body,
        "preview.title": entry["titleZh"],
        "preview.open": "在新标签页打开演示",
        "prev.label": "上一篇",
        "prev.title": prev["titleZh"] if prev else "",
        "next.title": next["titleZh"] if next else "",
        "edit.label": "在 GitHub 上编辑此页",
    })
    status = '<span class="doc-status" data-status="%s" data-i18n="status.%s">%s</span>' % (
        entry["status"], entry["status"], {"done": "Implemented", "partial": "Partial", "none": "Not implemented"}[entry["status"]]
    )
    prev_href = ("/docs/ui/%s/" % prev["slug"]) if prev and prev["detail"] else ""
    next_href = ("/docs/ui/%s/" % next["slug"]) if next and next["detail"] else ""
    if prev and not prev["detail"]:
        prev_href = "/docs/ui/#" + prev["anchor"]
    if next and not next["detail"]:
        next_href = "/docs/ui/#" + next["anchor"]
    prev_html = (
        '<a class="flex flex-col items-start gap-1 bg-background p-4 transition-colors hover:bg-code" href="%s">'
        '<span class="label" data-i18n="prev.label">Previous</span><span class="font-medium" data-i18n="prev.title">%s</span></a>'
        % (prev_href, prev["title"])
        if prev else '<span class="hidden bg-background sm:block"></span>'
    )
    if prev:
        prev_html = prev_html.replace('data-i18n="prev.title"', 'data-i18n="prev.title"')
    next_html = (
        '<a class="flex flex-col items-end gap-1 bg-background p-4 text-right transition-colors hover:bg-code" href="%s">'
        '<span class="label" data-i18n="next.label">Next</span><span class="font-medium" data-i18n="next.title">%s</span></a>'
        % (next_href, next["title"])
        if next else '<span class="hidden bg-background sm:block"></span>'
    )
    preview = ""
    if entry.get("preview"):
        preview = (
            '<div class="ui-preview"><div class="ui-preview-titlebar">'
            '<div class="ui-preview-lights"><i class="close"></i><i class="minimize"></i><i class="zoom"></i></div>'
            '<span class="ui-preview-title" data-i18n="preview.title">%s</span>'
            '<a class="ui-preview-open" href="/docs-demo/?demo=%s" target="_blank" rel="noopener" '
            'aria-label="Open demo in a new tab" data-i18n-aria="preview.open">↗</a></div>'
            '<iframe class="ui-preview-frame" src="/docs-demo/?demo=%s" title="%s" height="%s" loading="eager"></iframe></div>'
            % (entry["title"], entry["preview"], entry["preview"], entry["title"], entry["height"])
        )
    article = (
        '<header><p class="label" data-i18n="entry.section">%s</p>'
        '<div class="mt-3 flex flex-wrap items-center gap-3"><h1 class="text-3xl font-semibold tracking-[-0.03em] sm:text-4xl" data-i18n="entry.title">%s</h1>%s</div>'
        '<p class="mt-3 text-muted-foreground" data-i18n="entry.desc">%s</p></header>'
        '%s<div class="mt-10"><div class="prose-docs" data-i18n-html="entry.body">%s</div>'
        '<footer class="mt-16"><nav class="grid gap-px overflow-hidden rounded-lg border bg-border sm:grid-cols-2">%s%s</nav>'
        '<a href="https://github.com/exposir/myrust/edit/main/website/docs-src/%s.en.md" class="mt-6 inline-block text-sm text-muted-foreground hover:text-foreground" data-i18n="edit.label">Edit this page on GitHub</a>'
        '</footer></div>'
    ) % (entry["sectionTitle"], entry["title"], status, entry["description"], preview, en_body, prev_html, next_html, entry["slug"])
    extra_js = (
        "<script>document.addEventListener('click',function(e){var b=e.target.closest('.copy-button');if(!b)return;"
        "var block=b.closest('.code-block');if(!block)return;var code=block.querySelector('code');if(!code)return;"
        "navigator.clipboard.writeText(code.textContent).then(function(){b.setAttribute('data-copied','');"
        "setTimeout(function(){b.removeAttribute('data-copied')},1600)})});</script>"
    )
    return "\n".join([
        "<!doctype html>", '<html lang="en">', "<head>", '<meta charset="utf-8">',
        '<meta name="viewport" content="width=device-width, initial-scale=1">',
        '<title>%s · Native UI · MyRust</title>' % entry["title"],
        '<link rel="icon" href="data:image/svg+xml,%3Csvg xmlns=%22http://www.w3.org/2000/svg%22 viewBox=%220 0 16 16%22%3E%3Crect width=%2216%22 height=%2216%22 rx=%223%22 fill=%22%2322d3ee%22/%3E%3C/svg%3E">',
        '<meta name="description" content="%s">' % esc(entry["description"]),
        '<script src="/assets/theme-init.js"></script>',
        '<link rel="stylesheet" href="/docs/ui/docs.css">', style, "</head>",
        '<body class="bg-background text-foreground" data-docs-page="true">',
        '<div class="flex min-h-svh flex-col">', header,
        '<div class="frame flex flex-1 flex-col"><div class="flex-1 lg:grid lg:grid-cols-[15rem_minmax(0,1fr)]">',
        aside, '<div class="min-w-0">', mobile,
        '<article class="max-w-[52rem] px-4 pt-10 pb-20 sm:px-10 lg:pt-14">', article,
        "</article></div></div>", footer, search, drawer,
        '<script>window.__DOC_I18N__ = %s;</script>' % json.dumps(i18n, ensure_ascii=False, separators=(",", ":")),
        '<script src="/assets/docs-entries.js"></script>', '<script src="/assets/search.js"></script>',
        "<script>%s</script>" % js.replace("placeholder", PLACEHOLDER), extra_js, "</div></body></html>",
    ])


def main():
    entries = json.loads((SRC / "entries.json").read_text())
    by_key = {(e["section"], e["index"]): e for e in entries}
    index = patch_index(INDEX.read_text(), by_key)
    INDEX.write_text(index)
    shell_parts = shell(index)
    search_data = []
    for entry in entries:
        item = {
            "title": entry["title"],
            "titleZh": entry["titleZh"],
            "section": entry["sectionTitle"],
            "sectionZh": entry["sectionTitleZh"],
            "sectionId": entry["anchor"],
            "status": entry["status"],
            "text": entry["description"].replace("`", ""),
            "textZh": entry["descriptionZh"].replace("`", ""),
        }
        if entry["detail"]:
            item["url"] = "/docs/ui/%s/" % entry["slug"]
        search_data.append(item)
    (ROOT / "website" / "assets" / "docs-entries.js").write_text(
        "/* Search index for the Native UI documentation. */\n"
        "window.__DOC_ENTRIES__ = "
        + json.dumps(search_data, ensure_ascii=False, indent=0)
        + ";\n"
    )
    for i, entry in enumerate(entries):
        if entry["slug"] not in DETAILS:
            continue
        prev = entries[i - 1] if i > 0 else None
        nxt = entries[i + 1] if i + 1 < len(entries) else None
        out = DOCS / entry["slug"] / "index.html"
        out.parent.mkdir(parents=True, exist_ok=True)
        out.write_text(detail_page(entry, prev, nxt, None, shell_parts))
        print(out.relative_to(ROOT))


if __name__ == "__main__":
    main()
