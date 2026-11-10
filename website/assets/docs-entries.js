/* Search index for the Native UI documentation. */
window.__DOC_ENTRIES__ = [
{
"title": "Views",
"section": "Guides",
"sectionId": "guides",
"status": "done",
"text": "The per-frame closure, Context, inline state queries, and how a view is rebuilt whenever input or animation asks for a frame."
},
{
"title": "Layout",
"section": "Guides",
"sectionId": "guides",
"status": "done",
"text": "Rows and columns, fixed and filled sizes, padding, margins, gaps, alignment and justification, solved on the CPU."
},
{
"title": "Text",
"section": "Guides",
"sectionId": "guides",
"status": "done",
"text": "Text with font size, weight, color, wrap width and alignment, measured and wrapped without shaping or kerning."
},
{
"title": "Styling and themes",
"section": "Guides",
"sectionId": "guides",
"status": "done",
"text": "Colors, spacing and type tokens from Theme, light/dark detection, and raw drawing commands for backgrounds, borders and gradients."
},
{
"title": "Input",
"section": "Guides",
"sectionId": "guides",
"status": "done",
"text": "The pointer, hover and press, Tab focus traversal, keyboard activation, single-line text entry, and wheel scrolling."
},
{
"title": "Drag and drop",
"section": "Guides",
"sectionId": "guides",
"status": "none",
"text": "Values dragged within the window, and rows reordered by dragging. Native UI does not take drops; file drops reach WebView windows only."
},
{
"title": "Pages and navigation",
"section": "Guides",
"sectionId": "guides",
"status": "none",
"text": "A router of pages with a history, layouts, transitions, and going back and forward."
},
{
"title": "Overlays",
"section": "Guides",
"sectionId": "guides",
"status": "none",
"text": "What dialogs, popovers and the layer above the window share: the focus, Escape, and what is behind them."
},
{
"title": "Accessibility",
"section": "Guides",
"sectionId": "guides",
"status": "partial",
"text": "Semantic nodes are emitted for labels, buttons, fields, choices, sliders, scroll views and groups; no AT-SPI, UIA or NSAccessibility bridge is connected yet."
},
{
"title": "Right to left",
"section": "Guides",
"sectionId": "guides",
"status": "none",
"text": "Interfaces laid out from the right for Arabic and Hebrew, logical edges, and what mirrors."
},
{
"title": "Languages",
"section": "Guides",
"sectionId": "guides",
"status": "none",
"text": "The window's language and regional formats, numbers and dates written in them, and the controls' own strings."
},
{
"title": "Drawing and animation",
"section": "Guides",
"sectionId": "guides",
"status": "done",
"text": "Raw DisplayList commands, and Animation and Easing values driven from the frame clock."
},
{
"title": "Shaders",
"section": "Guides",
"sectionId": "guides",
"status": "none",
"text": "Pixels computed on the GPU by shaders."
},
{
"title": "Transitions",
"section": "Guides",
"sectionId": "guides",
"status": "none",
"text": "Elements that move, resize, recolor and shade smoothly as the layout changes."
},
{
"title": "Motion",
"section": "Guides",
"sectionId": "guides",
"status": "partial",
"text": "Easing and per-frame animate values are built in; springs, keyframes and timelines are not."
},
{
"title": "Transforms",
"section": "Guides",
"sectionId": "guides",
"status": "partial",
"text": "Display-list transform and scissor commands rasterize; pointer hit-testing and text input do not follow them yet."
},
{
"title": "Custom widgets",
"section": "Guides",
"sectionId": "guides",
"status": "partial",
"text": "Containers plus drawing commands can compose custom visuals; there is no widget-base protocol to reuse behavior."
},
{
"title": "Windows with native UI",
"section": "Guides",
"sectionId": "guides",
"status": "done",
"text": "WindowContent::NativeUi, window options, window-state persistence, and the CPU presenter that paints each frame."
},
{
"title": "Testing",
"section": "Guides",
"sectionId": "guides",
"status": "done",
"text": "NativeUi::render(size) runs a view without a window and returns the display list, frame timing and accessibility nodes."
},
{
"title": "Inspector",
"section": "Guides",
"sectionId": "guides",
"status": "none",
"text": "The elements of a window, their boxes and styles, and the time frames take, beside the content."
},
{
"title": "Rendering",
"section": "Guides",
"sectionId": "guides",
"status": "done",
"text": "Layout on the CPU, a rasterizer for geometry and text, softbuffer presentation, and an optional wgpu offscreen renderer."
},
{
"title": "Browsers",
"section": "Guides",
"sectionId": "guides",
"status": "none",
"text": "The same app in a page, compiled to WebAssembly and drawn with WebGL."
},
{
"title": "Migration",
"section": "Guides",
"sectionId": "guides",
"status": "none",
"text": "Element values, persistent handles, focus bindings and source migration."
},
{
"title": "Performance",
"section": "Guides",
"sectionId": "guides",
"status": "none",
"text": "Measured frame costs and retained memory."
},
{
"title": "Button",
"section": "Actions",
"sectionId": "actions",
"status": "done",
"text": "A push button with hover, press and keyboard activation; fill, border and label use the theme."
},
{
"title": "Link",
"section": "Actions",
"sectionId": "actions",
"status": "none",
"text": "Text that opens a URL, or a page of a router."
},
{
"title": "Menu button",
"section": "Actions",
"sectionId": "actions",
"status": "none",
"text": "A button opening a menu below it, the system's or one drawn in the window with the theme's look."
},
{
"title": "Context menu",
"section": "Actions",
"sectionId": "actions",
"status": "none",
"text": "A menu of the system's for any element, on a right click. Application-level context menus exist outside the view."
},
{
"title": "Toggle",
"section": "Actions",
"sectionId": "actions",
"status": "none",
"text": "A button that stays pressed, and toggles joined in a group."
},
{
"title": "Segmented control",
"section": "Actions",
"sectionId": "actions",
"status": "none",
"text": "Segments choosing one of a few views."
},
{
"title": "Toolbar",
"section": "Actions",
"sectionId": "actions",
"status": "none",
"text": "A row of controls along a window, which overflow into a menu."
},
{
"title": "Checkbox",
"section": "Choices",
"sectionId": "choices",
"status": "done",
"text": "A check box with hover, focus and click or keyboard toggling, drawn from the theme."
},
{
"title": "Switch",
"section": "Choices",
"sectionId": "choices",
"status": "none",
"text": "A switch that turns a setting on and off."
},
{
"title": "Radio",
"section": "Choices",
"sectionId": "choices",
"status": "none",
"text": "Radio buttons choosing one value, in a group."
},
{
"title": "Select",
"section": "Choices",
"sectionId": "choices",
"status": "none",
"text": "A drop-down choosing one of a list."
},
{
"title": "Slider",
"section": "Choices",
"sectionId": "choices",
"status": "done",
"text": "A horizontal slider setting a value in a range, with hover, drag and keyboard focus."
},
{
"title": "Range slider",
"section": "Choices",
"sectionId": "choices",
"status": "none",
"text": "Two knobs setting a low and a high value."
},
{
"title": "Stepper",
"section": "Choices",
"sectionId": "choices",
"status": "none",
"text": "Arrows stepping a value up and down."
},
{
"title": "Rating",
"section": "Choices",
"sectionId": "choices",
"status": "none",
"text": "Stars setting a rating."
},
{
"title": "Text input",
"section": "Text input",
"sectionId": "text-input",
"status": "done",
"text": "A single-line field with character input, Backspace, Delete, Home/End and cursor movement; selection, clipboard, undo and IME are not implemented."
},
{
"title": "Text-input primitives",
"section": "Text input",
"sectionId": "text-input",
"status": "none",
"text": "Application-owned text, native composition and queries, and retained geometry for custom controls."
},
{
"title": "Number input",
"section": "Text input",
"sectionId": "text-input",
"status": "none",
"text": "A number in a range, typed or stepped."
},
{
"title": "Decimal input",
"section": "Text input",
"sectionId": "text-input",
"status": "none",
"text": "An exact decimal, as an amount of money, typed or stepped, never rounded."
},
{
"title": "Search field",
"section": "Text input",
"sectionId": "text-input",
"status": "none",
"text": "A field for searching, which Escape clears."
},
{
"title": "Combobox",
"section": "Text input",
"sectionId": "text-input",
"status": "none",
"text": "A field choosing one of a list, which typing filters."
},
{
"title": "Autocomplete",
"section": "Text input",
"sectionId": "text-input",
"status": "none",
"text": "A text input suggesting what was typed."
},
{
"title": "Token field",
"section": "Text input",
"sectionId": "text-input",
"status": "none",
"text": "A list of strings as chips, as tags or recipients."
},
{
"title": "Editable text",
"section": "Text input",
"sectionId": "text-input",
"status": "none",
"text": "A text renamed in place, as a file's name in Finder."
},
{
"title": "Find bar",
"section": "Text input",
"sectionId": "text-input",
"status": "none",
"text": "A bar for finding text, with the count of matches and the next and previous ones."
},
{
"title": "Rich text editor",
"section": "Text input",
"sectionId": "text-input",
"status": "none",
"text": "Attributed text with formats, links, headings, lists, tasks and images."
},
{
"title": "Code editor",
"section": "Code",
"sectionId": "code",
"status": "none",
"text": "An editor of code with highlighting, several carets, folding, completions and diagnostics."
},
{
"title": "Diff viewer",
"section": "Code",
"sectionId": "code",
"status": "none",
"text": "The changes between two texts, side by side or unified, with the new text editable."
},
{
"title": "Markdown",
"section": "Documents",
"sectionId": "documents",
"status": "none",
"text": "CommonMark and GitHub's Markdown, as finished documents, selectable and copied as text."
},
{
"title": "Calendar",
"section": "Dates and colors",
"sectionId": "dates-and-colors",
"status": "none",
"text": "A month choosing a day."
},
{
"title": "Date input",
"section": "Dates and colors",
"sectionId": "dates-and-colors",
"status": "none",
"text": "A date in a field, chosen in a calendar below it."
},
{
"title": "Time input",
"section": "Dates and colors",
"sectionId": "dates-and-colors",
"status": "none",
"text": "The time of day, its hours and minutes typed or stepped."
},
{
"title": "Color picker",
"section": "Dates and colors",
"sectionId": "dates-and-colors",
"status": "none",
"text": "A color well, and the picker it opens."
},
{
"title": "List",
"section": "Collections",
"sectionId": "collections",
"status": "none",
"text": "Rows of any number and height, built only while in view."
},
{
"title": "Table",
"section": "Collections",
"sectionId": "collections",
"status": "none",
"text": "Rows in columns that the user sorts, resizes and moves."
},
{
"title": "Tree",
"section": "Collections",
"sectionId": "collections",
"status": "none",
"text": "Items that open and close, built inside the items they belong to."
},
{
"title": "Outline",
"section": "Collections",
"sectionId": "collections",
"status": "none",
"text": "A tree of any size, as a list or a table."
},
{
"title": "Grid view",
"section": "Collections",
"sectionId": "collections",
"status": "none",
"text": "Items in a grid of any size, as a photo library."
},
{
"title": "Scroll view",
"section": "Containers",
"sectionId": "containers",
"status": "done",
"text": "A clipped, vertically scrollable container with wheel, drag-bar and keyboard input."
},
{
"title": "Grid",
"section": "Containers",
"sectionId": "containers",
"status": "none",
"text": "Children in equal-width columns and rows. The layout engine can place them internally, but no public builder exposes a grid yet."
},
{
"title": "Split view",
"section": "Containers",
"sectionId": "containers",
"status": "none",
"text": "Two panes with a divider the user drags."
},
{
"title": "Collapsible",
"section": "Containers",
"sectionId": "containers",
"status": "none",
"text": "Content that a label shows and hides."
},
{
"title": "Accordion",
"section": "Containers",
"sectionId": "containers",
"status": "none",
"text": "Sections that open and close, one above the other."
},
{
"title": "Form",
"section": "Containers",
"sectionId": "containers",
"status": "none",
"text": "Labeled controls with descriptions and errors."
},
{
"title": "Web view",
"section": "Containers",
"sectionId": "containers",
"status": "none",
"text": "A web page in the window, under the native UI painted after it."
},
{
"title": "Tabs",
"section": "Navigation",
"sectionId": "navigation",
"status": "none",
"text": "A row of tabs choosing a page."
},
{
"title": "Sidebar",
"section": "Navigation",
"sectionId": "navigation",
"status": "none",
"text": "A source list of items in sections."
},
{
"title": "Breadcrumbs",
"section": "Navigation",
"sectionId": "navigation",
"status": "none",
"text": "A path whose items take the user back."
},
{
"title": "Back and forward buttons",
"section": "Navigation",
"sectionId": "navigation",
"status": "none",
"text": "Buttons going through a router's history."
},
{
"title": "Dialog",
"section": "Dialogs and messages",
"sectionId": "dialogs-and-messages",
"status": "none",
"text": "A dialog over the dimmed window."
},
{
"title": "Alert dialog",
"section": "Dialogs and messages",
"sectionId": "dialogs-and-messages",
"status": "none",
"text": "A question about something important, with buttons."
},
{
"title": "Popover",
"section": "Dialogs and messages",
"sectionId": "dialogs-and-messages",
"status": "none",
"text": "A panel below an element."
},
{
"title": "Tooltip",
"section": "Dialogs and messages",
"sectionId": "dialogs-and-messages",
"status": "none",
"text": "A tip shown as the pointer rests on an element, or the keyboard focus comes to it."
},
{
"title": "Toast",
"section": "Dialogs and messages",
"sectionId": "dialogs-and-messages",
"status": "none",
"text": "A short message near the bottom of the window, with an action or not."
},
{
"title": "Progress bar",
"section": "Status",
"sectionId": "status",
"status": "none",
"text": "Work done so far, or of unknown length."
},
{
"title": "Spinner",
"section": "Status",
"sectionId": "status",
"status": "none",
"text": "Spokes turning while work of unknown length goes on. The loading example draws one by hand."
},
{
"title": "Meter",
"section": "Status",
"sectionId": "status",
"status": "none",
"text": "A value in a range, colored by its level."
},
{
"title": "Badge",
"section": "Status",
"sectionId": "status",
"status": "none",
"text": "A short text in a pill, as a count."
},
{
"title": "Charts",
"section": "Charts",
"sectionId": "charts",
"status": "none",
"text": "Scales, axes and marks without a look, for bars, lines, areas, stacks and donuts. The streaming example draws lines by hand."
},
{
"title": "Image",
"section": "Images",
"sectionId": "images",
"status": "none",
"text": "Bitmaps and SVGs in their own colors."
},
{
"title": "Video",
"section": "Images",
"sectionId": "images",
"status": "none",
"text": "Video files and streams with sound, played by the system's media engine."
},
{
"title": "Icon",
"section": "Images",
"sectionId": "images",
"status": "none",
"text": "Built-in SVG path data exists in the crate, but the rasterizer does not draw SVG paths yet."
},
{
"title": "Avatar",
"section": "Images",
"sectionId": "images",
"status": "none",
"text": "A picture of a person, or their initials."
}
];
