# File-format parity with PowerPoint

> **Last reviewed:** 2026-10-10 · **Last updated:** 2026-10-12 · **Change:** minor (SmartArt text follows `dsp:txXfrm`; previous: trivial, Groups with a small legacy child coordinate space draw outlines, dashes, arrowheads and ink at their own width (#233)) · **Target:** Microsoft PowerPoint for Mac 16.113 (Microsoft 365)
> **Last reviewed:** 2026-10-10 · **Last updated:** 2026-10-13 · **Change:** trivial (Groups with a small legacy child coordinate space draw outlines, dashes, arrowheads and ink at their own width comments: modern threaded parts written, #70; groups with a small legacy child coordinate space draw outlines, dashes, arrowheads and ink at their own width (#233); previously major: created; formats taken from PowerPoint 16.113.4's Info.plist, PPTX coverage measured from `crates/pptx`) · **Target:** Microsoft PowerPoint for Mac 16.113 (Microsoft 365)
> **Last reviewed:** 2026-10-10 · **Last updated:** 2026-10-13 · **Change:** minor (EMF/WMF rectangle clip regions; previous: trivial, groups with a small legacy child space, #233) · **Target:** Microsoft PowerPoint for Mac 16.113 (Microsoft 365)
> **Last reviewed:** 2026-10-10 · **Last updated:** 2026-10-13 · **Change:** minor (embedded fonts read, written and drawn; MicroType Express subsets kept but not drawn; previously trivial: groups with a small legacy child coordinate space, #233) · **Target:** Microsoft PowerPoint for Mac 16.113 (Microsoft 365)
> **Last reviewed:** 2026-10-13 · **Last updated:** 2026-10-13 · **Change:** minor (owned corpus round-trip; notes-master header and footer text kept; previous: trivial, groups with a small legacy child coordinate space) · **Target:** Microsoft PowerPoint for Mac 16.113 (Microsoft 365)
> **Last reviewed:** 2026-10-13 · **Last updated:** 2026-10-13 · **Change:** minor (owned corpus for comments, links and actions, custom shows, rotated groups and inherited placeholders; paragraph `defRPr` and action sounds kept; previous: trivial, groups with a small legacy child coordinate space) · **Target:** Microsoft PowerPoint for Mac 16.113 (Microsoft 365)
> **Last reviewed:** 2026-10-13 · **Last updated:** 2026-10-13 · **Change:** minor (owned corpus for text effects, body layout, theme effects, fills, bevels and connectors; a run's inner shadow, reflection and soft edge, and a bevel preset with no kept 3-D XML, survive a save; previous: trivial, groups with a small legacy child coordinate space) · **Target:** Microsoft PowerPoint for Mac 16.113 (Microsoft 365)
> **Last reviewed:** 2026-10-14 · **Last updated:** 2026-10-14 · **Change:** minor (3-D model parts and animation bounce end round-trip; `corpus_media.rs` covers SmartArt and OLE XML, media bookmarks and trims, sections, fields, ink and timing) · **Target:** Microsoft PowerPoint for Mac 16.113 (Microsoft 365)
> **Last reviewed:** 2026-10-14 · **Last updated:** 2026-10-14 · **Change:** minor (chart trendlines round-trip; previously trivial: groups with a small legacy child coordinate space draw outlines, dashes, arrowheads and ink at their own width (#233); formats taken from PowerPoint 16.113.4's Info.plist, PPTX coverage measured from `crates/pptx`) · **Target:** Microsoft PowerPoint for Mac 16.113 (Microsoft 365)

Every format PowerPoint reads or writes, DeckCraft's support, fidelity and how it is tested.
Gaps are itemized in [gaps.md](gaps.md#file-formats). Dimension score: **~58% ready**
(estimated), **60–110 h** to parity.

Weights inside this dimension: PPTX 70%, PDF export 10%, image export 5%, legacy binary formats
5%, everything else (templates, shows, macros, ODP, video, GIF, outline) 10%.

## Documents

From `/Applications/Microsoft PowerPoint.app/Contents/Info.plist` `CFBundleDocumentTypes` (Editor =
PowerPoint opens and saves; Viewer = opens only).

| Format | PowerPoint | DeckCraft read | DeckCraft write | Fidelity / notes | Tested by |
|---|---|---|---|---|---|
| `.pptx` presentation | Editor | yes | yes | Broad (see below); corpus is decks we author, not measured against PowerPoint renders | `crates/pptx/tests/corpus.rs`, `roundtrip.rs`, `sample.rs`, `malformed.rs`, `sniff.rs`; opened in PowerPoint by hand 2026-10-06 |
| `.pptx` presentation | Editor | yes | yes | Broad (see below); an owned corpus covers comments, links and actions, custom shows, rotated groups and placeholders that inherit from the layout. Embedded fonts and a corpus of decks from outside this repo are still open | `crates/pptx/tests/roundtrip.rs`, `corpus_elements.rs`, `sample.rs`, `malformed.rs`, `sniff.rs`; opened in PowerPoint by hand 2026-10-06 |
| `.pptx` presentation | Editor | yes | yes | Broad (see below); an owned corpus covers text effects, autofit, columns, vertical and RTL text, tabs, line spacing, theme effects, gradient and pattern fills, bevels and connectors. A corpus of decks from outside this repo is still open | `crates/pptx/tests/roundtrip.rs`, `corpus_style.rs`, `sample.rs`, `malformed.rs`, `sniff.rs`; opened in PowerPoint by hand 2026-10-06 |
| `.pptx` presentation | Editor | yes | yes | Broad (see below); never measured on real decks | `crates/pptx/tests/roundtrip.rs`, `sample.rs`, `malformed.rs`, `sniff.rs` (30 tests), `corpus_media.rs`; opened in PowerPoint by hand 2026-10-06 |
| `.potx` template | Editor | yes (as a presentation) | no: written with the `.pptx` content type | `file.saveTemplate` is Save As under another name | none |
| `.ppsx` slide show | Editor | yes (as a presentation) | no: written as `.pptx` content | Doesn't start in show mode | none |
| `.pptm` / `.potm` / `.ppsm` macro-enabled | Editor | no | no | Should open and keep `vbaProject.bin` without running it | none |
| `.ppt` / `.pot` / `.pps` 97–2003 binary | Editor | no | no | Owner decision; import only would be enough | none |
| `.odp` OpenDocument | Editor | no | no | | none |
| `.thmx` Office theme | Viewer | no | no | Browse for themes is a P2 row | none |
| `.deckcraft` native (zip + JSON) | n/a | yes (also `.slidecraft`) | yes | Lossless for the DeckCraft model, forward compatible | `crates/format` tests |
| PDF | export | no | yes | Slides, notes pages, handouts (1–9); real-text layer with embedded subsets, links, bookmarks. Slide artwork is a raster image at print resolution, not vector | `crates/pdf` tests |
| Outline `.rtf` / `.txt` | export (RTF), import (outline) | `.txt` outline; `.md` as Markdown (headings are slides, list items bullets) (`slide.fromOutline`) | `.txt` outline | No RTF | engine tests |
| PNG / JPEG slide images | export | n/a | yes (`file.export`, `deckcraft-cli render`) | Matches the editor's renderer | render tests |
| TIFF / BMP / GIF slide images | export | n/a | no | Easy (the `image` crate writes them) | none |
| Animated GIF of the deck | export | n/a | no | | none |
| MP4 / MOV video of the deck | export | n/a | no | Needs timeline render + H.264 encode | none |
| Print | yes | n/a | no | PR #66 (browser) open | none |

## Pictures and media inside decks

| Kind | PowerPoint | DeckCraft | Notes |
|---|---|---|---|
| PNG, JPEG, GIF (animated), BMP, TIFF, WebP | yes | yes | `image` 0.25; animated GIFs play since #39 |
| SVG | yes (with PNG fallback) | **no** | The PNG fallback shows; SVG files can't be inserted |
| EMF / WMF | yes | partial | Drawn as vectors (paths, pens, pattern and hatch brushes, bitmaps, text in the document's fonts with the file's character advances). Rectangle clip regions (`INTERSECTCLIPRECT`, `EXCLUDECLIPRECT`, `EXTSELECTCLIPRGN`) limit later drawing; EMF+ records are not drawn. Legacy OLE objects (VML preview) and ActiveX controls show their pictures |
| HEIC | yes (macOS) | no | |
| Audio: MP3, AAC/M4A, ALAC, WAV, AIFF, CAF, FLAC, Ogg Vorbis, Opus | yes (Ogg/Opus partly) | yes | Pure-Rust decoders in `crates/media`, `crates/opus` |
| WMA | yes (Windows) | embedded, not playable | |
| Video: H.264, HEVC in MP4/MOV | yes | yes | Pure-Rust, software decode |
| Video: VP9, AV1 in WebM/MKV | partly (OS codecs) | yes | Ahead of PowerPoint for Mac |
| WMV | yes (Windows) | embedded, not playable | |
| Online video (YouTube embeds) | yes | no | Cloud feature |
| Embedded OLE objects | yes | kept on round trip (opaque), not insertable or editable | `ShapeKind::Opaque` |
| 3-D models (`.glb`) | yes | no | |
| Embedded fonts | yes | yes | `p:embeddedFontLst` read and written. TrueType, OpenType and Embedded OpenType faces are used to draw; MicroType Express subsets are kept and written back, not drawn |
| 3-D models (`.glb`) | yes | kept | The model part round-trips (`p:contentPart`, including an `mc:AlternateContent` choice and its fallback picture). Not drawn as a mesh and not inserted from the ribbon |
| Embedded fonts | yes | no | `p:embeddedFontLst` ignored |

## PPTX coverage (PresentationML / DrawingML)

What `crates/pptx` reads and writes, from ECMA-376. "Kept" means the XML is preserved untouched for
the round trip even though DeckCraft doesn't model it.

| Part / feature | Read | Write | Notes |
|---|---|---|---|
| OPC package, content types, relationships | yes | yes | Content types found anywhere in the zip (#19); zip bombs capped; relationship cycles bounded |
| Presentation, slide size, sections (p14), custom shows | yes | yes | |
| Slide masters, layouts, placeholders and inheritance | yes | yes | |
| Themes: colour, font (Latin/EA/CS), effect schemes, background styles | yes | yes | Unknown theme parts kept |
| Notes slides and notes master | yes | yes | Header, footer, date and slide-number text on the notes master is kept. No notes-master editing view |
| Handout master | no | no | |
| Shapes: preset geometry (~180 presets), custom geometry, groups, connectors | yes | yes | Presets re-derived in `crates/geom/src/preset.rs` (and `preset/extra.rs`). Outlines, dashes, arrowheads and ink inside a group, and text, keep their size in points whatever the group's child-coordinate scale (#233). |
| Fills: solid, gradient, picture (stretch/tile/crop), pattern, group | yes | yes | |
| Lines: dash, compound, caps, joins, arrowheads | yes | yes | |
| Effects: shadow, glow, soft edges, reflection | yes | yes | |
| 3-D: `scene3d`, `sp3d` | kept | kept | Not rendered |
| Text: runs, paragraphs, bullets, numbering, levels, autofit, columns, vertical, RTL, script fonts, fields, hyperlinks | yes | yes | Round-trip tests incl. `justLow`, date fields and paragraph `defRPr` (the defaults a placeholder's runs inherit, including a link). A click or hover action keeps its sound, `endSnd`, and a custom show's "Show and return" |
| Effects: shadow, glow, soft edges, reflection | yes | yes | On a run, inner shadow, reflection and soft edges are kept along with the outer shadow and glow |
| 3-D: `scene3d`, `sp3d` | kept | kept | A bevel preset with no `sp3d` of its own is written back on shapes and theme effect styles. Bevels, cameras and face lighting draw approximately; extrusion does not |
| Text: runs, paragraphs, bullets, numbering, levels, autofit, columns, vertical, RTL, script fonts, fields, hyperlinks | yes | yes | Round-trip tests incl. `justLow`, date fields, gradient and outline on a run, autofit, columns, vertical and RTL text, tab stops and line spacing (`corpus_style.rs`) |
| Text warp (`prstTxWarp`) | yes | yes | All 40 presets rendered (`crates/geom/src/warp.rs`); adjust values (`avLst`) round-trip |
| Pictures with crop, recolour, duotone, alpha, luminance | yes | yes | |
| Tables with styles | yes | yes | Merges and both diagonal borders round-trip |
| Charts (`c:chartSpace`) | yes | yes | Original XML kept in `Chart.raw`. Trendlines (type, order or period, forecast, intercept, equation and R²) are read and written; they are not drawn. A combo chart's secondary value axis is read and is not written back when the chart is rebuilt |
| Charts (`cx:` chartex: waterfall, funnel, treemap, sunburst, histogram, box) | no | no | |
| SmartArt (`dgm`) | drawing part only, as a group; text laid out on its `dsp:txXfrm` rectangle | as shapes | Diagram data model lost on save |
| SmartArt (`dgm`) | drawing part only, as a group | as shapes | Diagram data model lost on save. A diagram with no relationship parts keeps its XML; the drawing and data parts are still dropped (#203) |
| Equations (OMML, `a14:m`) | no | no | PR #55 open |
| Audio / video (`p14:media`, trim, fades, bookmarks) | yes | partly | Playback options lost on save (#74) |
| Ink (`contentPart`) | yes | as a group of freeform lines | PowerPoint sees shapes, not ink |
| Transitions incl. p14/p15 (Vortex, Morph…) | all 48 | all 48 | `every_transition_kind_round_trips` |
| Animations: timing tree, build paragraphs, triggers, motion paths | yes (flattened to the Animation Pane list) | yes (rebuilt) | `every_animation_effect_survives`; repeat, triggers and bounce end kept; rewind lost (#77) |
| Comments: legacy | yes | yes | |
| Comments: modern threaded (`p188`) | yes | yes | Written next to the flattened legacy part; replies and resolved state round-trip (`threads_and_resolved_round_trip`) |
| Unknown slide-level elements (`graphicFrame` OLE, extensions, `mc:AlternateContent`) | kept | kept | Rendered as a labelled placeholder with the preview picture when present |
| Tags, custom XML, VBA | no / no / no | | |
| Embedded fonts | yes | yes | MicroType Express subsets kept, not drawn |
| Encryption (password to open) | no | no | |

### What would move the PPTX number

1. A larger owned corpus (corporate templates, SmartArt, chartex charts, EMF, embedded fonts,
   every transition and animation) in a corpus repo. `crates/pptx/tests/corpus.rs` already checks
   open → save → reopen (parts and text) for decks we author. Still needed: render checks and a
   PowerPoint reference render for each (owner runs PowerPoint).
2. The round-trip losses above (#70, #74, #77).
3. SVG rendering. EMF+ records inside EMF/WMF are still not drawn.

## Revision history

| Date | Change | Summary |
|---|---|---|
| 2026-10-12 | minor | SmartArt import lays each shape's text out on its `dsp:txXfrm` text rectangle instead of the shape box |
| 2026-10-13 | minor | EMF/WMF rectangle clip regions limit later drawing; EMF+ records remain |
| 2026-10-13 | minor | Embedded fonts read and written; TrueType, OpenType and Embedded OpenType faces drawn |
| 2026-10-13 | minor | Owned corpus round-trip (parts and text). Notes-master header, footer, date and slide-number text is written back |
| 2026-10-13 | minor | Owned corpus for comments, links and actions, custom shows, rotated groups and inherited placeholders. Paragraph `defRPr` and action sounds (including custom-show return) survive a save |
| 2026-10-13 | minor | Owned corpus for text effects, body layout, theme effects, fills, bevels and connectors. A run's inner shadow, reflection and soft edge, and a bevel preset with no kept 3-D XML, survive a save |
| 2026-10-14 | minor | 3-D models (`.glb`) round-trip, including an alternate-content fallback picture; animation bounce end is read and written (`corpus_media.rs`) |
| 2026-10-14 | minor | Chart trendlines round-trip (`corpus_charts.rs`); merges, diagonal borders, columns, autofit, vertical text and WordArt warp are in that slice |
| 2026-10-12 | minor | EMF/WMF text and pattern brushes drawn; OLE (VML) and ActiveX previews read |
| 2026-10-12 | minor | Text warps rendered; their adjust values read and written |
| 2026-10-12 | trivial | Groups with a small legacy child coordinate space draw outlines, dashes, arrowheads and ink at their own width (#233) |
| 2026-10-10 | major | Created from PowerPoint 16.113.4's `CFBundleDocumentTypes`, `crates/pptx` reader/writer and tests, and the open round-trip issues |
| 2026-10-13 | minor | Comments: the writer emits the modern threaded part (`p188`) next to the flattened legacy one; the reader prefers the modern part and scales its EMU positions (#70) |
