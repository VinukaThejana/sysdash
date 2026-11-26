---
name: sysdash
description: A home Linux machine's health and services, signed like a Munich '72 venue in soft pastels on white.
colors:
  ground: "#ffffff"
  surface: "#ffffff"
  rule: "#ebebed"
  rule-strong: "#d9d9dd"
  ink: "#1f2328"
  ink-2: "#4b5058"
  ink-3: "#6b7079"
  ink-hover: "#3a3f46"
  on-ink: "#ffffff"
  hover: "#f6f6f7"
  press: "#eeeef0"
  track: "rgba(31, 35, 40, 0.07)"
  key-hairline: "rgba(31, 35, 40, 0.12)"
  logs: "#f8f8f9"
  select: "#d6eaf9"
  backdrop: "rgba(31, 35, 40, 0.35)"
  cpu: "#8ec5ee"
  cpu-wash: "#f2f8fd"
  mem: "#9fd89a"
  mem-wash: "#f3faf2"
  disk: "#c6bfe2"
  disk-wash: "#f7f5fc"
  net: "#ffc48f"
  net-wash: "#fff7ef"
  bat: "#8fd8d2"
  bat-wash: "#f0faf9"
  fail: "#c62f26"
  fail-wash: "#fdeceb"
  fail-ink: "#9f1f17"
  warn-wash: "#fff3c4"
  warn-ink: "#6b4e00"
typography:
  display:
    fontFamily: "Archivo, Helvetica Neue, Arial, sans-serif"
    fontSize: "36px"
    fontWeight: 700
    lineHeight: 1
    letterSpacing: "-0.01em"
    fontVariation: "\"wdth\" 75"
  figure:
    fontFamily: "Archivo, Helvetica Neue, Arial, sans-serif"
    fontSize: "clamp(40px, 3.6vw, 56px)"
    fontWeight: 600
    lineHeight: 1
    letterSpacing: "-0.015em"
    fontFeature: "\"tnum\""
    fontVariation: "\"wdth\" 75"
  headline:
    fontFamily: "Archivo, Helvetica Neue, Arial, sans-serif"
    fontSize: "32px"
    fontWeight: 700
    lineHeight: 1.05
    fontVariation: "\"wdth\" 75"
  title:
    fontFamily: "Archivo, Helvetica Neue, Arial, sans-serif"
    fontSize: "22px"
    fontWeight: 700
    lineHeight: 1.1
    letterSpacing: "-0.005em"
    fontVariation: "\"wdth\" 75"
  subtitle:
    fontFamily: "Archivo, Helvetica Neue, Arial, sans-serif"
    fontSize: "20px"
    fontWeight: 600
    lineHeight: 1.1
    fontFeature: "\"tnum\""
    fontVariation: "\"wdth\" 75"
  body:
    fontFamily: "Archivo, Helvetica Neue, Arial, sans-serif"
    fontSize: "15px"
    fontWeight: 400
    lineHeight: 1.45
    fontVariation: "\"wdth\" 100"
  label:
    fontFamily: "Archivo, Helvetica Neue, Arial, sans-serif"
    fontSize: "14px"
    fontWeight: 600
    lineHeight: 1.45
    fontVariation: "\"wdth\" 87.5"
  caption:
    fontFamily: "Archivo, Helvetica Neue, Arial, sans-serif"
    fontSize: "13px"
    fontWeight: 400
    lineHeight: 1.45
    fontVariation: "\"wdth\" 100"
  micro:
    fontFamily: "Archivo, Helvetica Neue, Arial, sans-serif"
    fontSize: "12px"
    fontWeight: 600
    lineHeight: 1.45
    fontVariation: "\"wdth\" 87.5"
  mono:
    fontFamily: "ui-monospace, SF Mono, JetBrains Mono, Menlo, Consolas, monospace"
    fontSize: "12px"
    fontWeight: 400
    lineHeight: 1.6
rounded:
  none: "0px"
spacing:
  s1: "4px"
  s2: "8px"
  s3: "12px"
  s4: "16px"
  s5: "20px"
  s6: "24px"
  s8: "32px"
components:
  header:
    backgroundColor: "{colors.surface}"
    textColor: "{colors.ink}"
    typography: "{typography.display}"
    rounded: "{rounded.none}"
    padding: "29px 32px 20px"
  unit-chip:
    backgroundColor: "{colors.fail-wash}"
    textColor: "{colors.fail-ink}"
    typography: "{typography.label}"
    rounded: "{rounded.none}"
    padding: "6px 10px"
    height: "32px"
  vital-tile-cpu:
    backgroundColor: "{colors.cpu-wash}"
    textColor: "{colors.ink}"
    typography: "{typography.figure}"
    rounded: "{rounded.none}"
    padding: "12px 16px 16px"
  vital-tile-mem:
    backgroundColor: "{colors.mem-wash}"
    textColor: "{colors.ink}"
  vital-tile-disk:
    backgroundColor: "{colors.disk-wash}"
    textColor: "{colors.ink}"
  vital-tile-net:
    backgroundColor: "{colors.net-wash}"
    textColor: "{colors.ink}"
  vital-tile-bat:
    backgroundColor: "{colors.bat-wash}"
    textColor: "{colors.ink}"
  vital-meter:
    backgroundColor: "{colors.track}"
    rounded: "{rounded.none}"
    height: "10px"
  vital-flag-high:
    backgroundColor: "{colors.warn-wash}"
    textColor: "{colors.warn-ink}"
    typography: "{typography.micro}"
    padding: "1px 6px"
  vital-flag-critical:
    backgroundColor: "{colors.fail-wash}"
    textColor: "{colors.fail-ink}"
    typography: "{typography.micro}"
    padding: "1px 6px"
  button:
    backgroundColor: "{colors.surface}"
    textColor: "{colors.ink}"
    typography: "{typography.label}"
    rounded: "{rounded.none}"
    padding: "0 16px"
    height: "40px"
  button-hover:
    backgroundColor: "{colors.hover}"
  button-primary:
    backgroundColor: "{colors.ink}"
    textColor: "{colors.on-ink}"
    rounded: "{rounded.none}"
    height: "40px"
  button-primary-hover:
    backgroundColor: "{colors.ink-hover}"
  button-danger:
    backgroundColor: "{colors.surface}"
    textColor: "{colors.fail}"
    rounded: "{rounded.none}"
    height: "40px"
  button-danger-hover:
    backgroundColor: "{colors.fail-wash}"
    textColor: "{colors.fail-ink}"
  button-quiet:
    textColor: "{colors.ink-2}"
    height: "40px"
  icon-button:
    textColor: "{colors.ink-3}"
    rounded: "{rounded.none}"
    size: "36px"
  search-field:
    backgroundColor: "{colors.hover}"
    textColor: "{colors.ink}"
    typography: "{typography.body}"
    rounded: "{rounded.none}"
    padding: "0 12px"
    height: "38px"
  segmented-selected:
    backgroundColor: "{colors.ink}"
    textColor: "{colors.surface}"
    typography: "{typography.label}"
    padding: "0 12px"
    height: "36px"
  service-row:
    textColor: "{colors.ink}"
    typography: "{typography.body}"
    padding: "9px 8px"
    height: "56px"
  service-row-hover:
    backgroundColor: "{colors.hover}"
  sheet-head:
    backgroundColor: "{colors.surface}"
    textColor: "{colors.ink}"
    typography: "{typography.headline}"
    padding: "20px 16px 16px 24px"
  sheet-head-failed:
    backgroundColor: "{colors.fail-wash}"
    textColor: "{colors.fail-ink}"
  logs:
    backgroundColor: "{colors.logs}"
    textColor: "{colors.ink-2}"
    typography: "{typography.mono}"
    padding: "12px"
  confirm:
    backgroundColor: "{colors.surface}"
    textColor: "{colors.ink}"
    typography: "{typography.body}"
    rounded: "{rounded.none}"
    width: "380px"
  confirm-ok-destructive:
    backgroundColor: "{colors.fail}"
    textColor: "{colors.on-ink}"
  toast:
    backgroundColor: "{colors.ink}"
    textColor: "{colors.surface}"
    typography: "{typography.label}"
    padding: "10px 16px"
  toast-error:
    backgroundColor: "{colors.fail-wash}"
    textColor: "{colors.fail-ink}"
  tabbar:
    backgroundColor: "{colors.surface}"
    textColor: "{colors.ink-3}"
    typography: "{typography.micro}"
    padding: "9px 0 8px"
---

# Design System: sysdash

## Overview

**Creative North Star: "Munich '72 information system, in pastel"**

The machine is signed like an Olympic venue, after Otl Aicher's system for the 1972 Games, but softened to pastels on white paper. Each subsystem owns one pastel and keeps it everywhere it appears: CPU powder blue, memory mint, disk lavender, network peach. The colour travels from the vital tile's key square and its faint tile wash to its meter, its history columns, and its process-table cell bars and sort key, so colour is wayfinding, not decoration. A thin four-pastel stripe tops the header and every dialog; the header itself is plain paper holding the one sentence that says whether the machine is fine.

Everything else is white, neutral grey and charcoal, in one light theme on every device; there is no dark mode. Failure is the only strong colour, and even it mostly arrives as a soft red field with deep red ink. Sections are opened by a 2px ink rule, never boxed in cards. One variable grotesk, Archivo, does all the talking at three fixed widths: condensed for figures and headings, semi-condensed for labels and controls, normal width for prose. The page is dense on purpose: two-minute histories, a 15-row process table and the full service list sit on one laptop screen; the phone shows one section at a time under a bottom tab bar.

This world rejects strong saturated fields and large blue areas, the category's grey panel grid with a chart in every tile, the generic SaaS admin template, and any neon, glow or HUD styling.

**Key Characteristics:**
- One pastel per subsystem, carried end to end, with a faint wash of it behind its tile.
- White paper, neutral grey rules, charcoal ink; one light theme on every device.
- A 5px four-pastel stripe along the top of the header, the service sheet and the confirm dialog.
- Failure is the only strong colour: red text and marks, soft red fields with deep red ink.
- Archivo variable at fixed widths: 75% for figures and headings, 87.5% for labels, 100% for prose.
- Flat fields, square corners, ruled sections (2px ink), tabular figures that never jitter.

## Colors

White paper and charcoal ink, four soft subsystem pastels (and a fifth for the battery) each with a near-white wash, and one strong red kept for failure.

### Primary
- **Charcoal Ink** (ink): text, figures, section rules, the selected segment, the default toast, and the primary and confirm buttons (white text via on-ink; hover lifts to ink-hover). The system's action colour is ink, not a hue.

### Secondary (the subsystem pastels)
- **CPU Powder Blue** (cpu, cpu-wash): CPU and load tiles, CPU history columns, per-thread bars, the CPU table column's cell bars and sort key.
- **Memory Mint** (mem, mem-wash): memory and swap tiles, memory history, the memory table column.
- **Disk Lavender** (disk, disk-wash): one tile per mounted disk.
- **Network Peach** (net, net-wash): the network tile's key and wash; its up/down arrows stay ink-3.
- **Battery Aqua** (bat, bat-wash): the battery tile, shown only on a machine with one. Not in the stripe, which stays the four subsystems. Its meter has no 70%/90% notches, and its flags mean low charge on battery: "Low" below 20%, "Critical" below 10%.
- Each pastel is a fill only: key squares (with a key-hairline inset so they hold on white), meters, history columns, cell bars, and the stripe. Each wash is the background of that subsystem's vital tile and the colour of its meter notches.

### Tertiary
- **Failure Red** (fail): the one strong colour. Failed state squares, failed service names and state words, over-90% figures, meters and thread bars, the danger button's text and outline, the 5px edge of a failed sheet head and a destructive confirm, the outline of an error toast, and the solid fill of a destructive confirm button (white text).
- **Failure Wash and Ink** (fail-wash, fail-ink): soft red fields with deep red text: failing-unit chips, the Critical flag, the failed sheet head, the error toast and the danger button's hover.
- **Warning Wash and Ink** (warn-wash, warn-ink): the soft yellow "High" flag above 70% (the battery's "Low" flag below 20%), a colour no subsystem owns.

### Neutral
- **Paper** (ground, surface): white everywhere; the header, sheet, dialogs and tab bar sit on it.
- **Grey Rules** (rule, rule-strong): hairline grid between vitals and facts, row dividers, the header's bottom rule; rule-strong outlines controls and the tab bar's top edge.
- **Ink Tiers** (ink-2, ink-3): ink-2 for secondary figures and meta; ink-3 for notes, captions, column heads, inactive tabs, icons and the network arrows.
- **Hover / Press** (hover, press): flat tonal fills for row hover and pressed states; hover also fills the search field.
- **Track** (track): the empty part of meters, thread bars and skeleton lines, a translucent charcoal so it reads on any wash.
- **Logs Paper** (logs): the journal block's off-white field.
- **Selection** (select): pale blue text selection under ink.
- **Backdrop** (backdrop): translucent charcoal behind modals.

### Named Rules
**The One Owner Rule.** Each pastel belongs to one subsystem and appears wherever that subsystem appears. No other element borrows cpu, mem, disk or net or their washes: not buttons, not links, not states, not warning levels.

**The Pastel Fill Rule.** Pastels are fills, never text colours; text on and around them stays ink.

**The Soft Failure Rule.** Red is the only strong colour and is spent only on failure: as text, marks, edges and outlines in fail red, and as soft fields in fail-wash with fail-ink. The single solid red field is the destructive confirm button.

**The Neutral State Rule.** State marks stay neutral (ink squares for running, outlined squares for done or changing, grey for inactive) except failure. Warning above 70% is expressed by weight and a soft yellow "High" flag, never by borrowing a subsystem colour.

## Typography

**Display Font:** Archivo variable (weight 100 to 900, width 62% to 125%), self-hosted and compiled into the binary, with Helvetica Neue and Arial as fallbacks.
**Body Font:** the same Archivo, at normal width.
**Label/Mono Font:** labels use Archivo at 87.5% width; journal logs alone use the platform monospace stack.

**Character:** a single grotesk whose width axis does the work a second family would: condensed figures read large in narrow tiles, the way Munich signage set numbers, while prose stays open and easy on a phone.

### Hierarchy
- **Display** (700, 36px, line-height 1, 75% width): the host name in the header.
- **Figure** (600, clamp 40px to 56px, line-height 1, 75% width, tabular): vital readings; the unit is a 22px ink-3 suffix. The network figure is 32px with an ink-3 arrow. Over 70% the weight goes to 800; over 90% it also turns red.
- **Headline** (700, 32px, line-height 1.05, 75% width): the service name in the sheet head.
- **Title** (700, 22px, line-height 1.1, 75% width): section headings (History, Processes, Services); the header's uptime figure uses the same size at 600.
- **Subtitle** (600 to 700, 20px, 75% width): history strip current values and the Logs heading.
- **Body** (400, 15px, line-height 1.45, 100% width): base text, process and service names (at 600), fact values, table figures. Inputs go to 16px below 1100px so iOS does not zoom.
- **Label** (600, 14px, 87.5% width): vital labels, strip captions, segmented and tab labels (15px for underline tabs), button labels (15px), sheet state line.
- **Caption** (400, 13px to 14px, 100% width): notes, descriptions (the sheet's description line at 14px), commands, summaries; column heads at 13px, 600, 87.5%.
- **Micro** (600, 11.5px to 12.5px, 87.5% width): tab bar labels, High/Critical flags, fact terms, the "user" tag.
- **Mono** (400, 12px, line-height 1.6): journal lines only.

### Named Rules
**The Width-as-Role Rule.** Width encodes role: 75% for anything read as a number or a heading, 87.5% for labels and controls, 100% for running text. Do not introduce a second family or a fourth width.

**The Still Figures Rule.** Every figure that updates in place is tabular, so live values never shift their neighbours.

## Layout

A 4px spacing scale (4, 8, 12, 16, 20, 24, 32) with occasional 5px, 6px, 9px and 10px optical insets; the 5px stripe adds to the top padding of whatever it tops.

- **Phone and tablet (under 1100px):** a single column. The header sits on top; one pane (Overview, Processes or Services) shows at a time, chosen by a fixed bottom tab bar that respects the safe-area inset. The page reserves 92px plus the inset at the bottom. Page gutter 16px, section gap 16px. Deep links (`#processes`, `#services`, `#services/<scope>/<unit>`) choose the pane and can open a service sheet.
- **Laptop (1100px and up):** a two-column grid, the main column flexible and the services column fixed at 420px. The header spans both. Main padding 24px top, 32px left, section gap 32px; the services column is sticky at full viewport height with its own scrolling list, separated by a 1px rule.
- **700px step:** vitals go from two to four columns, history strips from stacked to side by side, the process table regains User and PID columns, and the service sheet changes from a bottom sheet to a centred dialog.
- **No empty cells:** in the vitals grid and the facts grid the last item spans whatever its row has left.
- **Browser chrome:** one theme colour, white, with the default status bar style; the favicon is four pastel bars on white.

**The Dense Glance Rule.** The healthy state reads from the header's status sentence and the first row of vitals; everything else is present but quieter. Do not add empty hero space or whitespace-only sections.

## Elevation & Depth

Flat. There are no drop shadows, glows or blurs anywhere, including dialogs. Depth comes from rules and washes: the pastel stripe over the header and dialogs, 2px ink section rules, 1px hairlines, the faint subsystem wash behind each vital tile, and, for modals, a translucent charcoal backdrop. Box-shadow appears only as an inset drawing tool: key-square hairlines, control and button outlines, the error toast's and hovered chip's red outline, the underline of an active tab, and the top bar of an active phone tab.

**The Ruled-Not-Boxed Rule.** A section opens with a 2px ink rule and a condensed title; it is never wrapped in a card, border box or shadow.

**The Stripe Rule.** The four pastels in subsystem order (CPU, memory, disk, network, a quarter each) form a 5px stripe that marks the top edge of the header, the service sheet head and the confirm dialog, and nothing else. It is an edge, never a fill. A failed sheet head and a destructive confirm replace it with a solid fail-red edge.

## Shapes

Square-cut everywhere: zero radius on tiles, buttons, inputs, chips, dialogs, meters and marks. The recurring mark is a solid square: a 10px pastel key with a faint charcoal hairline, an 8px state square for services. Meters are flat bars notched at 70% and 90% by 2px gaps in the tile's wash. Icons are one family on a 24px grid with 2px strokes, square caps and mitred joins.

## Components

### Header
The shell and the status sentence. White paper with ink type, the 5px pastel stripe along its top edge and a 1px rule below. Host name (Display) with an ink-3 OS and kernel caption on the left, an ink-3 "Up" plus a condensed uptime figure on the right. Under a 1px rule, a 10px state square (ink when fine, red when not) and one sentence: "No failed or crash-looping services." or "N services need attention", followed by one chip per failing unit.

### Unit Chip
A soft red field (fail-wash) with fail-ink text: the unit name at 600 and a semi-condensed "failed" or "crash-looping" note. 32px minimum height. Hover adds a 1px fail-red inset outline. Opens that service's sheet. Press scales to 0.97.

### Vital Tile
Tiles share a 1px hairline grid; each tile's background is its subsystem's wash. Each holds a semi-condensed label led by its pastel key (disks add the mount path in ink-3), a large condensed ink figure, a one-line note, and a full-bleed meter along its foot: a 10px flat bar of the pastel on the track, notched at 70% and 90% in the wash. Over 70% a soft yellow "High" flag sits at the label's right; over 90% it becomes a soft red "Critical" flag and the figure and meter turn red. Network and Load tiles carry no meter.

### History Strips
Two figures (CPU, memory) of flat pastel columns on one shared 0 to 100% scale, 96px tall (64px on phones), with a dotted 50% line and a hairline baseline. Beside them, one 12px column per CPU thread on the track, filling in CPU powder blue (red over 90%).

### Process Table
Fixed-layout table under a 2px ink header rule, 1px row dividers, hover tint. Names at 600 with the command line as an ink-3 caption. CPU and memory columns are right-aligned tabular ink figures with a 4px flat pastel cell bar beneath, scaled to the top process; the non-sorted column's bars are at 45% opacity. Sorting is by clicking the column head (its key square and a chevron mark the sort). A stop button appears on hover for the owner's own processes and is always visible on touch.

### Services List
Rows 56px minimum: 8px state square, name at 600 (with an outlined "user" tag), description caption, and right-aligned memory and boot state. Failed rows turn the name red and replace memory with the red state word. Loading shows blinking skeleton lines on the track.

### Filters
A search field (hover-tinted, rule-strong outline, ink double outline on focus), a segmented control for scope (selected segment is solid ink with white text), and underline tabs for state (active tab gets a 4px ink underline). The filter block closes with a 2px ink rule.

### Buttons
- **Shape:** square, 40px tall, 16px side padding, semi-condensed 15px at 600.
- **Default:** white with a rule-strong inset outline; hover tints and darkens the outline.
- **Primary:** solid charcoal with white text (Start); hover lifts to ink-hover.
- **Danger:** fail-red text and outline; hover fills fail-wash with fail-ink. Stop, Kill and Delete sit together in a danger group pushed to the end of the action row (on phones, its own row under a 1px rule), apart from Restart/Start and the boot toggle. A masked unit shows only Unmask.
- **Quiet:** no outline, ink-2 text (Cancel).
- **Icon button:** 36px square, ink-3 icon, press fill on hover.
- **Text button:** underlined label, underline darkens on hover.
- **Press:** scale to 0.97 (0.94 for icon buttons); in-flight requests dim to 50%.

### Service Sheet
A modal dialog, centred up to 780px wide on laptops and a bottom sheet on phones. Its head is white with ink, the 5px pastel stripe on top and a 2px ink rule below. When the service has failed the head becomes a fail-wash field with fail-ink text, a 5px fail-red top edge and a fail-red bottom rule. The head holds the condensed service name, the state line with its square, the description on its own 14px line beneath, and a close button. Below: the action row, a facts grid (hairline grid like the vitals, the unit file as a full first row, terms in micro ink-3), and the Logs block under a 2px ink rule in a monospace logs field scrolled to the end.

### Agents

The third tab with Processes and Storage (a bottom tab on phones). Rows under a 2px ink rule: a state label in semi-condensed 600 with its square (Working in ink with a blinking square, Idle and Done in ink-3, Running for agents outside herdr in ink-2, and Waiting for you as the one attention state, a soft yellow warn-wash flag with warn-ink text), the task title at 600 (500 ink-2 when idle or done) over an ink-3 caption of agent · workspace · folder, then CPU over memory right-aligned in tabular figures and the running time in ink-3. Waiting-for-you rows come first, then working ones. On phones the state sits on its own line above the title.

### Storage browser

On laptops, Storage and Processes share one area under a ruled tab strip (22px condensed tab names, ink with a 4px ink underline when selected, ink-3 otherwise) that replaces both headings, so only one shows at a time and the last choice is remembered; on phones each is its own bottom tab. The head carries "Scanned … ago" and a Rescan text button. While the first scan runs: a condensed title, a progress caption, and a 6px track with a lavender sweep (stopped under reduced motion). Below: path crumbs in semi-condensed 600 ink-3 (current folder in ink), the folder's total at 22px condensed with its file count, a 2px ink rule, then rows largest first. Each row: a folder or file glyph (folders in deepened disk lavender), the name at 600 with an ink-3 caption (file count, "file", or "can't read inside"), the size at 600 tabular, and a full-width 4px lavender share bar relative to the folder. Folder rows are buttons that open the folder; rows inside home end in a trash icon button (danger hover) whose confirmation names the path, size and file count. Rows past 100 collapse into one "N smaller items" line. Disk tiles on the overview are links (inset ink-3 outline on hover) into the browser at their mount.

### Confirm Dialog
A 380px square white dialog topped by the 5px pastel stripe, the question in 16px text, and Cancel (quiet) plus a charcoal button that names the verb. When the action stops, restarts, kills or deletes, the edge turns fail red and the verb button is solid fail red with white text.

### Toast
A square field at the top centre under the safe-area inset: ink with white text normally; fail-wash with fail-ink and a 1px fail-red outline for errors. Slides down 8px and fades in.

### Phone Tab Bar
Fixed to the bottom, three equal tabs with 22px icons over micro labels. The active tab turns ink with a 3px ink bar along its top edge. Hidden at 1100px and up.

### Motion
One easing, a quick ease-out (`cubic-bezier(0.23, 1, 0.32, 1)`). Hover and press feedback 120ms; confirm dialog in 180ms; backdrop fade 200ms; sheet in 220ms (rise 8px) or 300ms slide-up on phones; toast 260ms; meters, cell bars and thread bars glide to new values over 600ms. A changing service square blinks in two hard steps. Under `prefers-reduced-motion` all animation and transition durations collapse to near zero, and the infinite blinks (changing square, loading rows) stop entirely.

## Do's and Don'ts

### Do:
- **Do** keep one white theme on every device.
- **Do** carry a subsystem's pastel everywhere that subsystem appears: key square, tile wash, meter, history columns, table bars, sort key.
- **Do** keep text ink on and around pastels.
- **Do** open every section with a 2px ink rule and a condensed 22px title.
- **Do** set figures and headings at 75% width, labels and controls at 87.5%, prose at 100%, all in Archivo.
- **Do** express warning (over 70%) with weight and the soft yellow "High" flag, and failure with red text, marks and soft red fields.
- **Do** use the pastel stripe only as the 5px top edge of the header, the sheet head and the confirm dialog.
- **Do** keep every corner square and every field flat.

### Don't:
- **Don't** add a dark theme or a theme toggle.
- **Don't** use strong saturated fields or large blue areas; red is the one strong colour, and only for failure.
- **Don't** let any element other than its subsystem use cpu, mem, disk or net or their washes, and don't colour healthy states.
- **Don't** set text in a pastel.
- **Don't** box sections in cards or add drop shadows, glows or blurs, including on dialogs.
- **Don't** round corners.
- **Don't** add a second type family for display or UI; monospace is for journal logs only.
- **Don't** draw history as line or area charts with gradients; it is flat columns on a shared 0 to 100% scale.
