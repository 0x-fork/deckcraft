//! DeckCraft's egui frontend: a PowerPoint-style UI over `deckcraft-engine`.
//!
//! The UI is thin: every action goes through [`SlideApp::run`], which handles UI commands
//! (views, panes, zoom) here and sends everything else to the engine. The ribbon, menus,
//! shortcuts, the command palette and the control channel all share that entry point.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]
#![forbid(unsafe_code)]

pub mod animtimeline;
pub mod backstage;
pub mod canvas;
pub mod capture;
pub mod control;
pub mod credits;
pub mod dialogs;
pub mod doc_window;
pub mod eyedropper;
pub mod fillui;
pub mod icons;
pub mod media;
pub mod menus;
pub mod notes;
pub mod opening;
pub mod panes;
mod preview;
pub mod qat;
pub mod ribbon;
pub mod selpane;
pub mod settings;
pub mod show;
pub mod sorter;
pub mod squiggles;
pub mod status;
pub mod textures;
pub mod theme;
pub mod thumbs;
pub mod toast;
pub mod uifonts;
#[cfg(not(target_arch = "wasm32"))]
mod updates_ui;
pub mod widgets;

use std::collections::HashMap;
use std::sync::mpsc::Receiver;

use deckcraft_engine::{Mods, Session, Target, UiRequest};
use deckcraft_model::{LayoutId, MasterId, SlideId};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

pub use control::{ControlRequest, ControlResponse};

pub type ReadFn = Box<dyn Fn(&str) -> Result<Vec<u8>, String>>;
pub type WriteFn = Box<dyn FnMut(&str, &[u8]) -> Result<(), String>>;
pub type PickFn = Box<dyn FnMut(&str) -> Option<String>>;
/// `(purpose, request id)`. The host shows a picker and later pushes an [`InboxEvent`]
/// carrying that same id. Completing a cancelled or unknown id does nothing: the file is
/// not applied to whichever document happens to be active.
pub type OpenAsyncFn = Box<dyn FnMut(&str, u64)>;
pub type DownloadFn = Box<dyn FnMut(&str, &[u8])>;

/// Temp key holding whether the current viewport is inside an egui UI pass.
fn ime_in_ui_pass_key(viewport: egui::ViewportId) -> egui::Id {
    egui::Id::unique(("deckcraft-ime-in-ui-pass", viewport))
}

/// Stable key marking that the IME pass hooks are installed.
fn ime_pass_hooks_key() -> egui::Id {
    egui::Id::unique(("deckcraft-ime-pass-hooks", "installed"))
}

/// Install the per-viewport UI-pass mode hooks exactly once for this [`egui::Context`].
///
/// Call this before the context's first `run_ui` (desktop/web shells and the test harness).
/// The callbacks receive `&mut Ui`; they obtain the context through `ui.ctx()` and capture no
/// context or app reference, so they cannot form a reference cycle. The flag distinguishes a
/// full `run_ui` pass (where `App::logic` runs before the UI that owns the final output) from a
/// standalone `run_logic` callback. It is only a mode marker, not an IME publication receipt.
pub fn install_ime_pass_hooks(ctx: &egui::Context) {
    if ctx.data(|d| d.get_temp::<bool>(ime_pass_hooks_key()).is_some_and(|v| v)) {
        return;
    }
    ctx.on_begin_pass(
        "deckcraft-ime-ui-pass-begin",
        std::sync::Arc::new(|ui: &mut egui::Ui| {
            let ctx = ui.ctx();
            let key = ime_in_ui_pass_key(ctx.viewport_id());
            ctx.data_mut(|d| d.insert_temp(key, true));
        }),
    );
    ctx.on_end_pass(
        "deckcraft-ime-ui-pass-end",
        std::sync::Arc::new(|ui: &mut egui::Ui| {
            let ctx = ui.ctx();
            let key = ime_in_ui_pass_key(ctx.viewport_id());
            ctx.data_mut(|d| d.insert_temp(key, false));
        }),
    );
    ctx.data_mut(|d| d.insert_temp(ime_pass_hooks_key(), true));
}

/// A file the host finished reading for the request `id` from [`SlideApp::begin_external_file`].
#[derive(Clone, Debug)]
pub struct InboxFile {
    /// Cancellation identity. A file whose id was cancelled, or never started, is ignored.
    pub id: u64,
    pub name: String,
    pub bytes: Vec<u8>,
}

/// What the host delivers through [`Inbox`] after an asynchronous read.
#[derive(Clone, Debug)]
pub enum InboxEvent {
    File(InboxFile),
    /// The picker was dismissed or the file could not be read. `id` will not be applied.
    Cancelled {
        id: u64,
    },
}

/// Files delivered asynchronously by the host (web file picker, dropped files).
pub type Inbox = std::sync::Arc<std::sync::Mutex<Vec<InboxEvent>>>;

/// The operation the user asked for. Kept with the request so a late completion does not
/// guess a command from the file name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FilePurpose {
    Open,
    Picture,
    Audio,
    Video,
    /// Change Picture: the picture replaces the selected one (`picture.change`).
    ChangePicture,
    /// A drop, or any other purpose: decide from the name and bytes.
    Infer,
}

/// Where in a presentation a file should land. Ids, not indices: slides can move while
/// the browser is still reading.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FilePlace {
    Slide(SlideId),
    Master(MasterId),
    Layout {
        master: MasterId,
        layout: LayoutId,
    },
    /// The handout or notes master (one per presentation, so no id is needed).
    NotesPage(Target),
    /// The document was open but had no current slide, master or layout.
    Unspecified,
}

/// The presentation (and place in it) captured when the request started.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FileTarget {
    /// Nothing was open. An insert makes its own document rather than using one opened
    /// while the file was being read.
    Absent,
    Document {
        doc: u64,
        place: FilePlace,
    },
}

struct PendingFile {
    id: u64,
    purpose: FilePurpose,
    target: FileTarget,
}

struct LocatedPlace {
    target: Target,
    slide: Option<usize>,
}

fn purpose_of(purpose: &str) -> FilePurpose {
    match purpose {
        "open" => FilePurpose::Open,
        "picture" => FilePurpose::Picture,
        "audio" => FilePurpose::Audio,
        "video" => FilePurpose::Video,
        _ => FilePurpose::Infer,
    }
}

fn is_presentation_name(lower: &str) -> bool {
    lower.ends_with(".deckcraft")
        || lower.ends_with(".slidecraft")
        || lower.ends_with(".pptx")
        || lower.ends_with(".potx")
        || lower.ends_with(".ppsx")
}

/// File → Open keeps the requested operation: a presentation opens, an outline becomes slides.
fn open_command(name: &str) -> Option<&'static str> {
    let lower = name.to_ascii_lowercase();
    if is_presentation_name(&lower) {
        Some("file.openBytes")
    } else if lower.ends_with(".txt") || lower.ends_with(".md") || lower.ends_with(".rtf") {
        Some("slide.fromOutline")
    } else {
        None
    }
}

/// A drop has no requested operation, so the name and bytes decide: a presentation opens,
/// then a recognised picture, sound or video is inserted, then an outline becomes slides.
fn infer_command(name: &str, bytes: &[u8]) -> Option<&'static str> {
    let lower = name.to_ascii_lowercase();
    if is_presentation_name(&lower) {
        return Some("file.openBytes");
    }
    let ct = deckcraft_engine::cmd::insert::content_type(name, bytes);
    if ct.starts_with("image/") {
        Some("insert.picture")
    } else if ct.starts_with("audio/") {
        Some("insert.audio")
    } else if ct.starts_with("video/") {
        Some("insert.video")
    } else if lower.ends_with(".txt") || lower.ends_with(".md") || lower.ends_with(".rtf") {
        Some("slide.fromOutline")
    } else {
        None
    }
}

fn command_for(purpose: FilePurpose, name: &str, bytes: &[u8]) -> Result<&'static str, String> {
    let cmd = match purpose {
        FilePurpose::Picture => Some("insert.picture"),
        FilePurpose::Audio => Some("insert.audio"),
        FilePurpose::Video => Some("insert.video"),
        FilePurpose::ChangePicture => deckcraft_engine::cmd::insert::content_type(name, bytes).starts_with("image/").then_some("picture.change"),
        FilePurpose::Open => open_command(name),
        FilePurpose::Infer => infer_command(name, bytes),
    };
    cmd.ok_or_else(|| format!("{name}: DeckCraft can't open this kind of file"))
}

/// How long a transient status message stays visible.
const STATUS_LINGER_MS: f64 = 6000.0;

/// Platform services injected by the host (desktop or web).
#[derive(Default)]
pub struct Services {
    /// Open-file dialog for a purpose (`open`, `picture`, `audio`, `video`) → path.
    pub pick_open: Option<PickFn>,
    /// Save dialog with a suggested name → path.
    pub pick_save: Option<PickFn>,
    pub read: Option<ReadFn>,
    pub write: Option<WriteFn>,
    /// Asynchronous open dialog (web). The callback receives the purpose and the request id
    /// from [`SlideApp::begin_external_file`]; the chosen file arrives through `inbox`.
    pub open_async: Option<OpenAsyncFn>,
    /// Hand bytes to the user as a named file (browser download).
    pub download: Option<DownloadFn>,
    pub inbox: Option<Inbox>,
    /// Image on the system clipboard (PNG bytes), when the host can read one.
    pub clipboard_image: Option<Box<dyn FnMut() -> Option<Vec<u8>>>>,
    /// HTML on the system clipboard (Pages, Keynote, and other apps that write `public.html`).
    pub clipboard_html: Option<Box<dyn FnMut() -> Option<String>>>,
    /// Put HTML and its plain-text alternative on the system clipboard. `true` when it landed.
    pub clipboard_set_html: Option<Box<dyn FnMut(&str, &str) -> bool>>,
    /// Audio output for media playback (cpal on desktop); without one media plays silently.
    pub audio_out: Option<Box<dyn deckcraft_media::AudioOut>>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ViewMode {
    #[default]
    Normal,
    Outline,
    Sorter,
    NotesPage,
    Reading,
}

/// Persisted UI state.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct UiState {
    pub brightness: theme::Brightness,
    pub tab: String,
    pub ribbon_collapsed: bool,
    pub view: ViewMode,
    pub notes: bool,
    pub notes_height: f32,
    pub thumbs_width: f32,
    /// Vertical scroll offset of the thumbnail pane, recorded each frame for the control
    /// channel and tests; not persisted.
    #[serde(skip)]
    pub thumbs_scroll: f32,
    /// Open task pane: `format`, `animation`, `selection`, `comments`, `designer`, `background`,
    /// `smartart` (Text Pane).
    pub pane: Option<String>,
    pub pane_width: f32,
    pub ruler: bool,
    pub gridlines: bool,
    /// Slides per page previewed in Handout Master view (1, 2, 3, 4, 6 or 9).
    pub handout_per_page: u8,
    pub guides: bool,
    pub smart_guides: bool,
    /// None = fit to window.
    pub zoom: Option<f32>,
    pub sorter_zoom: f32,
    pub recent: Vec<String>,
    pub presenter_view: bool,
    pub ui_scale: f32,
    /// Format Shape pane: tab (`fill`, `effects`, `size`, `picture`, `text`).
    pub format_tab: String,
    /// The Text Pane's current SmartArt line (Add Shape, Promote and Demote act on it).
    pub smartart_row: usize,
    /// A Text Pane line to focus once it is drawn (after Enter, Tab or Backspace rebuilt the list).
    pub smartart_focus: Option<usize>,
    /// The Text Pane's line drafts, for the SmartArt group they were read from.
    pub smartart_drafts: (u32, Vec<String>),
    pub grayscale: bool,
    /// Comment the Review Previous/Next buttons last landed on: `(slide, index)`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comment_focus: Option<(usize, usize)>,
    /// Quick Access Toolbar buttons, left to right.
    pub qat: Vec<qat::QatItem>,
}

impl Default for UiState {
    fn default() -> Self {
        UiState {
            brightness: theme::Brightness::Light,
            tab: "Home".into(),
            ribbon_collapsed: false,
            view: ViewMode::Normal,
            notes: true,
            notes_height: 52.0,
            thumbs_width: 236.0,
            thumbs_scroll: 0.0,
            pane: None,
            pane_width: 300.0,
            ruler: false,
            gridlines: false,
            handout_per_page: 6,
            guides: false,
            smart_guides: true,
            zoom: None,
            sorter_zoom: 1.0,
            recent: vec![],
            presenter_view: true,
            ui_scale: 1.0,
            format_tab: "fill".into(),
            smartart_row: 0,
            smartart_focus: None,
            smartart_drafts: (0, vec![]),
            grayscale: false,
            comment_focus: None,
            qat: qat::defaults(),
        }
    }
}

/// Text put on the system clipboard when the copied objects (a rectangle, a picture, slides…)
/// carry no text. egui-winit drops Ctrl+V when the system clipboard is empty, so without it the
/// shortcut never reached the session and only the context menu could paste objects (#182). An
/// invisible word joiner, so a paste into another app adds nothing visible.
const OBJECTS_MARKER: &str = "\u{2060}";

/// What a copy/cut leaves on the system clipboard for the session's plain-text rendering `t`.
fn system_clipboard_text(t: &str) -> String {
    if t.is_empty() { OBJECTS_MARKER.to_string() } else { t.to_string() }
}

/// The editor's zoom range (1 = 100 %).
pub const ZOOM_RANGE: std::ops::RangeInclusive<f32> = 0.1..=4.0;

impl UiState {
    /// The state with every value usable: `ui.json` is a file on disk, and a damaged or
    /// hand-edited one must not stop the window from opening. A zoom of 10,000 % asked the
    /// renderer for a texture larger than memory at start-up; panel sizes, scales and names are
    /// brought into range the same way, and the recent list is cut to its usual length.
    pub fn sanitized(mut self) -> UiState {
        let d = UiState::default();
        let within = |v: f32, lo: f32, hi: f32, default: f32| if v.is_finite() { v.clamp(lo, hi) } else { default };
        self.zoom = self.zoom.filter(|z| z.is_finite() && *z > 0.0).map(|z| z.clamp(*ZOOM_RANGE.start(), *ZOOM_RANGE.end()));
        self.sorter_zoom = within(self.sorter_zoom, 0.5, 3.0, d.sorter_zoom);
        self.ui_scale = within(self.ui_scale, 0.5, 3.0, d.ui_scale);
        self.notes_height = within(self.notes_height, 0.0, 2000.0, d.notes_height);
        self.thumbs_width = within(self.thumbs_width, 60.0, 1000.0, d.thumbs_width);
        self.pane_width = within(self.pane_width, 240.0, 520.0, d.pane_width);
        let short = |s: &String| s.len() <= 64;
        if !short(&self.tab) {
            self.tab = d.tab;
        }
        if !short(&self.format_tab) {
            self.format_tab = d.format_tab;
        }
        self.pane = self.pane.filter(short);
        self.recent.retain(|p| !p.is_empty() && p.len() <= 4096 && !p.contains('\0'));
        self.recent.dedup();
        self.recent.truncate(RECENT_MAX);
        self
    }
}

/// How many recent files File › Open and Home list.
pub const RECENT_MAX: usize = 12;

/// UI-only commands (id, label, shortcut, params).
pub const UI_COMMANDS: &[(&str, &str, Option<&str>, &str)] = &[
    ("view.normal", "Normal", None, "{}"),
    ("view.outline", "Outline View", None, "{}"),
    ("view.sorter", "Slide Sorter", None, "{}"),
    ("view.notesPage", "Notes Page", None, "{}"),
    ("view.reading", "Reading View", None, "{}"),
    ("view.notes", "Notes", None, "{on?: bool}"),
    ("view.ruler", "Ruler", None, "{on?: bool}"),
    ("view.gridlines", "Gridlines", Some("Shift+F9"), "{on?: bool}"),
    ("view.guides", "Guides", Some("Alt+F9"), "{on?: bool}"),
    ("view.gridOptions", "Grid Options…", None, "{spacing?: pt, snap?: bool, smartGuides?: bool, gridlines?: bool, guides?: bool}"),
    ("view.zoomIn", "Zoom In", Some("Cmd+="), "{}"),
    ("view.zoomOut", "Zoom Out", Some("Cmd+-"), "{}"),
    ("view.zoom", "Set Zoom", None, "{percent}"),
    ("view.zoomDialog", "Zoom…", None, "{}"),
    ("view.fit", "Fit to Window", None, "{}"),
    ("view.pane", "Task Pane", None, "{pane: format|animation|selection|comments|designer|background|smartart|null}"),
    ("review.thesaurusPane", "Thesaurus", Some("Shift+F7"), "{word?}: open the Thesaurus pane on the selected word"),
    ("view.tab", "Ribbon Tab", None, "{tab}"),
    ("view.collapseRibbon", "Collapse the Ribbon", Some("Cmd+F1"), "{}"),
    ("view.dark", "Dark Mode", None, "{on?: bool}"),
    ("view.grayscale", "Grayscale", None, "{on?: bool}"),
    ("view.newWindow", "New Window", None, "{on?: bool, slide?: index, delta?: int}"),
    ("show.start", "Slide Show", Some("F5"), "{from?: index, custom?: name}"),
    ("show.presenter", "Presenter View", None, "{on?: bool}"),
    ("show.end", "End Show", None, "{}"),
    ("animation.preview", "Preview Animations", None, "{on?: bool, from?: effect index}"),
    ("app.home", "Home", None, "{on?}"),
    ("app.backstage", "File", Some("Alt+F"), "{page?: home|new|open|info|saveAs|print|export|account, on?: bool}"),
    ("app.closePresentation", "Close", Some("Cmd+W"), "{}"),
    (
        "app.print",
        "Print",
        Some("Cmd+P"),
        "{layout?, perPage?, slides?, includeHidden?, frame?, paper?: letter|a4|legal|a3, orientation?: portrait|landscape}",
    ),
    ("app.about", "About DeckCraft", None, "{}"),
    ("app.checkUpdates", "Check for Updates…", None, "{}"),
    ("app.palette", "Command Palette", Some("Cmd+Shift+P"), "{}"),
    ("app.preferences", "Preferences…", Some("Cmd+,"), "{}"),
    ("app.openDialog", "Open…", None, "{purpose?}"),
    ("app.insertPictureDialog", "Picture from File…", None, "{}"),
    ("app.smartArtPictureDialog", "Insert Picture in SmartArt…", None, "{id?, index}"),
    ("app.changePictureDialog", "Change Picture…", None, "{}"),
    ("app.insertAudioDialog", "Audio from File…", None, "{}"),
    ("app.reuseSlidesDialog", "Reuse Slides…", None, "{keepFormatting?: bool}"),
    ("app.insertVideoDialog", "Video from File…", None, "{}"),
    ("app.saveAsDialog", "Save As…", None, "{}"),
    ("app.exportDialog", "Export…", None, "{}"),
    ("app.browseThemeDialog", "Browse for Themes…", None, "{}"),
    ("app.compareDialog", "Compare…", None, "{}"),
    ("app.dialog", "Open Dialog", None, "{id}"),
    ("app.pasteSpecial", "Paste Special…", Some("Cmd+Alt+V"), "{}"),
    ("app.qat", "Quick Access Toolbar", None, "{add: command id, params?, label?, icon?} | {remove: id, params?} | {reset: true} → [buttons]"),
    ("app.find", "Find…", Some("Cmd+F"), "{}"),
    ("app.replace", "Replace…", Some("Cmd+H"), "{}"),
    ("app.links", "Community Links", None, "{}"),
];

pub struct Perf {
    pub frame_ms: f64,
    pub render_ms: f64,
    pub fps: f64,
}

/// Playhead for the in-canvas animation preview.
struct AnimPreview {
    uid: u64,
    slide: usize,
    /// `Arc` address of the slide when preview started. An edit replaces the slide and stops playback.
    slide_ptr: usize,
    /// egui time when the first frame latched the clock (`None` until then).
    started: Option<f64>,
    /// Where on the play-everything clock playback began (Play From the selected effect).
    from: f64,
    elapsed: f64,
    timeline: deckcraft_anim::Timeline,
}

/// The app state a queued synthetic control-channel input is meant for.
///
/// A batch is injected only while this identity is unchanged. eframe carries a raw-input batch
/// from a logic-only (hidden) pass into the next full pass, so the hook also keeps the exact event
/// list it appended and retracts it when the identity no longer matches: the stale click never
/// reaches the new document, slide or canvas layout.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct InputTarget {
    documents: usize,
    active: Option<u64>,
    slide: Option<usize>,
    view: ViewMode,
    zoom: Option<u32>,
    canvas: Option<[u32; 4]>,
    canvas_scale: u32,
    /// Dialog, palette or Backstage view over the canvas. A modal transition changes what a
    /// queued window-point click means even when the document and canvas layout are unchanged.
    dialog: Option<egui::Id>,
    /// Command palette query *and* highlighted entry: the keyboard target moves with the
    /// selection even when the query does not.
    palette: Option<egui::Id>,
    backstage: Option<egui::Id>,
    /// Active ribbon tab, whether the ribbon is collapsed, and its horizontal scroll offset.
    /// A window-point click means a different button when any of them changes.
    ribbon_tab: egui::Id,
    ribbon_collapsed: bool,
    ribbon_scroll: u32,
    /// Full-screen slide show: the show instance, slide, step and the layout/mode state that
    /// changes what a click at the same window point does.
    show: Option<show::ShowInputState>,
    /// Background-open progress window, identified by the file *and* the attempt.
    opening: Option<(egui::Id, u64)>,
}

pub struct SlideApp {
    /// Shape Fill's last colour (a command colour parameter): the button applies it again. Accent 1 until one is picked.
    pub last_fill: serde_json::Value,
    /// Shape Outline's last colour, likewise. Text 1 until one is picked.
    pub last_outline: serde_json::Value,
    /// Table Design's border pen (`color`, `width`, `dash` for `table.borders`).
    pub table_pen: serde_json::Value,
    pub session: Session,
    pub services: Services,
    pub ui: UiState,
    pub textures: textures::Textures,
    pub control_rx: Option<Receiver<ControlRequest>>,
    /// Spelling squiggles, checked on a worker thread.
    pub squiggles: squiggles::Squiggles,
    pub dialog: Option<dialogs::Dialog>,
    /// The word looked up in the Thesaurus pane.
    pub thesaurus_query: String,
    pub show: Option<show::Show>,
    /// Current slide's animations playing on the editor canvas (not the full-screen show).
    anim_preview: Option<AnimPreview>,
    /// Where the Reuse Slides picker inserts: the presentation and slide current when it opened.
    reuse_target: Option<FileTarget>,
    /// Audio/video playback (editor control bar, slide show, `media.*` commands).
    pub media: media::MediaHost,
    pub status: Option<(String, f64)>,
    /// A presentation being read in the background, with its progress window ([`opening`]).
    pub opening: Option<opening::Opening>,
    /// A brief notice over the bottom of the window (missing fonts on opening a file).
    pub toast: Option<toast::Toast>,
    /// Documents (by uid) whose missing fonts have been mentioned.
    fonts_noted: std::collections::HashSet<u64>,
    pub perf: Perf,
    /// Synthetic input events (control channel `ui.click` etc.), injected one per frame.
    pub(crate) synthetic: Vec<egui::Event>,
    /// A synthetic batch has been injected but the UI pass has not acknowledged it. While this
    /// is set no second batch is injected, so a hidden/discarded frame cannot replay an event.
    synthetic_pending: bool,
    /// Events queued while an older batch is still pending. They belong to the target at the time
    /// they were queued, so a later document change cannot make them fall in behind the old batch
    /// and get cleared with it.
    synthetic_next: Vec<egui::Event>,
    synthetic_next_target: Option<InputTarget>,
    /// The exact events appended to the pending `RawInput`; retracted on a target change.
    synthetic_injected: Vec<egui::Event>,
    /// Where `synthetic_injected` was appended in `RawInput::events`. Retraction uses this
    /// index so an identical host event earlier in the batch is never removed instead.
    synthetic_injected_at: Option<usize>,
    /// Identity of the last injected synthetic batch (for diagnostics).
    synthetic_generation: u64,
    /// Document/slide/canvas identity the queued/injected batch belongs to.
    synthetic_target: Option<InputTarget>,
    /// Focus identity the pending batch was injected with; a focus change retracts it.
    synthetic_focus: Option<egui::Id>,
    /// `RawInput::time` of the pass the batch was injected into. A logic-only callback leaves
    /// `InputState` from the previous pass; comparing this value keeps retraction from draining
    /// an unrelated batch out of that stale input.
    synthetic_injected_time: Option<f64>,
    /// Horizontal scroll offset (as `f32` bits) of each ribbon tab, recorded for
    /// [`InputTarget`]. Scroll state is per tab in egui, so the identity is too.
    ribbon_scroll: HashMap<String, u32>,
    /// Monotonic identity for a new slide show instance (`start_show`).
    show_instance: u64,
    /// Monotonic identity for a new background-open attempt.
    opening_generation: u64,
    /// Canvas screen rect and points-per-slide-point scale (for control & tests).
    pub canvas_rect: Option<egui::Rect>,
    /// The Eyedropper is waiting for a click on the slide.
    pub eyedropper: Option<eyedropper::State>,
    pub canvas_scale: f32,
    pub slide_rect: Option<egui::Rect>,
    /// Where the canvas texture (the slide and the pasteboard drawn around it) is on screen.
    pub canvas_region: Option<egui::Rect>,
    pub palette: Option<(String, usize)>,
    /// Integrated title bar (macOS traffic lights over our chrome).
    pub integrated_titlebar: bool,
    /// The File tab's full-window view (Backstage). Over a presentation it replaces the ribbon,
    /// slides and status bar until Esc or Back; with nothing open it is the start screen. Opening,
    /// creating or closing a presentation leaves it.
    pub backstage: Option<backstage::Backstage>,
    pending_urls: Vec<String>,
    #[cfg(not(target_arch = "wasm32"))]
    updates_rt: updates_ui::Runtime,
    /// Text for the system clipboard from a Copy or Cut command, handed over on the next frame.
    pending_copy: Option<String>,
    /// Bounded screenshot completion slots for the control channel (egui 0.37 callbacks).
    capture: capture::Capture,
    styled: bool,
    restyle: bool,
    fonts_ready: bool,
    last_time: f64,
    /// When AutoRecover data was last written (monotonic seconds, not wall time).
    pub last_recovery: f64,
    /// Quit was confirmed (changes saved or discarded): let the window close.
    pub quit_confirmed: bool,
    pub(crate) notes_buf: (u64, usize, notes::Draft),
    /// Faces borrowed from the font database for text the interface fonts can't draw (#101).
    pub ui_fonts: uifonts::UiFonts,
    /// Thumbnail pane drag (slide index, current drop index).
    pub(crate) thumb_drag: Option<(usize, usize)>,
    /// Selection pane: folded groups, renaming, drag and drop.
    pub selpane: selpane::State,
    /// Arrange ▸ Align: Align to Slide (`true`) or Align Selected Objects, ticked for that selection.
    pub(crate) align_to: Option<(Vec<deckcraft_model::ShapeId>, bool)>,
    /// View › New Window: a second window on one presentation. `None` when it is closed.
    pub doc_window: Option<doc_window::DocWindow>,
    /// Slide Sorter: folded sections, inline rename, dragging a whole section.
    pub sorter: sorter::State,
    /// The slide thumbnail pane was the last thing clicked, so ⌘A selects slides, not shapes.
    pub(crate) slides_focused: bool,
    /// A V key press or a Paste event arrived since the last V release (see `keyboard`).
    v_seen: bool,
    /// Which recent paths exist on disk, probed once per path per session (a dead network share
    /// must not stall every frame). Not persisted.
    recent_checked: HashMap<String, bool>,
    /// A trackpad twist is in progress (one undo step when it ends).
    twisting: bool,
    /// Next cancellation identity for a browser pick or a dropped file.
    next_file_id: u64,
    /// Picks and drops whose bytes have not arrived yet.
    pending_files: Vec<PendingFile>,
    /// Owner/generation/cause bookkeeping for the canvas text IME producer.
    ime_tracker: egui::ime_context::ImeContextTracker,
    /// Canvas IME geometry for this pass: the caret while editing, or the selected shape.
    ime_pending: Option<egui::Rect>,
    /// The viewport this UI pass belongs to (not the last immediate child viewport drawn).
    ime_frame_viewport: Option<egui::ViewportId>,
    /// Viewport whose tracker state is current. Closing the editing window or the last
    /// presentation releases it (`ImeContextTracker::end_viewport`), so a later owner in that
    /// viewport starts from a fresh context.
    pub(crate) ime_viewport: Option<egui::ViewportId>,
    /// The platform composition we know about: the owner its pre-edit began in and whether the
    /// application has since settled it. See `ime_event`.
    ime_composition: Option<ImeComposition>,
    /// Empty preedit cleared visible model text, but its terminal mutation may arrive later.
    ime_terminal_pending: Option<ImeTerminalPending>,
    /// A composition generation the application settled. Events queued for it in the settling
    /// input batch stay stale until the pass that publishes its platform interrupt (or an accepted
    /// pass with no descriptor, which resets the backend); the tombstone is retired there so later
    /// commit-only input is not lost (see [`ImeCancelled`]).
    ime_cancelled: Option<ImeCancelled>,
    /// Monotonic composition generation; every accepted pre-edit starts a new one.
    ime_generation: u64,
    /// The last [`Session::composition_settles`] value seen.
    ime_settles_seen: u64,
    /// A composition the application settled has not been announced to the platform yet.
    ime_interrupt_pending: bool,
    /// Token-bound reset lifetime for the owning root viewport's custom canvas editor. The map is
    /// keyed by viewport so a request or completion stamped for another viewport is never bound;
    /// render-only child viewports are rejected before they can create or overwrite an entry.
    ime_reset: HashMap<egui::ViewportId, ImeResetPhase>,
}

/// The platform's in-progress IME composition, as far as this editor tracks it.
#[derive(Clone, Copy, Debug)]
struct ImeComposition {
    /// The target whose text the input method started composing in.
    owner: egui::Id,
    /// Generation of this composition; a new pre-edit after a settled generation starts the next.
    generation: u64,
    /// The application settled the composition itself (a click, a key, a command). The platform
    /// may still hold it and must be interrupted rather than believed.
    settled: bool,
}

/// A settled composition generation whose platform copy may still deliver the rest of the
/// input batch it was queued in.
///
/// The tombstone lives only until the pass that publishes this generation's
/// `should_interrupt_composition` in its platform output, because that is the point at which the
/// backend resets (disables and re-enables) the platform composition. Events earlier in the same
/// batch still belong to the old composition and are rejected; from the next batch on egui's
/// events carry no generation, so an event the OS queued across the reset cannot be distinguished
/// from a legitimate new composition (see `ime_event`).
#[derive(Clone, Copy, Debug)]
struct ImeCancelled {
    owner: egui::Id,
    generation: u64,
}

/// A cleared preedit still owns its separately delivered terminal delete/commit. Application
/// work invalidates this snapshot; publishing an interrupt is not delivery of that terminal.
/// Keep a cancelled record until the terminal commit (or platform enable/disable) is observed.
struct ImeTerminalPending {
    owner: egui::Id,
    revision: u64,
    anchor: (usize, usize),
    caret: (usize, usize),
    cancelled: bool,
    /// An interruption has been observed since this guard was recorded. This stays distinct from
    /// the origin snapshot even when there is no active destination to stamp: `interrupted_at ==
    /// None` then means "interrupted with no target", not "not interrupted yet".
    interrupted: bool,
    /// The destination at which this terminal guard was last interrupted. Kept separately from
    /// the retired origin above: repeated passes at the same interrupted destination must not
    /// request another reset, while a genuinely newer target/selection/revision change still
    /// supersedes the current fence. When `interrupted` is false, `None` means the origin snapshot
    /// is still the reference; once interrupted, it means the interruption left no destination.
    interrupted_at: Option<ImeTerminalStamp>,
}

/// The live target/selection stamp used to detect a new application interruption without
/// discarding the retired composition's origin. Owner is optional because an interruption can
/// remove the canvas target entirely.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ImeTerminalStamp {
    owner: Option<egui::Id>,
    revision: u64,
    anchor: Option<(usize, usize)>,
    caret: Option<(usize, usize)>,
}

/// The live canvas target and caret/selection stamp a reset fence is bound to.
///
/// The retired composition's origin is deliberately absent: an app-originated interruption can
/// move the destination, and the replacement request belongs to the current destination.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ImeDestination {
    owner: egui::Id,
    revision: u64,
    anchor: Option<(usize, usize)>,
    caret: Option<(usize, usize)>,
}

/// The actual custom descriptor identity a reset request is allowed to acknowledge.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ImeDescriptorKey {
    owner: egui::Id,
    context_generation: u64,
    purpose: egui::IMEPurpose,
}

impl ImeDescriptorKey {
    /// Construct the exact custom-IME descriptor identity from owner, context generation, and
    /// purpose.
    fn new(owner: egui::Id, context_generation: u64, purpose: egui::IMEPurpose) -> Self {
        Self { owner, context_generation, purpose }
    }
}

/// Explicit lifetime of the canvas's token-bound IME reset fence.
///
/// This is separate from the composition provenance/tombstone state above. Only a matching
/// ordered `ImeResetCompleted` clears `Awaiting`; `Enabled`, `Disabled`, empty preedits, idle
/// passes and timers never do. `Suspended` retains no acceptable old id and may carry one newly
/// requested id that a later eligible pass can bind.
#[derive(Clone, Copy, Debug)]
enum ImeResetPhase {
    /// No interrupted composition guard.
    Clear,
    /// The exact newest request has been published for this destination/descriptor.
    Awaiting { id: egui::ImeResetId, viewport: egui::ViewportId, destination: ImeDestination, output_context: ImeDescriptorKey },
    /// Guarded but no old id is acceptable; `request` is a new, not-yet-bound fence.
    Suspended { viewport: egui::ViewportId, needs_rearm: bool, request: Option<egui::ImeResetId> },
    /// No fence could be allocated; remain fail-closed without retrying on idle output.
    AllocationFailed { viewport: egui::ViewportId, destination: ImeDestination, output_context: ImeDescriptorKey },
}

impl ImeResetPhase {
    /// Only `Clear` admits custom-canvas input; awaiting, suspended, and allocation-failed phases
    /// remain fail-closed.
    fn blocks_custom_input(self) -> bool {
        !matches!(self, Self::Clear)
    }
}

fn grid_options_touched(p: &Value) -> bool {
    ["spacing", "snap", "smartGuides", "gridlines", "guides"].iter().any(|k| p.get(*k).is_some())
}

fn grid_flag(p: &Value, key: &str) -> Result<Option<bool>, String> {
    match p.get(key) {
        None => Ok(None),
        Some(Value::Bool(b)) => Ok(Some(*b)),
        Some(_) => Err(format!("`{key}` must be true or false")),
    }
}

impl SlideApp {
    /// Build application UI state, taking ownership of audio output and initialising media, dialogs,
    /// synthetic input, and timing fields.
    pub fn new(session: Session, mut services: Services) -> Self {
        let audio_out = services.audio_out.take();
        SlideApp {
            last_fill: serde_json::json!("accent1"),
            last_outline: serde_json::json!("tx1"),
            table_pen: serde_json::json!({"width": 1.0}),
            session,
            services,
            ui: UiState::default(),
            textures: textures::Textures::default(),
            control_rx: None,
            squiggles: Default::default(),
            dialog: None,
            thesaurus_query: String::new(),
            show: None,
            anim_preview: None,
            reuse_target: None,
            media: media::MediaHost::new(audio_out),
            status: None,
            opening: None,
            toast: None,
            fonts_noted: Default::default(),
            perf: Perf { frame_ms: 0.0, render_ms: 0.0, fps: 0.0 },
            synthetic: vec![],
            synthetic_pending: false,
            synthetic_next: vec![],
            synthetic_next_target: None,
            synthetic_injected: vec![],
            synthetic_injected_at: None,
            synthetic_generation: 0,
            synthetic_target: None,
            synthetic_focus: None,
            synthetic_injected_time: None,
            ribbon_scroll: HashMap::new(),
            show_instance: 0,
            opening_generation: 0,
            canvas_rect: None,
            eyedropper: None,
            canvas_scale: 1.0,
            slide_rect: None,
            canvas_region: None,
            palette: None,
            integrated_titlebar: false,
            backstage: None,
            pending_urls: vec![],
            #[cfg(not(target_arch = "wasm32"))]
            updates_rt: updates_ui::Runtime::default(),
            pending_copy: None,
            capture: capture::Capture::default(),
            styled: false,
            restyle: true,
            fonts_ready: false,
            last_time: 0.0,
            last_recovery: 0.0,
            quit_confirmed: false,
            notes_buf: (0, usize::MAX, notes::Draft::default()),
            ui_fonts: uifonts::UiFonts::default(),
            thumb_drag: None,
            selpane: selpane::State::default(),
            align_to: None,
            doc_window: None,
            sorter: sorter::State::default(),
            slides_focused: false,
            v_seen: false,
            recent_checked: HashMap::new(),
            twisting: false,
            next_file_id: 1,
            pending_files: Vec::new(),
            ime_tracker: egui::ime_context::ImeContextTracker::new(),
            ime_pending: None,
            ime_frame_viewport: None,
            ime_viewport: None,
            ime_composition: None,
            ime_terminal_pending: None,
            ime_cancelled: None,
            ime_generation: 0,
            ime_settles_seen: 0,
            ime_interrupt_pending: false,
            ime_reset: HashMap::new(),
        }
    }

    pub fn with_control(mut self, rx: Receiver<ControlRequest>) -> Self {
        self.control_rx = Some(rx);
        self
    }

    pub fn set_status(&mut self, msg: impl Into<String>) {
        self.status = Some((msg.into(), now_ms()));
    }

    /// Turn on the automatic check for a newer release at start-up (desktop shell only; tests and
    /// the web build never touch the network on their own).
    #[cfg(not(target_arch = "wasm32"))]
    pub fn enable_update_checks(&mut self) {
        updates_ui::enable(self);
    }

    /// A message that must be seen wherever the user is: a toast, which also shows over the start
    /// screen and the File tab, where there is no status bar.
    pub fn notify(&mut self, msg: impl Into<String>) {
        self.toast = Some(toast::Toast { text: msg.into(), at: now_ms() });
    }

    /// The transient status message while it is fresh; clears it once stale. Both the status bar
    /// and the start screen draw it — the start screen has no status bar, and errors raised there
    /// (a missing recent file, a failed sample) used to vanish (issue #172).
    pub fn fresh_status(&mut self) -> Option<String> {
        let (msg, at) = self.status.clone()?;
        if now_ms() - at < STATUS_LINGER_MS {
            Some(msg)
        } else {
            self.status = None;
            None
        }
    }

    /// Does this recent path exist on disk? Probed once per path per session and only on
    /// desktop (the web build has no file system to ask): a stat on a dead network share can
    /// block for seconds, so it must not repeat on every frame. A stale answer is harmless —
    /// opening still tries and reports, and a successful open re-probes.
    pub fn recent_exists(&mut self, path: &str) -> bool {
        #[cfg(target_arch = "wasm32")]
        {
            let _ = path;
            true
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            if let Some(ok) = self.recent_checked.get(path) {
                return *ok;
            }
            // A stat on a dead network location (UNC share, URL) can block for the OS timeout,
            // so remote-looking paths are never probed: they stay unmarked and are judged when
            // opened. A disconnected mapped drive would still stall once — accepted.
            if path.starts_with("\\\\") || path.contains("://") {
                return true;
            }
            let ok = std::path::Path::new(path).is_file();
            if self.recent_checked.len() > 64 {
                let live: std::collections::HashSet<&String> = self.ui.recent.iter().collect();
                self.recent_checked.retain(|p, _| live.contains(p));
            }
            self.recent_checked.insert(path.to_string(), ok);
            ok
        }
    }

    /// Drop a path from the Recent list (its file is gone or the user asked for it).
    pub fn forget_recent(&mut self, path: &str) {
        self.ui.recent.retain(|p| p != path);
        self.recent_checked.remove(path);
    }

    /// Run a command by id (UI commands here, everything else in the engine). Errors show in the
    /// status bar and are returned.
    pub fn run(&mut self, id: &str, params: Value) -> Result<Value, String> {
        let r = if UI_COMMANDS.iter().any(|c| c.0 == id) {
            self.run_ui(id, &params)
        } else {
            self.session.execute(id, &params).map_err(|e| e.to_string())
        };
        match &r {
            // Copy and Cut from the ribbon, menus, the slide's context menu or the control channel
            // put the text on the system clipboard, as Ctrl+C and Ctrl+X do (#431).
            Ok(v) if matches!(id, "edit.copy" | "edit.cut") => {
                if let Some(t) = v.get("text").and_then(Value::as_str) {
                    self.pending_copy = Some(t.to_string());
                }
            }
            Ok(_) => {}
            Err(e) => self.set_status(e.clone()),
        }
        self.drain_requests();
        r
    }

    /// Return the current document count plus the active document uid, so callers can detect a
    /// replacement document rather than only an index change.
    pub(crate) fn docs_key(&self) -> (usize, Option<u64>) {
        // The document uid, not just its index: closing one presentation and opening another
        // must not look like the same document to the Backstage view or a queued input batch.
        (self.session.documents().len(), self.session.active().map(|d| d.uid))
    }

    /// What a queued synthetic event is meant for. The control protocol hands out window
    /// coordinates, so a document, slide, view, zoom or canvas-layout change between queueing and
    /// the next UI pass makes the event stale.
    fn input_target(&self) -> InputTarget {
        let bits = |v: f32| if v.is_finite() { v.to_bits() } else { 0 };
        InputTarget {
            documents: self.session.documents().len(),
            active: self.session.active().map(|d| d.uid),
            slide: self.session.active().map(|d| d.selection.slide),
            view: self.ui.view,
            zoom: self.ui.zoom.map(bits),
            canvas: self.canvas_rect.map(|r| [bits(r.min.x), bits(r.min.y), bits(r.max.x), bits(r.max.y)]),
            canvas_scale: bits(self.canvas_scale),
            dialog: self.dialog.as_ref().map(|d| egui::Id::unique(("dialog", d.id.as_str()))),
            palette: self.palette.as_ref().map(|(q, sel)| egui::Id::unique(("palette", q.as_str(), *sel))),
            backstage: self.backstage.as_ref().map(|b| egui::Id::unique(("backstage", b.page.id()))),
            ribbon_tab: egui::Id::unique(("ribbon-tab", self.ui.tab.as_str())),
            ribbon_collapsed: self.ui.ribbon_collapsed,
            ribbon_scroll: self.ribbon_scroll.get(&self.ui.tab).copied().unwrap_or(0),
            show: self.show.as_ref().map(show::Show::input_state),
            opening: self.opening.as_ref().map(|o| (egui::Id::unique(("opening", o.path.as_str())), o.generation)),
        }
    }

    /// Queue one synthetic control-channel event. The target is recorded when the batch starts,
    /// before any host command in the same logic pass can change the document.
    pub fn queue_input(&mut self, event: egui::Event) {
        let target = self.input_target();
        // A batch that was already injected keeps the target it was injected for until its UI
        // pass consumes it or retracts it. New events queued meanwhile belong to the current
        // target, so they wait in a separate batch instead of joining the old one and being
        // cleared with it when that batch turns out to be stale.
        if self.synthetic_pending {
            if self.synthetic_next_target != Some(target) {
                self.synthetic_next.clear();
                self.synthetic_next_target = Some(target);
            }
            self.synthetic_next.push(event);
            return;
        }
        if self.synthetic.is_empty() {
            self.synthetic_target = Some(target);
        } else if self.synthetic_target != Some(target) {
            // The unsent queue belongs to a target that has already changed; drop it (and any
            // older batch still waiting behind it) rather than appending a click intended for the
            // new target to a stale batch.
            self.synthetic.clear();
            self.synthetic_next.clear();
            self.synthetic_next_target = None;
            self.synthetic_target = Some(target);
        }
        self.synthetic.push(event);
    }

    /// Events admitted by the control channel but not yet injected (including a batch waiting
    /// behind the one currently in flight).
    pub(crate) fn synthetic_queued_len(&self) -> usize {
        self.synthetic.len() + self.synthetic_next.len()
    }

    /// The File tab's view is open over a presentation (not just the start screen with nothing
    /// open).
    pub fn backstage_open(&self) -> bool {
        self.backstage.is_some() && self.session.active().is_some()
    }

    /// Opens the File tab's view on `page` (keeping its state when it is already open).
    pub fn open_backstage(&mut self, page: backstage::Page) {
        let key = self.docs_key();
        match &mut self.backstage {
            Some(b) => b.go(page),
            None => self.backstage = Some(backstage::Backstage::new(page, key)),
        }
    }

    /// Back to the presentation (no effect on the start screen, where nothing is open).
    pub fn close_backstage(&mut self) {
        if self.session.active().is_some() {
            self.backstage = None;
        }
    }

    /// Keeps the File tab's view in step with the open presentations: it closes when one is
    /// opened, created or closed (the result shows), and it is the start screen when none is open.
    fn sync_backstage(&mut self) {
        let key = self.docs_key();
        let has_doc = self.session.active().is_some();
        let stale = self.backstage.as_ref().is_some_and(|b| b.docs != key);
        if has_doc && stale {
            self.backstage = None;
        } else if !has_doc && (stale || self.backstage.is_none()) {
            self.backstage = Some(backstage::Backstage::new(backstage::Page::Home, key));
        }
    }

    /// File › Close and Ctrl+W: closes the active presentation, asking first when it has unsaved
    /// changes (Save, Don't Save, Cancel), as PowerPoint does.
    pub fn close_presentation(&mut self) {
        match self.session.active() {
            None => {}
            Some(d) if d.is_dirty() => self.dialog = Some(dialogs::Dialog::new("close")),
            Some(_) => {
                let _ = self.run("file.close", json!({}));
            }
        }
    }

    /// The file name a save or export suggests: the presentation's title with `ext`.
    fn suggested_name(&self, ext: &str) -> String {
        let name = self.session.active().map(|d| d.title()).unwrap_or_else(|| "Presentation".into());
        format!("{name}.{ext}")
    }

    /// Exports the active presentation as `fmt` (`pdf`, `png`, `pngAll`, `jpeg`, `pptx`,
    /// `deckcraft`, `outline`) through `file.export`: the save picker on the desktop, a download
    /// on the web. `extra` adds `file.export` params (the PDF's layout, slides…). The
    /// presentation keeps its own file. Returns whether a file was written or handed over.
    pub fn export_as(&mut self, fmt: &str, extra: Value) -> bool {
        let Some(slide) = self.session.active().map(|d| d.selection.slide) else { return false };
        let (ext, format, all) = match fmt {
            "pdf" => ("pdf", "pdf", false),
            "png" => ("png", "png", false),
            "pngAll" => ("png", "png", true),
            "jpeg" | "jpg" => ("jpg", "jpeg", false),
            "pptx" => ("pptx", "pptx", false),
            "outline" | "txt" => ("txt", "outline", false),
            "rtf" => ("rtf", "rtf", false),
            "deckcraft" => ("deckcraft", "deckcraft", false),
            "gif" => ("gif", "gif", false),
            other => {
                self.set_status(format!("DeckCraft can't export “{other}”"));
                return false;
            }
        };
        let suggested = self.suggested_name(ext);
        if let Some(pick) = self.services.pick_save.as_mut() {
            let Some(path) = pick(&suggested) else { return false };
            let mut params = json!({"path": path, "format": format, "all": all});
            if let (Some(o), Some(x)) = (params.as_object_mut(), extra.as_object()) {
                o.extend(x.clone());
            }
            return match self.run("file.export", params) {
                Ok(_) => {
                    self.set_status(format!("Exported {path}"));
                    true
                }
                Err(_) => false,
            };
        }
        let Some(doc) = self.session.active().map(|d| d.doc.clone()) else { return false };
        let bytes = match format {
            "png" | "jpeg" => {
                // Every slide for "all slides", one download each, named like the desktop export (#64).
                let name = self.session.active().map(|d| d.title()).unwrap_or_else(|| "Presentation".into());
                let mut any = false;
                let transparent = extra.get("transparentBackground").and_then(Value::as_bool).unwrap_or(false);
                for (file, bytes) in web_image_files(&doc, slide, all, &name, ext, transparent) {
                    any = self.download(&file, Ok(bytes)) || any;
                }
                return any;
            }
            "pdf" => deckcraft_engine::cmd::file::pdf_bytes(&doc, &extra),
            "gif" => deckcraft_engine::cmd::file::gif_bytes(&doc, &extra),
            other => deckcraft_engine::cmd::file::save_bytes(&doc, other),
        };
        self.download(&suggested, bytes)
    }

    /// Hands `bytes` to the browser as a download (web); false (with the reason in the status
    /// bar) when there are no bytes or no way to download.
    fn download(&mut self, name: &str, bytes: deckcraft_engine::Result<Vec<u8>>) -> bool {
        match (bytes, self.services.download.as_mut()) {
            (Ok(b), Some(dl)) => {
                dl(name, &b);
                true
            }
            (Err(e), _) => {
                self.set_status(e.to_string());
                false
            }
            (Ok(_), None) => {
                self.set_status("Saving a copy needs a file picker, which this window doesn't have.");
                false
            }
        }
    }

    /// File › Print: DeckCraft prints through a PDF. On the desktop the PDF (with the Print page's
    /// layout, slides and options) is written to the temporary folder and opened in the system's
    /// PDF viewer, which has the printers; on the web it downloads.
    pub fn print_pdf(&mut self, params: Value) -> bool {
        let Some((title, doc)) = self.session.active().map(|d| (d.title(), d.doc.clone())) else { return false };
        let mut params = if params.is_object() { params } else { json!({}) };
        if self.services.pick_save.is_none() {
            // Web: the browser hands the PDF to the user.
            let bytes = deckcraft_engine::cmd::file::pdf_bytes(&doc, &params);
            return self.download(&format!("{title}.pdf"), bytes);
        }
        let safe: String = title.chars().map(|c| if c.is_alphanumeric() || c == ' ' || c == '-' { c } else { '_' }).collect();
        let path = std::env::temp_dir().join(format!("{safe} (print).pdf")).to_string_lossy().to_string();
        if let Some(o) = params.as_object_mut() {
            o.insert("path".into(), json!(path));
            o.insert("format".into(), json!("pdf"));
        }
        match self.run("file.export", params) {
            Ok(_) => {
                self.open_url(&format!("file:///{}", path.replace('\\', "/").trim_start_matches('/')));
                self.set_status("Opened the presentation as a PDF to print");
                true
            }
            Err(_) => false,
        }
    }

    fn flag(&mut self, params: &Value, cur: bool) -> bool {
        params.get("on").and_then(Value::as_bool).unwrap_or(!cur)
    }

    fn run_ui(&mut self, id: &str, p: &Value) -> Result<Value, String> {
        match id {
            "view.normal" => self.ui.view = ViewMode::Normal,
            "view.outline" => self.ui.view = ViewMode::Outline,
            "view.sorter" => self.ui.view = ViewMode::Sorter,
            "view.notesPage" => self.ui.view = ViewMode::NotesPage,
            "view.reading" => self.start_show(self.session.active().map(|d| d.selection.slide).unwrap_or(0), true),
            "view.notes" => self.ui.notes = self.flag(p, self.ui.notes),
            "view.ruler" => self.ui.ruler = self.flag(p, self.ui.ruler),
            "view.gridlines" => self.ui.gridlines = self.flag(p, self.ui.gridlines),
            "view.guides" => {
                self.ui.guides = self.flag(p, self.ui.guides);
                // A presentation without guides gets PowerPoint's two centre guides.
                if self.ui.guides && self.session.active().is_some_and(|d| d.doc.guides.is_empty()) {
                    let _ = self.session.execute("guide.add", &json!({"defaults": true}));
                }
            }
            "view.gridOptions" => {
                if grid_options_touched(p) {
                    let spacing = match p.get("spacing") {
                        None => None,
                        Some(raw) => {
                            let n = raw.as_f64().filter(|v| v.is_finite()).ok_or("`spacing` must be a finite number of points")?;
                            if !(1.0..=288.0).contains(&n) {
                                return Err("`spacing` must be from 1 to 288 points".into());
                            }
                            Some(n)
                        }
                    };
                    let snap = grid_flag(p, "snap")?;
                    let smart = grid_flag(p, "smartGuides")?;
                    let gridlines = grid_flag(p, "gridlines")?;
                    let guides = grid_flag(p, "guides")?;
                    if let Some(n) = spacing {
                        self.session.prefs.grid_spacing = n;
                    }
                    if let Some(b) = snap {
                        self.session.prefs.snap_to_grid = b;
                    }
                    if let Some(b) = smart {
                        self.session.prefs.smart_guides = b;
                        self.ui.smart_guides = b;
                    }
                    if let Some(b) = gridlines {
                        self.ui.gridlines = b;
                    }
                    if let Some(b) = guides {
                        self.ui.guides = b;
                        // As View › Guides: a presentation without guides gets the two centre guides.
                        if b && self.session.active().is_some_and(|d| d.doc.guides.is_empty()) {
                            let _ = self.session.execute("guide.add", &json!({"defaults": true}));
                        }
                    }
                } else {
                    self.dialog = Some(dialogs::Dialog::new("gridOptions"));
                }
                return Ok(json!({
                    "spacing": self.session.prefs.grid_spacing,
                    "snap": self.session.prefs.snap_to_grid,
                    "smartGuides": self.session.prefs.smart_guides,
                    "gridlines": self.ui.gridlines,
                    "guides": self.ui.guides,
                }));
            }
            "view.zoomIn" | "view.zoomOut" => {
                let cur = self.ui.zoom.unwrap_or(self.canvas_scale);
                let steps = [0.1, 0.25, 0.33, 0.5, 0.66, 0.75, 1.0, 1.25, 1.5, 2.0, 3.0, 4.0];
                let next = if id == "view.zoomIn" {
                    steps.iter().copied().find(|s| *s > cur + 0.01).unwrap_or(4.0)
                } else {
                    steps.iter().rev().copied().find(|s| *s < cur - 0.01).unwrap_or(0.1)
                };
                self.ui.zoom = Some(next);
            }
            "view.zoom" => {
                let pct = p.get("percent").and_then(Value::as_f64).ok_or("missing `percent`")?;
                self.ui.zoom = Some((pct as f32 / 100.0).clamp(0.1, 4.0));
            }
            "view.zoomDialog" => self.dialog = Some(dialogs::Dialog::new("zoom")),
            "view.fit" => self.ui.zoom = None,
            "review.thesaurusPane" => {
                let word = match p.get("word").and_then(Value::as_str) {
                    Some(w) => w.to_string(),
                    None => self
                        .session
                        .execute("review.thesaurus", &json!({}))
                        .ok()
                        .and_then(|r| r["word"].as_str().map(String::from))
                        .unwrap_or_default(),
                };
                self.thesaurus_query = word;
                self.ui.pane = Some("thesaurus".into());
            }
            "view.pane" => {
                let pane = p.get("pane").and_then(Value::as_str).map(String::from);
                self.ui.pane =
                    if pane.is_some() && pane == self.ui.pane && p.get("toggle").and_then(Value::as_bool).unwrap_or(false) { None } else { pane };
                if let Some(tab) = p.get("tab").and_then(Value::as_str) {
                    self.ui.format_tab = tab.to_string();
                }
            }
            "view.tab" => {
                let tab = p.get("tab").and_then(Value::as_str).ok_or("missing `tab`")?;
                self.ui.tab = tab.to_string();
                self.ui.ribbon_collapsed = false;
            }
            "view.collapseRibbon" => self.ui.ribbon_collapsed = !self.ui.ribbon_collapsed,
            "view.dark" => {
                let on = self.flag(p, self.ui.brightness == theme::Brightness::Dark);
                self.ui.brightness = if on { theme::Brightness::Dark } else { theme::Brightness::Light };
                self.restyle = true;
            }
            "view.grayscale" => self.ui.grayscale = self.flag(p, self.ui.grayscale),
            "view.newWindow" => return doc_window::command(self, p),
            "show.start" => {
                if let Some(name) = p.get("custom").and_then(Value::as_str).filter(|s| !s.is_empty()) {
                    self.start_custom_show(name, false);
                } else {
                    let from = self.show_from(p)?.unwrap_or_else(|| self.show_start_index());
                    self.start_show(from, false);
                }
            }
            "show.presenter" => self.ui.presenter_view = self.flag(p, self.ui.presenter_view),
            "show.end" => {
                self.show = None;
                self.media.stop_owned(media::Owner::Show);
            }
            "animation.preview" => {
                if p.get("on").and_then(Value::as_bool) == Some(false) {
                    self.anim_preview = None;
                    return Ok(json!({"playing": false}));
                }
                let from = match p.get("from") {
                    None | Some(Value::Null) => None,
                    Some(v) => Some(v.as_u64().map(|v| usize::try_from(v).unwrap_or(usize::MAX)).ok_or("`from` must be an effect index")?),
                };
                return self.start_anim_preview(from);
            }
            "app.home" => {
                let on = self.flag(p, self.backstage_open());
                if on {
                    self.open_backstage(backstage::Page::Home);
                } else {
                    self.close_backstage();
                }
            }
            "app.backstage" => {
                if !p.get("on").and_then(Value::as_bool).unwrap_or(true) {
                    self.close_backstage();
                    return Ok(json!({"page": self.backstage.as_ref().map(|b| b.page.id())}));
                }
                let page = match p.get("page").and_then(Value::as_str) {
                    Some(id) => backstage::Page::parse(id).ok_or_else(|| format!("unknown File page `{id}`"))?,
                    None => backstage::Page::Home,
                };
                if page.needs_doc() && self.session.active().is_none() {
                    return Err(format!("File › {} needs an open presentation", page.label()));
                }
                self.open_backstage(page);
                return Ok(json!({"page": page.id()}));
            }
            "app.closePresentation" => self.close_presentation(),
            "app.print" => {
                if self.session.active().is_none() {
                    return Err("nothing to print: no presentation is open".into());
                }
                return Ok(json!({"printed": self.print_pdf(p.clone())}));
            }
            "app.about" => self.dialog = Some(dialogs::Dialog::new("about")),
            "app.checkUpdates" => {
                #[cfg(not(target_arch = "wasm32"))]
                updates_ui::request_check(self);
                #[cfg(target_arch = "wasm32")]
                return Err("updates are handled by the browser: reload the page".into());
            }
            "app.palette" => self.palette = Some((String::new(), 0)),
            "app.preferences" => self.dialog = Some(dialogs::Dialog::new("preferences")),
            "app.openDialog" => self.pick_and_open(),
            "app.insertPictureDialog" => self.pick_and_insert("picture", "insert.picture"),
            "app.smartArtPictureDialog" => {
                if self.services.pick_open.is_none() {
                    return Err("picking a SmartArt picture needs the desktop app".into());
                }
                self.pick_and_run("picture", "smartArt.picture", p);
            }
            "app.changePictureDialog" => self.pick_and_insert("picture", "picture.change"),
            "app.insertAudioDialog" => self.pick_and_insert("audio", "insert.audio"),
            "app.reuseSlidesDialog" => self.pick_and_reuse(p.get("keepFormatting").and_then(Value::as_bool).unwrap_or(false)),
            "app.insertVideoDialog" => self.pick_and_insert("video", "insert.video"),
            "app.saveAsDialog" => self.save_as_dialog("deckcraft"),
            "app.exportDialog" => self.dialog = Some(dialogs::Dialog::new("export")),
            "app.pasteSpecial" => self.dialog = Some(dialogs::Dialog::new("pasteSpecial")),
            "app.browseThemeDialog" => {
                if self.session.active().is_none() {
                    return Err("open a presentation first".into());
                }
                if self.services.pick_open.is_some() {
                    self.pick_and_insert("theme", "design.browseTheme");
                } else {
                    // The browser's asynchronous picker would open the file as a new document.
                    self.set_status("Browse for Themes needs the desktop app for now".to_string());
                }
            }
            "app.qat" => return qat::command(self, p),
            "app.find" | "app.replace" if self.session.active().is_some() => {
                let mut d = dialogs::Dialog::new("find");
                d.fields.insert("replace".into(), if id == "app.replace" { "1" } else { "0" }.into());
                self.dialog = Some(d);
            }
            "app.find" | "app.replace" => self.palette = Some((String::new(), 0)),
            "app.compareDialog" => {
                if self.session.active().is_none() {
                    return Err("open the presentation to compare first".into());
                }
                if self.services.pick_open.is_some() {
                    self.pick_and_insert("open", "review.compare");
                } else {
                    // The browser's asynchronous picker would open the file as a new document.
                    self.set_status("Compare needs the desktop app for now".to_string());
                }
            }
            "app.dialog" => {
                let d = p.get("id").and_then(Value::as_str).ok_or("missing `id`")?;
                self.dialog = Some(dialogs::Dialog::new(d));
            }
            "app.links" => {
                use deckcraft_engine::links::*;
                return Ok(json!({"discord": DISCORD, "website": WEBSITE, "apps": APPS, "appPage": APP_PAGE, "github": GITHUB}));
            }
            _ => return Err(format!("unknown UI command `{id}`")),
        }
        Ok(Value::Null)
    }

    /// Whether ⌘A should act on the slide list: always in the sorter, in Normal view once the
    /// thumbnail pane has been clicked.
    pub(crate) fn slides_list_focused(&self) -> bool {
        self.ui.view == ViewMode::Sorter || (self.ui.view == ViewMode::Normal && self.slides_focused)
    }

    /// First slide Play from Start should open: the custom show or range, skipping hidden slides.
    pub fn show_start_index(&self) -> usize {
        self.session.active().and_then(|d| deckcraft_anim::show_order(&d.doc).first().copied()).unwrap_or(0)
    }

    /// The slide `show.start {from}` names: `Ok(None)` when absent, `Err` when it is not a slide of
    /// the open deck (negative, non-integer, past the end), so a bad index never plays slide 0 or
    /// opens on the end screen. Hidden slides are allowed, as in "from current slide".
    pub(crate) fn show_from(&self, p: &Value) -> Result<Option<usize>, String> {
        let from = match p.get("from") {
            None | Some(Value::Null) => return Ok(None),
            Some(v) => v.as_u64().and_then(|v| usize::try_from(v).ok()).ok_or_else(|| format!("`from` is not a valid index: {v}"))?,
        };
        let count = self.session.active().map_or(0, |d| d.doc.slides.len());
        if from >= count {
            return Err(format!("no slide {from}"));
        }
        Ok(Some(from))
    }

    /// Leave animation preview, stop editor-owned media, and create a show at the requested slide
    /// with the current reading/presenter configuration; an out-of-range slide starts an ended show.
    pub fn start_show(&mut self, from: usize, reading: bool) {
        self.anim_preview = None;
        self.media.stop_owned(media::Owner::Editor);
        if let Some(d) = self.session.active() {
            self.show_instance = self.show_instance.wrapping_add(1).max(1);
            let mut show = show::Show::new(&d.doc, from, reading, self.ui.presenter_view && !reading);
            show.instance = self.show_instance;
            self.show = Some(show);
        }
    }

    /// Play custom show `name` from its first slide. Set Up Slide Show is left as it was.
    pub fn start_custom_show(&mut self, name: &str, reading: bool) {
        let state = self.session.active().and_then(|d| deckcraft_anim::ShowState::play_custom(&d.doc, name));
        let Some(state) = state else { return };
        let from = state.slide;
        self.start_show(from, reading);
        if let Some(show) = self.show.as_mut() {
            show.state = state;
        }
    }

    /// The current slide's animations are playing on the editor canvas.
    pub fn anim_preview_active(&self) -> bool {
        self.anim_preview.is_some()
    }

    /// `(slide, seconds played, sequence length)` while a preview is running.
    pub fn anim_preview_report(&self) -> Option<(usize, f64, f64)> {
        self.anim_preview.as_ref().map(|p| (p.slide, p.elapsed, p.timeline.total_duration()))
    }

    pub fn stop_anim_preview(&mut self) {
        self.anim_preview = None;
    }

    /// Timeline and playhead for this frame's canvas raster. `None` when the editor should draw the slide at rest.
    pub fn anim_preview_draw(&self) -> Option<(deckcraft_anim::Timeline, f64)> {
        self.anim_preview.as_ref().map(|p| (p.timeline.clone(), p.elapsed))
    }

    /// Advance the preview clock. Stops when the slide changes, the slide is edited, or the sequence
    /// (plus a short hold) has finished.
    pub fn tick_anim_preview(&mut self, now: f64) -> bool {
        let Some(p) = self.anim_preview.as_ref() else { return false };
        let (uid, slide, ptr) = (p.uid, p.slide, p.slide_ptr);
        let alive = self.show.is_none()
            && self.session.active().is_some_and(|d| {
                d.uid == uid
                    && d.selection.target == deckcraft_engine::Target::Slides
                    && d.selection.slide == slide
                    && d.doc.slides.get(slide).is_some_and(|s| std::sync::Arc::as_ptr(s) as usize == ptr)
            });
        if !alive {
            self.anim_preview = None;
            return false;
        }
        let Some(p) = self.anim_preview.as_mut() else { return false };
        let now = if now.is_finite() { now } else { p.started.unwrap_or(0.0) };
        let start = *p.started.get_or_insert(now);
        let elapsed = p.from + (now - start).max(0.0);
        if !elapsed.is_finite() || elapsed > p.timeline.total_duration() + preview::HOLD_SECS {
            self.anim_preview = None;
            return false;
        }
        p.elapsed = elapsed;
        true
    }

    /// Play the current slide's animations on the canvas, from effect `from` (index into the slide's
    /// animations) or from the start.
    fn start_anim_preview(&mut self, from: Option<usize>) -> Result<Value, String> {
        let (uid, slide, ptr, timeline) = {
            let Some(d) = self.session.active() else {
                return Err("no presentation is open".into());
            };
            if d.selection.target != deckcraft_engine::Target::Slides {
                return Err("animation preview plays the current slide".into());
            }
            let slide = d.selection.slide;
            let Some(slide_arc) = d.doc.slides.get(slide) else {
                return Err("no slide to preview".into());
            };
            let ptr = std::sync::Arc::as_ptr(slide_arc) as usize;
            let timeline = deckcraft_anim::ShowState::timeline(&d.doc, slide);
            (d.uid, slide, ptr, timeline)
        };
        if timeline.is_empty() {
            self.anim_preview = None;
            self.set_status("This slide has no animations");
            return Ok(json!({"playing": false}));
        }
        let from_secs = match from {
            Some(i) => timeline.start_of(i).ok_or_else(|| format!("no effect {i} in this slide's click sequence"))?,
            None => 0.0,
        };
        if matches!(self.ui.view, ViewMode::Sorter | ViewMode::NotesPage) {
            self.ui.view = ViewMode::Normal;
        }
        self.close_backstage();
        let duration = timeline.total_duration();
        self.anim_preview = Some(AnimPreview { uid, slide, slide_ptr: ptr, started: None, from: from_secs, elapsed: from_secs, timeline });
        self.set_status("Previewing animations");
        Ok(json!({"playing": true, "slide": slide, "duration": duration, "from": from_secs}))
    }

    fn pick_and_open(&mut self) {
        if let Some(pick) = self.services.pick_open.as_mut() {
            if let Some(path) = pick("open") {
                self.open_path_in_background(&path);
            }
        } else if self.services.open_async.is_some() {
            let id = self.begin_external_file("open");
            if let Some(f) = self.services.open_async.as_mut() {
                f("open", id);
            }
        }
    }

    pub fn open_path(&mut self, path: &str) -> Result<(), String> {
        self.session.execute("file.open", &json!({"path": path})).map_err(|e| e.to_string())?;
        self.note_recent(path);
        Ok(())
    }

    /// Files the system hands over (the command line, a double-click through the file
    /// association), opened in order. When the last one is a PowerPoint Show (`.ppsx`), its slide
    /// show starts at once from where Play from Start begins, as PowerPoint does; anything else
    /// opens for editing, and File › Open opens a `.ppsx` for editing too. Returns the files that
    /// couldn't be opened, with the reason.
    pub fn open_from_system(&mut self, paths: &[String]) -> Vec<(String, String)> {
        let mut failed = Vec::new();
        let mut last = None;
        for p in paths {
            match self.open_path(p) {
                Ok(()) => last = Some(p),
                Err(e) => failed.push((p.clone(), e)),
            }
        }
        if last.is_some_and(|p| p.to_ascii_lowercase().ends_with(".ppsx")) {
            self.start_show(self.show_start_index(), false);
        }
        failed
    }

    /// A presentation was opened from `path`: first in the recent list, shown in Normal view.
    pub(crate) fn note_recent(&mut self, path: &str) {
        self.status = None; // an older error (a missing file just before this one) must not linger
        self.recent_checked.remove(path);
        self.ui.recent.retain(|p| p != path);
        self.ui.recent.insert(0, path.to_string());
        self.ui.recent.truncate(RECENT_MAX);
        self.ui.view = ViewMode::Normal;
    }

    /// Transitions › Sound › Other Sound…: pick an audio file for the selected slides.
    pub(crate) fn pick_transition_sound(&mut self) {
        if self.services.pick_open.is_some() {
            self.pick_and_insert("audio", "transition.sound");
        }
    }

    /// Home › New Slide › Reuse Slides…: pick a presentation and insert all its slides after the
    /// current one (desktop only: the browser picker opens what it picks as a document).
    fn pick_and_reuse(&mut self, keep: bool) {
        let Some(pick) = self.services.pick_open.as_mut() else {
            self.set_status("Reuse Slides needs the desktop app".to_string());
            return;
        };
        let Some(path) = pick("open") else { return };
        // Read the file once: the list shown and the slides inserted come from the same bytes, even
        // if the file changes while the dialog is open. Without a reader the path is used as is.
        let mut source = json!({});
        match self.services.read.as_ref().map(|r| r(&path)) {
            Some(Ok(bytes)) => {
                let name = std::path::Path::new(&path).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                source["name"] = json!(name);
                source["data"] = json!(deckcraft_engine::cmd::base64_encode(&bytes));
            }
            Some(Err(e)) => {
                self.set_status(e.to_string());
                return;
            }
            None => source["path"] = json!(path),
        }
        // Offer the file's slides to choose from; all of them are ticked to begin with.
        match self.session.execute("slide.reuseList", &source) {
            Ok(list) => {
                let mut d = dialogs::Dialog::new("reuseSlides");
                d.params = json!({"path": path, "source": source, "keep": keep, "slides": list["slides"]});
                self.reuse_target = Some(self.capture_file_target());
                self.dialog = Some(d);
            }
            Err(e) => self.set_status(e.to_string()),
        }
    }

    /// Insert the slides `picked` (indices) of `source` (the `name` + `data`, or `path`, that
    /// `slide.reuseList` listed) after the slide that was current when the picker opened, in that
    /// presentation, even if another one is active now; if it is closed or the slide is gone,
    /// nothing is inserted. Returns whether they were inserted.
    pub fn reuse_slides(&mut self, source: &Value, keep: bool, picked: &[usize]) -> bool {
        let mut args = if source.is_object() { source.clone() } else { json!({}) };
        args["keepFormatting"] = json!(keep);
        args["slides"] = json!(picked);
        let result = match self.reuse_target {
            Some(target) if !self.viewing_target(&target) => self.execute_on_target(&target, "Reuse Slides", "slide.reuse", &args),
            _ => self.session.execute("slide.reuse", &args).map(|_| ()).map_err(|e| e.to_string()),
        };
        match result {
            Ok(()) => {
                self.reuse_target = None;
                self.set_status(format!("Inserted {} slides", picked.len()));
                true
            }
            Err(e) => {
                self.set_status(e);
                false
            }
        }
    }

    fn pick_and_insert(&mut self, purpose: &str, cmd: &str) {
        self.pick_and_run(purpose, cmd, &json!({}));
    }

    /// Pick a file and run `cmd` with it (`name` + `data`, or `path`) added to `extra`.
    fn pick_and_run(&mut self, purpose: &str, cmd: &str, extra: &Value) {
        if let Some(pick) = self.services.pick_open.as_mut() {
            if let Some(path) = pick(purpose) {
                let mut args = if extra.is_object() { extra.clone() } else { json!({}) };
                let r = match self.services.read.as_ref().map(|r| r(&path)) {
                    Some(Ok(bytes)) => {
                        let name = std::path::Path::new(&path).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                        args["name"] = json!(name);
                        args["data"] = json!(deckcraft_engine::cmd::base64_encode(&bytes));
                        self.session.execute(cmd, &args)
                    }
                    _ => {
                        args["path"] = json!(path);
                        self.session.execute(cmd, &args)
                    }
                };
                if let Err(e) = r {
                    self.set_status(e.to_string());
                }
            }
        } else if self.services.open_async.is_some() {
            let id = self.begin_external_file(purpose);
            // Change Picture: the picture that arrives replaces the selected one (#115).
            if cmd == "picture.change"
                && let Some(r) = self.pending_files.iter_mut().find(|r| r.id == id)
            {
                r.purpose = FilePurpose::ChangePicture;
            }
            if let Some(f) = self.services.open_async.as_mut() {
                f(purpose, id);
            }
        }
    }

    pub fn save_as_dialog(&mut self, format: &str) {
        let name = self.session.active().map(|d| d.title()).unwrap_or_else(|| "Presentation".into());
        let ext = if format == "pptx" { "pptx" } else { "deckcraft" };
        let suggested = format!("{name}.{ext}");
        if let Some(pick) = self.services.pick_save.as_mut() {
            if let Some(path) = pick(&suggested) {
                match self.session.execute("file.saveAs", &json!({"path": path})) {
                    Ok(_) => {
                        self.set_status(format!("Saved {path}"));
                        self.ui.recent.retain(|p| *p != path);
                        self.ui.recent.insert(0, path);
                    }
                    Err(e) => self.set_status(e.to_string()),
                }
            }
        } else if let Some(dl) = self.services.download.as_mut()
            && let Some(d) = self.session.active()
        {
            match deckcraft_engine::cmd::file::save_bytes(&d.doc, ext) {
                Ok(bytes) => dl(&suggested, &bytes),
                Err(e) => self.set_status(e.to_string()),
            }
        }
    }

    /// Save every presentation with unsaved changes (asking for a location where needed).
    /// Returns false if one wasn't saved (cancelled or failed).
    pub fn save_all(&mut self) -> bool {
        let dirty: Vec<usize> = self.session.documents().iter().enumerate().filter(|(_, d)| d.is_dirty()).map(|(i, _)| i).collect();
        for i in dirty {
            self.session.set_active(i);
            self.save();
            if self.session.documents().get(i).is_some_and(|d| d.is_dirty()) {
                return false;
            }
        }
        true
    }

    pub fn save(&mut self) {
        if self.session.active().and_then(|d| d.path.clone()).is_some() {
            match self.session.execute("file.save", &json!({})) {
                Ok(_) => self.set_status("Saved"),
                Err(e) => self.set_status(e.to_string()),
            }
        } else {
            self.save_as_dialog("deckcraft");
        }
    }

    /// Act on requests commands left for the UI.
    fn drain_requests(&mut self) {
        for r in std::mem::take(&mut self.session.ui_requests) {
            match r {
                UiRequest::Dialog { id, params } => {
                    let mut d = dialogs::Dialog::new(&id);
                    d.params = params;
                    self.dialog = Some(d);
                }
                UiRequest::Pane { id } => self.ui.pane = Some(id),
                UiRequest::StartShow { from } => self.start_show(from, false),
                UiRequest::Message { text } => self.set_status(text),
                UiRequest::PickFile { purpose } => {
                    if purpose == "saveAs" {
                        self.save_as_dialog("deckcraft");
                    }
                }
                UiRequest::Media { action, shape, ms } => {
                    let owner = if self.show.is_some() { media::Owner::Show } else { media::Owner::Editor };
                    // The command resolved `shape` on the editor's slide, also while a show runs.
                    if let Some((uid, doc, slide)) = self.session.active().map(|d| (d.uid, d.doc.clone(), d.selection.slide)) {
                        self.media.request(uid, &doc, slide, &action, shape, ms, owner);
                    }
                }
            }
        }
    }

    fn drain_inbox(&mut self) {
        let Some(inbox) = self.services.inbox.clone() else { return };
        let events = match inbox.lock() {
            Ok(mut v) => std::mem::take(&mut *v),
            Err(poisoned) => std::mem::take(&mut *poisoned.into_inner()),
        };
        for event in events {
            match event {
                InboxEvent::Cancelled { id } => self.pending_files.retain(|r| r.id != id),
                InboxEvent::File(file) => self.apply_external_file(file.id, &file.name, &file.bytes),
            }
        }
    }

    /// Remember the open presentation, the slide (or master/layout) and the operation for a
    /// file the host will read asynchronously. Returns the cancellation identity the host must
    /// echo on the [`InboxEvent`]. Call this when the pick or the drop starts, not when the
    /// bytes arrive.
    pub fn begin_external_file(&mut self, purpose: &str) -> u64 {
        let id = self.next_file_id;
        self.next_file_id = self.next_file_id.wrapping_add(1).max(1);
        self.pending_files.push(PendingFile { id, purpose: purpose_of(purpose), target: self.capture_file_target() });
        id
    }

    /// A dropped or picked file read synchronously: presentations open, media is inserted on
    /// the slide that is current now.
    pub fn open_or_insert_bytes(&mut self, name: &str, bytes: &[u8]) {
        let id = self.begin_external_file("drop");
        self.apply_external_file(id, name, bytes);
    }

    fn capture_file_target(&self) -> FileTarget {
        let Some(d) = self.session.active() else { return FileTarget::Absent };
        let place = match d.selection.target {
            Target::Slides => match d.current_slide() {
                Some(s) => FilePlace::Slide(s.id),
                None => FilePlace::Unspecified,
            },
            Target::Master { master } => match d.doc.masters.get(master) {
                Some(m) => FilePlace::Master(m.id),
                None => FilePlace::Unspecified,
            },
            Target::Layout { master, layout } => match d.doc.masters.get(master) {
                Some(m) => match m.layouts.get(layout) {
                    Some(l) => FilePlace::Layout { master: m.id, layout: l.id },
                    None => FilePlace::Unspecified,
                },
                None => FilePlace::Unspecified,
            },
            t @ (Target::Handout | Target::NotesMaster) => FilePlace::NotesPage(t),
        };
        FileTarget::Document { doc: d.uid, place }
    }

    /// True when the user is still looking at the place this request captured.
    fn viewing_target(&self, target: &FileTarget) -> bool {
        let FileTarget::Document { doc, place } = target else { return false };
        let Some(d) = self.session.active() else { return false };
        if d.uid != *doc {
            return false;
        }
        match place {
            FilePlace::Slide(id) => d.selection.target == Target::Slides && d.current_slide().is_some_and(|s| s.id == *id),
            FilePlace::Master(id) => {
                matches!(d.selection.target, Target::Master { master } if d.doc.masters.get(master).is_some_and(|m| m.id == *id))
            }
            FilePlace::Layout { master, layout } => {
                matches!(d.selection.target, Target::Layout { master: mi, layout: li }
                    if d.doc.masters.get(mi).is_some_and(|m| m.id == *master && m.layouts.get(li).is_some_and(|l| l.id == *layout)))
            }
            FilePlace::NotesPage(t) => d.selection.target == *t,
            FilePlace::Unspecified => false,
        }
    }

    fn apply_external_file(&mut self, id: u64, name: &str, bytes: &[u8]) {
        let Some(pos) = self.pending_files.iter().position(|r| r.id == id) else { return };
        let req = self.pending_files.remove(pos);
        let cmd = match command_for(req.purpose, name, bytes) {
            Ok(cmd) => cmd,
            Err(e) => {
                self.set_status(e);
                return;
            }
        };
        let params = if cmd == "slide.fromOutline" {
            json!({"text": deckcraft_engine::cmd::file::outline_text(bytes), "markdown": name.to_ascii_lowercase().ends_with(".md")})
        } else {
            json!({"name": name, "data": deckcraft_engine::cmd::base64_encode(bytes)})
        };
        let result = if cmd == "file.openBytes" || self.viewing_target(&req.target) {
            self.session.execute(cmd, &params).map(|_| ()).map_err(|e| e.to_string())
        } else {
            self.execute_on_target(&req.target, name, cmd, &params)
        };
        match result {
            Err(e) if cmd == "file.openBytes" => self.notify(e),
            Err(e) => self.set_status(e),
            Ok(()) => {}
        }
        self.drain_requests();
    }

    /// Run `cmd` on the document and slide captured for the request, then put the view back
    /// if the user has since moved. A missing document or slide cancels the file.
    fn execute_on_target(&mut self, target: &FileTarget, name: &str, cmd: &str, params: &Value) -> Result<(), String> {
        let FileTarget::Document { doc, place } = target else {
            self.session.execute("file.new", &json!({})).map_err(|e| e.to_string())?;
            return self.session.execute(cmd, params).map(|_| ()).map_err(|e| e.to_string());
        };
        let Some(idx) = self.session.documents().iter().position(|d| d.uid == *doc) else {
            return Err(format!("{name} was not applied: the presentation it was chosen for is closed"));
        };
        let view = self.session.active_index();
        let saved = self.session.documents().get(idx).map(|d| d.selection.clone());
        self.session.set_active(idx);
        let located = match self.locate_place(place) {
            Ok(located) => located,
            Err(e) => {
                self.restore_file_view(view);
                return Err(format!("{name} was not applied: {e}"));
            }
        };
        if let Some(st) = self.session.active_mut() {
            st.selection.target = located.target;
            if let Some(slide) = located.slide {
                st.selection.slide = slide;
                st.selection.slides.clear();
            }
            st.selection.shapes.clear();
            st.selection.text = None;
            st.selection.cells = None;
        }
        let result = self.session.execute(cmd, params).map(|_| ()).map_err(|e| e.to_string());
        if view == Some(idx) {
            if let (Some(sel), Some(st)) = (saved, self.session.active_mut()) {
                st.selection = sel;
            }
        } else {
            self.restore_file_view(view);
        }
        result
    }

    fn restore_file_view(&mut self, view: Option<usize>) {
        if let Some(i) = view
            && i < self.session.documents().len()
        {
            self.session.set_active(i);
        }
    }

    fn locate_place(&self, place: &FilePlace) -> Result<LocatedPlace, String> {
        let d = self.session.doc().map_err(|e| e.to_string())?;
        match place {
            FilePlace::Slide(id) => {
                let slide = d.doc.slide_index(*id).ok_or_else(|| "the slide it was chosen for was deleted".to_string())?;
                Ok(LocatedPlace { target: Target::Slides, slide: Some(slide) })
            }
            FilePlace::Master(id) => {
                let master =
                    d.doc.masters.iter().position(|m| m.id == *id).ok_or_else(|| "the slide master it was chosen for was deleted".to_string())?;
                Ok(LocatedPlace { target: Target::Master { master }, slide: None })
            }
            FilePlace::Layout { master, layout } => {
                let mi =
                    d.doc.masters.iter().position(|m| m.id == *master).ok_or_else(|| "the slide master it was chosen for was deleted".to_string())?;
                let li = d
                    .doc
                    .masters
                    .get(mi)
                    .and_then(|m| m.layouts.iter().position(|l| l.id == *layout))
                    .ok_or_else(|| "the layout it was chosen for was deleted".to_string())?;
                Ok(LocatedPlace { target: Target::Layout { master: mi, layout: li }, slide: None })
            }
            FilePlace::NotesPage(t) => Ok(LocatedPlace { target: *t, slide: None }),
            FilePlace::Unspecified => Err("there was no slide to put it on".into()),
        }
    }

    /// Keep a hidden show playing from logic alone. `run_logic` does not refresh
    /// `InputState::time`, so the show anchors its last visible time to this monotonic instant
    /// and advances from there. A forced hidden paint draws the current state but does not
    /// advance it a second time, and no paused-show product policy is invented.
    fn tick_hidden_show(&mut self, ctx: &egui::Context, now: web_time::Instant) {
        let visible = show::viewport_visible(ctx);
        let Some(doc) = self.session.active().map(|d| d.doc.clone()) else { return };
        let Some(show) = self.show.as_mut() else { return };
        if visible {
            // A visible logic-only pass (`run_logic`) carries the last full frame's input time.
            // Anchoring that stale time to `now` would move the hidden clock backwards if the
            // window hides before the next full pass. `show::ui` already records the anchor on
            // every visible full pass, where `InputState::time` is current.
            return;
        }
        if !show.hidden_started() {
            return;
        }
        let input_time = ctx.input(|i| i.time);
        let show_time = show.hidden_logic_time(input_time, now);
        let keep = show.advance_hidden(&doc, show_time);
        let time_driven = show.hidden_time_driven(&doc);
        if !keep {
            self.show = None;
            return;
        }
        if time_driven {
            // A hidden event loop has no draw to piggyback on. A delayed repaint keeps the
            // logic clock ticking; with no paint request pending eframe services it as a
            // logic-only pass, not a visible frame.
            ctx.request_repaint_after_for(std::time::Duration::from_millis(100), ctx.viewport_id());
        }
    }

    /// Background work, run on every logic call (visible, hidden or `run_logic`).
    ///
    /// Only consumed queues and monotonic deadlines live here: control requests, asynchronous
    /// opening/inbox results, screenshot completions/deadlines, AutoRecover pacing and media
    /// clocks. UI events are handled in [`Self::ui`], where a forced hidden paint also arrives.
    pub fn logic(&mut self, ctx: &egui::Context) {
        if !Self::root_viewport_admitted(ctx) {
            log::debug!("ignoring SlideApp::logic for render-only viewport {:?}", ctx.viewport_id());
            return;
        }
        self.tick_hidden_show(ctx, web_time::Instant::now());
        // `raw_input_hook` runs before `App::logic`. In a full pass egui has already built
        // `PointerState` and `keys_down` from every injected event, so retracting the events after
        // this pass would not stop the derived click or key from reaching the UI. Defer instead:
        // a control request, background-open result or inbox file that changes the document,
        // target or modal owner waits until the admitted batch has been consumed by the old
        // target's UI pass. A logic-only callback carries the RawInput into the next full pass
        // (no live `InputState` here), so that pass still drains normally and the next
        // `raw_input_hook` retracts a batch that went stale.
        if !self.synthetic_batch_is_live(ctx) {
            self.drain_control(ctx);
            self.poll_opening();
            #[cfg(not(target_arch = "wasm32"))]
            updates_ui::tick(self, ctx);
            // Inbox results are host file bytes and do not need fonts or a painted frame. A
            // window that starts hidden must still apply a file picked before its first UI pass.
            self.drain_inbox();
        }
        self.drain_screenshots();
        // `raw_input_hook` ran before this logic pass. This is normally a no-op now that
        // target-changing work is deferred for a live batch; it still covers a target change
        // made by other logic, and keeps a logic-only batch pending for the next hook.
        self.retract_synthetic_after_logic(ctx);
        // Queued/settling screenshots force the bounded hidden paints they need; issued ones
        // rely on the screenshot command and the completion callback, with only a deadline wake.
        self.capture.request_paint_if_needed(ctx, web_time::Instant::now());
        if !self.synthetic.is_empty() || !self.synthetic_next.is_empty() || self.synthetic_pending {
            ctx.request_repaint();
        }
        // Closing the window with unsaved changes asks first.
        if ctx.input(|i| i.viewport().close_requested()) && !self.quit_confirmed && self.session.documents().iter().any(|d| d.is_dirty()) {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.show = None;
            if self.dialog.as_ref().is_none_or(|d| d.id != "quit") {
                self.dialog = Some(dialogs::Dialog::new("quit"));
            }
        }
        self.autosave_if_due(ctx);
        self.tick_media_clock(ctx);
        self.note_settled_composition(ctx);
        // In a full run_ui pass the UI callback after this logic half still owns the pass's final
        // IME descriptor. Only a standalone run_logic callback proves that no custom editor was
        // painted for this tick. Do not infer this from cumulative frame counters or stale
        // geometry.
        if !self.ime_in_ui_pass(ctx) {
            self.finalize_ime_logic(ctx);
        }
    }

    /// Style, fonts and zoom setup. Runs in a real UI pass, not from `run_logic`, because
    /// `set_fonts`/`set_visuals` only take effect for the pass that lays out the UI.
    fn ui_setup(&mut self, ctx: &egui::Context) -> bool {
        let scale = self.ui.ui_scale.clamp(0.5, 3.0);
        if (ctx.zoom_factor() - scale).abs() > 1e-3 {
            ctx.set_zoom_factor(scale);
        }
        if !self.styled {
            theme::install_fonts_with(ctx, &self.ui_fonts.faces);
            self.styled = true;
            self.restyle = true;
            // Font definitions take effect at the next pass boundary. Do not paint a language
            // or symbol with the old font set while the new set is still pending.
            ctx.request_discard("install interface fonts before layout");
            return false;
        }
        if self.restyle {
            theme::apply(ctx, &theme::Tokens::for_brightness(self.ui.brightness));
            self.restyle = false;
        }
        self.fonts_ready = true;
        true
    }

    /// AutoRecover on a monotonic deadline, even when nothing is painted.
    fn autosave_if_due(&mut self, ctx: &egui::Context) {
        if self.session.recovery_dir.is_none() {
            return;
        }
        let every = (self.session.prefs.recovery_minutes * 60.0).max(10.0);
        let now = monotonic_secs();
        if now - self.last_recovery > every {
            self.last_recovery = now;
            if self.session.documents().iter().any(|d| d.is_dirty())
                && let Err(e) = self.session.execute("file.recovery.save", &json!({}))
            {
                self.set_status(format!("Couldn't save AutoRecover information: {e}"));
            }
        }
        if self.session.documents().iter().any(|d| d.is_dirty()) {
            let wait = (every - (monotonic_secs() - self.last_recovery)).clamp(0.5, every);
            ctx.request_repaint_after(std::time::Duration::from_secs_f64(wait));
        }
    }

    /// Issue due screenshots and finish pass-local UI bookkeeping. Call once at the end of a real
    /// pass after layout; the outer [`Self::ui`] wrapper observes IME output after this returns.
    fn ui_end(&mut self, ctx: &egui::Context) {
        self.ime_frame_viewport = None;
        self.capture.observe_ui(ctx);
        if !ctx.will_discard() {
            let doc = self.session.active().map(|d| d.uid);
            self.capture.issue(ctx, doc, web_time::Instant::now());
        }
        if self.fonts_ready && !ctx.will_discard() {
            self.synthetic_pending = false;
            self.synthetic_injected.clear();
            self.synthetic_injected_at = None;
            self.synthetic_injected_time = None;
            self.synthetic_focus = None;
            if self.synthetic.is_empty() {
                self.synthetic_target = None;
            }
            self.promote_next_synthetic();
        }
        if self.fonts_ready && !ctx.will_discard() {
            let now = ctx.input(|i| i.time);
            let dt = now - self.last_time;
            if dt > 0.0 {
                self.perf.fps = self.perf.fps * 0.9 + (1.0 / dt).min(240.0) * 0.1;
            }
            self.last_time = now;
        }
    }

    /// Borrow faces for interface text in scripts the interface fonts lack (#101): the open
    /// presentations' names and text, recent files, and what is typed or composed this frame.
    fn note_ui_text(&mut self, ctx: &egui::Context) {
        let typed: String = ctx.input(|i| {
            i.events
                .iter()
                .filter_map(|e| match e {
                    egui::Event::Text(t) | egui::Event::Paste(t) => Some(t.as_str()),
                    egui::Event::Ime(egui::ImeEvent::Preedit { text, .. } | egui::ImeEvent::Commit(text)) => Some(text.as_str()),
                    _ => None,
                })
                .collect()
        });
        self.ui_fonts.note(ctx, &typed);
        for d in self.session.documents() {
            self.ui_fonts.note(ctx, &d.title());
        }
        for r in &self.ui.recent {
            self.ui_fonts.note(ctx, r);
        }
        if let Some(st) = self.session.active() {
            self.ui_fonts.note_doc(ctx, st);
        }
        self.ui_fonts.apply(ctx);
    }

    /// Stable IME owner for the canvas text target: document, view, shape and table cell.
    ///
    /// The caret, selection, text, revision and geometry are deliberately absent, so ordinary
    /// edits and caret movement keep the same owner. A new document, shape or cell is a new
    /// owner; [`egui::ime_context::ImeTarget::replacement_epoch`] stays zero because that key
    /// itself changes on replacement. `file.revert` keeps the ids, so a composition it settles is
    /// covered by the settle interrupt and by [`Self::ime_event`]'s staleness check instead.
    fn ime_target(&self) -> Option<egui::ime_context::ImeTarget> {
        let st = self.session.active()?;
        let (shape, cell, notes) = match &st.selection.text {
            Some(t) => (t.shape, t.cell, t.notes),
            None => (*st.selection.shapes.first()?, None, false),
        };
        if st.selection.text.is_none() && st.selection.shapes.len() != 1 {
            return None;
        }
        let target = match st.selection.target {
            Target::Slides => (0u8, 0usize, 0usize),
            Target::Master { master } => (1, master, 0),
            Target::Layout { master, layout } => (2, master, layout),
            Target::Handout => (3, 0, 0),
            Target::NotesMaster => (4, 0, 0),
        };
        let owner = egui::Id::unique(("deckcraft-ime", st.uid, target, shape.0, cell, notes));
        Some(egui::ime_context::ImeTarget::new(owner, 0))
    }

    /// The committed surrounding snapshot for the current edit target, or `Unavailable` when
    /// there is an owner but no committed text to describe.
    ///
    /// An unavailable stamp carries the document revision. The tracker requires a strictly
    /// increasing revision on both sides of a receipt that involves `Unavailable`; without it, a
    /// wide selection or a commit that starts from a selected shape would poison the whole frame.
    fn ime_receipt_stamp(&self) -> Option<egui::ime_context::ImeMutationStamp> {
        let st = self.session.active()?;
        self.ime_target()?;
        Some(st.selection.text.as_ref().and_then(|t| deckcraft_engine::cmd::text::ime_surrounding(st, t)).map_or_else(
            || egui::ime_context::ImeMutationStamp::unavailable_at_revision(st.revision),
            |s| egui::ime_context::ImeMutationStamp::from_committed(&s.text, s.cursor, s.anchor, None),
        ))
    }

    /// Report an application-origin edit in this frame. The tracker lets `Other` dominate any
    /// earlier or later IME receipt in the same frame.
    fn note_application_edit(&mut self, ctx: &egui::Context) {
        let Some(viewport) = self.ime_frame_viewport else { return };
        if let Some(target) = self.ime_target() {
            let frame = ctx.cumulative_frame_nr_for(viewport);
            self.ime_tracker.note_application_change(viewport, frame, target);
        }
    }

    /// Record the expected selection after an accepted IME event. Its own deletion may move
    /// the caret/revision; that is not an application interruption of the following commit.
    fn refresh_ime_terminal_selection(&mut self) {
        let state = self.session.active().and_then(|st| st.selection.text.as_ref().map(|t| (st.revision, t.anchor, t.caret)));
        if let (Some(pending), Some((revision, anchor, caret))) = (&mut self.ime_terminal_pending, state) {
            pending.revision = revision;
            pending.anchor = anchor;
            pending.caret = caret;
        }
    }

    /// The current live destination for an IME reset fence.
    fn ime_destination(&self) -> Option<ImeDestination> {
        let st = self.session.active()?;
        let target = self.ime_target()?;
        let (anchor, caret) = st.selection.text.as_ref().map_or((None, None), |t| (Some(t.anchor), Some(t.caret)));
        Some(ImeDestination { owner: target.owner, revision: st.revision, anchor, caret })
    }

    /// Use the recorded IME viewport only while it matches the current context viewport.
    fn ime_viewport_or(&self, ctx: &egui::Context) -> egui::ViewportId {
        self.ime_frame_viewport.filter(|v| *v == ctx.viewport_id()).unwrap_or_else(|| ctx.viewport_id())
    }

    /// The reset guard belonging to exactly this viewport. There is deliberately no fallback to
    /// another viewport's phase.
    pub(crate) fn ime_reset_for(&self, viewport: egui::ViewportId) -> ImeResetPhase {
        self.ime_reset.get(&viewport).copied().unwrap_or(ImeResetPhase::Clear)
    }

    /// Store a non-clear reset phase for one viewport, or remove the entry when the phase is `Clear`.
    fn set_ime_reset(&mut self, viewport: egui::ViewportId, phase: ImeResetPhase) {
        if matches!(phase, ImeResetPhase::Clear) {
            let _ = self.ime_reset.remove(&viewport);
        } else {
            self.ime_reset.insert(viewport, phase);
        }
    }

    /// The live stamp used to compare against the origin/interrupted terminal destination.
    fn ime_terminal_stamp(&self) -> Option<ImeTerminalStamp> {
        let st = self.session.active()?;
        let (anchor, caret) = st.selection.text.as_ref().map_or((None, None), |t| (Some(t.anchor), Some(t.caret)));
        Some(ImeTerminalStamp { owner: self.ime_target().map(|t| t.owner), revision: st.revision, anchor, caret })
    }

    /// Positive route predicate for the canvas IME producer: this pass must be one that can
    /// actually paint `canvas::show` for the active document, with no modal, stock field or
    /// overlay owning input.
    ///
    /// Sorter and Notes Page are editor views without a canvas producer, so they are named here
    /// even though the retained selection still yields an `ime_target`. A hidden, unfocused or
    /// closing window, a missing document/target, a focused stock field, modal dialog, palette,
    /// Backstage, slide show and opening overlay lose admission too. The finalizer remains the
    /// authority for this pass's actual descriptor; keeping this predicate positive means a view
    /// added later is not admitted by omission simply because it does not call `canvas::show`.
    fn ime_custom_canvas_route(&self, ctx: &egui::Context) -> bool {
        matches!(self.ui.view, ViewMode::Normal | ViewMode::Outline | ViewMode::Reading)
            && self.session.active().is_some()
            && self.ime_target().is_some()
            && ctx.input(|i| i.viewport().focused.unwrap_or(true))
            && ctx.input(|i| i.viewport().visible() != Some(false))
            && !ctx.input(|i| i.viewport().close_requested())
            && ctx.memory(|m| m.focused().is_none())
            && self.dialog.as_ref().is_none_or(|d| !d.modal())
            && self.palette.is_none()
            && self.backstage.is_none()
            && self.show.is_none()
            && self.opening.is_none()
    }

    /// The singleton app's session, composition tracker and terminal guard belong to its owning
    /// viewport. View › New Window and Presenter View render through their own immediate
    /// viewports and never call `logic`/`ui`; admitting such a callback would let a render-only
    /// viewport mutate root composition state while only the reset map is per-viewport. Reject it
    /// before any state changes instead of pretending to support multi-view editing.
    fn root_viewport_admitted(ctx: &egui::Context) -> bool {
        ctx.viewport_id() == egui::ViewportId::ROOT
    }

    /// Report whether the current pass is a full UI pass for the IME-owning viewport rather than
    /// standalone logic.
    fn ime_in_ui_pass(&self, ctx: &egui::Context) -> bool {
        let key = ime_in_ui_pass_key(self.ime_viewport_or(ctx));
        ctx.data(|d| d.get_temp::<bool>(key).unwrap_or(false))
    }

    /// A new application-origin interruption supersedes any previous pending fence.
    ///
    /// This runs from the logic half of a full pass and from raw input handling, before the
    /// finalizer has seen this pass's actual custom descriptor. No request is allocated here:
    /// putting a token into egui `Memory` now would let egui attach it to a later stock
    /// descriptor, or to a canvas this route does not paint. Retire the superseded id
    /// immediately and store an unbound `Suspended { request: None }`; only the finalizer may
    /// allocate its one replacement after confirming an eligible custom output.
    fn note_ime_interruption(&mut self, ctx: &egui::Context, fresh: bool) {
        let viewport = self.ime_viewport_or(ctx);
        if !fresh
            && matches!(
                self.ime_reset_for(viewport),
                ImeResetPhase::Awaiting { viewport: guard_viewport, .. }
                    | ImeResetPhase::Suspended { viewport: guard_viewport, .. }
                    if guard_viewport == viewport
            )
        {
            // This viewport already has a live or suspended guard. The finalizer replaces it
            // once if the actual destination changed; do not churn a request per event.
            return;
        }
        self.set_ime_reset(viewport, ImeResetPhase::Suspended { viewport, needs_rearm: true, request: None });
    }

    /// Custom admission was lost on this route. Retire the old id without inventing an
    /// acknowledgement and block until a later eligible pass can request a replacement.
    fn suspend_ime_reset(&mut self, viewport: egui::ViewportId) {
        if self.ime_reset_for(viewport).blocks_custom_input() {
            self.set_ime_reset(viewport, ImeResetPhase::Suspended { viewport, needs_rearm: true, request: None });
        }
    }

    /// An ordered completion for the exact newest request. Wrong/foreign/obsolete markers are
    /// inert; this never mutates text, caret or undo history.
    fn ime_reset_completed(&mut self, request_id: egui::ImeResetId, ctx: &egui::Context) {
        let viewport = self.ime_viewport_or(ctx);
        if request_id.viewport_id() != viewport {
            return;
        }
        if let ImeResetPhase::Awaiting { id, viewport: waiting_viewport, .. } = self.ime_reset_for(viewport)
            && id == request_id
            && waiting_viewport == viewport
        {
            self.set_ime_reset(viewport, ImeResetPhase::Clear);
            // The exact ordered completion is the platform-reset boundary. It also releases a
            // cleared-preedit terminal guard: the app no longer has to wait for the old
            // Commit/Enabled/Disabled before a fresh composition may start. Events ordered
            // before this marker remain rejected; a late old terminal after the reset has the
            // same documented residual as any other event crossing a platform reset.
            if self.ime_terminal_pending.as_ref().is_some_and(|p| p.cancelled) {
                self.ime_terminal_pending = None;
            }
            if self.ime_composition.is_some_and(|c| c.settled) {
                self.ime_composition = None;
            }
            self.ime_cancelled = None;
            self.ime_interrupt_pending = false;
        }
    }

    /// Controls-only routing for a route where the custom editor cannot mutate. Markers are
    /// visited in raw order; stock text/key events stay untouched for the stock widgets.
    fn route_ime_controls_only(&mut self, ctx: &egui::Context, events: &[egui::Event]) {
        for e in events {
            if let egui::Event::ImeResetCompleted { request_id } = e {
                self.ime_reset_completed(*request_id, ctx);
            }
        }
    }

    /// Bind a suspended reset to the actual custom descriptor, or retire it on loss.
    fn finalize_ime_reset(&mut self, ctx: &egui::Context, custom: Option<ImeDescriptorKey>) {
        let viewport = self.ime_viewport_or(ctx);
        let phase = self.ime_reset_for(viewport);
        let actual = ctx.output(|o| o.ime.as_ref().map(|i| ImeDescriptorKey::new(i.owner, i.context_generation, i.purpose)));
        let destination = self.ime_destination();
        // The finalizer is the allocation authority, so it rechecks the positive canvas route
        // and that this pass is not already known to be discarded. Sorter/Notes Page, a modal,
        // palette, Backstage, show, opening overlay, hidden window, stock focus, font/layout
        // discard or a missing target must suspend without putting a token into egui Memory.
        let eligible = self.ime_custom_canvas_route(ctx) && !ctx.will_discard();
        let custom_output = match (eligible, custom, actual, destination) {
            (true, Some(custom), Some(actual), Some(destination)) if custom == actual => Some((custom, destination)),
            _ => None,
        };
        let Some((output_context, destination)) = custom_output else {
            match phase {
                ImeResetPhase::Clear => {}
                ImeResetPhase::Awaiting { .. } => {
                    self.set_ime_reset(viewport, ImeResetPhase::Suspended { viewport, needs_rearm: true, request: None });
                }
                ImeResetPhase::Suspended { viewport: guard_viewport, request, .. } => {
                    // A request that was not bound to the actual custom output in this pass is
                    // retired locally; a later eligible activation obtains a fresh one. A guard
                    // stamped for another viewport (or a request issued for one) is never reused.
                    if guard_viewport != viewport || request.is_some() {
                        self.set_ime_reset(viewport, ImeResetPhase::Suspended { viewport, needs_rearm: true, request: None });
                    }
                }
                ImeResetPhase::AllocationFailed { .. } => {
                    self.set_ime_reset(viewport, ImeResetPhase::Suspended { viewport, needs_rearm: true, request: None });
                }
            }
            return;
        };
        match phase {
            ImeResetPhase::Clear => {}
            ImeResetPhase::Awaiting { viewport: old_viewport, destination: old_destination, output_context: old_context, .. } => {
                if old_viewport != viewport || old_destination != destination || old_context != output_context {
                    self.request_ime_rearm(ctx, viewport, destination, output_context);
                }
            }
            ImeResetPhase::Suspended { viewport: guard_viewport, request, needs_rearm } => {
                let request = if needs_rearm && guard_viewport == viewport {
                    // Only a request issued for this exact viewport may be bound here.
                    request.filter(|id| id.viewport_id() == viewport)
                } else {
                    None
                };
                match request.or_else(|| ctx.request_ime_reset(viewport)) {
                    Some(id) => {
                        self.set_ime_reset(viewport, ImeResetPhase::Awaiting { id, viewport, destination, output_context });
                    }
                    None => {
                        self.set_ime_reset(viewport, ImeResetPhase::AllocationFailed { viewport, destination, output_context });
                    }
                }
            }
            ImeResetPhase::AllocationFailed { viewport: old_viewport, destination: old_destination, output_context: old_context } => {
                // Repeated idle output for the same destination does not retry allocation.
                if old_viewport != viewport || old_destination != destination || old_context != output_context {
                    self.request_ime_rearm(ctx, viewport, destination, output_context);
                }
            }
        }
    }

    /// Request one replacement IME reset token and record `Awaiting` on success or
    /// `AllocationFailed` when no token can be allocated.
    fn request_ime_rearm(&mut self, ctx: &egui::Context, viewport: egui::ViewportId, destination: ImeDestination, output_context: ImeDescriptorKey) {
        match ctx.request_ime_reset(viewport) {
            Some(id) => self.set_ime_reset(viewport, ImeResetPhase::Awaiting { id, viewport, destination, output_context }),
            None => self.set_ime_reset(viewport, ImeResetPhase::AllocationFailed { viewport, destination, output_context }),
        }
    }

    /// A standalone run_logic tick paints no custom geometry. Suspend once, without replaying
    /// stale input; the next full UI pass can rearm from its actual descriptor.
    fn finalize_ime_logic(&mut self, ctx: &egui::Context) {
        let viewport = self.ime_viewport_or(ctx);
        if self.ime_reset_for(viewport).blocks_custom_input() {
            // Only this viewport's guard is touched. A root/child logic tick must not suspend or
            // rebind another viewport's independent reset lifetime.
            self.set_ime_reset(viewport, ImeResetPhase::Suspended { viewport, needs_rearm: true, request: None });
        }
    }

    /// Note compositions the application settled since the last check (a click, a key, another
    /// command). Those are announced to the platform as an interrupt in [`Self::observe_ime`],
    /// and a platform event for them is stale from that moment on, even in the same pass.
    fn note_settled_composition(&mut self, ctx: &egui::Context) {
        let settles = self.session.composition_settles();
        let current_stamp = self.ime_terminal_stamp();
        // `file.close` then `file.new` can replace the active uid before any pass observes the
        // empty document. That is not a safe boundary for the old terminal: the platform may
        // still deliver its suffix. Keep the guard bound to its retired owner and treat the new
        // destination as a fresh interruption; only exact reset completion or an observed
        // teardown may clear the terminal metadata.
        let pending_interrupted = self.ime_terminal_pending.as_ref().is_some_and(|p| {
            let reference = if p.interrupted {
                p.interrupted_at
            } else {
                Some(ImeTerminalStamp { owner: Some(p.owner), revision: p.revision, anchor: Some(p.anchor), caret: Some(p.caret) })
            };
            reference != current_stamp
        });
        if settles == self.ime_settles_seen && !pending_interrupted {
            return;
        }
        let fresh = settles != self.ime_settles_seen;
        self.ime_settles_seen = settles;
        if let Some(pending) = &mut self.ime_terminal_pending {
            pending.cancelled = true;
            pending.interrupted = true;
            // Compare later passes against the destination where interruption actually happened,
            // not forever against the retired composition's origin. The destination may be absent
            // (the interruption closed its target); `interrupted` keeps that distinct from "not
            // interrupted yet".
            pending.interrupted_at = current_stamp;
        }
        self.ime_interrupt_pending = true;
        self.note_ime_interruption(ctx, fresh);
        let cancelled = self.ime_composition.as_mut().map(|c| {
            c.settled = true;
            (c.owner, c.generation)
        });
        if let Some((owner, generation)) = cancelled {
            // Retain a tombstone beyond the first stale event: clearing the composition must
            // not let the rest of this input batch commit into the next target. `observe_ime`
            // retires it in the pass that publishes the matching interrupt.
            self.ime_cancelled = Some(ImeCancelled { owner, generation });
        }
    }

    /// An input method's event for the slide's text (#101): the preedit is shown in the text and
    /// replaced on each update; the commit replaces it as one undo step.
    ///
    /// An event only ever changes the target its composition began in. The application settles a
    /// composition itself on a click, a key, a command or a target change, and the platform may
    /// still hold it: pre-edits and commits queued for that generation in the settling input batch
    /// are dropped (with [`egui::Memory::interrupt_ime`]) instead of being typed into whatever is
    /// selected now. After the interrupt has been published the platform is reset, so the next
    /// batch is treated as new input; egui's event payload has no generation, so an OS-queued
    /// event that crosses that reset cannot be distinguished from a legitimate one.
    ///
    /// A receipt is captured immediately before and after a successful commit or
    /// delete-surrounding mutation. Preedit, rejected/failed events and no-ops never earn
    /// [`egui::output::ImeChangeCause::InputMethod`].
    fn ime_event(&mut self, ctx: &egui::Context, e: &egui::ImeEvent) {
        self.note_settled_composition(ctx);
        let viewport = self.ime_frame_viewport.unwrap_or_else(|| ctx.viewport_id());
        let frame = ctx.cumulative_frame_nr_for(viewport);
        let target = self.ime_target();
        let owner = target.map(|t| t.owner);
        let accepted =
            matches!(e, egui::ImeEvent::Commit(_) | egui::ImeEvent::DeleteSurrounding { .. } | egui::ImeEvent::DeleteSurroundingBytes { .. });
        if self.reject_stale_ime_event(ctx, e, owner) {
            return;
        }
        // Until the exact newest reset request completes at its ordered stream position, custom
        // mutations stay blocked. Platform enable/disable signals are not acknowledgements and do
        // not release this guard; they still settle the old composition below.
        #[allow(deprecated)]
        if self.ime_reset_for(viewport).blocks_custom_input() && !matches!(e, egui::ImeEvent::Disabled | egui::ImeEvent::Enabled) {
            return;
        }
        let before = if accepted { self.ime_receipt_stamp() } else { None };
        let r = self.apply_ime_event(e);
        self.settle_ime_event_state(e, owner, r.is_ok());
        if r.is_ok() && accepted {
            self.refresh_ime_terminal_selection();
        }
        if let Err(e) = &r {
            self.set_status(e.to_string());
        } else if accepted
            && let (Some(target), Some(before), Some(after)) = (target, before, self.ime_receipt_stamp())
            && before != after
            && r.as_ref().is_ok_and(|v| !v.is_null() && v.get("handled").and_then(Value::as_bool) != Some(false))
        {
            // A no-op receipt (an empty commit, a zero-length delete, same text reinserted) is
            // not a mutation and must not poison the frame for a real commit later in the batch.
            self.ime_tracker.note_input_method_apply(viewport, frame, target, before, after);
        }
    }

    /// Drop an event that belongs to a composition the application already settled, or to another
    /// owner; returns `true` when the event must not be applied.
    ///
    /// A commit or deletion belongs to the composition that began the pre-edit: another owner
    /// (the user clicked elsewhere) or a composition the app already settled makes it stale.
    /// While the reset is still pending, the tombstone rejects pre-edits too (the platform has
    /// not yet been told to cancel its copy). `observe_ime` retires the tombstone in the pass
    /// that publishes the interrupt (or in an accepted no-descriptor pass, which resets the
    /// backend just the same), so a later commit-only is not lost forever: it is a
    /// legitimate direct commit or a new platform composition. egui's events carry no
    /// generation, so an event queued by the OS across that reset is indistinguishable from
    /// one the user just typed; only events in the same input batch as the settle are rejected
    /// deterministically. A cleared-preedit terminal is a distinct case: its pending owner
    /// remains guarded across that output boundary until its actual commit/enable/disable
    /// arrives. Publishing a reset is not evidence that the separately queued terminal was
    /// delivered. With no incoming event epoch, the first ambiguous new composition after an
    /// unobserved reset may be rejected; no timer guesses that it became safe.
    /// The tombstone is not filtered by the *current* target: a click can change the owner in
    /// the settling pass, and every remaining event of that batch still belongs to the
    /// pre-reset composition. Filtering would let the event after the first dropped pre-edit
    /// start composing in the new target before the platform is reset.
    fn reject_stale_ime_event(&mut self, ctx: &egui::Context, e: &egui::ImeEvent, owner: Option<egui::Id>) -> bool {
        let cancelled = self.ime_cancelled;
        let pending_cancelled = self.ime_terminal_pending.as_ref().is_some_and(|p| p.cancelled);
        #[allow(deprecated)]
        let stale = !matches!(e, egui::ImeEvent::Disabled | egui::ImeEvent::Enabled)
            && (self.ime_composition.is_some_and(|c| c.settled || Some(c.owner) != owner) || cancelled.is_some() || pending_cancelled);
        if !stale {
            return false;
        }
        if pending_cancelled && matches!(e, egui::ImeEvent::Commit(_)) {
            // The old terminal finally arrived: reject it and release its pending owner now,
            // so a new preedit later in this same raw batch can belong to the current target.
            self.ime_terminal_pending = None;
            self.ime_composition = None;
            self.ime_cancelled = None;
            self.ime_interrupt_pending = false;
            return true;
        }
        if pending_cancelled {
            // note_settled_composition already queued the reset. Do not also latch the
            // frame's memory interrupt: a terminal later in this batch can consume it and
            // permit a fresh preedit without resetting that new composition at pass end.
            self.ime_composition = None;
            return true;
        }
        if let Some(c) = cancelled {
            log::debug!("IGNORING IME event for settled composition owner {:?} generation {}", c.owner, c.generation);
        }
        self.ime_composition = None;
        // Ask the backend to drop its copy, so this is one stale event and not a stream.
        ctx.memory_mut(|m| m.interrupt_ime());
        true
    }

    /// Apply the document mutation an IME event asks for, before any receipt or tracker state.
    fn apply_ime_event(&mut self, e: &egui::ImeEvent) -> deckcraft_engine::Result<Value> {
        match e {
            egui::ImeEvent::Preedit { text, active_range_chars } => {
                self.session.compose_text(text, active_range_chars.as_ref().map(|r| (r.start, r.end)))
            }
            // A commit only replaces text that is being edited. It must never start editing a
            // merely selected shape: without a tracked pre-edit, a stale platform commit would
            // replace that shape's whole text.
            egui::ImeEvent::Commit(text) => {
                if self.session.active().is_some_and(|d| d.selection.text.is_some()) {
                    self.session.commit_text(text)
                } else {
                    Ok(Value::Null)
                }
            }
            egui::ImeEvent::DeleteSurrounding { before_chars, after_chars } => {
                if self.session.active().is_some_and(|d| d.selection.text.is_some()) {
                    self.session.delete_surrounding(*before_chars, *after_chars, false)
                } else {
                    Ok(Value::Null)
                }
            }
            egui::ImeEvent::DeleteSurroundingBytes { before_bytes, after_bytes } => {
                if self.session.active().is_some_and(|d| d.selection.text.is_some()) {
                    self.session.delete_surrounding(*before_bytes, *after_bytes, true)
                } else {
                    Ok(Value::Null)
                }
            }
            #[allow(deprecated)]
            egui::ImeEvent::Disabled | egui::ImeEvent::Enabled => {
                // The platform ended (or is about to restart) its composition itself.
                self.session.end_composition();
                Ok(Value::Null)
            }
        }
    }

    /// Update the tracked composition after `e` has been applied; `applied` says whether the
    /// document mutation succeeded.
    fn settle_ime_event_state(&mut self, e: &egui::ImeEvent, owner: Option<egui::Id>, applied: bool) {
        match e {
            // The platform ends its composition on a commit (and on Disabled/Enabled): nothing is
            // left to interrupt. A pre-edit keeps (or starts) the tracked composition.
            egui::ImeEvent::Commit(_) => {
                self.ime_terminal_pending = None;
                self.ime_composition = None;
                self.ime_cancelled = None;
                self.ime_interrupt_pending = false;
            }
            #[allow(deprecated)]
            egui::ImeEvent::Disabled | egui::ImeEvent::Enabled => {
                // A platform-side disable/enable already reset the composition. Do not carry an
                // older app-settled interrupt into the next (possibly new) composition.
                self.ime_terminal_pending = None;
                self.ime_composition = None;
                self.ime_cancelled = None;
                self.ime_interrupt_pending = false;
            }
            egui::ImeEvent::Preedit { text, .. } => {
                match (owner, applied, self.session.active().is_some_and(|d| d.composition.is_some())) {
                    (Some(owner), true, true) => {
                        // Updating the same composition keeps its generation. After a settle (or
                        // a first pre-edit) this is a new generation and clears the tombstone.
                        let generation = match self.ime_composition {
                            Some(c) if c.owner == owner && !c.settled => c.generation,
                            _ => {
                                self.ime_generation = self.ime_generation.wrapping_add(1).max(1);
                                self.ime_generation
                            }
                        };
                        self.ime_composition = Some(ImeComposition { owner, generation, settled: false });
                        self.ime_terminal_pending = None;
                        self.ime_cancelled = None;
                    }
                    (Some(owner), true, false) if text.is_empty() && self.ime_composition.is_some_and(|c| !c.settled && c.owner == owner) => {
                        // The model restored committed text, but no terminal was delivered. Keep
                        // this owner across passes; a later pointer/key/target change must reject
                        // its separately queued old delete/commit, even after an outgoing reset.
                        if let Some(st) = self.session.active()
                            && let Some(t) = &st.selection.text
                        {
                            self.ime_terminal_pending = Some(ImeTerminalPending {
                                owner,
                                revision: st.revision,
                                anchor: t.anchor,
                                caret: t.caret,
                                cancelled: false,
                                interrupted: false,
                                interrupted_at: None,
                            });
                        }
                    }
                    _ => {
                        self.ime_composition = None;
                        self.ime_terminal_pending = None;
                    }
                }
            }
            _ => {}
        }
    }

    /// Observe the canvas IME owner once per pass and publish its descriptor.
    ///
    /// No canvas target, a Sorter/Notes Page view, a focused egui field, a modal, the palette,
    /// the Backstage view, a slide show, an opening overlay, or an unfocused/hidden window means
    /// the custom owner is hidden: the tracker is told so and the next focus starts a new
    /// generation. The descriptor is rebuilt from the current committed text every pass.
    ///
    /// A composition the application settled itself is announced once as
    /// `should_interrupt_composition`, so the backend cancels the platform's copy of it. That
    /// settled pass is also the active-composition tombstone's lifecycle end. A cleared-preedit
    /// terminal retains its separate pending owner until the incoming terminal is observed.
    fn observe_ime(&mut self, ctx: &egui::Context) -> Option<ImeDescriptorKey> {
        let viewport = self.ime_frame_viewport.unwrap_or_else(|| ctx.viewport_id());
        self.note_settled_composition(ctx);
        // The editing window going away really ends the viewport's context; a merely hidden
        // owner (modal, palette, unfocused field) stays observed as unfocused instead.
        if self.session.active().is_none() || ctx.input(|i| i.viewport().close_requested()) {
            self.ime_interrupt_pending = false;
            if let Some(old) = self.ime_viewport.take() {
                self.ime_tracker.end_viewport(old);
                // Explicit viewport destruction removes only that viewport's guard.
                let _ = self.ime_reset.remove(&old);
            }
            let _ = self.ime_reset.remove(&viewport);
            self.ime_composition = None;
            self.ime_cancelled = None;
            self.ime_terminal_pending = None;
            return None;
        }
        if !ctx.input(|i| i.viewport().focused.unwrap_or(true)) {
            // The native window itself lost focus: no custom editor is eligible this pass, even
            // if the model still has a text selection and a last-pass geometry rect.
            self.ime_tracker.observe(viewport, None);
            return None;
        }
        let Some(rect) = self.ime_pending.take() else {
            // No canvas editor was painted in this pass. A discarded font/layout pass publishes
            // nothing the platform sees, so keep the interrupt and tombstone for a later pass.
            // An accepted pass is a backend reset only when it publishes *no* IME descriptor at
            // all: a focused stock `TextEdit` (notes pane, dialog, palette field) publishes its
            // own `o.ime` while `ime_pending` is `None`, and that descriptor is not a reset. Gate
            // retirement on the actual committed output, not on the absence of the canvas rect;
            // otherwise the next legitimate commit in the focused field would be rejected.
            let no_descriptor = ctx.output(|o| o.ime.is_none());
            if !ctx.will_discard() && no_descriptor {
                self.ime_interrupt_pending = false;
                self.ime_cancelled = None;
                if self.ime_composition.is_some_and(|c| c.settled) {
                    self.ime_composition = None;
                }
            }
            self.ime_tracker.observe(viewport, None);
            return None;
        };
        if !self.ime_custom_canvas_route(ctx) {
            // The canvas can still paint behind an opening overlay or a Sorter/Notes Page
            // reroute, and a forced hidden paint can run after the window lost focus or
            // visibility. The positive route and the UI wrapper both reject those, so a pending
            // rect must not republish the custom owner.
            self.ime_tracker.observe(viewport, None);
            return None;
        }
        let Some(st) = self.session.active() else {
            self.ime_interrupt_pending = false;
            self.ime_tracker.observe(viewport, None);
            return None;
        };
        let Some(target) = self.ime_target() else {
            self.ime_interrupt_pending = false;
            self.ime_cancelled = None;
            self.ime_tracker.observe(viewport, None);
            return None;
        };
        if self.ime_viewport != Some(viewport)
            && let Some(old) = self.ime_viewport.replace(viewport)
        {
            self.ime_tracker.end_viewport(old);
            let _ = self.ime_reset.remove(&old);
        }
        let committed = st.selection.text.as_ref().and_then(|t| deckcraft_engine::cmd::text::ime_surrounding(st, t));
        let mut observation =
            egui::ime_context::ImeObservation::new(ctx.cumulative_frame_nr_for(viewport), ctx.cumulative_pass_nr_for(viewport), target)
                .purpose(egui::IMEPurpose::Normal)
                .committed(committed.as_ref().map(|s| egui::ime_context::CommittedText::new(&s.text, s.cursor, s.anchor)));
        if committed.is_none() {
            observation = observation.unavailable_revision(st.revision);
        }
        let update = self.ime_tracker.observe(viewport, Some(observation))?;
        // Clear the interrupt only once it is part of a settled pass's output descriptor. A
        // discarded font/layout pass publishes nothing the platform sees, so keep it pending;
        // that pass is not the boundary at which stale pre-edits become a new generation.
        let interrupt = if ctx.will_discard() { false } else { std::mem::take(&mut self.ime_interrupt_pending) };
        if interrupt {
            // This pass's output carries the backend reset. The tombstone did its job for the
            // events already processed in this batch; from the next batch on every pre-edit or
            // commit is a new platform generation. A late event the OS queued across the reset
            // has no generation in its payload, so it cannot be rejected without also dropping
            // legitimate direct commits (documented residual in `ime_event`).
            self.ime_cancelled = None;
            if self.ime_composition.is_some_and(|c| c.settled) {
                self.ime_composition = None;
            }
        }
        let key = ImeDescriptorKey::new(update.owner, update.context_generation, update.purpose);
        ctx.output_mut(|o| {
            o.ime = Some(egui::output::IMEOutput {
                owner: update.owner,
                context_generation: update.context_generation,
                purpose: update.purpose,
                rect,
                cursor_rect: rect,
                should_interrupt_composition: interrupt,
                reset_request: None,
                change_cause: update.change_cause,
                surrounding_text: update.surrounding_text,
            });
        });
        Some(key)
    }

    /// Forget the injected or queued synthetic batch and everything recorded about it. Events
    /// waiting in `synthetic_next` are a separate, newer batch and are promoted by the caller.
    fn clear_synthetic_batch(&mut self) {
        self.synthetic.clear();
        self.synthetic_pending = false;
        self.synthetic_injected.clear();
        self.synthetic_injected_at = None;
        self.synthetic_injected_time = None;
        self.synthetic_target = None;
        self.synthetic_focus = None;
    }

    /// Start the batch that was queued behind an older one once the older batch is gone.
    fn promote_next_synthetic(&mut self) {
        if self.synthetic_pending || !self.synthetic.is_empty() || self.synthetic_next.is_empty() {
            return;
        }
        self.synthetic = std::mem::take(&mut self.synthetic_next);
        self.synthetic_target = self.synthetic_next_target.take();
        self.synthetic_focus = None;
    }

    /// A font/layout pass will be discarded: egui already consumed the injected events but the
    /// app deliberately lays nothing out, so put them back at the head of the queue. The next
    /// frame injects them again after the new fonts are installed; otherwise a control click that
    /// happened to arrive on the very first frame would be acknowledged without ever reaching the
    /// UI.
    fn requeue_injected_synthetic(&mut self) {
        if !self.synthetic_pending {
            return;
        }
        let mut combined = std::mem::take(&mut self.synthetic_injected);
        combined.append(&mut self.synthetic);
        self.synthetic = combined;
        self.synthetic_pending = false;
        self.synthetic_injected_at = None;
        self.synthetic_injected_time = None;
        self.synthetic_focus = None;
        if self.synthetic.is_empty() {
            self.synthetic_target = None;
            self.promote_next_synthetic();
        }
    }

    /// Remove exactly the recorded synthetic events from `events`. The recorded index is tried
    /// first so an identical host event earlier in the batch is never removed; the last exact
    /// match is only a fallback for a raw input that was rewritten around the events.
    fn drain_synthetic_from(&self, events: &mut Vec<egui::Event>) -> bool {
        let injected = &self.synthetic_injected;
        let n = injected.len();
        if n == 0 {
            return false;
        }
        let exact = self.synthetic_injected_at.filter(|start| events.get(*start..*start + n).is_some_and(|w| w == injected.as_slice()));
        let at = exact.or_else(|| events.windows(n).rposition(|w| w == injected.as_slice()));
        if let Some(at) = at {
            events.drain(at..at + n);
            true
        } else {
            false
        }
    }

    /// Drop an injected batch whose document, canvas, focus or modal owner changed during
    /// `logic`. eframe calls `raw_input_hook` before `App::logic`, so a control request drained
    /// there can still retarget the same `RawInput`.
    ///
    /// This must not infer the pass phase from `ViewportInfo::visible()`: a visible root can run
    /// logic-only (an overdue redraw), and a hidden root can run a full pass for a screenshot or
    /// `RequestPaintWhileHidden`. A logic-only callback leaves `InputState` from the *previous*
    /// pass; remove the batch only when the live input carries exactly what this pass injected
    /// (same `RawInput::time` and the exact recorded event window). Otherwise keep the pending
    /// bookkeeping so the next `raw_input_hook` can retract the events from the carried
    /// `RawInput` before eframe delivers them in a full pass.
    fn retract_synthetic_after_logic(&mut self, ctx: &egui::Context) {
        if !self.synthetic_pending {
            return;
        }
        let current = self.input_target();
        let focus = ctx.memory(|m| m.focused());
        if self.synthetic_target == Some(current) && focus == self.synthetic_focus {
            return;
        }
        let injected_time = self.synthetic_injected_time;
        let mut removed = false;
        ctx.input_mut(|i| {
            // A logic-only call keeps the previous pass's `InputState`; its `raw.time` is not the
            // time of the RawInput this batch was injected into. Never drain an older batch that
            // happens to look similar.
            if i.raw.time != injected_time {
                return;
            }
            let mut events = std::mem::take(&mut i.events);
            let ok = self.drain_synthetic_from(&mut events);
            i.events = events;
            if ok {
                // Keep `InputState::raw` consistent with what UI code reads as `events`.
                let mut raw_events = std::mem::take(&mut i.raw.events);
                let _ = self.drain_synthetic_from(&mut raw_events);
                i.raw.events = raw_events;
                removed = true;
            }
        });
        if removed {
            self.clear_synthetic_batch();
            self.promote_next_synthetic();
        } else {
            // Logic-only (or a pass whose input does not contain the batch): the events are in
            // eframe's carried RawInput. Keep `synthetic_pending`, `synthetic_injected` and the
            // stale target so the next hook retracts them there. Events queued after the batch
            // belong to the same stale target as well.
            self.synthetic.clear();
        }
    }

    /// True when the pending synthetic batch is in this pass's live `InputState`, i.e. this is
    /// the full pass the batch was admitted for and egui has already derived pointer/key state
    /// from it. A logic-only callback leaves the previous pass's `InputState` (with another
    /// `raw.time`) and must not be mistaken for the live pass, so control/background work is
    /// still drained there and the next `raw_input_hook` retracts a batch that went stale.
    fn synthetic_batch_is_live(&self, ctx: &egui::Context) -> bool {
        if !self.synthetic_pending || self.synthetic_injected.is_empty() {
            return false;
        }
        let Some(injected_time) = self.synthetic_injected_time else { return false };
        let injected = &self.synthetic_injected;
        ctx.input(|i| i.raw.time == Some(injected_time) && i.events.windows(injected.len()).any(|w| w == injected.as_slice()))
    }

    /// Input that belongs to the pass about to run: native dropped files and one synthetic
    /// control-channel batch. The synthetic batch is injected at most once; while it is
    /// unacknowledged, later passes cannot replay it. A document, canvas, focus or modal change
    /// drops the whole queued batch rather than typing into the wrong place.
    ///
    /// The hook always injects into the `RawInput`; it does not try to guess whether eframe will
    /// run this as a full or a logic-only pass. `run_logic` does not consume events but eframe
    /// carries the whole `RawInput` into the next full pass, so an already-injected batch is
    /// retracted at the next hook if a control request changed the target, and otherwise is
    /// delivered exactly once in the next real UI pass. `ViewportInfo::visible()` is not a phase
    /// signal: a hidden root still gets a full pass for a screenshot or `RequestPaintWhileHidden`.
    pub fn raw_input_hook(&mut self, ctx: &egui::Context, raw: &mut egui::RawInput) {
        #[cfg(not(target_arch = "wasm32"))]
        for f in std::mem::take(&mut raw.dropped_files) {
            let name = f.path().file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "dropped file".into());
            match f.bytes() {
                Ok(bytes) => self.open_or_insert_bytes(&name, &bytes),
                Err(e) => self.set_status(e),
            }
        }
        let focus = ctx.memory(|m| m.focused());
        if self.synthetic_pending {
            let current = self.input_target();
            if self.synthetic_target != Some(current) || focus != self.synthetic_focus {
                // eframe keeps the whole RawInput from a logic-only pass in `pending_raw_input`
                // and appends the next host input to it. The events are already in `raw.events`;
                // clearing only the unsent queue would still deliver the stale click or key into
                // the new document on the next full pass. Retract exactly the batch we appended.
                let _ = self.drain_synthetic_from(&mut raw.events);
                self.clear_synthetic_batch();
                // New control input queued for the current target while the stale batch was in
                // flight is a separate batch; keep it instead of clearing it with the old one.
                self.promote_next_synthetic();
            } else {
                // This RawInput is the carrier for the next pass, and eframe refreshes its time
                // on every call. Record the current time so `retract_synthetic_after_logic` can
                // tell this full pass's live `InputState` from a logic-only callback's stale one.
                self.synthetic_injected_time = raw.time;
            }
        }
        if self.synthetic_pending || self.synthetic.is_empty() {
            return;
        }
        let current = self.input_target();
        if self.synthetic_target != Some(current) {
            // The batch was queued for a document/slide/canvas that has since changed.
            self.synthetic.clear();
            self.synthetic_target = None;
            self.promote_next_synthetic();
            if self.synthetic.is_empty() {
                return;
            }
            if self.synthetic_target != Some(current) {
                // The newer batch is stale too: drop it rather than delivering it to this target.
                self.synthetic.clear();
                self.synthetic_target = None;
                self.synthetic_next.clear();
                self.synthetic_next_target = None;
                return;
            }
        }
        self.synthetic_generation = self.synthetic_generation.wrapping_add(1).max(1);
        log::debug!("injecting synthetic input batch {}", self.synthetic_generation);
        let n = match self.synthetic.first() {
            Some(egui::Event::PointerMoved(_) | egui::Event::PointerButton { .. }) => 1,
            _ => self.synthetic.iter().position(|e| matches!(e, egui::Event::Key { pressed: false, .. })).map_or(self.synthetic.len(), |i| i + 1),
        };
        let n = n.min(self.synthetic.len());
        let batch: Vec<egui::Event> = self.synthetic.drain(..n).collect();
        let at = raw.events.len();
        raw.events.extend(batch.iter().cloned());
        self.synthetic_injected = batch;
        self.synthetic_injected_at = Some(at);
        self.synthetic_injected_time = raw.time;
        self.synthetic_pending = true;
        self.synthetic_focus = focus;
    }

    /// The app shortcuts that still make sense on the empty File/start screen: create, open,
    /// the command palette, preferences and the File tab itself. Document commands (save, find,
    /// hyperlink, show, zoom) stay out; none of them can act on a presentation that is not open.
    fn start_screen_shortcut(key: egui::Key, m: Mods) -> bool {
        use egui::Key::*;
        matches!(
            (key, m.cmd, m.shift, m.alt),
            (O, true, false, false) | (P, true, true, false) | (Comma, true, false, false) | (F, false, false, true)
        )
    }

    /// File-tab navigation and document-free shortcuts, also used on the start screen.
    fn route_backstage_keys(&mut self, events: Vec<egui::Event>) {
        if self.backstage.is_none() {
            return;
        }
        // The File tab: its own keys (Esc goes back, arrows walk the side menu) and app
        // shortcuts (New, Open, Command Palette…); typing, pasting or deleting would edit the
        // hidden presentation.
        let start_screen = self.session.active().is_none();
        for e in events {
            if let egui::Event::Key { key, pressed: true, modifiers, .. } = e {
                let mods = Mods { shift: modifiers.shift, alt: modifiers.alt, cmd: modifiers.command || modifiers.ctrl };
                let handled = backstage::key(self, key, mods)
                    // With no presentation the File home only owns the shortcuts that create or
                    // open one. Borrowing the whole document shortcut table here would also arm
                    // Save/Save As, hyperlink, find/replace, show and zoom against the empty
                    // start screen, which is a scope expansion no editing route needs.
                    || (mods.cmd || mods.alt)
                        && (!start_screen || Self::start_screen_shortcut(key, mods))
                        && menus::ui_shortcut(self, key, mods);
                if handled {
                    continue;
                }
                // Engine shortcuts that don't touch the hidden slide (Ctrl+N).
                if mods.cmd
                    && matches!(key, egui::Key::N)
                    && let Err(e) = self.session.key(key_name(key), mods)
                {
                    self.set_status(e.to_string());
                }
            }
        }
    }

    /// Keyboard and clipboard for the slide (when no text field has focus).
    fn keyboard(&mut self, ctx: &egui::Context) {
        let events = ctx.input(|i| i.events.clone());
        // Ctrl+V with only an image on the clipboard: egui-winit sends no Paste event and swallows the
        // key press, so the release of a V press never seen is the paste. Tracked even while a text
        // field has focus, so that a V typed there is not taken for one.
        let mut image_paste = false;
        let mut pasted_image_event = false;
        for e in &events {
            match e {
                egui::Event::PasteImage(_) => {
                    pasted_image_event = true;
                    self.v_seen = true;
                }
                egui::Event::Paste(_) | egui::Event::Key { key: egui::Key::V, pressed: true, .. } => self.v_seen = true,
                egui::Event::Key { key: egui::Key::V, pressed: false, modifiers, .. } => {
                    // Only a release with Command/Ctrl held is a paste: a bare V release is not.
                    let seen = std::mem::take(&mut self.v_seen);
                    image_paste = !seen && (modifiers.command || modifiers.ctrl);
                }
                _ => {}
            }
        }
        let viewport = self.ime_viewport_or(ctx);
        if self.session.active().is_none() {
            // No document: keep slide-editing input suspended, while the start screen's
            // File-tab navigation and document-free shortcuts remain available.
            self.suspend_ime_reset(viewport);
            self.route_ime_controls_only(ctx, &events);
            if self.palette.is_none() && self.dialog.as_ref().is_none_or(|d| !d.modal()) && ctx.memory(|m| m.focused().is_none()) {
                self.route_backstage_keys(events);
            }
            self.drain_requests();
            return;
        }
        let modal = self.dialog.as_ref().is_some_and(|d| d.modal()) || self.palette.is_some();
        // While slide text is being edited, the text owns the keyboard (#93). Tab and the arrow
        // keys move egui's keyboard focus onto ordinary widgets (the window's drag handle, ribbon
        // buttons), and that stray focus would swallow every later key and end text entry. Let it
        // go; a real text field or an open menu keeps its keys.
        if !modal
            && self.session.active().is_some_and(|d| d.selection.text.is_some())
            && !ctx.text_edit_focused()
            && !ctx.any_popup_open()
            && let Some(id) = ctx.memory(|m| m.focused())
        {
            ctx.memory_mut(|m| m.surrender_focus(id));
        }
        if ctx.memory(|m| m.focused().is_some()) || modal {
            // A stock field/modal/palette owns input now. Retire the custom fence, then visit
            // controls in raw order; stock text/key events are left for the stock widgets.
            self.suspend_ime_reset(viewport);
            self.route_ime_controls_only(ctx, &events);
            return;
        }
        if self.backstage.is_some() {
            self.suspend_ime_reset(viewport);
            self.route_ime_controls_only(ctx, &events);
            self.route_backstage_keys(events);
            self.drain_requests();
            return;
        }
        let img = if image_paste { self.services.clipboard_image.as_mut().and_then(|f| f()) } else { None };
        if let Some(png) = img {
            let data = deckcraft_engine::cmd::base64_encode(&png);
            if let Err(e) = self.session.execute("insert.picture", &json!({"name": "Pasted image.png", "data": data})) {
                self.set_status(e.to_string());
            }
        }
        for e in events {
            // An earlier command in this raw batch may have settled the composition. Request the
            // replacement at this ordered boundary, before the current event is considered.
            self.note_settled_composition(ctx);
            match e {
                egui::Event::ImeResetCompleted { request_id } => self.ime_reset_completed(request_id, ctx),
                egui::Event::PasteImage(image) => match color_image_png(&image) {
                    Ok(png) => {
                        let params = json!({"name": "Pasted image.png", "data": deckcraft_engine::cmd::base64_encode(&png)});
                        if let Err(e) = self.session.execute("insert.picture", &params) {
                            self.set_status(e.to_string());
                        }
                    }
                    Err(e) => self.set_status(e),
                },
                egui::Event::Text(t) => {
                    let revision = self.session.active().map(|d| d.revision);
                    if self.session.active().is_some_and(|d| d.selection.text.is_some() || d.selection.shapes.len() == 1)
                        && let Err(e) = self.session.type_text(&t)
                    {
                        self.set_status(e.to_string());
                    }
                    if self.session.active().map(|d| d.revision) != revision {
                        self.note_application_edit(ctx);
                    }
                }
                egui::Event::Ime(ime) => self.ime_event(ctx, &ime),
                egui::Event::Copy => self.publish_clipboard(ctx, false),
                egui::Event::Cut => self.publish_clipboard(ctx, true),
                egui::Event::Paste(text) => {
                    // Our own marker for objects without text: paste what the session copied.
                    let text = if text == OBJECTS_MARKER { String::new() } else { text };
                    // A picture (TIFF from Keynote, a screenshot) lands when the paste has no text.
                    // Text keeps its HTML (bold, italic, underline) when the host can read it.
                    let html = self.services.clipboard_html.as_mut().and_then(|f| f()).filter(|h| !h.is_empty());
                    // egui 0.37 sends `PasteImage` directly; the services fallback stays for
                    // integrations that still send an empty `Paste`. Never insert twice.
                    let img = if text.is_empty() && !pasted_image_event { self.services.clipboard_image.as_mut().and_then(|f| f()) } else { None };
                    let revision = self.session.active().map(|d| d.revision);
                    let r = match img {
                        Some(png) => self
                            .session
                            .execute("insert.picture", &json!({"name": "Pasted image.png", "data": deckcraft_engine::cmd::base64_encode(&png)})),
                        None => {
                            let mut params = json!({"text": text});
                            if let Some(h) = html {
                                params["html"] = Value::String(h);
                            }
                            self.session.execute("edit.paste", &params)
                        }
                    };
                    if let Err(e) = r {
                        self.set_status(e.to_string());
                    }
                    if self.session.active().map(|d| d.revision) != revision {
                        self.note_application_edit(ctx);
                    }
                }
                egui::Event::Key { key, pressed: true, modifiers, .. } => {
                    if key == egui::Key::Tab {
                        // The slide has the keyboard, so Tab is ours (indent, tab character, next
                        // shape). Left to egui it would also focus the first ribbon widget, and
                        // every key after it would go there instead of the slide (#78).
                        ctx.memory_mut(|m| m.move_focus(egui::FocusDirection::None));
                    }
                    if key == egui::Key::Escape && self.anim_preview.is_some() && !modifiers.command && !modifiers.ctrl && !modifiers.alt {
                        self.anim_preview = None;
                        continue;
                    }
                    let mods = Mods { shift: modifiers.shift, alt: modifiers.alt, cmd: modifiers.command || modifiers.ctrl };
                    if !self.session.tool.drawing_freeform() && menus::ui_shortcut(self, key, mods) {
                        continue;
                    }
                    let name = key_name(key);
                    let revision = self.session.active().map(|d| d.revision);
                    match self.session.key(name, mods) {
                        Ok(_) => {}
                        Err(e) => self.set_status(e.to_string()),
                    }
                    if self.session.active().map(|d| d.revision) != revision {
                        self.note_application_edit(ctx);
                    }
                }
                _ => {}
            }
        }
        self.drain_requests();
    }

    /// Offer the copy or cut on the system clipboard.
    ///
    /// Slides go out as plain text. Text goes out as HTML plus the same plain words, so a paste
    /// back into DeckCraft still uses the internal clipboard. A textless shape leaves the
    /// invisible [`OBJECTS_MARKER`] rather than a picture: with only an image on the clipboard
    /// egui-winit drops Ctrl+V, and the shortcut could no longer paste the shape here (#182).
    fn publish_clipboard(&mut self, ctx: &egui::Context, cut: bool) {
        let cmd = if cut { "edit.cut" } else { "edit.copy" };
        let Ok(v) = self.session.execute(cmd, &json!({})) else { return };
        let text = v.get("text").and_then(Value::as_str).unwrap_or("").to_string();
        let html = v.get("html").and_then(Value::as_str).unwrap_or("").to_string();
        if self.session.clipboard.slides.is_empty() && !html.is_empty() && self.services.clipboard_set_html.as_mut().is_some_and(|f| f(&html, &text))
        {
            return;
        }
        ctx.copy_text(system_clipboard_text(&text));
    }

    /// Lay out the whole window.
    ///
    /// Every return route, including fonts/layout discards, show mode, modal/Backstage and the
    /// no-document paths, reaches the same outer reset observation and finalizer below. No early
    /// route may publish or inspect IME output on its own.
    pub fn ui(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        if !Self::root_viewport_admitted(&ctx) {
            log::debug!("ignoring SlideApp::ui for render-only viewport {:?}", ctx.viewport_id());
            return;
        }
        self.ui_inner(ui);
        // The last writer (all panels, dialogs, the custom producer and deferred child windows)
        // has run. Observe the actual custom descriptor for this pass, then reconcile the reset
        // lifetime against the actual committed output. This also runs for discarded passes.
        self.ime_frame_viewport = Some(ctx.viewport_id());
        let custom = self.observe_ime(&ctx);
        self.finalize_ime_reset(&ctx, custom);
    }

    /// Run the admitted root UI pass, including setup gating, panels, canvas, dialogs, overlays, and
    /// show mode.
    fn ui_inner(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        self.ime_frame_viewport = Some(ctx.viewport_id());
        self.ime_pending = None;
        if !self.ui_setup(&ctx) {
            // This pass is discarded for the new fonts. The input events were already consumed
            // by egui, so keep them queued for the next frame instead of silently losing them.
            self.requeue_injected_synthetic();
            return;
        }
        let t0 = now_ms();
        let t = theme::Tokens::get(&ctx);
        self.note_ui_text(&ctx);
        self.media.sync_video_textures(&ctx);
        canvas::trackpad_twist(self, &ctx);
        eyedropper::tick(self, &ctx);
        self.opening_ui(&ctx);
        if self.show.is_some() {
            self.anim_preview = None;
            show::ui(self, ui);
            doc_window::show(self, &ctx);
            self.perf.frame_ms = now_ms() - t0;
            self.ui_end(&ctx);
            return;
        }
        self.sync_backstage();
        self.keyboard(&ctx);
        self.sync_backstage();
        ribbon::title_bar(self, ui);
        if self.backstage.is_none() && self.session.active().is_some() {
            ribbon::show(self, ui);
            status::status_bar(self, ui);
        }
        let now = ui.input(|i| i.time);
        if self.tick_anim_preview(now) {
            ctx.request_repaint();
        }
        egui::CentralPanel::default().frame(egui::Frame::NONE.fill(t.chrome)).show(ui, |ui| {
            if self.backstage.is_some() {
                // File tab: the Backstage view replaces everything below the title bar.
                backstage::show(self, ui);
                return;
            }
            if self.session.active().is_none() {
                return;
            }
            match self.ui.view {
                ViewMode::Sorter => sorter::show(self, ui),
                ViewMode::NotesPage => sorter::notes_page(self, ui),
                ViewMode::Outline | ViewMode::Normal | ViewMode::Reading => {
                    if self.ui.pane.is_some() {
                        panes::show(self, ui);
                    }
                    if self.ui.view == ViewMode::Outline {
                        sorter::outline_pane(self, ui);
                    } else {
                        thumbs::show(self, ui);
                    }
                    egui::CentralPanel::default().frame(egui::Frame::NONE.fill(t.canvas)).show(ui, |ui| {
                        if self.ui.notes {
                            status::notes_pane(self, ui);
                        }
                        canvas::show(self, ui);
                    });
                }
            }
        });
        toast::note_missing_fonts(self);
        toast::show(self, &ctx);
        dialogs::show(self, &ctx);
        #[cfg(not(target_arch = "wasm32"))]
        updates_ui::show(self, &ctx);
        menus::palette(self, &ctx);
        for url in std::mem::take(&mut self.pending_urls) {
            ctx.open_url(egui::OpenUrl::new_tab(url));
        }
        if let Some(t) = self.pending_copy.take() {
            ctx.copy_text(t);
        }
        self.drain_requests();
        doc_window::show(self, &ctx);
        self.perf.frame_ms = now_ms() - t0;
        self.ui_end(&ctx);
    }

    /// Media clocks. Voices belong to the open presentation: closing the last one, or switching
    /// decks, stops them. The editor's media also stops when its slide is left. This runs from
    /// `logic` on every call so a minimized window keeps playing; texture upload stays in `ui`.
    fn tick_media_clock(&mut self, ctx: &egui::Context) {
        let uid = self.session.active().map(|d| d.uid);
        self.media.set_document(uid);
        if !self.media.any_active() {
            self.session.media_status.clear();
            return;
        }
        if self.show.is_none()
            && let Some(d) = self.session.active()
        {
            let here: Vec<deckcraft_model::ShapeId> = if d.selection.target == deckcraft_engine::Target::Slides {
                media::slide_media(&d.doc, d.selection.slide).into_iter().map(|m| m.0).collect()
            } else {
                vec![]
            };
            let gone: Vec<_> =
                self.media.ids().into_iter().filter(|id| !here.contains(id) && self.media.owner(*id) == Some(media::Owner::Editor)).collect();
            for id in gone {
                self.media.stop(id);
            }
        }
        let playing = self.media.tick_clock(monotonic_secs(), &mut self.session);
        if playing {
            ctx.request_repaint_after(std::time::Duration::from_millis(15));
        }
    }

    pub fn open_url(&mut self, url: &str) {
        self.pending_urls.push(url.to_string());
    }

    /// Drain pending control requests, executing ordinary requests and admitting screenshots for
    /// later saving and reply.
    fn drain_control(&mut self, ctx: &egui::Context) {
        let Some(rx) = self.control_rx.take() else { return };
        while let Ok(req) = rx.try_recv() {
            // A request whose sender gave up waiting (reply timeout) must not run late (#719).
            if !req.take() {
                continue;
            }
            let reply = req.reply.clone();
            let revision = self.session.active().map(|d| d.revision);
            match control::handle(self, ctx, &req) {
                control::Outcome::Done(v) => {
                    let _ = reply.send(v);
                }
                control::Outcome::Screenshot { path } => {
                    let doc = self.session.active().map(|d| d.uid);
                    if let Err(e) = self.capture.admit(path, reply.clone(), doc, web_time::Instant::now()) {
                        let _ = reply.send(json!({"ok": false, "error": e}));
                    }
                }
            }
            if self.session.active().map(|d| d.revision) != revision {
                self.note_application_edit(ctx);
            }
        }
        self.control_rx = Some(rx);
    }

    /// Save completed screenshots on the app thread and send exactly one reply each.
    fn drain_screenshots(&mut self) {
        let doc = self.session.active().map(|d| d.uid);
        let completed = self.capture.drain(web_time::Instant::now(), doc);
        for c in completed {
            let v = control::save_screenshot(self, c.image.as_ref(), c.path.as_deref());
            // Keep the aggregate charged through RGBA/PNG/base64 encoding and the write.
            self.capture.release(c.reserved);
            let _ = c.reply.send(v);
        }
    }
}

/// Engine key name for an egui key.
pub fn key_name(key: egui::Key) -> &'static str {
    use egui::Key::*;
    match key {
        Enter => "Enter",
        Escape => "Escape",
        Backspace => "Backspace",
        Delete => "Delete",
        Tab => "Tab",
        ArrowLeft => "Left",
        ArrowRight => "Right",
        ArrowUp => "Up",
        ArrowDown => "Down",
        Home => "Home",
        End => "End",
        PageUp => "PageUp",
        PageDown => "PageDown",
        Space => "Space",
        F2 => "F2",
        F5 => "F5",
        F7 => "F7",
        other => other.name(),
    }
}

/// Seconds since this process's monotonic origin.
///
/// `web_time::Instant` is `std::time::Instant` natively and `performance.now()` on wasm; it never
/// jumps backwards and is not `Date.now()` (which is wall time and may be zero/frozen in a
/// browser). Use this for deadlines, show/media clocks and duration displays.
static CLOCK_ORIGIN: std::sync::OnceLock<web_time::Instant> = std::sync::OnceLock::new();

/// Monotonic seconds since the first call.
pub fn monotonic_secs() -> f64 {
    CLOCK_ORIGIN.get_or_init(web_time::Instant::now).elapsed().as_secs_f64()
}

/// Monotonic milliseconds. Kept for status/toast durations and frame timings; never wall time.
pub fn now_ms() -> f64 {
    monotonic_secs() * 1000.0
}

/// Largest clipboard image accepted into the document: 16 megapixels, the same bound as
/// background removal. The image was already allocated by the integration; this keeps the
/// RGBA copy, PNG and base64 from multiplying an arbitrarily large paste before `insert.picture`.
const MAX_PASTE_IMAGE_PIXELS: usize = 16_000_000;

/// Encode an egui color image as the PNG bytes `insert.picture` stores.
fn color_image_png(image: &egui::ColorImage) -> Result<Vec<u8>, String> {
    let [w, h] = image.size;
    let pixels = w.checked_mul(h).ok_or_else(|| "the pasted image is too large".to_string())?;
    if pixels == 0 {
        return Err("the pasted image is empty".into());
    }
    if pixels > MAX_PASTE_IMAGE_PIXELS {
        return Err(format!("the pasted image is {w}×{h} pixels; the limit is 16 megapixels"));
    }
    let rgba: Vec<u8> = image.pixels.iter().flat_map(|c| c.to_array()).collect();
    Ok(deckcraft_render::Image { width: w as u32, height: h as u32, pixels: rgba }.to_png())
}

/// Images for the browser download: the current slide, or every slide (named `<name>1.png`,
/// `<name>2.png`... like the native export) when `all`. `transparent` leaves the slide
/// background out (PNG only, #784).
fn web_image_files(
    doc: &deckcraft_model::Presentation,
    current: usize,
    all: bool,
    name: &str,
    ext: &str,
    transparent: bool,
) -> Vec<(String, Vec<u8>)> {
    let slides: Vec<usize> = if all { (0..doc.slides.len()).collect() } else { vec![current] };
    let single = slides.len() == 1;
    let scale = 2.0;
    let dpi = deckcraft_render::dpi_for_scale(scale);
    slides
        .into_iter()
        .enumerate()
        .map(|(k, i)| {
            let opts = deckcraft_render::RenderOpts { scale, transparent_background: transparent && ext == "png", ..Default::default() };
            let img = deckcraft_render::render_slide(doc, i, &opts);
            let bytes = if ext == "png" { img.to_png_at(dpi) } else { img.to_jpeg_at(92, Some(dpi)) };
            let file = if single { format!("{name}.{ext}") } else { format!("{name}{}.{ext}", k + 1) };
            (file, bytes)
        })
        .collect()
}

#[cfg(test)]
mod animpane_tests;
#[cfg(test)]
mod browsetheme_tests;
#[cfg(test)]
mod compare_tests;
#[cfg(test)]
mod eyedropper_tests;
#[cfg(test)]
mod field_frame_tests;
#[cfg(test)]
mod find_dialog_tests;
#[cfg(test)]
mod gpu_lock;
#[cfg(test)]
mod huge_deck_tests;
#[cfg(test)]
mod ime_tests;
#[cfg(test)]
mod last_color_tests;
#[cfg(test)]
mod replacefonts_tests;
#[cfg(test)]
mod reuse_picker_tests;
#[cfg(test)]
mod slide_size_tests;
#[cfg(test)]
mod split_button_tests;
#[cfg(test)]
mod texture_limit_tests;
#[cfg(test)]
mod ui_soak_tests;
#[cfg(test)]
mod units_tests;

#[cfg(test)]
#[path = "datetime_tests.rs"]
mod datetime_tests;

#[cfg(test)]
mod inbox_tests;

#[cfg(test)]
mod tests;
