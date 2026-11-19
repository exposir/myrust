#!/usr/bin/env python3
"""Build the Native UI index links and the detail pages.

Entry metadata lives in entries.json. New detail entries are inserted into the
index shell and i18n table when they are not already present. Detail pages are
emitted for entries marked `detail`; prev/next span the full list.
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
LEGACY_DETAILS = {"tabs", "markdown", "views", "layout", "text", "styling-and-themes", "input", "drag-and-drop", "pages-and-navigation", "overlays", "accessibility", "right-to-left", "languages", "drawing-and-animation", "shaders", "transitions", "motion", "transforms", "custom-widgets", "windows-with-native-ui", "testing", "inspector", "rendering", "browsers", "migration", "performance", "button", "link", "menu-button", "context-menu", "toggle", "segmented-control", "toolbar", "checkbox", "switch", "radio", "select", "slider", "range-slider", "stepper", "rating", "text-input", "text-input-primitives", "number-input", "decimal-input", "search-field", "combobox", "autocomplete", "token-field", "editable-text", "find-bar", "rich-text-editor", "code-editor", "diff-viewer", "calendar", "date-input", "time-input", "color-picker", "list", "table", "tree", "outline", "grid-view", "scroll-view", "grid", "split-view", "collapsible", "accordion", "form", "web-view", "sidebar", "breadcrumbs", "back-and-forward-buttons", "dialog", "alert-dialog", "popover", "tooltip", "toast", "progress-bar", "spinner", "meter", "badge", "charts", "image", "video", "icon", "avatar"}
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


def ensure_index_entries(index, entries):
    """Adds metadata that is not yet present in the hand-authored index shell."""
    status_label = {
        "done": "Implemented",
        "partial": "Partial",
        "none": "Not implemented",
    }
    new_entries = [
        entry
        for entry in entries
        if entry["detail"] and entry["slug"] not in LEGACY_DETAILS
    ]
    new_side_links = []
    for entry in new_entries:
        href = '/docs/ui/%s/' % entry["slug"]
        side_link = (
            '<li><a class="%s" href="%s"><span data-i18n="t.%s.%d">%s</span>'
            '<span class="side-dot" data-status="%s"></span></a></li>'
            % (
                ITEM_CLASS,
                href,
                entry["section"],
                entry["index"],
                esc(entry["title"]),
                entry["status"],
            )
        )
        new_side_links.append(side_link)
        if href not in index:
            card = (
                '<a href="%s" class="doc-card"><span class="doc-card-title" '
                'data-i18n="t.%s.%d">%s</span><span class="doc-card-description" '
                'data-i18n-html="d.%s.%d">%s</span><span class="doc-status" '
                'data-status="%s" data-i18n="status.%s">%s</span></a>'
                % (
                    href,
                    entry["section"],
                    entry["index"],
                    esc(entry["title"]),
                    entry["section"],
                    entry["index"],
                    inline(entry["description"]),
                    entry["status"],
                    entry["status"],
                    status_label[entry["status"]],
                )
            )
            heading = '<h2 id="%s"' % entry["anchor"]
            section = index.find(heading)
            if section < 0:
                raise ValueError("missing docs section %s" % entry["anchor"])
            cards = index.find('<div class="doc-cards">', section)
            if cards < 0:
                raise ValueError("missing doc cards for %s" % entry["anchor"])
            end = index.find("</div>", cards)
            if end < 0:
                raise ValueError("unterminated doc cards for %s" % entry["anchor"])
            index = index[:end] + card + index[end:]

            nav_heading = '<h4 class="label mb-2 px-2.5" data-i18n="section.%s">' % entry["anchor"]
            nav = index.find(nav_heading)
            if nav < 0:
                raise ValueError("missing docs navigation for %s" % entry["anchor"])
            nav_list = index.find("<ul", nav)
            nav_end = index.find("</ul>", nav_list)
            if nav_list < 0 or nav_end < 0:
                raise ValueError("unterminated docs navigation for %s" % entry["anchor"])
            index = index[:nav_end] + side_link + index[nav_end:]

    match = re.search(r"window\.__DOC_I18N__ = (.*?);\s*</script>", index, re.S)
    if not match:
        raise ValueError("missing docs i18n data")
    i18n = json.loads(match.group(1))
    for entry in entries:
        if not entry["detail"]:
            continue
        for lang, title, description in (
            ("en", entry["title"], entry["description"]),
            ("zh", entry["titleZh"], entry["descriptionZh"]),
        ):
            i18n[lang]["t.%s.%d" % (entry["section"], entry["index"])] = title
            i18n[lang]["d.%s.%d" % (entry["section"], entry["index"])] = inline(description)
    replacement = json.dumps(i18n, ensure_ascii=False, separators=(",", ":"))
    index = index[:match.start(1)] + replacement + index[match.end(1):]
    new_keys = [
        ("t.%s.%d" % (entry["section"], entry["index"]),
         "d.%s.%d" % (entry["section"], entry["index"]))
        for entry in new_entries
    ]
    return index, new_entries, new_side_links, new_keys


def legacy_shell(full_shell, new_side_links, new_i18n_keys):
    """Restores the shared shell for pages that predate newly added docs."""
    header, aside, footer, drawer, mobile, style, i18n, js, search = full_shell
    for link in new_side_links:
        aside = aside.replace(link, "")
    legacy_i18n = json.loads(json.dumps(i18n))
    for lang in ("en", "zh"):
        for title_key, description_key in new_i18n_keys:
            legacy_i18n[lang].pop(title_key, None)
            legacy_i18n[lang].pop(description_key, None)
    return header, aside, footer, drawer, mobile, style, legacy_i18n, js, search


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


ACTIVE_ITEM = ' bg-muted font-medium text-foreground!'
ITEM_CLASS = 'flex items-center gap-1 rounded-md px-2.5 py-1.5 text-muted-foreground transition-colors hover:text-foreground'
TOC_OPEN = '<ul class="my-1.5 ml-3.5 border-l">'


def drop_toc(aside):
    """Drops the section headings hanging under the overview link.

    They are the overview page's own table of contents; a detail page has
    none in the sidebar, as MyGo's DocsNav shows headings of the current
    page alone. The block nests a list of items, so its end is found by
    counting the tags rather than by a regular expression.
    """
    start = aside.find(TOC_OPEN)
    if start < 0:
        return aside
    depth = 0
    index = start
    while index < len(aside):
        if aside.startswith("<ul", index):
            depth += 1
            index += 3
            continue
        if aside.startswith("</ul>", index):
            depth -= 1
            index += 5
            if depth == 0:
                return aside[:start] + aside[index:]
            continue
        index += 1
    return aside


def mark_current(aside, entry):
    """Marks entry's own link in the sidebar, as MyGo's DocsNav does."""
    aside = drop_toc(aside)
    aside = aside.replace(ITEM_CLASS + ACTIVE_ITEM, ITEM_CLASS)
    aside = aside.replace(' data-status="active" aria-current="page"', '')
    # The link's class attribute comes before its href: the active classes
    # join the ones it already has, and the state marks it as current.
    link = 'class="%s" href="/docs/ui/%s/">' % (ITEM_CLASS, entry["slug"])
    marked = 'class="%s%s" href="/docs/ui/%s/" data-status="active" aria-current="page">' % (
        ITEM_CLASS, ACTIVE_ITEM, entry["slug"])
    if link in aside:
        aside = aside.replace(link, marked, 1)
    return aside


def detail_page(entry, prev, next, meta, shell_parts):
    header, aside, footer, drawer, mobile, style, base_i18n, js, search = shell_parts
    aside = aside.replace('href="#', 'href="/docs/ui/#')
    aside = mark_current(aside, entry)
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
            '<a class="ui-preview-open" href="/docs-demo/?demo=%s" target="_blank" rel="noopener" title="Open in a new tab" '
            'aria-label="Open demo in a new tab" data-i18n-aria="preview.open"><svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M15 3h6v6"></path><path d="M10 14 21 3"></path><path d="M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6"></path></svg></a></div>'
            '<iframe class="ui-preview-frame" src="/docs-demo/?demo=%s" title="%s" height="%s" loading="eager"></iframe></div>'
            % (entry["title"], entry["preview"], entry["preview"], entry["title"], entry["height"])
        )
    article = (
        '<header><p class="label" data-i18n="entry.section">%s</p>'
        '<div class="mt-3 flex flex-wrap items-center gap-3"><h1 class="text-3xl font-semibold tracking-[-0.03em] sm:text-4xl" data-i18n="entry.title">%s</h1></div>'
        '<p class="mt-3 text-lg leading-8 text-muted-foreground" data-i18n="entry.desc">%s</p></header>'
        '%s<div class="mt-10"><div class="prose-docs" data-i18n-html="entry.body">%s</div>'
        '<footer class="mt-16"><nav class="grid gap-px overflow-hidden rounded-lg border bg-border sm:grid-cols-2">%s%s</nav>'
        '<a href="https://github.com/exposir/myrust/edit/main/website/docs-src/%s.en.md" class="mt-6 inline-block text-sm text-muted-foreground hover:text-foreground" data-i18n="edit.label">Edit this page on GitHub</a>'
        '</footer></div>'
    ) % (entry["sectionTitle"], entry["title"], entry["description"], preview, en_body, prev_html, next_html, entry["slug"])
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
        '<link rel="stylesheet" href="/docs/ui/docs.css">',
        # docs.css is a generated Tailwind artifact; hand-written overrides
        # live in custom.css so rebuilding the artifact cannot drop them.
        '<link rel="stylesheet" href="/docs/ui/custom.css">', style, "</head>",
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
    index, new_entries, new_side_links, new_i18n_keys = ensure_index_entries(
        INDEX.read_text(), entries
    )
    index = patch_index(index, by_key)
    INDEX.write_text(index)
    shell_parts = shell(index)
    legacy_shell_parts = legacy_shell(shell_parts, new_side_links, new_i18n_keys)
    new_slugs = {entry["slug"] for entry in new_entries}
    legacy_entries = [entry for entry in entries if entry["slug"] not in new_slugs]
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
        if not entry["detail"]:
            continue
        out = DOCS / entry["slug"] / "index.html"
        if entry["slug"] in new_slugs:
            prev = entries[i - 1] if i > 0 else None
            nxt = entries[i + 1] if i + 1 < len(entries) else None
            page_shell = shell_parts
        else:
            legacy_position = legacy_entries.index(entry)
            prev = legacy_entries[legacy_position - 1] if legacy_position > 0 else None
            nxt = (
                legacy_entries[legacy_position + 1]
                if legacy_position + 1 < len(legacy_entries)
                else None
            )
            # Keep each pre-existing page's shared shell/i18n so a metadata
            # change elsewhere does not rewrite the whole legacy corpus.
            page_shell = shell(out.read_text()) if out.exists() else legacy_shell_parts
        out.parent.mkdir(parents=True, exist_ok=True)
        out.write_text(detail_page(entry, prev, nxt, None, page_shell))
        print(out.relative_to(ROOT))


if __name__ == "__main__":
    main()
