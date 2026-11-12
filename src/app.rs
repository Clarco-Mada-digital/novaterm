//! Application NovaTerm : onglets, splits, rendu et interactions.
//!
//! Le clavier est intercepté *avant* egui (`raw_input_hook`) : ainsi Tab, les flèches
//! ou Entrée ne déplacent jamais le focus vers un bouton de l'interface (ce qui pouvait
//! fermer la fenêtre), et Ctrl+C / Ctrl+V / Ctrl+X arrivent bien au programme.

use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::sync::mpsc::Receiver;
use std::time::Duration;

use alacritty_terminal::grid::{Dimensions, Scroll};
use alacritty_terminal::index::{Column, Direction, Line, Point, Side};
use alacritty_terminal::selection::{Selection, SelectionType};
use alacritty_terminal::term::search::RegexSearch;
use alacritty_terminal::term::TermMode;
use egui::widgets::{paint_texture_at, ImageOptions};
use egui::{
    Color32, CornerRadius, Id, Key, Modifiers, Pos2, Rect, Sense, Stroke, TextureHandle, TextureOptions, Vec2,
};

use crate::blocks::{self, fmt_duration};
use crate::completions::Completer;
use crate::config::{AcceptKey, AutoTheme, BadgeMode, BellMode, BlinkStyle, BroadcastScope, Config, RestoreMode, RightClick, StartDir};
use crate::guard;
use crate::links::{self, Target};
use crate::platform;
use crate::quake;
use crate::session::{self, Node, Profile, Session, TabState};
use crate::history::{self, History};
use crate::fonts::{self, FontEntry};
use crate::input::{self, MouseButton};
use crate::keys::{Action, Bindings, KeyCombo, KeyContext, ALL_ACTIONS};
use crate::layout::{Dir, SplitTree};
use crate::render::{self, FontCache, FontMetrics};
use crate::settings::{self, SettingsUi};
use crate::terminal::Pane;
use crate::theme::{self, rgb, Theme};

struct Tab {
    tree: SplitTree,
    focused: u64,
    zoomed: Option<u64>,
    /// Titre choisi par l'utilisateur (sinon titre du panneau).
    title: Option<String>,
    /// Le clavier est diffusé à tous les panneaux de l'onglet.
    broadcast: bool,
    /// Activité en arrière-plan : None = rien, Some(None) = nouvelle sortie,
    /// Some(Some(ok)) = commande terminée (succès / échec).
    activity: Option<Option<bool>>,
}

impl Tab {
    fn new(id: u64) -> Self {
        Tab { tree: SplitTree::Leaf(id), focused: id, zoomed: None, title: None, broadcast: false, activity: None }
    }
}

/// Options de démarrage (ligne de commande, garde-fou).
pub struct StartOptions {
    pub config: Config,
    pub quake: bool,
    /// Raison du mode sans échec (lancement précédent planté, --safe-mode).
    pub safe_reason: Option<String>,
    pub config_warning: Option<String>,
    pub profile: Option<String>,
    /// Dossier demandé (« Ouvrir NovaTerm ici »).
    pub cwd: Option<PathBuf>,
}

/// Contenu de la palette.
#[derive(Clone, Copy, PartialEq, Eq)]
enum PaletteMode {
    All,
    Snippets,
    Profiles,
}

#[derive(Clone)]
enum PaletteItem {
    Action(Action),
    Snippet(usize),
    Profile(usize),
    Theme(String),
}

struct Toast {
    message: String,
    born: f64,
}

struct SearchState {
    pane: u64,
    query: String,
    regex: Option<RegexSearch>,
    last: Option<Point>,
    focus: bool,
    not_found: bool,
}

struct Palette {
    query: String,
    selected: usize,
    focus: bool,
    mode: PaletteMode,
}

/// Suggestions courantes pour la ligne du panneau actif.
#[derive(Default)]
struct Suggest {
    pane: u64,
    buffer: String,
    items: Vec<String>,
    /// Description (complétion intelligente) ; None = vient de l'historique.
    descs: Vec<Option<String>>,
    selected: usize,
}

/// Action déclenchée depuis la barre d'un bloc de commande.
#[derive(Clone, Copy)]
enum BlockAct {
    CopyCommand,
    CopyOutput,
    Rerun,
    ScrollTop,
}

/// Fenêtre « Rechercher dans l'historique ».
struct HistoryPopup {
    pane: u64,
    query: String,
    selected: usize,
    focus: bool,
}

struct Rename {
    tab: usize,
    text: String,
    focus: bool,
}

#[derive(Clone)]
enum ConfirmKind {
    CloseWindow,
    CloseTab(usize),
    ClosePane(u64),
    Paste(String),
}

struct Confirm {
    kind: ConfirmKind,
    title: String,
    message: String,
    button: String,
}

/// Commandes clavier des fenêtres internes (palette, recherche…).
enum UiCmd {
    ConfirmYes,
    ConfirmNo,
    PaletteMove(i32),
    PaletteRun,
    PaletteClose,
    RenameApply,
    RenameCancel,
    SearchNext(bool),
    SearchClose,
    HistoryMove(i32),
    HistoryRun(bool),
    HistoryClose,
}

/// Entrées clavier collectées dans `raw_input_hook`, traitées au début de `update`.
enum Pending {
    Action(Action),
    Key { key: Key, mods: Modifiers },
    Text(String),
    Paste(String),
    Ui(UiCmd),
}

fn search_input_id() -> Id {
    Id::new("nova_search_input")
}
fn palette_input_id() -> Id {
    Id::new("nova_palette_input")
}
fn history_input_id() -> Id {
    Id::new("nova_history_input")
}
fn rename_input_id() -> Id {
    Id::new("nova_rename_input")
}

pub struct NovaApp {
    ctx: egui::Context,
    config: Config,
    theme: Theme,
    bindings: Bindings,
    bindings_src: BTreeMap<String, Vec<String>>,
    tabs: Vec<Tab>,
    active: usize,
    panes: HashMap<u64, Pane>,
    next_id: u64,
    font_cache: FontCache,
    metrics: FontMetrics,
    font_catalog: Option<BTreeMap<String, FontEntry>>,
    applied_font: String,
    font_reload: u8,
    term_opts_sig: (usize, String),
    bg_texture: Option<TextureHandle>,
    applied_image: Option<String>,
    theme_sig: String,
    settings_open: bool,
    settings_ui: SettingsUi,
    help_open: bool,
    file_browser: Option<settings::FileBrowser>,
    search: Option<SearchState>,
    palette: Option<Palette>,
    rename: Option<Rename>,
    confirm: Option<Confirm>,
    allow_close: bool,
    pending: Vec<Pending>,
    config_dirty: bool,
    last_save: f64,
    bell_until: f64,
    pane_rects: Vec<(u64, Rect)>,
    fullscreen: bool,
    applied_decorations: bool,
    applied_on_top: bool,
    window_title: String,
    window_size: Vec2,
    toasts: Vec<Toast>,
    /// Bouton de souris enfoncé dans un panneau en mode « rapport souris ».
    mouse_down: Option<(u64, MouseButton)>,
    last_mouse_cell: Option<(usize, usize)>,
    zoom_accum: f32,
    /// Panneau qui a reçu le dernier rapport de focus (ESC [ I / ESC [ O).
    reported_focus: Option<u64>,
    history: History,
    history_rx: Option<Receiver<History>>,
    history_sig: (bool, bool),
    /// Commandes exécutées pendant la session (rejouées après un rechargement).
    session_cmds: Vec<String>,
    suggest: Suggest,
    history_popup: Option<HistoryPopup>,
    completer: Completer,
    /// Instance « terminal rapide ».
    quake: bool,
    quake_rx: Option<Receiver<()>>,
    quake_visible: bool,
    quake_placed: bool,
    /// Message important affiché en bandeau (mode sans échec, config invalide).
    banner: Option<String>,
    frames: u64,
    /// Désarmer le garde-fou après N frames réussies.
    guard_frames: Option<u64>,
    last_autosave: f64,
    last_input_time: f64,
    /// Bloc mis en évidence après une navigation : (panneau, id, jusqu'à).
    highlight: Option<(u64, u32, f64)>,
    /// Dialogue « enregistrer comme profil » : nom en cours de saisie.
    save_profile: Option<(String, bool)>,
    theme_import: Option<settings::FileBrowser>,
    dark_rx: Option<Receiver<Option<bool>>>,
    last_dark_poll: f64,
    system_dark: Option<bool>,
    /// Mode démo (captures d'écran du README) : script injecté dans l'application.
    demo: Option<Demo>,
    /// AppImage en cours d'exécution, pas encore dans le menu (proposition affichée).
    appimage_offer: Option<PathBuf>,
}

/// Étape du mode démo (`NOVATERM_DEMO=fichier`). Les entrées sont injectées dans
/// NovaTerm lui-même : aucune touche n'est envoyée au système.
enum DemoStep {
    Wait(f64),
    Type(String),
    Key(KeyCombo),
    Shot(PathBuf),
    Quit,
}

struct Demo {
    steps: std::collections::VecDeque<DemoStep>,
    resume_at: f64,
    pending_shot: Option<PathBuf>,
}

impl Demo {
    /// Format : une commande par ligne — `wait 1.5`, `type texte`, `enter`,
    /// `key Ctrl+Shift+E`, `shot chemin.png`, `quit` (# = commentaire).
    fn load(path: &str) -> Option<Self> {
        let text = std::fs::read_to_string(path).map_err(|e| eprintln!("NovaTerm: démo illisible: {e}")).ok()?;
        let mut steps = std::collections::VecDeque::new();
        for line in text.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')) {
            let (cmd, arg) = line.split_once(' ').unwrap_or((line, ""));
            let step = match cmd {
                "wait" => DemoStep::Wait(arg.trim().parse().unwrap_or(0.5)),
                "type" => DemoStep::Type(arg.to_string()),
                "enter" => DemoStep::Key(KeyCombo::new(false, false, false, Key::Enter)),
                "key" => match KeyCombo::parse(arg) {
                    Some(c) => DemoStep::Key(c),
                    None => {
                        eprintln!("NovaTerm: démo : touche inconnue {arg}");
                        continue;
                    }
                },
                "shot" => DemoStep::Shot(PathBuf::from(arg.trim())),
                "quit" => DemoStep::Quit,
                other => {
                    eprintln!("NovaTerm: démo : commande inconnue {other}");
                    continue;
                }
            };
            steps.push_back(step);
        }
        Some(Demo { steps, resume_at: 1.5, pending_shot: None })
    }
}

fn theme_sig(config: &Config) -> String {
    match &config.custom_theme {
        Some(t) => format!("custom:{:?}", t),
        None => config.theme_name.clone(),
    }
}

fn surface_color(theme: &Theme) -> Color32 {
    let b = theme.background;
    Color32::from_rgba_unmultiplied(b[0].saturating_add(14), b[1].saturating_add(14), b[2].saturating_add(18), 248)
}

fn apply_visuals(ctx: &egui::Context, theme: &Theme) {
    let b = theme.background;
    let luma = 0.299 * b[0] as f32 + 0.587 * b[1] as f32 + 0.114 * b[2] as f32;
    ctx.style_mut(|s| {
        s.visuals = if luma > 140.0 { egui::Visuals::light() } else { egui::Visuals::dark() };
        s.visuals.panel_fill = Color32::TRANSPARENT;
        s.visuals.window_fill = surface_color(theme);
        s.visuals.window_stroke = Stroke::new(1.0, Color32::from_black_alpha(60));
        s.visuals.selection.bg_fill = rgb(theme.cursor).gamma_multiply(0.45);
    });
}

#[derive(Clone, Copy)]
enum WinIcon {
    Minimize,
    Maximize,
    Restore,
    Close,
}

fn window_button(ui: &mut egui::Ui, icon: WinIcon, tooltip: &str, fg: Color32, surface: Color32, danger: bool) -> bool {
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(34.0, 24.0), Sense::click());
    let painter = ui.painter();
    let hover_fill = ui.style().visuals.widgets.hovered.bg_fill;
    let bg = if resp.hovered() {
        if danger {
            Color32::from_rgb(0xC4, 0x2B, 0x1C)
        } else {
            hover_fill
        }
    } else {
        Color32::TRANSPARENT
    };
    painter.rect_filled(rect, CornerRadius::same(7), bg);
    let color = if danger && resp.hovered() { Color32::WHITE } else { fg.gamma_multiply(0.9) };
    let stroke = Stroke::new(1.6, color);
    let c = rect.center();
    let mid = egui::epaint::StrokeKind::Middle;
    match icon {
        WinIcon::Minimize => {
            painter.line_segment([Pos2::new(c.x - 5.0, c.y + 2.0), Pos2::new(c.x + 5.0, c.y + 2.0)], stroke);
        }
        WinIcon::Maximize => {
            painter.rect_stroke(Rect::from_center_size(c, Vec2::splat(9.0)), CornerRadius::same(2), stroke, mid);
        }
        WinIcon::Restore => {
            let back = Rect::from_center_size(c + Vec2::new(1.5, -1.5), Vec2::splat(8.0));
            let front = Rect::from_center_size(c + Vec2::new(-1.5, 1.5), Vec2::splat(8.0));
            painter.rect_stroke(back, CornerRadius::same(2), stroke, mid);
            painter.rect_filled(front, CornerRadius::same(2), if resp.hovered() { hover_fill } else { surface });
            painter.rect_stroke(front, CornerRadius::same(2), stroke, mid);
        }
        WinIcon::Close => {
            painter.line_segment([c + Vec2::new(-4.5, -4.5), c + Vec2::new(4.5, 4.5)], stroke);
            painter.line_segment([c + Vec2::new(-4.5, 4.5), c + Vec2::new(4.5, -4.5)], stroke);
        }
    }
    resp.on_hover_text(tooltip).clicked()
}

/// Correspondance floue simple : tous les mots de la requête présents dans le texte.
fn fuzzy_match(query: &str, text: &str) -> bool {
    let text = text.to_lowercase();
    query.to_lowercase().split_whitespace().all(|w| text.contains(w))
}

#[cfg(target_os = "linux")]
fn set_primary_selection(text: &str) {
    use arboard::SetExtLinux;
    if let Ok(mut c) = arboard::Clipboard::new() {
        let _ = c.set().clipboard(arboard::LinuxClipboardKind::Primary).text(text.to_string());
    }
}

#[cfg(not(target_os = "linux"))]
fn set_primary_selection(_text: &str) {}

#[cfg(target_os = "linux")]
fn get_primary_selection() -> Option<String> {
    use arboard::GetExtLinux;
    arboard::Clipboard::new()
        .ok()?
        .get()
        .clipboard(arboard::LinuxClipboardKind::Primary)
        .text()
        .ok()
}

#[cfg(not(target_os = "linux"))]
fn get_primary_selection() -> Option<String> {
    None
}

fn get_clipboard() -> Option<String> {
    arboard::Clipboard::new().ok()?.get_text().ok()
}

/// Fichier qui pourrait s'exécuter si on l'« ouvre » avec l'application par défaut.
fn is_executable_file(path: &std::path::Path) -> bool {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    if matches!(ext.as_str(), "desktop" | "appimage" | "exe" | "msi" | "bat" | "cmd" | "ps1" | "vbs" | "jar" | "app" | "command" | "sh" | "run" | "bin") {
        return true;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(m) = std::fs::metadata(path) {
            return m.is_file() && m.permissions().mode() & 0o111 != 0;
        }
    }
    false
}

fn open_external(target: &str) {
    crate::platform::open(target);
}

impl NovaApp {
    pub fn new(cc: &eframe::CreationContext<'_>, start: StartOptions) -> Self {
        let ctx = cc.egui_ctx.clone();
        let mut config = start.config;
        let font_catalog = (!config.font_family.is_empty()).then(fonts::discover);
        let mut font_error = None;
        if let Err(e) = fonts::install(&ctx, &config.font_family, font_catalog.as_ref()) {
            // Police introuvable ou invalide : on revient à la police intégrée.
            font_error = Some(format!("{e} : police intégrée utilisée."));
            config.font_family.clear();
            config.save();
        }
        ctx.options_mut(|o| o.zoom_with_keyboard = false);
        let banner = match (&start.safe_reason, &start.config_warning, &font_error) {
            (Some(r), _, _) if r == "manual" => Some("Mode sans échec : police intégrée.".to_string()),
            (Some(r), _, _) => Some(guard::explain(r)),
            (None, Some(w), _) => Some(w.clone()),
            (None, None, Some(f)) => Some(f.clone()),
            _ => None,
        };

        let theme = config.theme();
        apply_visuals(&ctx, &theme);

        let metrics = FontMetrics { cell_w: 8.0, cell_h: 16.0 };
        let applied_decorations = config.custom_decorations;
        let mut app = Self {
            ctx: ctx.clone(),
            theme_sig: theme_sig(&config),
            theme,
            bindings: Bindings::build(&config.keybindings),
            bindings_src: config.keybindings.clone(),
            tabs: Vec::new(),
            active: 0,
            panes: HashMap::new(),
            next_id: 1,
            font_cache: FontCache::new(),
            metrics,
            font_catalog,
            applied_font: config.font_family.clone(),
            font_reload: 0,
            term_opts_sig: (config.scrollback, config.word_separators.clone()),
            bg_texture: None,
            applied_image: None,
            settings_open: false,
            settings_ui: SettingsUi::default(),
            help_open: false,
            file_browser: None,
            search: None,
            palette: None,
            rename: None,
            confirm: None,
            allow_close: false,
            pending: Vec::new(),
            config_dirty: false,
            last_save: 0.0,
            bell_until: 0.0,
            pane_rects: Vec::new(),
            fullscreen: config.start_fullscreen,
            applied_decorations,
            applied_on_top: config.always_on_top,
            window_title: "NovaTerm".into(),
            window_size: Vec2::new(config.window_width, config.window_height),
            toasts: Vec::new(),
            mouse_down: None,
            last_mouse_cell: None,
            zoom_accum: 0.0,
            reported_focus: None,
            history: History::default(),
            history_rx: Some(history::load_async(config.history_zsh, config.history_bash)),
            history_sig: (config.history_zsh, config.history_bash),
            session_cmds: Vec::new(),
            suggest: Suggest::default(),
            history_popup: None,
            completer: Completer::default(),
            quake: start.quake,
            quake_rx: if start.quake { quake::listen(ctx.clone()) } else { None },
            quake_visible: true,
            quake_placed: false,
            banner,
            frames: 0,
            guard_frames: Some(20),
            last_autosave: 0.0,
            last_input_time: 0.0,
            highlight: None,
            save_profile: None,
            theme_import: None,
            dark_rx: None,
            last_dark_poll: -100.0,
            system_dark: None,
            demo: std::env::var("NOVATERM_DEMO").ok().and_then(|p| Demo::load(&p)),
            appimage_offer: crate::appimage::current().filter(|p| !crate::appimage::is_integrated(p)),
            config,
        };
        // Démarrage : profil demandé, sinon session précédente, sinon un onglet vide.
        let profile = start.profile.clone().or_else(|| (!app.config.startup_profile.is_empty()).then(|| app.config.startup_profile.clone()));
        let mut opened = false;
        if let Some(dir) = start.cwd.clone() {
            if let Some(id) = app.spawn_pane(Some(dir)) {
                app.tabs.push(Tab::new(id));
                opened = true;
            }
        }
        if !app.quake && !opened {
            if let Some(name) = profile {
                opened = app.open_profile_by_name(&name);
            }
            if !opened && app.config.restore_session != RestoreMode::Off {
                if let Some(sess) = session::load() {
                    let content = app.config.restore_session == RestoreMode::LayoutAndContent;
                    app.restore_session(&sess, content);
                }
            }
        }
        if app.tabs.is_empty() {
            app.new_tab();
        }
        app
    }

    // ------------------------------------------------------------------
    // Session & profils
    // ------------------------------------------------------------------

    fn tree_to_node(&self, tree: &SplitTree, content: bool) -> Node {
        match tree {
            SplitTree::Leaf(id) => {
                let pane = self.panes.get(id);
                Node::Leaf {
                    cwd: pane.and_then(|p| p.cwd()).map(|p| p.display().to_string()),
                    command: None,
                    content: if content {
                        pane.map(|p| p.tail_text(self.config.restore_lines.clamp(10, 5000))).filter(|t| !t.trim().is_empty())
                    } else {
                        None
                    },
                }
            }
            SplitTree::Split { dir, ratio, left, right } => Node::Split {
                horizontal: *dir == Dir::Horizontal,
                ratio: *ratio,
                a: Box::new(self.tree_to_node(left, content)),
                b: Box::new(self.tree_to_node(right, content)),
            },
        }
    }

    /// Crée les panneaux d'un nœud de disposition.
    fn node_to_tree(&mut self, node: &Node) -> Option<SplitTree> {
        match node {
            Node::Leaf { cwd, command, content } => {
                let dir = cwd.as_ref().map(PathBuf::from).filter(|p| p.is_dir()).or_else(|| self.start_cwd());
                let id = self.spawn_pane(dir)?;
                if let Some(pane) = self.panes.get_mut(&id) {
                    if let Some(text) = content {
                        pane.inject_restored(text);
                    }
                    if let Some(cmd) = command.as_ref().filter(|c| !c.trim().is_empty()) {
                        pane.write_input(format!("{cmd}\r").as_bytes());
                    }
                }
                Some(SplitTree::Leaf(id))
            }
            Node::Split { horizontal, ratio, a, b } => {
                let l = self.node_to_tree(a);
                let r = self.node_to_tree(b);
                match (l, r) {
                    (Some(l), Some(r)) => Some(SplitTree::Split {
                        dir: if *horizontal { Dir::Horizontal } else { Dir::Vertical },
                        ratio: *ratio,
                        left: Box::new(l),
                        right: Box::new(r),
                    }),
                    (l, r) => l.or(r),
                }
            }
        }
    }

    fn capture_tabs(&self, content: bool) -> Vec<TabState> {
        self.tabs
            .iter()
            .map(|t| TabState {
                title: t.title.clone(),
                focused: t.tree.leaves().iter().position(|l| *l == t.focused).unwrap_or(0),
                root: self.tree_to_node(&t.tree, content),
            })
            .collect()
    }

    fn open_tabs(&mut self, tabs: &[TabState]) -> usize {
        let mut n = 0;
        for ts in tabs {
            if let Some(tree) = self.node_to_tree(&ts.root) {
                let leaves = tree.leaves();
                let focused = leaves.get(ts.focused).copied().unwrap_or(leaves[0]);
                self.tabs.push(Tab { tree, focused, zoomed: None, title: ts.title.clone(), broadcast: false, activity: None });
                n += 1;
            }
        }
        n
    }

    fn restore_session(&mut self, sess: &Session, content: bool) {
        let mut tabs = sess.tabs.clone();
        if !content {
            for t in &mut tabs {
                for leaf in t.root.leaves_mut() {
                    if let Node::Leaf { content, .. } = leaf {
                        *content = None;
                    }
                }
            }
        }
        if self.open_tabs(&tabs) > 0 {
            self.active = sess.active.min(self.tabs.len() - 1);
        }
    }

    fn save_session(&self) {
        if self.quake || self.config.restore_session == RestoreMode::Off {
            return;
        }
        let content = self.config.restore_session == RestoreMode::LayoutAndContent;
        session::save(&Session { active: self.active, tabs: self.capture_tabs(content) });
    }

    fn open_profile(&mut self, index: usize) {
        let Some(p) = self.config.profiles.get(index).cloned() else { return };
        let first = self.tabs.len();
        if self.open_tabs(&p.tabs) > 0 {
            self.active = first;
            self.toast(format!("Profil « {} » ouvert", p.name));
        }
    }

    fn open_profile_by_name(&mut self, name: &str) -> bool {
        match self.config.profiles.iter().position(|p| p.name.eq_ignore_ascii_case(name)) {
            Some(i) => {
                self.open_profile(i);
                true
            }
            None => {
                self.toast(format!("Profil « {name} » introuvable"));
                false
            }
        }
    }

    fn save_profile_named(&mut self, name: &str) {
        let name = name.trim();
        if name.is_empty() {
            return;
        }
        let tabs = self.capture_tabs(false);
        let profile = Profile { name: name.to_string(), tabs };
        match self.config.profiles.iter_mut().find(|p| p.name == name) {
            Some(p) => *p = profile,
            None => self.config.profiles.push(profile),
        }
        self.config_dirty = true;
        self.toast(format!("Profil « {name} » enregistré (Paramètres → Session pour les commandes)"));
    }

    // ------------------------------------------------------------------
    // Blocs de commandes
    // ------------------------------------------------------------------

    /// Navigue vers la commande précédente / suivante du panneau actif.
    fn jump_command(&mut self, up: bool) {
        let Some(id) = self.focused_id() else { return };
        let time = self.ctx.input(|i| i.time);
        let Some(pane) = self.panes.get_mut(&id) else { return };
        let tops = pane.block_tops();
        let offset = pane.term.grid().display_offset() as i32;
        let view_top = alacritty_terminal::index::Line(-offset);
        let target = if up {
            tops.iter().rev().find(|t| **t < view_top).copied()
        } else {
            tops.iter().find(|t| **t > view_top).copied()
        };
        match target {
            Some(t) => {
                pane.scroll_to_line(t);
                pane.refresh_marks();
                let bid = pane.vis_blocks.iter().find(|b| b.top_row == 0).map(|b| b.id);
                if let Some(bid) = bid {
                    self.highlight = Some((id, bid, time + 0.8));
                }
            }
            None if !up => {
                pane.term.scroll_display(Scroll::Bottom);
                pane.dirty = true;
            }
            None => {}
        }
    }

    fn last_block(&self) -> Option<(u64, u32)> {
        let id = self.focused_id()?;
        let pane = self.panes.get(&id)?;
        pane.blocks.iter().rev().find(|b| b.finished.is_some() || pane.running != Some(b.id)).map(|b| (id, b.id))
    }

    fn copy_block_output(&mut self, pane_id: u64, block: u32) {
        let Some(text) = self.panes.get(&pane_id).and_then(|p| p.block_output(block)) else {
            self.toast("Sortie introuvable (effacée de l'historique)");
            return;
        };
        let lines = text.lines().count();
        self.ctx.copy_text(text);
        self.toast(format!("Sortie copiée ({lines} lignes)"));
    }

    fn block_command(&self, pane_id: u64, block: u32) -> Option<String> {
        self.panes.get(&pane_id)?.block(block).map(|b| b.command.clone())
    }

    /// Notifications de fin de commande et des programmes.
    fn process_finished(&mut self, ctx: &egui::Context) {
        let window_focused = ctx.input(|i| i.focused);
        let startup_done = ctx.input(|i| i.time) > 3.0;
        let active_ids: Vec<u64> = self.tabs.get(self.active).map(|t| t.tree.leaves()).unwrap_or_default();
        let mut notes: Vec<(String, String, bool)> = Vec::new();
        let mut activity: Vec<(u64, Option<bool>)> = Vec::new();
        for (id, pane) in self.panes.iter_mut() {
            let visible = active_ids.contains(id);
            for f in pane.finished.drain(..) {
                let ok = f.exit == 0;
                if !visible {
                    activity.push((*id, Some(ok)));
                }
                let long = f.duration.as_secs() >= self.config.notify_threshold_secs as u64;
                let away = !window_focused || !visible;
                if self.config.notify_long_commands && long && (away || !self.config.notify_only_unfocused) {
                    let title = if ok { "✔ Commande terminée".to_string() } else { format!("✖ Échec (code {})", f.exit) };
                    let cmd: String = f.command.chars().take(80).collect();
                    notes.push((title, format!("{cmd} — {}", fmt_duration(f.duration)), true));
                }
            }
            for (title, body) in pane.notifications.drain(..) {
                notes.push((title, body, false));
            }
            if !visible && startup_done && pane.last_output.elapsed().as_millis() < 200 {
                activity.push((*id, None));
            }
        }
        if self.config.tab_activity {
            for (id, a) in activity {
                if let Some(tab) = self.tabs.iter_mut().find(|t| t.tree.contains(id)) {
                    // Un résultat (succès / échec) prime sur une simple activité.
                    if a.is_some() || tab.activity.is_none() {
                        tab.activity = Some(a);
                    }
                }
            }
        }
        for (title, body, from_cmd) in notes {
            if !window_focused || !from_cmd {
                platform::notify(&title, &body);
            }
            if from_cmd && self.config.notify_sound {
                platform::play_sound();
            }
            self.toast(format!("{title} : {body}"));
        }
    }

    /// Thème automatique clair / sombre.
    fn update_auto_theme(&mut self, time: f64) {
        let want_dark = match self.config.auto_theme {
            AutoTheme::Off => return,
            AutoTheme::System => {
                if let Some(rx) = &self.dark_rx {
                    if let Ok(v) = rx.try_recv() {
                        self.system_dark = v;
                        self.dark_rx = None;
                    }
                }
                if time - self.last_dark_poll > 5.0 && self.dark_rx.is_none() {
                    self.last_dark_poll = time;
                    let (tx, rx) = std::sync::mpsc::channel();
                    let winit_theme = self.ctx.input(|i| i.raw.system_theme);
                    std::thread::spawn(move || {
                        let v = match winit_theme {
                            Some(egui::Theme::Dark) => Some(true),
                            Some(egui::Theme::Light) => Some(false),
                            None => platform::desktop_prefers_dark(),
                        };
                        let _ = tx.send(v);
                    });
                    self.dark_rx = Some(rx);
                }
                match self.system_dark {
                    Some(d) => d,
                    None => return,
                }
            }
            AutoTheme::Schedule => {
                if time - self.last_dark_poll < 30.0 {
                    return;
                }
                self.last_dark_poll = time;
                let hour = local_hour();
                let (day, night) = (self.config.day_hour.min(23), self.config.night_hour.min(23));
                if day <= night {
                    !(hour >= day && hour < night)
                } else {
                    hour >= night && hour < day
                }
            }
        };
        let name = if want_dark { self.config.dark_theme.clone() } else { self.config.light_theme.clone() };
        if self.config.custom_theme.is_none() && self.config.theme_name != name && !name.is_empty() {
            self.config.theme_name = name;
            self.config_dirty = true;
        }
    }

    /// Terminal rapide : placement, affichage / masquage.
    fn update_quake(&mut self, ctx: &egui::Context) {
        if !self.quake {
            return;
        }
        let monitor = ctx.input(|i| i.viewport().monitor_size);
        if !self.quake_placed {
            if let Some(m) = monitor {
                let w = (m.x * self.config.quake_width.clamp(0.3, 1.0)).round();
                let h = (m.y * self.config.quake_height.clamp(0.15, 1.0)).round();
                ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(Vec2::new(w, h)));
                ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(Pos2::new(((m.x - w) / 2.0).max(0.0), 0.0)));
                ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                self.quake_placed = true;
            }
        }
        let mut toggle = false;
        if let Some(rx) = &self.quake_rx {
            while rx.try_recv().is_ok() {
                toggle = true;
            }
        }
        if toggle {
            let focused = ctx.input(|i| i.focused);
            if self.quake_visible && focused {
                self.quake_hide(ctx);
            } else {
                self.quake_visible = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
                ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                self.quake_placed = false;
            }
        }
        if self.config.quake_hide_on_unfocus && self.quake_visible && self.frames > 30 {
            let lost = ctx.input(|i| i.events.iter().any(|e| matches!(e, egui::Event::WindowFocused(false))));
            if lost && !self.settings_open && self.confirm.is_none() {
                self.quake_hide(ctx);
            }
        }
        // Réveil régulier pour recevoir « toggle » même fenêtre masquée.
        ctx.request_repaint_after(Duration::from_millis(150));
    }

    fn quake_hide(&mut self, ctx: &egui::Context) {
        self.quake_visible = false;
        ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
    }

    // ------------------------------------------------------------------
    // Onglets & panneaux
    // ------------------------------------------------------------------

    fn focused_id(&self) -> Option<u64> {
        self.tabs.get(self.active).map(|t| t.focused)
    }

    fn start_cwd(&self) -> Option<PathBuf> {
        match self.config.start_dir {
            StartDir::Inherit => self.focused_id().and_then(|id| self.panes.get(&id)).and_then(|p| p.cwd()),
            StartDir::Home => dirs::home_dir(),
            StartDir::Custom => {
                let s = self.config.custom_start_dir.trim();
                let p = if let Some(rest) = s.strip_prefix('~') {
                    dirs::home_dir().map(|h| h.join(rest.trim_start_matches('/')))
                } else if s.is_empty() {
                    None
                } else {
                    Some(PathBuf::from(s))
                };
                p.filter(|p| p.is_dir())
            }
        }
    }

    /// Taille (colonnes, lignes) estimée d'un nouveau panneau : le shell démarre
    /// directement à la bonne taille (évite le « % » de zsh et les reflows).
    fn spawn_size(&self, split: Option<Dir>) -> (usize, usize) {
        let area = self.pane_rects.iter().map(|(_, r)| *r).reduce(|a, b| a.union(b));
        let rect = match (split, self.focused_id()) {
            (Some(dir), Some(f)) => self.pane_rects.iter().find(|(id, _)| *id == f).map(|(_, r)| {
                let mut r = *r;
                match dir {
                    Dir::Horizontal => r.set_width(r.width() / 2.0 - 3.0),
                    Dir::Vertical => r.set_height(r.height() / 2.0 - 3.0),
                }
                r
            }),
            _ => area,
        };
        match rect {
            // Une colonne de marge : trop petit est sans conséquence, trop grand fait déborder.
            Some(r) => {
                let (c, l) = render::grid_size(r, &self.config, self.metrics);
                (c.saturating_sub(1).max(2), l.max(1))
            }
            None => (80, 24),
        }
    }

    fn spawn_pane(&mut self, cwd: Option<PathBuf>) -> Option<u64> {
        let size = self.spawn_size(None);
        self.spawn_pane_sized(cwd, size)
    }

    fn spawn_pane_sized(&mut self, cwd: Option<PathBuf>, (cols, rows): (usize, usize)) -> Option<u64> {
        let id = self.next_id;
        self.next_id += 1;
        match Pane::spawn(cols, rows, &self.config, cwd, self.ctx.clone()) {
            Ok(pane) => {
                self.panes.insert(id, pane);
                Some(id)
            }
            Err(e) => {
                eprintln!("NovaTerm: impossible de lancer le shell: {e}");
                self.toast(format!("Échec du lancement du shell : {e}"));
                None
            }
        }
    }

    /// Relance un shell dans un panneau dont le processus est terminé.
    fn respawn(&mut self, id: u64) {
        let cwd = self.start_cwd().or_else(dirs::home_dir);
        let (cols, rows) = self.panes.get(&id).map(|p| (p.cols, p.rows)).unwrap_or((80, 24));
        match Pane::spawn(cols, rows, &self.config, cwd, self.ctx.clone()) {
            Ok(pane) => {
                self.panes.insert(id, pane);
                self.toast("Shell relancé");
            }
            Err(e) => self.toast(format!("Échec du lancement du shell : {e}")),
        }
    }

    fn new_tab(&mut self) {
        let cwd = self.start_cwd();
        if let Some(id) = self.spawn_pane(cwd) {
            self.tabs.push(Tab::new(id));
            self.active = self.tabs.len() - 1;
            if self.tabs.len() > 1 {
                self.toast("Nouvel onglet");
            }
        }
    }

    fn split_focused(&mut self, dir: Dir) {
        let Some(focused) = self.focused_id() else { return };
        let cwd = self.start_cwd();
        let size = self.spawn_size(Some(dir));
        if let Some(new_id) = self.spawn_pane_sized(cwd, size) {
            let tab = &mut self.tabs[self.active];
            tab.tree.split(focused, dir, new_id);
            tab.focused = new_id;
            tab.zoomed = None;
            self.toast("Panneau divisé");
        }
    }

    /// Retire un panneau (le processus est tué), quel que soit l'onglet qui le contient.
    fn remove_pane(&mut self, id: u64) {
        if let Some(mut pane) = self.panes.remove(&id) {
            pane.close();
        }
        if self.search.as_ref().is_some_and(|s| s.pane == id) {
            self.search = None;
        }
        if self.mouse_down.is_some_and(|(p, _)| p == id) {
            self.mouse_down = None;
        }
        let Some(ti) = self.tabs.iter().position(|t| t.tree.contains(id)) else { return };
        let tab = &mut self.tabs[ti];
        match tab.tree.clone().removed(id) {
            Some(tree) => {
                if tab.focused == id {
                    tab.focused = tree.leaves().first().copied().unwrap_or(0);
                }
                if tab.zoomed == Some(id) {
                    tab.zoomed = None;
                }
                tab.tree = tree;
            }
            None => {
                self.tabs.remove(ti);
                if ti < self.active || self.active >= self.tabs.len() {
                    self.active = self.active.saturating_sub(1);
                }
            }
        }
        if self.tabs.is_empty() {
            self.allow_close = true;
            self.ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }

    /// Programmes en cours (autres que le shell) dans ces panneaux.
    fn busy(&self, ids: &[u64]) -> Vec<String> {
        ids.iter().filter_map(|id| self.panes.get(id)).filter_map(|p| p.foreground_process()).collect()
    }

    fn ask_confirm(&mut self, kind: ConfirmKind, busy: Vec<String>) {
        let list = busy.join(", ");
        let (title, message, button) = match &kind {
            ConfirmKind::CloseWindow => (
                "Quitter NovaTerm ?",
                format!("Des programmes sont encore en cours : {list}.\nIls seront arrêtés."),
                "Quitter",
            ),
            ConfirmKind::CloseTab(_) => (
                "Fermer l'onglet ?",
                format!("Des programmes sont encore en cours : {list}.\nIls seront arrêtés."),
                "Fermer l'onglet",
            ),
            ConfirmKind::ClosePane(_) => (
                "Fermer le panneau ?",
                format!("« {list} » est encore en cours d'exécution.\nIl sera arrêté."),
                "Fermer",
            ),
            ConfirmKind::Paste(text) => (
                "Coller plusieurs lignes ?",
                format!(
                    "Le texte contient {} lignes : chaque ligne peut être exécutée comme une commande.",
                    text.lines().count()
                ),
                "Coller",
            ),
        };
        self.confirm = Some(Confirm { kind, title: title.into(), message, button: button.into() });
    }

    fn request_close_pane(&mut self, id: u64) {
        let busy = if self.config.confirm_close { self.busy(&[id]) } else { Vec::new() };
        if busy.is_empty() {
            self.remove_pane(id);
        } else {
            self.ask_confirm(ConfirmKind::ClosePane(id), busy);
        }
    }

    fn close_tab_now(&mut self, index: usize) {
        if let Some(tab) = self.tabs.get(index) {
            for id in tab.tree.leaves() {
                self.remove_pane(id);
            }
        }
    }

    fn request_close_tab(&mut self, index: usize) {
        let Some(tab) = self.tabs.get(index) else { return };
        let ids = tab.tree.leaves();
        let busy = if self.config.confirm_close { self.busy(&ids) } else { Vec::new() };
        if busy.is_empty() {
            self.close_tab_now(index);
        } else {
            self.ask_confirm(ConfirmKind::CloseTab(index), busy);
        }
    }

    fn request_quit(&mut self) {
        let ids: Vec<u64> = self.panes.keys().copied().collect();
        let busy = if self.config.confirm_close { self.busy(&ids) } else { Vec::new() };
        if busy.is_empty() {
            self.allow_close = true;
            self.ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        } else {
            self.ask_confirm(ConfirmKind::CloseWindow, busy);
        }
    }

    fn confirm_yes(&mut self) {
        let Some(c) = self.confirm.take() else { return };
        match c.kind {
            ConfirmKind::CloseWindow => {
                self.allow_close = true;
                self.ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            ConfirmKind::CloseTab(i) => self.close_tab_now(i),
            ConfirmKind::ClosePane(id) => self.remove_pane(id),
            ConfirmKind::Paste(text) => self.paste_text(&text),
        }
    }

    fn set_active(&mut self, index: usize) {
        if index < self.tabs.len() && index != self.active {
            self.active = index;
            self.search = None;
        }
    }

    fn cycle_tab(&mut self, delta: isize) {
        let n = self.tabs.len() as isize;
        if n > 0 {
            self.set_active(((self.active as isize + delta).rem_euclid(n)) as usize);
        }
    }

    fn move_tab(&mut self, delta: isize) {
        let n = self.tabs.len() as isize;
        let to = self.active as isize + delta;
        if n < 2 || to < 0 || to >= n {
            return;
        }
        self.tabs.swap(self.active, to as usize);
        self.active = to as usize;
    }

    fn set_focus(&mut self, id: u64) {
        if let Some(tab) = self.tabs.get_mut(self.active) {
            if tab.focused != id {
                tab.focused = id;
                if tab.zoomed.is_some() {
                    tab.zoomed = Some(id);
                }
            }
        }
    }

    fn focus_dir(&mut self, dir: Key) {
        let Some(focused) = self.focused_id() else { return };
        let Some((_, cur)) = self.pane_rects.iter().find(|(id, _)| *id == focused).copied() else { return };
        let mut best: Option<(f32, u64)> = None;
        for (id, r) in &self.pane_rects {
            if *id == focused {
                continue;
            }
            // Distance le long de l'axe + pénalité si pas de recouvrement perpendiculaire.
            let (gap, overlap, perp) = match dir {
                Key::ArrowRight => (r.left() - cur.right(), r.y_range().intersection(cur.y_range()).span(), (r.center().y - cur.center().y).abs()),
                Key::ArrowLeft => (cur.left() - r.right(), r.y_range().intersection(cur.y_range()).span(), (r.center().y - cur.center().y).abs()),
                Key::ArrowDown => (r.top() - cur.bottom(), r.x_range().intersection(cur.x_range()).span(), (r.center().x - cur.center().x).abs()),
                Key::ArrowUp => (cur.top() - r.bottom(), r.x_range().intersection(cur.x_range()).span(), (r.center().x - cur.center().x).abs()),
                _ => continue,
            };
            if gap < -2.0 {
                continue;
            }
            let score = gap.max(0.0) + if overlap > 0.0 { perp * 0.1 } else { 10_000.0 + perp };
            if best.is_none_or(|(s, _)| score < s) {
                best = Some((score, *id));
            }
        }
        if let Some((_, id)) = best {
            self.set_focus(id);
        }
    }

    fn focus_cycle(&mut self, delta: isize) {
        let Some(tab) = self.tabs.get(self.active) else { return };
        let leaves = tab.tree.leaves();
        if leaves.len() < 2 {
            return;
        }
        let pos = leaves.iter().position(|&l| l == tab.focused).unwrap_or(0) as isize;
        let next = leaves[((pos + delta).rem_euclid(leaves.len() as isize)) as usize];
        self.set_focus(next);
    }

    fn resize_pane(&mut self, dir: Dir, delta: f32) {
        let Some(id) = self.focused_id() else { return };
        let tab = &mut self.tabs[self.active];
        tab.zoomed = None;
        tab.tree.nudge(id, dir, delta);
    }

    fn toggle_zoom(&mut self) {
        let Some(focused) = self.focused_id() else { return };
        let tab = &mut self.tabs[self.active];
        if tab.tree.leaves().len() < 2 {
            return;
        }
        if tab.zoomed == Some(focused) {
            tab.zoomed = None;
            self.toast("Zoom désactivé");
        } else {
            tab.zoomed = Some(focused);
            self.toast("Panneau zoomé");
        }
    }

    fn pane_to_tab(&mut self) {
        let Some(id) = self.focused_id() else { return };
        let tab = &mut self.tabs[self.active];
        if tab.tree.leaves().len() < 2 {
            return;
        }
        if let Some(tree) = tab.tree.clone().removed(id) {
            tab.focused = tree.leaves()[0];
            tab.zoomed = None;
            tab.tree = tree;
        }
        self.tabs.insert(self.active + 1, Tab::new(id));
        self.active += 1;
        self.toast("Panneau déplacé dans un nouvel onglet");
    }

    // ------------------------------------------------------------------
    // Presse-papiers, recherche, défilement
    // ------------------------------------------------------------------

    fn selection_text(&self) -> Option<String> {
        let id = self.focused_id()?;
        self.panes.get(&id)?.term.selection_to_string().map(|t| blocks::strip_tags(&t)).filter(|t| !t.is_empty())
    }

    fn copy(&mut self) -> bool {
        if let Some(text) = self.selection_text() {
            set_primary_selection(&text);
            self.ctx.copy_text(text);
            self.toast("Copié dans le presse-papiers");
            true
        } else {
            false
        }
    }

    fn paste_clipboard(&mut self) {
        if let Some(text) = get_clipboard() {
            self.paste_checked(text);
        }
    }

    fn paste_primary(&mut self) {
        if let Some(text) = get_primary_selection().filter(|t| !t.is_empty()).or_else(get_clipboard) {
            self.paste_checked(text);
        }
    }

    /// Colle en demandant confirmation pour du multi-lignes (si activé).
    fn paste_checked(&mut self, text: String) {
        if text.is_empty() {
            return;
        }
        let multiline = text.trim_end_matches(['\n', '\r']).contains('\n');
        let bracketed = self
            .focused_id()
            .and_then(|id| self.panes.get(&id))
            .is_some_and(|p| p.term.mode().contains(TermMode::BRACKETED_PASTE));
        if self.config.warn_multiline_paste && multiline && !bracketed {
            self.ask_confirm(ConfirmKind::Paste(text), Vec::new());
        } else {
            self.paste_text(&text);
        }
    }

    fn paste_text(&mut self, text: &str) {
        let text = text.replace("\r\n", "\r").replace('\n', "\r");
        for id in self.input_targets() {
            let Some(pane) = self.panes.get_mut(&id) else { continue };
            let payload = if pane.term.mode().contains(TermMode::BRACKETED_PASTE) {
                // Empêche un texte collé de terminer lui-même le mode « bracketed paste ».
                format!("\x1b[200~{}\x1b[201~", text.replace('\x1b', ""))
            } else {
                text.clone()
            };
            pane.write_input(payload.as_bytes());
        }
    }

    fn select_all(&mut self) {
        let Some(id) = self.focused_id() else { return };
        let Some(pane) = self.panes.get_mut(&id) else { return };
        let top = pane.term.topmost_line();
        let bottom = pane.term.bottommost_line();
        let last_col = Column(pane.term.columns().saturating_sub(1));
        let mut sel = Selection::new(SelectionType::Simple, Point::new(top, Column(0)), Side::Left);
        sel.update(Point::new(bottom, last_col), Side::Right);
        pane.term.selection = Some(sel);
        pane.dirty = true;
        if let Some(text) = pane.term.selection_to_string().map(|t| blocks::strip_tags(&t)) {
            set_primary_selection(&text);
        }
    }

    fn scroll_focused(&mut self, scroll: Scroll) {
        if let Some(pane) = self.focused_id().and_then(|id| self.panes.get_mut(&id)) {
            pane.term.scroll_display(scroll);
            pane.dirty = true;
        }
    }

    fn open_search(&mut self) {
        if let Some(s) = &mut self.search {
            s.focus = true;
        } else if let Some(pane) = self.focused_id() {
            // Pré-remplir avec la sélection courante (une ligne).
            let query = self.selection_text().filter(|t| !t.contains('\n')).unwrap_or_default();
            self.search = Some(SearchState { pane, query, regex: None, last: None, focus: true, not_found: false });
        }
    }

    fn close_search(&mut self) {
        self.search = None;
        self.clear_egui_focus();
    }

    fn search_next(&mut self, back: bool) {
        let Some(s) = self.search.as_mut() else { return };
        if s.query.is_empty() {
            return;
        }
        if s.regex.is_none() {
            // Regex invalide → recherche littérale.
            s.regex = RegexSearch::new(&s.query).ok().or_else(|| RegexSearch::new(&regex_escape(&s.query)).ok());
        }
        let Some(regex) = s.regex.as_mut() else { return };
        let Some(pane) = self.panes.get_mut(&s.pane) else { return };
        let origin = s.last.unwrap_or_else(|| pane.term.renderable_content().cursor.point);
        let (direction, side) = if back { (Direction::Left, Side::Left) } else { (Direction::Right, Side::Right) };
        if let Some(range) = pane.term.search_next(regex, origin, direction, side, None) {
            let start = *range.start();
            let end = *range.end();
            pane.term.scroll_to_point(start);
            let mut sel = Selection::new(SelectionType::Simple, start, Side::Left);
            sel.update(end, Side::Right);
            pane.term.selection = Some(sel);
            s.last = Some(if back { start } else { end });
            s.not_found = false;
            pane.dirty = true;
        } else {
            s.not_found = true;
        }
    }

    // ------------------------------------------------------------------
    // Divers
    // ------------------------------------------------------------------

    fn toast(&mut self, message: impl Into<String>) {
        if !self.config.toasts {
            return;
        }
        let time = self.ctx.input(|i| i.time);
        self.toasts.push(Toast { message: message.into(), born: time });
        if self.toasts.len() > 5 {
            self.toasts.remove(0);
        }
    }

    fn zoom_font(&mut self, delta: f32) {
        self.config.font_size = (self.config.font_size + delta).clamp(6.0, 48.0);
        self.config_dirty = true;
        self.toast(format!("Police : {:.0} px", self.config.font_size));
    }

    fn clear_egui_focus(&self) {
        if let Some(id) = self.ctx.memory(|m| m.focused()) {
            self.ctx.memory_mut(|m| m.surrender_focus(id));
        }
    }

    fn toggle_fullscreen(&mut self) {
        self.fullscreen = !self.fullscreen;
        self.ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(self.fullscreen));
    }

    fn is_maximized(&self) -> bool {
        self.ctx.input(|i| i.viewport().maximized.unwrap_or(false)) || self.fullscreen
    }

    fn window_radius(&self) -> u8 {
        if !self.config.custom_decorations || self.is_maximized() || self.quake {
            0
        } else {
            self.config.corner_radius.clamp(0.0, 24.0) as u8
        }
    }

    fn pane_title(&self, id: u64) -> String {
        self.panes.get(&id).map(|p| p.title.clone()).unwrap_or_else(|| "shell".into())
    }

    fn active_pane_title(&self) -> String {
        let title = self
            .tabs
            .get(self.active)
            .and_then(|t| t.title.clone())
            .or_else(|| self.focused_id().map(|id| self.pane_title(id)))
            .unwrap_or_else(|| "shell".into());
        truncate(&title, 48)
    }

    fn next_theme(&mut self) {
        let presets = theme::presets();
        let cur = presets.iter().position(|p| p.name == self.config.theme_name).unwrap_or(0);
        let next = if self.config.custom_theme.is_some() { cur } else { (cur + 1) % presets.len() };
        self.config.theme_name = presets[next].name.clone();
        self.config.custom_theme = None;
        self.config_dirty = true;
    }

    fn new_window(&self) {
        let Ok(exe) = std::env::current_exe() else { return };
        let mut cmd = std::process::Command::new(exe);
        if let Some(cwd) = self.start_cwd() {
            cmd.current_dir(cwd);
        }
        if let Err(e) = cmd.spawn() {
            eprintln!("NovaTerm: nouvelle fenêtre impossible: {e}");
        }
    }

    fn run_action(&mut self, action: Action) {
        use Action::*;
        match action {
            NewTab => self.new_tab(),
            CloseTab => self.request_close_tab(self.active),
            NextTab => self.cycle_tab(1),
            PrevTab => self.cycle_tab(-1),
            MoveTabLeft => self.move_tab(-1),
            MoveTabRight => self.move_tab(1),
            RenameTab => {
                let text = self.tabs.get(self.active).and_then(|t| t.title.clone()).unwrap_or_default();
                self.rename = Some(Rename { tab: self.active, text, focus: true });
            }
            LastTab => self.set_active(self.tabs.len().saturating_sub(1)),
            GotoTab1 | GotoTab2 | GotoTab3 | GotoTab4 | GotoTab5 | GotoTab6 | GotoTab7 | GotoTab8 | GotoTab9 => {
                let n = action as usize - GotoTab1 as usize;
                self.set_active(n);
            }
            SplitRight => self.split_focused(Dir::Horizontal),
            SplitDown => self.split_focused(Dir::Vertical),
            ClosePane => {
                if let Some(id) = self.focused_id() {
                    self.request_close_pane(id);
                }
            }
            FocusLeft => self.focus_dir(Key::ArrowLeft),
            FocusRight => self.focus_dir(Key::ArrowRight),
            FocusUp => self.focus_dir(Key::ArrowUp),
            FocusDown => self.focus_dir(Key::ArrowDown),
            FocusNext => self.focus_cycle(1),
            FocusPrev => self.focus_cycle(-1),
            ResizeLeft => self.resize_pane(Dir::Horizontal, -0.04),
            ResizeRight => self.resize_pane(Dir::Horizontal, 0.04),
            ResizeUp => self.resize_pane(Dir::Vertical, -0.04),
            ResizeDown => self.resize_pane(Dir::Vertical, 0.04),
            EqualizeSplits => {
                if let Some(tab) = self.tabs.get_mut(self.active) {
                    tab.tree.equalize();
                    self.toast("Panneaux égalisés");
                }
            }
            ToggleZoom => self.toggle_zoom(),
            PaneToTab => self.pane_to_tab(),
            Copy => {
                self.copy();
            }
            Paste => self.paste_clipboard(),
            PasteSelection => self.paste_primary(),
            SelectAll => self.select_all(),
            Find => self.open_search(),
            FindNext => self.search_next(false),
            FindPrev => self.search_next(true),
            Clear => {
                if let Some(pane) = self.focused_id().and_then(|id| self.panes.get_mut(&id)) {
                    pane.clear_all();
                    self.toast("Écran effacé");
                }
            }
            ResetTerminal => {
                if let Some(pane) = self.focused_id().and_then(|id| self.panes.get_mut(&id)) {
                    pane.reset();
                    self.toast("Terminal réinitialisé");
                }
            }
            ScrollLineUp => self.scroll_focused(Scroll::Delta(1)),
            ScrollLineDown => self.scroll_focused(Scroll::Delta(-1)),
            ScrollPageUp => self.scroll_focused(Scroll::PageUp),
            ScrollPageDown => self.scroll_focused(Scroll::PageDown),
            ScrollTop => self.scroll_focused(Scroll::Top),
            ScrollBottom => self.scroll_focused(Scroll::Bottom),
            FontInc => self.zoom_font(1.0),
            FontDec => self.zoom_font(-1.0),
            FontReset => {
                self.config.font_size = Config::default().font_size;
                self.config_dirty = true;
                self.toast("Police réinitialisée");
            }
            NewWindow => self.new_window(),
            Fullscreen => self.toggle_fullscreen(),
            ToggleMaximize => {
                let max = self.ctx.input(|i| i.viewport().maximized.unwrap_or(false));
                self.ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!max));
            }
            ToggleAlwaysOnTop => {
                self.config.always_on_top = !self.config.always_on_top;
                self.config_dirty = true;
                self.toast(if self.config.always_on_top { "Toujours au premier plan" } else { "Premier plan désactivé" });
            }
            ToggleTabBar => {
                self.config.show_tab_bar = !self.config.show_tab_bar;
                self.config_dirty = true;
            }
            ToggleStatusBar => {
                self.config.show_status_bar = !self.config.show_status_bar;
                self.config_dirty = true;
            }
            NextTheme => self.next_theme(),
            OpacityUp | OpacityDown => {
                let d = if action == OpacityUp { 0.05 } else { -0.05 };
                self.config.opacity = (self.config.opacity + d).clamp(0.15, 1.0);
                self.config_dirty = true;
                self.toast(format!("Opacité : {:.0} %", self.config.opacity * 100.0));
            }
            CommandPalette => {
                if self.palette.take().is_none() {
                    self.open_palette(PaletteMode::All);
                } else {
                    self.clear_egui_focus();
                }
            }
            Settings => self.settings_open = !self.settings_open,
            Help => self.help_open = !self.help_open,
            OpenConfig => {
                self.config.save();
                if let Some(p) = Config::path() {
                    open_external(&p.display().to_string());
                }
            }
            ReloadConfig => {
                self.config = Config::load();
                self.toast("Configuration rechargée");
            }
            Quit => self.request_quit(),
            HistorySearch => {
                if self.history_popup.take().is_some() {
                    self.clear_egui_focus();
                } else if let Some(pane) = self.focused_id() {
                    let query = self.panes.get(&pane).and_then(|p| p.cmdline.as_ref()).map(|(_, b)| b.clone()).unwrap_or_default();
                    self.palette = None;
                    self.history_popup = Some(HistoryPopup { pane, query, selected: 0, focus: true });
                }
            }
            PrevCommand => self.jump_command(true),
            NextCommand => self.jump_command(false),
            CopyLastOutput => match self.last_block() {
                Some((p, b)) => self.copy_block_output(p, b),
                None => self.toast("Aucune commande (intégration shell inactive ?)"),
            },
            CopyLastCommand => {
                if let Some(cmd) = self.last_block().and_then(|(p, b)| self.block_command(p, b)) {
                    self.ctx.copy_text(cmd);
                    self.toast("Commande copiée");
                }
            }
            RerunLast => {
                if let Some((p, b)) = self.last_block() {
                    if let Some(cmd) = self.block_command(p, b) {
                        self.insert_command(p, &cmd, true);
                    }
                }
            }
            ToggleBroadcast => {
                if let Some(tab) = self.tabs.get_mut(self.active) {
                    tab.broadcast = !tab.broadcast;
                    let on = tab.broadcast;
                    self.toast(if on { "📡 Diffusion activée : le clavier va à tous les panneaux" } else { "Diffusion désactivée" });
                }
            }
            Snippets => self.open_palette(PaletteMode::Snippets),
            Profiles => self.open_palette(PaletteMode::Profiles),
            SaveProfile => self.save_profile = Some((String::new(), true)),
            ImportTheme => self.theme_import = Some(settings::FileBrowser::new_any()),
            QuickTerminal => {
                if self.quake {
                    let ctx = self.ctx.clone();
                    self.quake_hide(&ctx);
                } else {
                    quake::toggle_or_launch();
                }
            }
            ToggleSuggestions => {
                self.config.suggestions = !self.config.suggestions;
                self.config_dirty = true;
                self.toast(if self.config.suggestions { "Suggestions activées" } else { "Suggestions désactivées" });
            }
        }
    }

    // ------------------------------------------------------------------
    // Clavier
    // ------------------------------------------------------------------

    /// Une action peut-elle s'exécuter alors qu'un champ texte a le focus ?
    fn allowed_in_text_field(action: Action, combo_mods: &Modifiers, key: Key) -> bool {
        if matches!(action, Action::FindNext | Action::FindPrev) {
            return true;
        }
        let f_key = matches!(
            key,
            Key::F1 | Key::F2 | Key::F3 | Key::F4 | Key::F5 | Key::F6 | Key::F7 | Key::F8 | Key::F9 | Key::F10 | Key::F11 | Key::F12
        );
        let editing = matches!(
            key,
            Key::C | Key::V | Key::X | Key::A | Key::Z | Key::Y | Key::ArrowLeft | Key::ArrowRight | Key::ArrowUp
                | Key::ArrowDown | Key::Home | Key::End | Key::Insert | Key::Backspace | Key::Delete
        );
        f_key || (combo_mods.ctrl && !editing) || (combo_mods.alt && !editing)
    }

    /// Suggestion affichée (texte restant à insérer), si elle concerne ce panneau.
    fn suggestion_remainder(&self, id: u64) -> Option<String> {
        if self.suggest.pane != id {
            return None;
        }
        let item = self.suggest.items.get(self.suggest.selected)?;
        item.strip_prefix(self.suggest.buffer.as_str()).map(String::from).filter(|r| !r.is_empty())
    }

    /// Touches d'acceptation / de navigation des suggestions. Retourne true si consommée.
    fn handle_suggestion_key(&mut self, id: u64, key: Key, mods: Modifiers) -> bool {
        let Some(rest) = self.suggestion_remainder(id) else { return false };
        let plain = !mods.ctrl && !mods.alt && !mods.shift;
        let accept = plain
            && match self.config.suggest_accept {
                AcceptKey::RightOrEnd => matches!(key, Key::ArrowRight | Key::End),
                AcceptKey::Right => key == Key::ArrowRight,
                AcceptKey::Tab => key == Key::Tab,
            };
        let text = if accept {
            rest
        } else if self.config.suggest_word_accept && mods.ctrl && !mods.alt && !mods.shift && key == Key::ArrowRight {
            // Mot suivant : espaces de tête + caractères jusqu'au prochain séparateur.
            let trimmed = rest.trim_start();
            let lead = rest.len() - trimmed.len();
            let word_len = trimmed.find([' ', '/']).map(|i| i + 1).unwrap_or(trimmed.len());
            rest[..lead + word_len].to_string()
        } else if self.config.suggest_list && mods.ctrl && !mods.alt && !mods.shift && matches!(key, Key::ArrowDown | Key::ArrowUp) {
            let n = self.suggest.items.len();
            if n > 1 {
                let d = if key == Key::ArrowDown { 1 } else { n - 1 };
                self.suggest.selected = (self.suggest.selected + d) % n;
                return true;
            }
            return false;
        } else {
            return false;
        };
        if let Some(pane) = self.panes.get_mut(&id) {
            pane.write_input(text.as_bytes());
        }
        true
    }

    fn handle_terminal_key(&mut self, key: Key, mods: Modifiers) {
        let Some(id) = self.focused_id() else { return };
        if self.handle_suggestion_key(id, key, mods) {
            return;
        }
        // Ctrl+C intelligent : copie s'il y a une sélection.
        if self.config.smart_copy && mods.ctrl && !mods.shift && !mods.alt && key == Key::C && self.copy() {
            if let Some(p) = self.panes.get_mut(&id) {
                p.term.selection = None;
                p.dirty = true;
            }
            return;
        }
        if self.config.ctrl_v_paste && mods.ctrl && !mods.shift && !mods.alt && key == Key::V {
            self.paste_clipboard();
            return;
        }
        let Some(pane) = self.panes.get_mut(&id) else { return };
        if pane.exited {
            if key == Key::Enter {
                self.respawn(id);
            }
            return;
        }
        let app_cursor = pane.term.mode().contains(TermMode::APP_CURSOR);
        if let Some(bytes) = input::encode_key(key, &mods, app_cursor) {
            // Taper efface la sélection (comme la plupart des terminaux).
            if pane.term.selection.is_some() && !matches!(key, Key::Escape) {
                pane.term.selection = None;
            }
            pane.write_input(&bytes);
        }
        // Diffusion : même touche vers les autres panneaux (encodée selon leur mode).
        for other in self.input_targets().into_iter().filter(|o| *o != id) {
            if let Some(p) = self.panes.get_mut(&other) {
                let app_cursor = p.term.mode().contains(TermMode::APP_CURSOR);
                if let Some(bytes) = input::encode_key(key, &mods, app_cursor) {
                    p.write_input(&bytes);
                }
            }
        }
    }

    /// Panneaux qui reçoivent le clavier (le panneau actif, ou tous si diffusion).
    fn input_targets(&self) -> Vec<u64> {
        let Some(tab) = self.tabs.get(self.active) else { return Vec::new() };
        if !tab.broadcast {
            return vec![tab.focused];
        }
        match self.config.broadcast_scope {
            BroadcastScope::Tab => tab.tree.leaves(),
            BroadcastScope::All => self.tabs.iter().flat_map(|t| t.tree.leaves()).collect(),
        }
    }

    fn process_pending(&mut self) {
        let pending = std::mem::take(&mut self.pending);
        for p in pending {
            match p {
                Pending::Action(a) => self.run_action(a),
                Pending::Key { key, mods } => self.handle_terminal_key(key, mods),
                Pending::Text(text) => {
                    for id in self.input_targets() {
                        if let Some(pane) = self.panes.get_mut(&id) {
                            if !pane.exited {
                                pane.term.selection = None;
                                pane.write_input(text.as_bytes());
                            }
                        }
                    }
                }
                Pending::Paste(text) => self.paste_checked(text),
                Pending::Ui(cmd) => match cmd {
                    UiCmd::ConfirmYes => self.confirm_yes(),
                    UiCmd::ConfirmNo => self.confirm = None,
                    UiCmd::PaletteMove(d) => {
                        if let Some(pal) = &mut self.palette {
                            pal.selected = (pal.selected as i32 + d).max(0) as usize;
                        }
                    }
                    UiCmd::PaletteRun => {
                        if let Some(pal) = self.palette.take() {
                            let items = self.palette_items(&pal.query, pal.mode);
                            self.clear_egui_focus();
                            let n = items.len();
                            if let Some((item, ..)) = items.into_iter().nth(pal.selected.min(n.saturating_sub(1))) {
                                self.run_palette_item(item);
                            }
                        }
                    }
                    UiCmd::PaletteClose => {
                        self.palette = None;
                        self.clear_egui_focus();
                    }
                    UiCmd::RenameApply => {
                        if let Some(r) = self.rename.take() {
                            let t = r.text.trim().to_string();
                            if let Some(tab) = self.tabs.get_mut(r.tab) {
                                tab.title = (!t.is_empty()).then_some(t);
                            }
                            self.clear_egui_focus();
                        }
                    }
                    UiCmd::RenameCancel => {
                        self.rename = None;
                        self.clear_egui_focus();
                    }
                    UiCmd::SearchNext(back) => self.search_next(back),
                    UiCmd::SearchClose => self.close_search(),
                    UiCmd::HistoryMove(d) => {
                        if let Some(h) = &mut self.history_popup {
                            h.selected = (h.selected as i32 + d).max(0) as usize;
                        }
                    }
                    UiCmd::HistoryRun(exec) => {
                        if let Some(h) = self.history_popup.take() {
                            let items = self.history.search(&h.query, 300);
                            self.clear_egui_focus();
                            if let Some(cmd) = items.get(h.selected.min(items.len().saturating_sub(1))) {
                                self.insert_command(h.pane, cmd, exec);
                            }
                        }
                    }
                    UiCmd::HistoryClose => {
                        self.history_popup = None;
                        self.clear_egui_focus();
                    }
                },
            }
        }
    }

    /// Remplace la ligne de commande du panneau par `cmd` (et l'exécute si demandé).
    fn insert_command(&mut self, id: u64, cmd: &str, exec: bool) {
        self.set_focus(id);
        if let Some(pane) = self.panes.get_mut(&id) {
            // Ctrl+E (fin de ligne) puis Ctrl+U (effacer la ligne) : zsh, bash, fish.
            let mut bytes = b"\x05\x15".to_vec();
            bytes.extend_from_slice(cmd.as_bytes());
            if exec {
                bytes.push(b'\r');
            }
            pane.write_input(&bytes);
        }
    }

    /// Recalcule les suggestions pour la ligne en cours du panneau actif.
    fn update_suggestions(&mut self) {
        if let Some(rx) = &self.history_rx {
            if let Ok(mut h) = rx.try_recv() {
                for c in &self.session_cmds {
                    h.push(c);
                }
                self.history = h;
                self.history_rx = None;
                self.suggest.buffer.clear();
            }
        }
        let sig = (self.config.history_zsh, self.config.history_bash);
        if sig != self.history_sig {
            self.history_sig = sig;
            self.history_rx = Some(history::load_async(sig.0, sig.1));
        }
        for pane in self.panes.values_mut() {
            for cmd in pane.executed.drain(..) {
                self.history.push(&cmd);
                self.session_cmds.push(cmd);
            }
        }

        let line = self.focused_id().and_then(|id| {
            let p = self.panes.get(&id)?;
            let (cursor, buf) = p.cmdline.as_ref()?;
            let mode = p.term.mode();
            let ok = self.config.suggestions
                && !p.suggest_conflict
                && *cursor == buf.chars().count()
                && buf.trim().chars().count() >= self.config.suggest_min_chars.max(1)
                && !mode.contains(TermMode::ALT_SCREEN)
                && p.term.grid().display_offset() == 0;
            ok.then(|| (id, buf.clone()))
        });
        match line {
            Some((id, buf)) => {
                if self.suggest.pane != id || self.suggest.buffer != buf || self.suggest.items.is_empty() && !buf.is_empty() {
                    let max = if self.config.suggest_list { self.config.suggest_max.clamp(1, 15) } else { 1 };
                    let ignore: Vec<String> =
                        self.config.suggest_ignore.lines().map(|l| l.trim().to_string()).filter(|l| !l.is_empty()).collect();
                    let keep = self.suggest.items.get(self.suggest.selected).cloned();
                    // Complétions d'abord calculées : elles gardent des places dans la liste
                    // même quand l'historique est très fourni.
                    let completions = if self.config.completions {
                        let cwd = self.panes.get(&id).and_then(|p| p.cwd());
                        self.completer.complete(&buf, cwd.as_deref(), max)
                    } else {
                        Vec::new()
                    };
                    let reserved = if max <= 1 { 0 } else { completions.len().min(max / 2) };
                    let mut items = self.history.suggest(&buf, max - reserved, self.config.suggest_ranking, &ignore);
                    let mut descs: Vec<Option<String>> = vec![None; items.len()];
                    for c in completions {
                        if items.len() >= max {
                            break;
                        }
                        if !items.iter().any(|i| i.trim_end() == c.full.trim_end()) {
                            items.push(c.full);
                            descs.push(Some(c.desc));
                        }
                    }
                    self.suggest.items = items;
                    self.suggest.descs = descs;
                    // Garder la sélection si elle prolonge encore ce qui est tapé.
                    self.suggest.selected = keep
                        .and_then(|k| self.suggest.items.iter().position(|i| *i == k))
                        .unwrap_or(0);
                    self.suggest.pane = id;
                    self.suggest.buffer = buf;
                }
            }
            None => {
                self.suggest.items.clear();
                self.suggest.descs.clear();
                self.suggest.buffer.clear();
                self.suggest.selected = 0;
            }
        }
    }

    /// Texte fantôme après le curseur + liste déroulante (si activée).
    fn render_suggestions(&self, ctx: &egui::Context, painter: &egui::Painter, id: u64, pr: Rect) {
        let Some(rest) = self.suggestion_remainder(id) else { return };
        let Some(pane) = self.panes.get(&id) else { return };
        let cursor = pane.term.grid().cursor.point;
        let row = cursor.line.0.max(0) as usize;
        let cols = pane.cols;
        let origin = pr.min + egui::vec2(self.config.padding, self.config.padding);
        let (cw, ch) = (self.metrics.cell_w, self.metrics.cell_h);
        let fg = self.theme.foreground;
        let ghost = Color32::from_rgba_unmultiplied(fg[0], fg[1], fg[2], (self.config.suggest_opacity.clamp(0.1, 1.0) * 255.0) as u8);
        let font = egui::FontId::new(self.config.font_size, egui::FontFamily::Name("MonoItalic".into()));
        let grid = pane.term.grid();
        let mut col = cursor.column.0;
        for c in rest.chars() {
            if col >= cols {
                break;
            }
            // Ne pas recouvrir ce qui est déjà affiché (prompt de droite, autre plugin…).
            let cell = &grid[cursor.line][Column(col)];
            if cell.c != ' ' && cell.c != '\0' {
                break;
            }
            let pos = Pos2::new(origin.x + col as f32 * cw, origin.y + row as f32 * ch + ch * 0.5);
            painter.text(pos, egui::Align2::LEFT_CENTER, c, font.clone(), ghost);
            col += 1;
        }

        if !self.config.suggest_list || self.suggest.items.len() < 2 {
            return;
        }
        let items = &self.suggest.items;
        let longest = items.iter().map(|i| i.chars().count()).max().unwrap_or(10).clamp(16, 90) as f32;
        let row_h = (ch * 1.15).max(18.0);
        let w = (longest + 4.0) * cw;
        let h = row_h * (items.len() as f32 + 1.0) + 8.0;
        let x = (origin.x + (cursor.column.0.saturating_sub(self.suggest.buffer.chars().count())) as f32 * cw - 6.0)
            .min(pr.right() - w - 4.0)
            .max(pr.left() + 2.0);
        let below = origin.y + (row as f32 + 1.0) * ch + 4.0;
        let y = if below + h <= pr.bottom() { below } else { (origin.y + row as f32 * ch - h - 4.0).max(pr.top()) };
        let rect = Rect::from_min_size(Pos2::new(x, y), Vec2::new(w, h));
        let lp = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, Id::new("suggest_list")));
        let accent = rgb(self.theme.cursor);
        lp.rect_filled(rect, CornerRadius::same(8), surface_color(&self.theme));
        lp.rect_stroke(rect, CornerRadius::same(8), Stroke::new(1.0, accent.gamma_multiply(0.5)), egui::epaint::StrokeKind::Inside);
        let mono = egui::FontId::monospace((self.config.font_size * 0.92).max(10.0));
        let fgc = rgb(fg);
        for (i, item) in items.iter().enumerate() {
            let r = Rect::from_min_size(Pos2::new(x + 4.0, y + 4.0 + i as f32 * row_h), Vec2::new(w - 8.0, row_h));
            if i == self.suggest.selected {
                lp.rect_filled(r, CornerRadius::same(5), accent.gamma_multiply(0.25));
            }
            let typed = self.suggest.buffer.as_str();
            let shown = truncate(item, longest as usize + 2);
            let galley = {
                let mut job = egui::text::LayoutJob::default();
                let split = typed.len().min(shown.len());
                let (a, b) = if shown.is_char_boundary(split) { shown.split_at(split) } else { (shown.as_str(), "") };
                job.append(a, 0.0, egui::TextFormat::simple(mono.clone(), fgc));
                job.append(b, 0.0, egui::TextFormat::simple(mono.clone(), fgc.gamma_multiply(0.6)));
                ctx.fonts(|f| f.layout_job(job))
            };
            lp.galley(Pos2::new(r.left() + 6.0, r.center().y - galley.size().y * 0.5), galley, fgc);
            match self.suggest.descs.get(i).cloned().flatten() {
                Some(desc) => {
                    lp.text(r.right_center() - Vec2::new(6.0, 0.0), egui::Align2::RIGHT_CENTER, desc, egui::FontId::proportional(11.0), accent.gamma_multiply(0.8));
                }
                None => {
                    let n = self.history.count(item);
                    let label = if n > 1 { format!("↺ ×{n}") } else { "↺".to_string() };
                    lp.text(r.right_center() - Vec2::new(6.0, 0.0), egui::Align2::RIGHT_CENTER, label, egui::FontId::proportional(11.0), fgc.gamma_multiply(0.45));
                }
            }
        }
        let hint = Rect::from_min_size(Pos2::new(x + 10.0, y + 4.0 + items.len() as f32 * row_h), Vec2::new(w - 20.0, row_h));
        lp.text(hint.left_center(), egui::Align2::LEFT_CENTER, "→ accepter   Ctrl+→ un mot   Ctrl+↑↓ choisir", egui::FontId::proportional(11.0), fgc.gamma_multiply(0.5));
    }

    /// Décorations des blocs d'un panneau ; retourne l'action cliquée.
    fn render_blocks(&self, ctx: &egui::Context, painter: &egui::Painter, id: u64, pr: Rect, time: f64) -> Option<(u64, u32, BlockAct)> {
        let pane = self.panes.get(&id)?;
        if pane.vis_blocks.is_empty() || pane.term.mode().contains(TermMode::ALT_SCREEN) {
            return None;
        }
        let origin = pr.min + egui::vec2(self.config.padding, self.config.padding);
        let ch = self.metrics.cell_h;
        let fg = rgb(self.theme.foreground);
        let accent = rgb(self.theme.cursor);
        let green = rgb(self.theme.ansi[2]);
        let red = rgb(self.theme.ansi[1]);
        let clip = painter.with_clip_rect(pr.intersect(painter.clip_rect()));
        // Survol réel du terminal (pas d'une fenêtre posée dessus, sauf nos barres de blocs).
        let hover = ctx.input(|i| i.pointer.hover_pos()).filter(|p| {
            pr.contains(*p)
                && ctx.layer_id_at(*p).is_none_or(|l| {
                    l.order == egui::Order::Background
                        || pane.vis_blocks.iter().any(|b| l.id == Id::new(("block_bar", id, b.id)))
                })
        });
        let mut action = None;
        for vb in &pane.vis_blocks {
            let Some(block) = pane.block(vb.id) else { continue };
            let top = origin.y + vb.top_row as f32 * ch;
            let bottom = origin.y + (vb.end_row + 1) as f32 * ch;
            if bottom < pr.top() || top > pr.bottom() {
                continue;
            }
            let (color, ok) = match block.finished {
                None => (accent, None),
                Some((0, _)) => (green, Some(true)),
                Some(_) => (red, Some(false)),
            };
            if self.config.blocks_separator && vb.top_row > 0 {
                clip.line_segment([Pos2::new(pr.left() + 2.0, top - 1.0), Pos2::new(pr.right() - 2.0, top - 1.0)], Stroke::new(1.0, fg.gamma_multiply(0.08)));
            }
            if self.config.blocks_gutter {
                let x = pr.left() + (self.config.padding * 0.5 - 1.5).max(0.0);
                clip.rect_filled(Rect::from_min_max(Pos2::new(x, top + 1.0), Pos2::new(x + 3.0, bottom - 1.0)), CornerRadius::same(2), color.gamma_multiply(0.75));
            }
            if let Some((hid, hb, until)) = self.highlight {
                if hid == id && hb == vb.id && time < until {
                    let a = ((until - time) / 0.8) as f32;
                    clip.rect_filled(Rect::from_min_max(Pos2::new(pr.left(), top), Pos2::new(pr.right(), bottom)), 0.0, accent.gamma_multiply(0.12 * a));
                }
            }
            let cmd_y = origin.y + vb.cmd_row as f32 * ch + ch * 0.5;
            let hovered = hover.is_some_and(|p| p.y >= top && p.y < bottom);
            if hovered && self.config.blocks_actions && cmd_y > pr.top() && cmd_y < pr.bottom() {
                // Barre d'actions au survol, à droite de la ligne de commande.
                let pos = Pos2::new(pr.right() - self.config.padding - 4.0, cmd_y - 11.0);
                egui::Area::new(Id::new(("block_bar", id, vb.id)))
                    .order(egui::Order::Foreground)
                    .pivot(egui::Align2::RIGHT_TOP)
                    .fixed_pos(pos)
                    .show(ctx, |ui| {
                        egui::Frame::NONE
                            .fill(surface_color(&self.theme))
                            .stroke(Stroke::new(1.0, color.gamma_multiply(0.6)))
                            .corner_radius(CornerRadius::same(6))
                            .inner_margin(egui::Margin::symmetric(4, 1))
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    ui.spacing_mut().item_spacing.x = 2.0;
                                    let status = match block.finished {
                                        None => format!("⏳ {}", fmt_duration(block.elapsed())),
                                        Some((0, d)) => format!("✔ {}", fmt_duration(d)),
                                        Some((c, d)) => format!("✖ {c} · {}", fmt_duration(d)),
                                    };
                                    ui.label(egui::RichText::new(status).small().color(color));
                                    let b = |ui: &mut egui::Ui, t: &str, tip: &str| ui.small_button(t).on_hover_text(tip).clicked();
                                    if b(ui, "📋 cmd", "Copier la commande") {
                                        action = Some((id, vb.id, BlockAct::CopyCommand));
                                    }
                                    if b(ui, "📋 sortie", "Copier la sortie") {
                                        action = Some((id, vb.id, BlockAct::CopyOutput));
                                    }
                                    if b(ui, "↻", "Relancer la commande") {
                                        action = Some((id, vb.id, BlockAct::Rerun));
                                    }
                                    if b(ui, "⬆", "Remonter au début de la commande") {
                                        action = Some((id, vb.id, BlockAct::ScrollTop));
                                    }
                                });
                            });
                    });
                continue;
            }
            let elapsed = block.elapsed();
            let show = match self.config.blocks_badges {
                BadgeMode::Always => true,
                BadgeMode::Notable => ok == Some(false) || elapsed.as_secs_f32() >= 2.0,
                BadgeMode::Never => false,
            };
            if show && cmd_y > pr.top() && cmd_y < pr.bottom() {
                let text = match block.finished {
                    None => format!("⏳ {}", fmt_duration(elapsed)),
                    Some((0, d)) => format!("✔ {}", fmt_duration(d)),
                    Some((c, d)) => format!("✖ {c} · {}", fmt_duration(d)),
                };
                let font = egui::FontId::proportional((self.config.font_size * 0.78).clamp(10.0, 14.0));
                let galley = ctx.fonts(|f| f.layout_no_wrap(text, font, color));
                let size = galley.size() + Vec2::new(12.0, 4.0);
                let rect = Rect::from_min_size(Pos2::new(pr.right() - self.config.padding - size.x - 4.0, cmd_y - size.y * 0.5), size);
                clip.rect_filled(rect, CornerRadius::same(7), surface_color(&self.theme).gamma_multiply(0.92));
                clip.rect_stroke(rect, CornerRadius::same(7), Stroke::new(1.0, color.gamma_multiply(0.45)), egui::epaint::StrokeKind::Inside);
                clip.galley(rect.min + Vec2::new(6.0, 2.0), galley, color);
            }
        }
        if pane.running.is_some() {
            ctx.request_repaint_after(Duration::from_millis(500));
        }
        action
    }

    fn block_action(&mut self, pane_id: u64, block: u32, act: BlockAct) {
        match act {
            BlockAct::CopyCommand => {
                if let Some(cmd) = self.block_command(pane_id, block) {
                    self.ctx.copy_text(cmd);
                    self.toast("Commande copiée");
                }
            }
            BlockAct::CopyOutput => self.copy_block_output(pane_id, block),
            BlockAct::Rerun => {
                if let Some(cmd) = self.block_command(pane_id, block) {
                    self.insert_command(pane_id, &cmd, true);
                }
            }
            BlockAct::ScrollTop => {
                if let Some(pane) = self.panes.get_mut(&pane_id) {
                    if let Some(vb) = pane.vis_blocks.iter().find(|b| b.id == block) {
                        let delta = -vb.top_row;
                        pane.term.scroll_display(Scroll::Delta(delta));
                        pane.dirty = true;
                    }
                }
            }
        }
    }

    fn render_history_popup(&mut self, ctx: &egui::Context) {
        let Some(hp) = self.history_popup.as_mut() else { return };
        let mut query = std::mem::take(&mut hp.query);
        let mut selected = hp.selected;
        let focus = std::mem::take(&mut hp.focus);
        let pane = hp.pane;
        let surface = surface_color(&self.theme);
        let fg = rgb(self.theme.foreground);
        let accent = rgb(self.theme.cursor);
        let mut run: Option<(String, bool)> = None;
        let screen = ctx.screen_rect();
        let width = (screen.width() - 40.0).clamp(260.0, 720.0);
        let loading = self.history_rx.is_some();
        egui::Area::new(Id::new("history_popup"))
            .order(egui::Order::Foreground)
            .anchor(egui::Align2::CENTER_TOP, Vec2::new(0.0, 70.0))
            .show(ctx, |ui| {
                egui::Frame::NONE
                    .fill(surface)
                    .corner_radius(CornerRadius::same(10))
                    .stroke(Stroke::new(1.0, accent.gamma_multiply(0.6)))
                    .shadow(egui::Shadow { offset: [0, 6], blur: 24, spread: 0, color: Color32::from_black_alpha(90) })
                    .inner_margin(egui::Margin::same(10))
                    .show(ui, |ui| {
                        ui.set_width(width);
                        ui.label(egui::RichText::new(format!("Historique — {} commandes", self.history.len())).strong().color(fg));
                        let resp = ui.add(
                            egui::TextEdit::singleline(&mut query)
                                .id(history_input_id())
                                .font(egui::TextStyle::Monospace)
                                .desired_width(f32::INFINITY)
                                .hint_text("Filtrer (plusieurs mots possibles)…"),
                        );
                        if focus {
                            resp.request_focus();
                        }
                        if resp.changed() {
                            selected = 0;
                        }
                        ui.label(
                            egui::RichText::new("Entrée : insérer   Maj+Entrée : exécuter   ↑↓ : choisir   Échap : fermer")
                                .small()
                                .color(fg.gamma_multiply(0.55)),
                        );
                        ui.add_space(4.0);
                        let items = self.history.search(&query, 300);
                        if loading {
                            ui.label(egui::RichText::new("Chargement de l'historique…").color(fg.gamma_multiply(0.6)));
                        } else if items.is_empty() {
                            ui.label(egui::RichText::new("Aucune commande").color(fg.gamma_multiply(0.6)));
                        }
                        selected = selected.min(items.len().saturating_sub(1));
                        egui::ScrollArea::vertical().max_height(380.0).show(ui, |ui| {
                            for (i, cmd) in items.iter().enumerate() {
                                let (rect, r) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 24.0), Sense::click());
                                if i == selected {
                                    ui.painter().rect_filled(rect, CornerRadius::same(6), accent.gamma_multiply(0.22));
                                    r.scroll_to_me(None);
                                } else if r.hovered() {
                                    ui.painter().rect_filled(rect, CornerRadius::same(6), fg.gamma_multiply(0.07));
                                }
                                ui.painter().text(
                                    rect.left_center() + Vec2::new(8.0, 0.0),
                                    egui::Align2::LEFT_CENTER,
                                    truncate(cmd, 110),
                                    egui::FontId::monospace(13.0),
                                    fg,
                                );
                                let n = self.history.count(cmd);
                                if n > 1 {
                                    ui.painter().text(
                                        rect.right_center() - Vec2::new(8.0, 0.0),
                                        egui::Align2::RIGHT_CENTER,
                                        format!("×{n}"),
                                        egui::FontId::proportional(11.0),
                                        fg.gamma_multiply(0.5),
                                    );
                                }
                                if r.clicked() {
                                    run = Some((cmd.clone(), false));
                                }
                                if r.double_clicked() {
                                    run = Some((cmd.clone(), true));
                                }
                            }
                        });
                    });
            });
        let close = ctx.input(|i| i.pointer.any_pressed()) && !ctx.is_pointer_over_area();
        if let Some(hp) = self.history_popup.as_mut() {
            hp.query = query;
            hp.selected = selected;
        }
        if let Some((cmd, exec)) = run {
            self.history_popup = None;
            self.clear_egui_focus();
            self.insert_command(pane, &cmd, exec);
        } else if close {
            self.history_popup = None;
            self.clear_egui_focus();
        }
    }

    fn open_palette(&mut self, mode: PaletteMode) {
        self.history_popup = None;
        self.palette = Some(Palette { query: String::new(), selected: 0, focus: true, mode });
    }

    /// Éléments de la palette : (élément, libellé, texte de droite).
    fn palette_items(&self, query: &str, mode: PaletteMode) -> Vec<(PaletteItem, String, String)> {
        let ok = |text: &str| query.is_empty() || fuzzy_match(query, text);
        let mut out = Vec::new();
        if mode != PaletteMode::Profiles {
            for (i, sn) in self.config.snippets.iter().enumerate() {
                let label = format!("▶ {}", sn.name);
                if ok(&format!("{label} {} snippet favori", sn.command)) {
                    out.push((PaletteItem::Snippet(i), label, truncate(&sn.command, 40)));
                }
            }
        }
        if mode != PaletteMode::Snippets {
            for (i, p) in self.config.profiles.iter().enumerate() {
                let label = format!("▣ Profil : {}", p.name);
                if ok(&format!("{label} projet workspace")) {
                    out.push((PaletteItem::Profile(i), label, format!("{} onglet(s)", p.tabs.len())));
                }
            }
        }
        if mode == PaletteMode::All {
            for a in ALL_ACTIONS.iter().copied().filter(|a| !a.only_when_searching()) {
                if ok(&format!("{} {} {}", a.label(), a.category(), a.id())) {
                    let sc = self.bindings.first_shortcut(a);
                    let right = if sc.is_empty() { a.category().to_string() } else { format!("{}   {sc}", a.category()) };
                    out.push((PaletteItem::Action(a), a.label().to_string(), right));
                }
            }
            if !query.is_empty() {
                for name in self.config.theme_names() {
                    let label = format!("◐ Thème : {name}");
                    if ok(&format!("{label} theme couleurs")) {
                        out.push((PaletteItem::Theme(name), label, "Apparence".into()));
                    }
                }
            }
        }
        out
    }

    fn run_palette_item(&mut self, item: PaletteItem) {
        match item {
            PaletteItem::Action(a) => self.run_action(a),
            PaletteItem::Snippet(i) => self.run_snippet(i),
            PaletteItem::Profile(i) => self.open_profile(i),
            PaletteItem::Theme(name) => {
                self.config.theme_name = name;
                self.config.custom_theme = None;
                self.config_dirty = true;
            }
        }
    }

    /// Insère (ou exécute) un snippet ; variables {clipboard}, {selection}, {cwd}.
    fn run_snippet(&mut self, index: usize) {
        let Some(sn) = self.config.snippets.get(index).cloned() else { return };
        let Some(id) = self.focused_id() else { return };
        let mut cmd = sn.command.clone();
        if cmd.contains("{clipboard}") {
            cmd = cmd.replace("{clipboard}", &get_clipboard().unwrap_or_default());
        }
        if cmd.contains("{selection}") {
            cmd = cmd.replace("{selection}", &self.selection_text().unwrap_or_default());
        }
        if cmd.contains("{cwd}") {
            let cwd = self.panes.get(&id).and_then(|p| p.cwd()).map(|p| p.display().to_string()).unwrap_or_default();
            cmd = cmd.replace("{cwd}", &cwd);
        }
        self.insert_command(id, &cmd, sn.run);
    }

    // ------------------------------------------------------------------
    // Rendu de la fenêtre
    // ------------------------------------------------------------------

    /// Fond + image de fond peints avant les panneaux (même couche, donc dessous).
    fn render_window_bg(&self, ctx: &egui::Context) {
        let screen = ctx.screen_rect();
        let radius = self.window_radius();
        let painter = ctx.layer_painter(egui::LayerId::background());
        if self.config.custom_decorations {
            let b = self.theme.background;
            let bg = Color32::from_rgba_unmultiplied(b[0], b[1], b[2], (self.config.opacity * 255.0) as u8);
            painter.rect_filled(screen, CornerRadius::same(radius), bg);
        }
        if let Some(tex) = &self.bg_texture {
            let img_size = tex.size_vec2();
            let uv = if self.config.bg_image_stretch {
                Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0))
            } else {
                let scale = (screen.width() / img_size.x).max(screen.height() / img_size.y);
                let w = img_size.x * scale;
                let h = img_size.y * scale;
                let u = (w - screen.width()) / w * 0.5;
                let v = (h - screen.height()) / h * 0.5;
                Rect::from_min_max(Pos2::new(u, v), Pos2::new(1.0 - u, 1.0 - v))
            };
            let options = ImageOptions {
                uv,
                tint: Color32::from_white_alpha((self.config.bg_image_opacity * 255.0) as u8),
                corner_radius: CornerRadius::same(radius),
                ..Default::default()
            };
            paint_texture_at(&painter, screen, &options, &egui::load::SizedTexture::from(tex));
        }
    }

    fn render_title_bar(&mut self, ctx: &egui::Context) {
        let surface = surface_color(&self.theme);
        let fg = rgb(self.theme.foreground);
        let accent = rgb(self.theme.cursor);
        let radius = self.window_radius();
        let cr = CornerRadius { nw: radius, ne: radius, sw: 0, se: 0 };
        let maximized = self.is_maximized();
        let margin = egui::Margin::symmetric(10, 4);
        let mut minimize = false;
        let mut toggle_max = false;
        let mut close = false;
        egui::TopBottomPanel::top("title_bar")
            .frame(egui::Frame::NONE.fill(surface).corner_radius(cr).inner_margin(margin))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(">_").monospace().strong().size(15.0).color(accent));
                    ui.add_space(4.0);
                    let title_w = (ui.available_width() - 114.0).max(20.0);
                    ui.allocate_ui_with_layout(
                        Vec2::new(title_w, 24.0),
                        egui::Layout::left_to_right(egui::Align::Center),
                        |ui| {
                            if self.config.custom_title.is_empty() {
                                ui.add(egui::Label::new(egui::RichText::new("NovaTerm").strong().color(fg)).truncate());
                                ui.add(
                                    egui::Label::new(
                                        egui::RichText::new(format!(" — {}", self.active_pane_title()))
                                            .color(fg.gamma_multiply(0.6)),
                                    )
                                    .truncate(),
                                );
                            } else {
                                ui.add(
                                    egui::Label::new(egui::RichText::new(&self.config.custom_title).strong().color(fg))
                                        .truncate(),
                                );
                            }
                        },
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.spacing_mut().item_spacing.x = 4.0;
                        if window_button(ui, WinIcon::Close, "Fermer", fg, surface, true) {
                            close = true;
                        }
                        let (icon, tip) =
                            if maximized { (WinIcon::Restore, "Restaurer") } else { (WinIcon::Maximize, "Agrandir") };
                        if window_button(ui, icon, tip, fg, surface, false) {
                            toggle_max = true;
                        }
                        if window_button(ui, WinIcon::Minimize, "Réduire", fg, surface, false) {
                            minimize = true;
                        }
                    });
                });
                let bar = ui.min_rect() + margin;
                let drag_rect = Rect::from_min_max(bar.min, Pos2::new(bar.max.x - 124.0, bar.max.y));
                let resp = ui.interact(drag_rect, Id::new("title_drag"), Sense::click_and_drag());
                if resp.drag_started_by(egui::PointerButton::Primary) {
                    ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
                }
                if resp.double_clicked() {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized));
                }
            });
        if minimize {
            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
        }
        if toggle_max {
            ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized));
        }
        if close {
            self.request_quit();
        }
    }

    /// Poignées de redimensionnement sur les bords (décorations custom uniquement).
    fn render_resize_handles(&self, ctx: &egui::Context) {
        if !self.config.custom_decorations || self.is_maximized() {
            return;
        }
        let s = ctx.screen_rect();
        let t = 6.0;
        let (l, r, tp, b) = (s.left(), s.right(), s.top(), s.bottom());
        use egui::ResizeDirection as RD;
        let zones = [
            (Rect::from_min_max(Pos2::new(l + t, tp), Pos2::new(r - t, tp + t)), RD::North, egui::CursorIcon::ResizeVertical),
            (Rect::from_min_max(Pos2::new(l + t, b - t), Pos2::new(r - t, b)), RD::South, egui::CursorIcon::ResizeVertical),
            (Rect::from_min_max(Pos2::new(l, tp + t), Pos2::new(l + t, b - t)), RD::West, egui::CursorIcon::ResizeHorizontal),
            (Rect::from_min_max(Pos2::new(r - t, tp + t), Pos2::new(r, b - t)), RD::East, egui::CursorIcon::ResizeHorizontal),
            (Rect::from_min_max(Pos2::new(l, tp), Pos2::new(l + t, tp + t)), RD::NorthWest, egui::CursorIcon::ResizeNwSe),
            (Rect::from_min_max(Pos2::new(r - t, tp), Pos2::new(r, tp + t)), RD::NorthEast, egui::CursorIcon::ResizeNeSw),
            (Rect::from_min_max(Pos2::new(l, b - t), Pos2::new(l + t, b)), RD::SouthWest, egui::CursorIcon::ResizeNeSw),
            (Rect::from_min_max(Pos2::new(r - t, b - t), Pos2::new(r, b)), RD::SouthEast, egui::CursorIcon::ResizeNwSe),
        ];
        egui::Area::new(Id::new("resize_handles")).fixed_pos(s.min).show(ctx, |ui| {
            for (i, (rect, dir, cursor)) in zones.iter().enumerate() {
                let resp = ui.interact(*rect, Id::new(("win_resize", i)), Sense::drag());
                if resp.hovered() {
                    ctx.set_cursor_icon(*cursor);
                }
                if resp.drag_started_by(egui::PointerButton::Primary) {
                    ctx.send_viewport_cmd(egui::ViewportCommand::BeginResize(*dir));
                }
            }
        });
    }

    fn reload_bg_image(&mut self) {
        self.bg_texture = None;
        let Some(path) = &self.config.bg_image else { return };
        match std::fs::read(path).map_err(|e| e.to_string()).and_then(|b| crate::images::decode_bytes(&b)) {
            Ok(d) => {
                let color_image = (*d.image).clone();
                self.bg_texture = Some(self.ctx.load_texture("bg_image", color_image, TextureOptions::LINEAR));
            }
            Err(e) => {
                eprintln!("NovaTerm: image de fond illisible ({path}): {e}");
                self.toast(format!("Image de fond illisible : {e}"));
            }
        }
    }

    fn apply_config_changes(&mut self) {
        let sig = theme_sig(&self.config);
        if sig != self.theme_sig {
            self.theme_sig = sig;
            self.theme = self.config.theme();
            for pane in self.panes.values_mut() {
                pane.dirty = true;
            }
            apply_visuals(&self.ctx, &self.theme);
            let name = if self.config.custom_theme.is_some() {
                "personnalisé".to_string()
            } else {
                self.config.theme_name.clone()
            };
            self.toast(format!("Thème : {name}"));
        }
        if self.config.custom_decorations != self.applied_decorations {
            self.applied_decorations = self.config.custom_decorations;
            self.ctx.send_viewport_cmd(egui::ViewportCommand::Decorations(!self.config.custom_decorations));
        }
        if self.config.always_on_top != self.applied_on_top {
            self.applied_on_top = self.config.always_on_top;
            let level = if self.config.always_on_top { egui::WindowLevel::AlwaysOnTop } else { egui::WindowLevel::Normal };
            self.ctx.send_viewport_cmd(egui::ViewportCommand::WindowLevel(level));
        }
        if self.config.font_family != self.applied_font {
            self.applied_font = self.config.font_family.clone();
            if self.font_catalog.is_none() && !self.config.font_family.is_empty() {
                self.font_catalog = Some(fonts::discover());
            }
            // Garde-fou : si cette police fait planter le rendu, le prochain lancement
            // reviendra automatiquement à la police intégrée.
            guard::arm(&format!("font:{}", self.config.font_family));
            self.guard_frames = Some(self.frames + 30);
            match fonts::install(&self.ctx, &self.config.font_family, self.font_catalog.as_ref()) {
                Ok(()) => {
                    let name = if self.config.font_family.is_empty() { fonts::BUILTIN } else { &self.config.font_family };
                    self.toast(format!("Police : {name}"));
                }
                Err(e) => {
                    self.toast(format!("{e} : police intégrée conservée"));
                    self.config.font_family.clear();
                    self.applied_font.clear();
                    self.config_dirty = true;
                }
            }
            self.font_reload = 2;
        }
        if self.font_reload > 0 {
            self.font_reload -= 1;
            self.font_cache.clear();
            for pane in self.panes.values_mut() {
                pane.dirty = true;
            }
        }
        let opts = (self.config.scrollback, self.config.word_separators.clone());
        if opts != self.term_opts_sig {
            self.term_opts_sig = opts;
            for pane in self.panes.values_mut() {
                pane.apply_options(&self.config);
            }
        }
        if self.config.keybindings != self.bindings_src {
            self.bindings_src = self.config.keybindings.clone();
            self.bindings = Bindings::build(&self.config.keybindings);
        }
        let rounding = CornerRadius::same(self.window_radius());
        self.ctx.style_mut(|s| s.visuals.window_corner_radius = rounding);
        if self.config.bg_image != self.applied_image {
            self.applied_image = self.config.bg_image.clone();
            self.reload_bg_image();
        }
        let time = self.ctx.input(|i| i.time);
        if self.config_dirty && time - self.last_save > 0.5 {
            self.config.save();
            self.config_dirty = false;
            self.last_save = time;
        }
    }

    fn tab_title(&self, index: usize) -> String {
        let tab = &self.tabs[index];
        let title = tab.title.clone().unwrap_or_else(|| self.pane_title(tab.focused));
        truncate(&title, 24)
    }

    fn tab_bar_visible(&self) -> bool {
        self.config.show_tab_bar && !(self.config.hide_single_tab && self.tabs.len() == 1)
    }

    fn render_tab_bar(&mut self, ctx: &egui::Context) {
        if !self.tab_bar_visible() {
            return;
        }
        let surface = surface_color(&self.theme);
        let fg = rgb(self.theme.foreground);
        let accent = rgb(self.theme.cursor);
        let mut clicked = false;
        let mut select: Option<usize> = None;
        let mut close: Option<usize> = None;
        let mut rename: Option<usize> = None;
        let mut actions: Vec<Action> = Vec::new();
        let new_tab_tip = format!("Nouvel onglet ({})", self.bindings.first_shortcut(Action::NewTab));
        let palette_tip = format!("Palette de commandes ({})", self.bindings.first_shortcut(Action::CommandPalette));
        let settings_tip = format!("Paramètres ({})", self.bindings.first_shortcut(Action::Settings));
        let help_tip = format!("Raccourcis ({})", self.bindings.first_shortcut(Action::Help));
        egui::TopBottomPanel::top("tab_bar")
            .frame(egui::Frame::NONE.fill(surface).inner_margin(egui::Margin::symmetric(8, 5)))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let count = self.tabs.len();
                    for i in 0..count {
                        let active = i == self.active;
                        let panes = self.tabs[i].tree.leaves().len();
                        let mut text = format!("{}  {}", i + 1, self.tab_title(i));
                        if panes > 1 {
                            text.push_str(&format!("  ⊞{panes}"));
                        }
                        if self.tabs[i].broadcast {
                            text.insert_str(0, "📡 ");
                        }
                        let (badge, badge_color) = match self.tabs[i].activity {
                            Some(Some(true)) => ("✔ ", rgb(self.theme.ansi[2])),
                            Some(Some(false)) => ("✖ ", rgb(self.theme.ansi[1])),
                            Some(None) => ("● ", accent),
                            None => ("", fg),
                        };
                        let mut job = egui::text::LayoutJob::default();
                        let font = egui::TextStyle::Button.resolve(ui.style());
                        if !badge.is_empty() {
                            job.append(badge, 0.0, egui::TextFormat::simple(font.clone(), badge_color));
                        }
                        job.append(&text, 0.0, egui::TextFormat::simple(font, if active { fg } else { fg.gamma_multiply(0.6) }));
                        let label = egui::WidgetText::LayoutJob(job);
                        let btn = egui::Button::new(label)
                            .fill(if active { fg.gamma_multiply(0.12) } else { Color32::TRANSPARENT })
                            .stroke(if active { Stroke::new(1.0, accent.gamma_multiply(0.6)) } else { Stroke::NONE });
                        let resp = ui.add(btn);
                        if resp.clicked() {
                            select = Some(i);
                        }
                        if resp.double_clicked() {
                            rename = Some(i);
                        }
                        if resp.middle_clicked() {
                            close = Some(i);
                        }
                        resp.context_menu(|ui| {
                            if ui.button("Renommer…").clicked() {
                                rename = Some(i);
                                ui.close_menu();
                            }
                            if ui.button("Déplacer à gauche").clicked() {
                                select = Some(i);
                                actions.push(Action::MoveTabLeft);
                                ui.close_menu();
                            }
                            if ui.button("Déplacer à droite").clicked() {
                                select = Some(i);
                                actions.push(Action::MoveTabRight);
                                ui.close_menu();
                            }
                            ui.separator();
                            if ui.button("Fermer l'onglet").clicked() {
                                close = Some(i);
                                ui.close_menu();
                            }
                        });
                        let hint = if i < 9 { format!("Alt+{} · double-clic : renommer · clic milieu : fermer", i + 1) } else { String::new() };
                        if !hint.is_empty() {
                            resp.on_hover_text(hint);
                        }
                        if count > 1 {
                            let x = egui::RichText::new("×").color(fg.gamma_multiply(0.6));
                            if ui
                                .add(egui::Button::new(x).fill(Color32::TRANSPARENT).stroke(Stroke::NONE))
                                .on_hover_text("Fermer l'onglet")
                                .clicked()
                            {
                                close = Some(i);
                            }
                        }
                        ui.add_space(2.0);
                    }
                    if ui
                        .add(egui::Button::new(egui::RichText::new("+").color(fg)).fill(Color32::TRANSPARENT).stroke(Stroke::NONE))
                        .on_hover_text(new_tab_tip)
                        .clicked()
                    {
                        actions.push(Action::NewTab);
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let flat = |t: &str| egui::Button::new(egui::RichText::new(t).color(fg)).fill(Color32::TRANSPARENT).stroke(Stroke::NONE);
                        if ui.add(flat("?")).on_hover_text(help_tip).clicked() {
                            actions.push(Action::Help);
                        }
                        if ui.add(flat("⚙")).on_hover_text(settings_tip).clicked() {
                            actions.push(Action::Settings);
                        }
                        if ui.add(flat("⌘")).on_hover_text(palette_tip).clicked() {
                            actions.push(Action::CommandPalette);
                        }
                    });
                });
                clicked = ui.ui_contains_pointer() && ui.input(|i| i.pointer.any_click());
            });
        if let Some(i) = select {
            self.set_active(i);
        }
        if let Some(i) = rename {
            self.set_active(i);
            self.run_action(Action::RenameTab);
        }
        for a in actions {
            self.run_action(a);
        }
        if let Some(i) = close {
            self.request_close_tab(i);
        }
        if clicked && rename.is_none() && self.palette.is_none() {
            self.clear_egui_focus();
        }
    }

    fn render_status_bar(&mut self, ctx: &egui::Context, time: f64) {
        if !self.config.show_status_bar {
            return;
        }
        let mut surface = surface_color(&self.theme);
        if time < self.bell_until {
            surface = rgb(self.theme.ansi[3]);
        }
        let fg = rgb(self.theme.foreground);
        let mut info = self
            .focused_id()
            .and_then(|id| self.panes.get(&id))
            .map(|p| {
                let mut s = format!("{}  ·  {}×{}", p.title, p.cols, p.rows);
                let off = p.term.grid().display_offset();
                if off > 0 {
                    s.push_str(&format!("  ·  ↑ {off} lignes"));
                }
                s
            })
            .unwrap_or_default();
        if let Some(tab) = self.tabs.get(self.active) {
            if tab.zoomed.is_some() {
                info.push_str("  ·  ZOOM");
            }
        }
        if self.config.suggestions && self.focused_id().and_then(|id| self.panes.get(&id)).is_some_and(|p| p.integration && !p.suggest_conflict) {
            info.push_str("  ·  ✨ suggestions");
        }
        if self.tabs.get(self.active).is_some_and(|t| t.broadcast) {
            info.push_str("  ·  📡 DIFFUSION");
        }
        if let Some(p) = self.focused_id().and_then(|id| self.panes.get(&id)) {
            if let Some(id) = p.running {
                if let Some(b) = p.block(id) {
                    info.push_str(&format!("  ·  ⏳ {} ({})", truncate(&b.command, 30), fmt_duration(b.elapsed())));
                }
            }
        }
        let hint = format!(
            "{} : commandes  ·  {} : aide",
            self.bindings.first_shortcut(Action::CommandPalette),
            self.bindings.first_shortcut(Action::Help)
        );
        let radius = self.window_radius();
        let cr = CornerRadius { nw: 0, ne: 0, sw: radius, se: radius };
        egui::TopBottomPanel::bottom("status_bar")
            .frame(egui::Frame::NONE.fill(surface).corner_radius(cr).inner_margin(egui::Margin::symmetric(10, 4)))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.add(egui::Label::new(egui::RichText::new(info).small().color(fg)).truncate());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(egui::RichText::new(hint).small().color(fg.gamma_multiply(0.6)));
                    });
                });
            });
    }

    // ------------------------------------------------------------------
    // Panneaux : rendu + souris
    // ------------------------------------------------------------------

    fn begin_selection(&mut self, id: u64, rect: Rect, pos: Pos2, ty: SelectionType) {
        let Some(pane) = self.panes.get_mut(&id) else { return };
        let offset = pane.term.grid().display_offset();
        let (point, side) = render::pixel_to_point(pos, rect, &self.config, self.metrics, offset);
        pane.term.selection = Some(Selection::new(ty, point, side));
        pane.selecting = true;
        pane.dirty = true;
    }

    fn update_selection(&mut self, id: u64, rect: Rect, pos: Pos2) {
        let Some(pane) = self.panes.get_mut(&id) else { return };
        if !pane.selecting {
            return;
        }
        // Défilement automatique quand on sélectionne au-delà du bord.
        if pos.y < rect.top() {
            pane.term.scroll_display(Scroll::Delta(1));
        } else if pos.y > rect.bottom() {
            pane.term.scroll_display(Scroll::Delta(-1));
        }
        let pos = Pos2::new(pos.x, pos.y.clamp(rect.top(), rect.bottom() - 1.0));
        let offset = pane.term.grid().display_offset();
        let (point, side) = render::pixel_to_point(pos, rect, &self.config, self.metrics, offset);
        if let Some(sel) = &mut pane.term.selection {
            sel.update(point, side);
        }
        pane.dirty = true;
    }

    fn finish_selection(&mut self, id: u64) {
        let Some(pane) = self.panes.get_mut(&id) else { return };
        pane.selecting = false;
        if let Some(text) = pane.term.selection_to_string().map(|t| blocks::strip_tags(&t)).filter(|t| !t.is_empty()) {
            set_primary_selection(&text);
            if self.config.copy_on_select {
                self.ctx.copy_text(text);
            }
        }
    }

    /// Lien sous le pointeur : (url, ligne écran, col début, col fin).
    /// Lien sous le pointeur : (cible, ligne écran, col début, col fin).
    fn link_at(&self, id: u64, rect: Rect, pos: Pos2) -> Option<(Target, usize, usize, usize)> {
        let pane = self.panes.get(&id)?;
        let (col, row) = render::pixel_to_cell(pos, rect, &self.config, self.metrics, pane.cols, pane.rows);
        let offset = pane.term.grid().display_offset() as i32;
        let line = Line(row as i32 - offset);
        if line < pane.term.topmost_line() || line > pane.term.bottommost_line() {
            return None;
        }
        let grid = pane.term.grid();
        let cols = pane.term.columns();
        // Lien OSC 8 (posé par ls --hyperlink, gcc, cargo…).
        if let Some(h) = grid[line][Column(col)].hyperlink() {
            let uri = h.uri().to_string();
            let same = |c: usize| grid[line][Column(c)].hyperlink().is_some_and(|x| x.uri() == uri);
            let mut s = col;
            while s > 0 && same(s - 1) {
                s -= 1;
            }
            let mut e = col + 1;
            while e < cols && same(e) {
                e += 1;
            }
            let target = match uri.strip_prefix("file://") {
                Some(rest) => {
                    let path = &rest[rest.find('/').unwrap_or(0)..];
                    Target::File { path: PathBuf::from(path), line: None, col: None }
                }
                None => Target::Url(uri),
            };
            return Some((target, row, s, e));
        }
        let chars: Vec<char> = (0..cols).map(|c| grid[line][Column(c)].c).collect();
        let hit = if self.config.detect_paths {
            links::find_at(&chars, col, pane.cwd().as_deref())
        } else {
            links::find_url(&chars, col)
        };
        hit.map(|h| (h.target, row, h.start, h.end))
    }

    /// Ouvre un lien : navigateur, éditeur configuré ou nouvel onglet.
    fn open_target(&mut self, target: Target) {
        match target {
            Target::Url(url) => {
                // Seuls les schémas courants sont ouverts (pas de javascript:, smb:, etc.).
                let scheme = url.split(':').next().unwrap_or("").to_ascii_lowercase();
                if !matches!(scheme.as_str(), "http" | "https" | "mailto" | "ftp" | "sftp" | "ssh") {
                    self.toast(format!("Lien « {scheme}: » non ouvert par sécurité"));
                    return;
                }
                open_external(&url);
                self.toast("Ouverture du lien…");
            }
            Target::File { path, line, col } => {
                let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                match links::editor_command(&self.config.editor_command, &path, line, col) {
                    Some(cmd) if cmd.starts_with("@tab ") => {
                        let cmd = cmd.trim_start_matches("@tab ").to_string();
                        let dir = path.parent().map(PathBuf::from);
                        if let Some(pid) = self.spawn_pane(dir) {
                            self.tabs.push(Tab::new(pid));
                            self.active = self.tabs.len() - 1;
                            if let Some(p) = self.panes.get_mut(&pid) {
                                p.write_input(format!("{cmd}\r").as_bytes());
                            }
                        }
                    }
                    Some(cmd) => {
                        if let Err(e) = platform::spawn_command_line(&cmd) {
                            self.toast(format!("Éditeur introuvable ({e}) : vérifiez Paramètres → Liens"));
                            return;
                        }
                        self.toast(format!("Ouverture de {name}…"));
                    }
                    None => {
                        // Sécurité : jamais d'exécution via un clic (programme, .desktop, AppImage…).
                        if is_executable_file(&path) {
                            self.toast(format!("« {name} » est un programme : non ouvert par sécurité"));
                            return;
                        }
                        open_external(&path.display().to_string());
                        self.toast(format!("Ouverture de {name}…"));
                    }
                }
            }
        }
    }

    fn mouse_cell(&self, id: u64, rect: Rect, pos: Pos2) -> (usize, usize) {
        let (cols, rows) = self.panes.get(&id).map(|p| (p.cols, p.rows)).unwrap_or((1, 1));
        render::pixel_to_cell(pos, rect, &self.config, self.metrics, cols, rows)
    }

    fn send_mouse(&mut self, id: u64, button: MouseButton, cell: (usize, usize), pressed: bool, motion: bool, mods: &Modifiers) {
        let Some(pane) = self.panes.get_mut(&id) else { return };
        let sgr = pane.term.mode().contains(TermMode::SGR_MOUSE);
        if let Some(bytes) = input::mouse_report(button, cell.0, cell.1, pressed, motion, mods, sgr) {
            pane.write_raw(&bytes);
        }
    }

    /// Molette : historique, flèches (écran alternatif) ou rapport souris.
    fn handle_wheel(&mut self, id: u64, rect: Rect, events: &[egui::Event], mouse_mode: bool) {
        let pointer = self.ctx.input(|i| i.pointer.hover_pos());
        for ev in events {
            let egui::Event::MouseWheel { unit, delta, modifiers } = ev else { continue };
            if modifiers.ctrl && self.config.ctrl_scroll_zoom {
                let step = match unit {
                    egui::MouseWheelUnit::Point => delta.y / 40.0,
                    _ => delta.y.signum(),
                };
                self.zoom_accum += step;
                while self.zoom_accum.abs() >= 1.0 {
                    let s = self.zoom_accum.signum();
                    self.zoom_accum -= s;
                    self.zoom_font(s);
                }
                continue;
            }
            let cell_h = self.metrics.cell_h;
            let Some(pane) = self.panes.get_mut(&id) else { return };
            let lines = match unit {
                egui::MouseWheelUnit::Line => delta.y * self.config.scroll_multiplier,
                egui::MouseWheelUnit::Point => delta.y / cell_h,
                egui::MouseWheelUnit::Page => delta.y * pane.rows as f32,
            };
            pane.scroll_accum += lines;
            let n = pane.scroll_accum.trunc() as i32;
            if n == 0 {
                continue;
            }
            pane.scroll_accum -= n as f32;
            let mode = *pane.term.mode();
            if mouse_mode {
                let cell = pointer.map(|p| self.mouse_cell(id, rect, p)).unwrap_or((0, 0));
                let button = if n > 0 { MouseButton::WheelUp } else { MouseButton::WheelDown };
                for _ in 0..n.unsigned_abs().min(20) {
                    self.send_mouse(id, button, cell, true, false, modifiers);
                }
            } else if mode.contains(TermMode::ALT_SCREEN) && mode.contains(TermMode::ALTERNATE_SCROLL) {
                let app = mode.contains(TermMode::APP_CURSOR);
                let seq: &[u8] = match (n > 0, app) {
                    (true, true) => b"\x1bOA",
                    (true, false) => b"\x1b[A",
                    (false, true) => b"\x1bOB",
                    (false, false) => b"\x1b[B",
                };
                let bytes: Vec<u8> = seq.repeat(n.unsigned_abs().min(50) as usize);
                pane.write_raw(&bytes);
            } else {
                pane.term.scroll_display(Scroll::Delta(n));
                pane.dirty = true;
            }
        }
    }

    fn render_central(&mut self, ctx: &egui::Context, time: f64) {
        let pointer_events: Vec<egui::Event> = ctx.input(|i| {
            i.events
                .iter()
                .filter(|e| {
                    matches!(e, egui::Event::PointerButton { .. } | egui::Event::PointerMoved(_) | egui::Event::MouseWheel { .. })
                })
                .cloned()
                .collect()
        });
        let mods_now = ctx.input(|i| i.modifiers);
        let pointer_moved = ctx.input(|i| i.pointer.delta() != Vec2::ZERO);
        let any_down = ctx.input(|i| i.pointer.any_down());
        let popup_open = ctx.memory(|m| m.any_popup_open());

        egui::CentralPanel::default().frame(egui::Frame::NONE).show(ctx, |ui| {
            let rect = ui.max_rect();
            let painter = ui.painter().clone();

            let Some(tab) = self.tabs.get(self.active) else { return };
            let tree = tab.tree.clone();
            let focused = tab.focused;
            let zoomed = tab.zoomed;
            let mut leaves = Vec::new();
            let mut dividers = Vec::new();
            if let Some(zid) = zoomed {
                leaves.push((zid, rect));
            } else {
                tree.layout(rect, &mut leaves, &mut dividers);
            }
            self.pane_rects = leaves.clone();
            let multi = leaves.len() > 1;
            let mut ghost_target: Option<(u64, Rect)> = None;
            let blinking = self.config.cursor_blink
                && ctx.input(|i| i.focused)
                && (self.config.cursor_blink_timeout == 0 || time - self.last_input_time < self.config.cursor_blink_timeout as f64);
            let geo = (self.metrics.cell_w.to_bits(), self.metrics.cell_h.to_bits(), self.config.font_size.to_bits(), self.config.padding.to_bits());
            let theme_hash = {
                use std::hash::{Hash, Hasher};
                let mut h = std::collections::hash_map::DefaultHasher::new();
                self.theme_sig.hash(&mut h);
                self.font_reload.hash(&mut h);
                self.applied_font.hash(&mut h);
                h.finish()
            };
            let render_key = |r: Rect| -> u64 {
                use std::hash::{Hash, Hasher};
                let mut h = std::collections::hash_map::DefaultHasher::new();
                (r.min.x.to_bits(), r.min.y.to_bits(), r.max.x.to_bits(), r.max.y.to_bits(), geo, theme_hash).hash(&mut h);
                h.finish()
            };

            // Panneaux.
            for (id, pr) in &leaves {
                let (cols, rows) = render::grid_size(*pr, &self.config, self.metrics);
                let Some(pane) = self.panes.get_mut(id) else { continue };
                pane.resize(cols, rows, self.metrics.cell_w, self.metrics.cell_h);
                if pane.dirty || pane.cached.is_empty() || pane.cache_key != render_key(*pr) {
                    render::build_shapes(&self.ctx, pane, *pr, &self.theme, &self.config, self.metrics, &mut self.font_cache, render_key(*pr));
                    pane.refresh_marks();
                    pane.dirty = false;
                }
                painter.extend(pane.cached.iter().cloned());
                // Images en ligne.
                if !pane.vis_images.is_empty() {
                    let origin = pr.min + egui::vec2(self.config.padding, self.config.padding);
                    let clip = painter.with_clip_rect(pr.intersect(painter.clip_rect()));
                    for (row, col, iid) in pane.vis_images.clone() {
                        let Some(img) = pane.images.get_mut(&iid) else { continue };
                        let tex = img.texture.get_or_insert_with(|| {
                            self.ctx.load_texture(format!("nova_img_{id}_{iid}"), (*img.image).clone(), TextureOptions::LINEAR)
                        });
                        let r = Rect::from_min_size(
                            Pos2::new(origin.x + col as f32 * self.metrics.cell_w, origin.y + row as f32 * self.metrics.cell_h),
                            Vec2::new(img.cols as f32 * self.metrics.cell_w, img.rows as f32 * self.metrics.cell_h),
                        );
                        // Conserver les proportions dans la zone réservée.
                        let isz = Vec2::new(img.image.size[0] as f32, img.image.size[1] as f32);
                        let scale = (r.width() / isz.x).min(r.height() / isz.y);
                        let draw = Rect::from_min_size(r.min, isz * scale);
                        clip.image(tex.id(), draw, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
                    }
                }
                render::draw_cursor(&painter, pane, *pr, &self.theme, &self.config, self.metrics, *id == focused, time, blinking);
                if *id == focused {
                    ghost_target = Some((*id, *pr));
                }
                if multi && *id != focused && self.config.dim_inactive > 0.0 {
                    let b = self.theme.background;
                    let a = (self.config.dim_inactive.clamp(0.0, 0.8) * 255.0) as u8;
                    painter.rect_filled(*pr, 0.0, Color32::from_rgba_unmultiplied(b[0] / 2, b[1] / 2, b[2] / 2, a));
                }
                if multi && *id == focused {
                    painter.rect_stroke(
                        pr.shrink(0.5),
                        0.0,
                        Stroke::new(1.0, rgb(self.theme.cursor).gamma_multiply(0.45)),
                        egui::epaint::StrokeKind::Middle,
                    );
                }
                if pane.exited {
                    let msg = match pane.exit_code {
                        Some(c) => format!("Processus terminé (code {c})  —  Entrée : relancer  ·  fermer : {}", self.bindings.first_shortcut(Action::ClosePane)),
                        None => format!("Processus terminé  —  Entrée : relancer  ·  fermer : {}", self.bindings.first_shortcut(Action::ClosePane)),
                    };
                    let band = Rect::from_min_max(Pos2::new(pr.left(), pr.bottom() - 30.0), pr.max);
                    painter.rect_filled(band, 0.0, surface_color(&self.theme));
                    painter.text(band.center(), egui::Align2::CENTER_CENTER, msg, egui::FontId::proportional(13.0), rgb(self.theme.foreground));
                }
            }

            if let Some((gid, gr)) = ghost_target {
                self.render_suggestions(ui.ctx(), &painter, gid, gr);
            }

            // Blocs de commandes (barre de statut à gauche, badge, actions au survol).
            let mut block_actions = Vec::new();
            if self.config.blocks_enabled {
                for (id, pr) in &leaves {
                    if let Some(a) = self.render_blocks(ui.ctx(), &painter, *id, *pr, time) {
                        block_actions.push(a);
                    }
                }
            }
            for (pane_id, bid, act) in block_actions {
                self.block_action(pane_id, bid, act);
            }

            // Diffusion du clavier : cadre rouge sur les panneaux concernés.
            if self.tabs.get(self.active).is_some_and(|t| t.broadcast) {
                for (_, pr) in &leaves {
                    painter.rect_stroke(pr.shrink(1.0), 0.0, Stroke::new(2.0, rgb(self.theme.ansi[1]).gamma_multiply(0.8)), egui::epaint::StrokeKind::Middle);
                }
            }

            // Séparateurs.
            for div in &dividers {
                painter.rect_filled(div.rect, 0.0, surface_color(&self.theme));
            }
            for div in &dividers {
                let resp = ui.interact(div.rect, Id::new(("divider", &div.path)), Sense::click_and_drag());
                if resp.dragged() {
                    let delta = resp.drag_delta();
                    let change = match div.dir {
                        Dir::Horizontal => delta.x / div.span,
                        Dir::Vertical => delta.y / div.span,
                    };
                    let ratio = self.tabs[self.active].tree.ratio(&div.path) + change;
                    self.tabs[self.active].tree.set_ratio(&div.path, ratio);
                }
                if resp.double_clicked() {
                    self.tabs[self.active].tree.set_ratio(&div.path, 0.5);
                }
                if resp.hovered() || resp.dragged() {
                    ui.ctx().set_cursor_icon(match div.dir {
                        Dir::Horizontal => egui::CursorIcon::ResizeHorizontal,
                        Dir::Vertical => egui::CursorIcon::ResizeVertical,
                    });
                }
            }

            for (id, pr) in &leaves {
                let (id, pr) = (*id, *pr);
                let resp = ui.interact(pr, Id::new(("pane", id)), Sense::click_and_drag());
                let Some(mode) = self.panes.get(&id).map(|p| *p.term.mode()) else { continue };

                // Focus qui suit la souris.
                if self.config.focus_follows_mouse
                    && resp.hovered()
                    && pointer_moved
                    && !any_down
                    && !popup_open
                    && self.focused_id() != Some(id)
                {
                    self.set_focus(id);
                }

                let mouse_mode = self.config.mouse_reporting && mode.intersects(TermMode::MOUSE_MODE) && !mods_now.shift;
                if resp.contains_pointer() || self.mouse_down.is_some_and(|(p, _)| p == id) {
                    self.handle_wheel(id, pr, &pointer_events, mouse_mode && resp.contains_pointer());
                }

                if mouse_mode {
                    for ev in &pointer_events {
                        match ev {
                            egui::Event::PointerButton { pos, button, pressed, modifiers } => {
                                let b = match button {
                                    egui::PointerButton::Primary => MouseButton::Left,
                                    egui::PointerButton::Secondary => MouseButton::Right,
                                    egui::PointerButton::Middle => MouseButton::Middle,
                                    _ => continue,
                                };
                                let cell = self.mouse_cell(id, pr, *pos);
                                if *pressed && pr.contains(*pos) && resp.contains_pointer() {
                                    self.set_focus(id);
                                    self.clear_egui_focus();
                                    self.mouse_down = Some((id, b));
                                    self.last_mouse_cell = Some(cell);
                                    self.send_mouse(id, b, cell, true, false, modifiers);
                                } else if !*pressed && self.mouse_down.is_some_and(|(p, _)| p == id) {
                                    self.mouse_down = None;
                                    self.send_mouse(id, b, cell, false, false, modifiers);
                                }
                            }
                            egui::Event::PointerMoved(pos) => {
                                let cell = self.mouse_cell(id, pr, *pos);
                                if self.last_mouse_cell == Some(cell) {
                                    continue;
                                }
                                match self.mouse_down {
                                    Some((p, b)) if p == id && mode.intersects(TermMode::MOUSE_DRAG | TermMode::MOUSE_MOTION) => {
                                        self.last_mouse_cell = Some(cell);
                                        self.send_mouse(id, b, cell, true, true, &mods_now);
                                    }
                                    None if mode.contains(TermMode::MOUSE_MOTION) && pr.contains(*pos) && resp.contains_pointer() => {
                                        self.last_mouse_cell = Some(cell);
                                        self.send_mouse(id, MouseButton::None, cell, true, true, &mods_now);
                                    }
                                    _ => {}
                                }
                            }
                            _ => {}
                        }
                    }
                    continue;
                }

                // Liens : Ctrl + survol = soulignés, Ctrl + clic = ouverture (ou simple clic).
                let mut hovered_link = None;
                let link_mode = self.config.ctrl_click_links && (mods_now.ctrl || self.config.link_plain_click);
                if link_mode && resp.hovered() && !any_down {
                    if let Some(pos) = resp.hover_pos() {
                        hovered_link = self.link_at(id, pr, pos);
                        if let Some((_, row, s, e)) = &hovered_link {
                            let origin = pr.min + egui::vec2(self.config.padding, self.config.padding);
                            let y = origin.y + (*row as f32 + 1.0) * self.metrics.cell_h - 1.5;
                            let x0 = origin.x + *s as f32 * self.metrics.cell_w;
                            let x1 = origin.x + *e as f32 * self.metrics.cell_w;
                            painter.line_segment([Pos2::new(x0, y), Pos2::new(x1, y)], Stroke::new(1.2, rgb(self.theme.cursor)));
                            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                        }
                    }
                } else if resp.hovered() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::Text);
                }

                if resp.clicked() || resp.drag_started() || resp.secondary_clicked() || resp.middle_clicked() {
                    self.set_focus(id);
                    self.clear_egui_focus();
                }
                if resp.clicked() {
                    if let Some((target, ..)) = hovered_link {
                        self.open_target(target);
                    } else if let Some(pane) = self.panes.get_mut(&id) {
                        pane.term.selection = None;
                        pane.dirty = true;
                    }
                }
                if resp.double_clicked() {
                    if let Some(pos) = resp.interact_pointer_pos() {
                        self.begin_selection(id, pr, pos, SelectionType::Semantic);
                        self.finish_selection(id);
                    }
                } else if resp.triple_clicked() {
                    if let Some(pos) = resp.interact_pointer_pos() {
                        self.begin_selection(id, pr, pos, SelectionType::Lines);
                        self.finish_selection(id);
                    }
                } else if resp.drag_started_by(egui::PointerButton::Primary) {
                    if let Some(pos) = resp.interact_pointer_pos() {
                        let ty = if mods_now.alt { SelectionType::Block } else { SelectionType::Simple };
                        self.begin_selection(id, pr, pos, ty);
                    }
                }
                if resp.dragged_by(egui::PointerButton::Primary) {
                    if let Some(pos) = resp.interact_pointer_pos() {
                        self.update_selection(id, pr, pos);
                    }
                }
                if resp.drag_stopped_by(egui::PointerButton::Primary) {
                    self.finish_selection(id);
                }
                if resp.middle_clicked() && self.config.middle_click_paste {
                    self.paste_primary();
                }

                match self.config.right_click {
                    RightClick::Menu => {
                        let mut action: Option<Action> = None;
                        let b = &self.bindings;
                        resp.context_menu(|ui| {
                            let item = |ui: &mut egui::Ui, a: Action, action: &mut Option<Action>| {
                                let sc = b.first_shortcut(a);
                                if ui.add(egui::Button::new(a.label()).shortcut_text(sc)).clicked() {
                                    *action = Some(a);
                                    ui.close_menu();
                                }
                            };
                            item(ui, Action::Copy, &mut action);
                            item(ui, Action::Paste, &mut action);
                            item(ui, Action::SelectAll, &mut action);
                            item(ui, Action::Find, &mut action);
                            ui.separator();
                            item(ui, Action::SplitRight, &mut action);
                            item(ui, Action::SplitDown, &mut action);
                            item(ui, Action::ToggleZoom, &mut action);
                            item(ui, Action::PaneToTab, &mut action);
                            ui.separator();
                            item(ui, Action::Clear, &mut action);
                            item(ui, Action::ResetTerminal, &mut action);
                            ui.separator();
                            item(ui, Action::NewTab, &mut action);
                            item(ui, Action::Settings, &mut action);
                            ui.separator();
                            item(ui, Action::ClosePane, &mut action);
                        });
                        if let Some(a) = action {
                            self.set_focus(id);
                            self.run_action(a);
                        }
                    }
                    RightClick::Paste => {
                        if resp.secondary_clicked() {
                            self.paste_clipboard();
                        }
                    }
                    RightClick::CopyOrPaste => {
                        if resp.secondary_clicked() && !self.copy() {
                            self.paste_clipboard();
                        } else if resp.secondary_clicked() {
                            if let Some(p) = self.panes.get_mut(&id) {
                                p.term.selection = None;
                                p.dirty = true;
                            }
                        }
                    }
                }
            }
        });

        self.render_search_bar(ctx);
    }

    // ------------------------------------------------------------------
    // Fenêtres internes : recherche, palette, renommage, confirmation
    // ------------------------------------------------------------------

    fn render_search_bar(&mut self, ctx: &egui::Context) {
        let Some(s) = self.search.as_mut() else { return };
        let anchor = self.pane_rects.iter().find(|(id, _)| *id == s.pane).map(|(_, r)| *r);
        let Some(rect) = anchor else { return };
        let pos = Pos2::new((rect.right() - 380.0).max(rect.left() + 4.0), rect.top() + 8.0);
        let surface = surface_color(&self.theme);
        let accent = rgb(self.theme.cursor);
        let mut next = false;
        let mut prev = false;
        let mut close = false;
        egui::Area::new(Id::new("search_bar")).fixed_pos(pos).order(egui::Order::Foreground).show(ctx, |ui| {
            egui::Frame::NONE
                .fill(surface)
                .corner_radius(CornerRadius::same(8))
                .stroke(Stroke::new(1.0, accent.gamma_multiply(0.5)))
                .inner_margin(egui::Margin::same(6))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        let edit = egui::TextEdit::singleline(&mut s.query)
                            .id(search_input_id())
                            .desired_width(170.0)
                            .hint_text("Rechercher (regex)…");
                        let resp = ui.add(edit);
                        if s.focus {
                            resp.request_focus();
                            s.focus = false;
                        }
                        if resp.changed() {
                            s.regex = None;
                            s.last = None;
                            s.not_found = false;
                        }
                        if s.not_found {
                            ui.label(egui::RichText::new("Aucun résultat").small().color(Color32::from_rgb(230, 110, 110)));
                        }
                        if ui.button("↑").on_hover_text("Précédent (Maj+Entrée)").clicked() {
                            prev = true;
                        }
                        if ui.button("↓").on_hover_text("Suivant (Entrée)").clicked() {
                            next = true;
                        }
                        if ui.button("×").on_hover_text("Fermer (Échap)").clicked() {
                            close = true;
                        }
                    });
                });
        });
        if prev {
            self.search_next(true);
        }
        if next {
            self.search_next(false);
        }
        if close {
            self.close_search();
        }
    }

    fn render_palette(&mut self, ctx: &egui::Context) {
        let Some(pal) = self.palette.as_mut() else { return };
        let mut query = std::mem::take(&mut pal.query);
        let mut selected = pal.selected;
        let focus = std::mem::take(&mut pal.focus);
        let mode = pal.mode;
        let surface = surface_color(&self.theme);
        let fg = rgb(self.theme.foreground);
        let accent = rgb(self.theme.cursor);
        let mut run: Option<PaletteItem> = None;
        let mut close = false;
        let screen = ctx.screen_rect();
        let width = (screen.width() - 40.0).clamp(240.0, 600.0);
        let hint = match mode {
            PaletteMode::All => "Commande, snippet, profil ou « thème … »  (↑ ↓ Entrée, Échap)",
            PaletteMode::Snippets => "Commandes favorites…  (Paramètres → Snippets pour en ajouter)",
            PaletteMode::Profiles => "Profils de projet…  (« Enregistrer la disposition comme profil »)",
        };
        egui::Area::new(Id::new("palette"))
            .order(egui::Order::Foreground)
            .anchor(egui::Align2::CENTER_TOP, Vec2::new(0.0, 70.0))
            .show(ctx, |ui| {
                egui::Frame::NONE
                    .fill(surface)
                    .corner_radius(CornerRadius::same(10))
                    .stroke(Stroke::new(1.0, accent.gamma_multiply(0.6)))
                    .shadow(egui::Shadow { offset: [0, 6], blur: 24, spread: 0, color: Color32::from_black_alpha(90) })
                    .inner_margin(egui::Margin::same(10))
                    .show(ui, |ui| {
                        ui.set_width(width);
                        let resp = ui.add(
                            egui::TextEdit::singleline(&mut query)
                                .id(palette_input_id())
                                .desired_width(f32::INFINITY)
                                .hint_text(hint),
                        );
                        if focus {
                            resp.request_focus();
                        }
                        if resp.changed() {
                            selected = 0;
                        }
                        ui.add_space(6.0);
                        let items = self.palette_items(&query, mode);
                        if items.is_empty() {
                            ui.label(egui::RichText::new("Aucun résultat").color(fg.gamma_multiply(0.6)));
                        }
                        selected = selected.min(items.len().saturating_sub(1));
                        egui::ScrollArea::vertical().max_height(360.0).show(ui, |ui| {
                            for (i, (item, label, right)) in items.iter().enumerate() {
                                let sel = i == selected;
                                let (rect, r) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 26.0), Sense::click());
                                if sel {
                                    ui.painter().rect_filled(rect, CornerRadius::same(6), accent.gamma_multiply(0.22));
                                    r.scroll_to_me(None);
                                } else if r.hovered() {
                                    ui.painter().rect_filled(rect, CornerRadius::same(6), fg.gamma_multiply(0.07));
                                }
                                let p = ui.painter();
                                p.text(
                                    rect.left_center() + Vec2::new(8.0, 0.0),
                                    egui::Align2::LEFT_CENTER,
                                    label,
                                    egui::FontId::proportional(14.0),
                                    fg,
                                );
                                p.text(
                                    rect.right_center() - Vec2::new(8.0, 0.0),
                                    egui::Align2::RIGHT_CENTER,
                                    right,
                                    egui::FontId::monospace(12.0),
                                    fg.gamma_multiply(0.55),
                                );
                                if r.clicked() {
                                    run = Some(item.clone());
                                }
                            }
                        });
                    });
            });
        // Clic en dehors : fermer.
        if ctx.input(|i| i.pointer.any_pressed()) && !ctx.is_pointer_over_area() {
            close = true;
        }
        if let Some(pal) = self.palette.as_mut() {
            pal.query = query;
            pal.selected = selected;
        }
        if let Some(item) = run {
            self.palette = None;
            self.clear_egui_focus();
            self.run_palette_item(item);
        } else if close {
            self.palette = None;
            self.clear_egui_focus();
        }
    }

    fn render_rename(&mut self, ctx: &egui::Context) {
        let Some(r) = self.rename.as_mut() else { return };
        let surface = surface_color(&self.theme);
        let accent = rgb(self.theme.cursor);
        let mut apply = false;
        let mut cancel = false;
        egui::Area::new(Id::new("rename_tab"))
            .order(egui::Order::Foreground)
            .anchor(egui::Align2::CENTER_TOP, Vec2::new(0.0, 80.0))
            .show(ctx, |ui| {
                egui::Frame::NONE
                    .fill(surface)
                    .corner_radius(CornerRadius::same(10))
                    .stroke(Stroke::new(1.0, accent.gamma_multiply(0.6)))
                    .inner_margin(egui::Margin::same(12))
                    .show(ui, |ui| {
                        ui.label(egui::RichText::new(format!("Renommer l'onglet {}", r.tab + 1)).strong());
                        let resp = ui.add(
                            egui::TextEdit::singleline(&mut r.text)
                                .id(rename_input_id())
                                .desired_width(280.0)
                                .hint_text("Vide = titre automatique"),
                        );
                        if r.focus {
                            resp.request_focus();
                            r.focus = false;
                        }
                        ui.horizontal(|ui| {
                            if ui.button("Valider (Entrée)").clicked() {
                                apply = true;
                            }
                            if ui.button("Annuler (Échap)").clicked() {
                                cancel = true;
                            }
                        });
                    });
            });
        if apply {
            self.pending.push(Pending::Ui(UiCmd::RenameApply));
        }
        if cancel {
            self.pending.push(Pending::Ui(UiCmd::RenameCancel));
        }
    }

    fn render_confirm(&mut self, ctx: &egui::Context) {
        let Some(c) = &self.confirm else { return };
        let mut yes = false;
        let mut no = false;
        let accent = rgb(self.theme.cursor);
        egui::Modal::new(Id::new("confirm_modal")).show(ctx, |ui| {
            ui.set_max_width(380.0);
            ui.heading(&c.title);
            ui.add_space(6.0);
            ui.label(&c.message);
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                let primary = egui::Button::new(egui::RichText::new(format!("{} (Entrée)", c.button)).strong())
                    .stroke(Stroke::new(1.0, accent));
                if ui.add(primary).clicked() {
                    yes = true;
                }
                if ui.button("Annuler (Échap)").clicked() {
                    no = true;
                }
            });
        });
        if yes {
            self.confirm_yes();
        } else if no {
            self.confirm = None;
        }
    }

    /// Toasts empilés en bas au centre : entrée par glissement vers le haut, sortie en fondu.
    fn render_toasts(&mut self, ctx: &egui::Context, time: f64) {
        const LIFETIME: f64 = 2.6;
        self.toasts.retain(|t| time - t.born < LIFETIME);
        if self.toasts.is_empty() {
            return;
        }
        let surface = surface_color(&self.theme);
        let fg = rgb(self.theme.foreground);
        let accent = rgb(self.theme.cursor);
        let n = self.toasts.len();
        let start = n.saturating_sub(3);
        let shown = n - start;
        let bottom_gap = if self.config.show_status_bar { 34.0 } else { 16.0 };
        for (idx, t) in self.toasts[start..].iter().enumerate() {
            let age = time - t.born;
            let enter = (age / 0.18).clamp(0.0, 1.0);
            let ease = 1.0 - (1.0 - enter).powi(3);
            let slide = ((1.0 - ease) * 24.0) as f32;
            let mut alpha = ease as f32;
            let remain = LIFETIME - age;
            if remain < 0.35 {
                alpha *= (remain / 0.35).clamp(0.0, 1.0) as f32;
            }
            let stack = (shown - 1 - idx) as f32;
            let offset = Vec2::new(0.0, -(bottom_gap + stack * 42.0) + slide);
            let fill = Color32::from_rgba_unmultiplied(surface.r(), surface.g(), surface.b(), (alpha * 248.0) as u8);
            egui::Area::new(Id::new(("toast", t.born.to_bits())))
                .anchor(egui::Align2::CENTER_BOTTOM, offset)
                .order(egui::Order::Tooltip)
                .interactable(false)
                .show(ctx, |ui| {
                    egui::Frame::NONE
                        .fill(fill)
                        .corner_radius(CornerRadius::same(10))
                        .stroke(Stroke::new(1.0, accent.gamma_multiply(alpha * 0.5)))
                        .inner_margin(egui::Margin::symmetric(14, 8))
                        .show(ui, |ui| {
                            ui.label(egui::RichText::new(&t.message).color(fg.gamma_multiply(alpha)).strong());
                        });
                });
        }
    }

    /// Exécute le script de démo : injecte touches et texte, capture l'écran.
    fn run_demo(&mut self, ctx: &egui::Context, time: f64) {
        let Some(demo) = self.demo.as_mut() else { return };
        ctx.request_repaint_after(Duration::from_millis(50));
        // Capture reçue ?
        let shot = ctx.input(|i| {
            i.events.iter().find_map(|e| match e {
                egui::Event::Screenshot { image, .. } => Some(image.clone()),
                _ => None,
            })
        });
        let received = match (shot, demo.pending_shot.is_some()) {
            (Some(img), true) => demo.pending_shot.take().map(|p| (img, p)),
            _ => None,
        };
        if let Some((img, path)) = received {
            let mut rgba = Vec::with_capacity(img.pixels.len() * 4);
            for p in &img.pixels {
                let [r, g, b, _] = p.to_srgba_unmultiplied();
                rgba.extend_from_slice(&[r, g, b, 255]);
            }
            if let Some(buf) = image::RgbaImage::from_raw(img.size[0] as u32, img.size[1] as u32, rgba) {
                if let Some(parent) = path.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                match buf.save(&path) {
                    Ok(()) => println!("capture : {}", path.display()),
                    Err(e) => eprintln!("NovaTerm: capture impossible: {e}"),
                }
            }
        }
        if demo.pending_shot.is_some() || time < demo.resume_at {
            return;
        }
        let Some(step) = demo.steps.pop_front() else { return };
        match step {
            DemoStep::Wait(s) => demo.resume_at = time + s,
            DemoStep::Type(t) => {
                match self.palette.as_mut() {
                    Some(p) => {
                        p.query.push_str(&t);
                        p.selected = 0;
                    }
                    None => self.pending.push(Pending::Text(t)),
                }
                demo.resume_at = time + 0.15;
            }
            DemoStep::Key(c) => {
                let mods = Modifiers { ctrl: c.ctrl, shift: c.shift, alt: c.alt, command: c.ctrl, ..Default::default() };
                let ctx_keys = KeyContext { searching: self.search.is_some(), prompt_nav: false };
                match self.bindings.find(c.key, None, &mods, &ctx_keys) {
                    Some(action) => self.pending.push(Pending::Action(action)),
                    None => match (self.palette.is_some(), c.key) {
                        (true, Key::Escape) => self.pending.push(Pending::Ui(UiCmd::PaletteClose)),
                        _ => self.pending.push(Pending::Key { key: c.key, mods }),
                    },
                }
                demo.resume_at = time + 0.15;
            }
            DemoStep::Shot(p) => {
                demo.pending_shot = Some(p);
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
            }
            DemoStep::Quit => {
                self.allow_close = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
    }

    /// Bandeau d'information important (mode sans échec, configuration invalide).
    fn render_banner(&mut self, ctx: &egui::Context) {
        let Some(msg) = self.banner.clone() else { return };
        let font_related = msg.contains("olice");
        let warn = rgb(self.theme.ansi[3]);
        let mut close = false;
        let mut open_settings = false;
        egui::TopBottomPanel::top("banner")
            .frame(egui::Frame::NONE.fill(warn.gamma_multiply(0.25)).inner_margin(egui::Margin::symmetric(10, 5)))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.small_button("×").on_hover_text("Fermer").clicked() {
                            close = true;
                        }
                        if font_related && ui.small_button("Paramètres de police").clicked() {
                            open_settings = true;
                            close = true;
                        }
                        ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                            ui.label(egui::RichText::new("⚠").color(warn).strong());
                            ui.add(egui::Label::new(egui::RichText::new(&msg).color(rgb(self.theme.foreground))).wrap());
                        });
                    });
                });
            });
        if open_settings {
            self.settings_open = true;
            self.settings_ui.page = settings::Page::Font;
        }
        if close {
            self.banner = None;
        }
    }

    /// Proposition d'intégration de l'AppImage (premier lancement).
    fn render_appimage_offer(&mut self, ctx: &egui::Context) {
        if self.quake || !self.config.appimage_prompt {
            return;
        }
        let Some(path) = self.appimage_offer.clone() else { return };
        let accent = rgb(self.theme.cursor);
        let mut choice = 0;
        egui::TopBottomPanel::top("appimage_offer")
            .frame(egui::Frame::NONE.fill(accent.gamma_multiply(0.18)).inner_margin(egui::Margin::symmetric(10, 6)))
            .show(ctx, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label(egui::RichText::new("📦").color(accent));
                    ui.label(
                        egui::RichText::new("Ajouter NovaTerm au menu des applications ? Le fichier AppImage sera rangé dans ~/Applications.")
                            .color(rgb(self.theme.foreground)),
                    )
                    .on_hover_text(format!("Emplacement actuel : {}", path.display()));
                    if ui.button("Ranger et ajouter au menu").clicked() {
                        choice = 1;
                    }
                    if ui.button("Ajouter au menu sans déplacer").clicked() {
                        choice = 2;
                    }
                    if ui.small_button("Plus tard").clicked() {
                        choice = 3;
                    }
                    if ui.small_button("Ne plus demander").clicked() {
                        choice = 4;
                    }
                });
            });
        match choice {
            1 | 2 => {
                match crate::appimage::integrate(choice == 1) {
                    Ok(p) => self.toast(format!("NovaTerm est dans le menu ({})", p.display())),
                    Err(e) => self.toast(format!("Intégration impossible : {e}")),
                }
                self.appimage_offer = None;
            }
            3 => self.appimage_offer = None,
            4 => {
                self.config.appimage_prompt = false;
                self.config_dirty = true;
                self.appimage_offer = None;
            }
            _ => {}
        }
    }

    /// Dialogue « Enregistrer la disposition comme profil ».
    fn render_save_profile(&mut self, ctx: &egui::Context) {
        let Some((mut name, focus)) = self.save_profile.take() else { return };
        let mut keep = true;
        let mut save = false;
        egui::Modal::new(Id::new("save_profile")).show(ctx, |ui| {
            ui.set_max_width(380.0);
            ui.heading("Enregistrer comme profil");
            ui.label(format!(
                "Les {} onglet(s) actuels (splits, dossiers, noms) seront enregistrés. Les commandes de démarrage se règlent ensuite dans Paramètres → Session.",
                self.tabs.len()
            ));
            let r = ui.add(egui::TextEdit::singleline(&mut name).hint_text("Nom du profil (ex. GPlus)").desired_width(300.0));
            if focus {
                r.request_focus();
            }
            if r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                save = true;
            }
            ui.horizontal(|ui| {
                if ui.button("Enregistrer").clicked() {
                    save = true;
                }
                if ui.button("Annuler").clicked() || ui.input(|i| i.key_pressed(Key::Escape)) {
                    keep = false;
                }
            });
        });
        if save && !name.trim().is_empty() {
            self.save_profile_named(&name);
            keep = false;
        }
        if keep {
            self.save_profile = Some((name, false));
        } else {
            self.clear_egui_focus();
        }
    }

    /// Rapports de focus (ESC [ I / ESC [ O) pour vim, tmux… quand ils les demandent.
    fn report_focus(&mut self, ctx: &egui::Context) {
        let window_focused = ctx.input(|i| i.focused);
        let now = if window_focused { self.focused_id() } else { None };
        if now == self.reported_focus {
            return;
        }
        if let Some(old) = self.reported_focus.and_then(|id| self.panes.get_mut(&id)) {
            if old.term.mode().contains(TermMode::FOCUS_IN_OUT) {
                old.write_raw(b"\x1b[O");
            }
            old.term.is_focused = false;
            old.dirty = true;
        }
        if let Some(new) = now.and_then(|id| self.panes.get_mut(&id)) {
            if new.term.mode().contains(TermMode::FOCUS_IN_OUT) {
                new.write_raw(b"\x1b[I");
            }
            new.term.is_focused = true;
            new.dirty = true;
        }
        self.reported_focus = now;
    }
}

/// Heure locale (0–23), sans dépendance de fuseau horaire.
fn local_hour() -> u32 {
    let out = if cfg!(windows) {
        std::process::Command::new("powershell").args(["-NoProfile", "-Command", "(Get-Date).Hour"]).output()
    } else {
        std::process::Command::new("date").arg("+%H").output()
    };
    out.ok()
        .and_then(|o| String::from_utf8_lossy(&o.stdout).trim().parse().ok())
        .unwrap_or(12)
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() > max {
        let t: String = s.chars().take(max - 1).collect();
        format!("{t}…")
    } else {
        s.to_string()
    }
}

fn regex_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 2);
    for c in s.chars() {
        if "\\.+*?()|[]{}^$#&-~".contains(c) {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

impl eframe::App for NovaApp {
    /// Interception du clavier avant egui : les raccourcis et la saisie du terminal ne
    /// sont jamais interprétés comme de la navigation dans l'interface.
    fn raw_input_hook(&mut self, ctx: &egui::Context, raw: &mut egui::RawInput) {
        let focused = ctx.memory(|m| m.focused());
        let mut text_focus = focused.is_some_and(|id| egui::text_edit::TextEditState::load(ctx, id).is_some());
        if let (Some(id), false) = (focused, text_focus) {
            // Jamais de focus clavier sur un bouton : Entrée ne peut plus « cliquer » dessus.
            ctx.memory_mut(|m| m.surrender_focus(id));
        }
        let capturing = self.settings_ui.capture.is_some();
        if capturing {
            text_focus = false;
        }
        let mods = raw.modifiers;
        let searching = self.search.is_some();
        let key_ctx = {
            let pane = self.focused_id().and_then(|id| self.panes.get(&id));
            let list_open = self.config.suggest_list && self.suggest.items.len() > 1;
            KeyContext {
                searching,
                prompt_nav: !list_open
                    && pane.is_some_and(|p| !p.blocks.is_empty() && !p.term.mode().contains(TermMode::ALT_SCREEN)),
            }
        };
        let in_search = focused == Some(search_input_id()) && text_focus;
        let in_palette = self.palette.is_some();
        let in_history = self.history_popup.is_some();
        let in_rename = self.rename.is_some();
        let modal = self.confirm.is_some();

        let mut kept = Vec::with_capacity(raw.events.len());
        // egui-winit émet `Text` juste après le `Key` correspondant : si la touche a servi
        // de raccourci (ou part avec Ctrl/Alt), son texte ne doit pas être tapé.
        let mut swallow_text = false;
        for ev in std::mem::take(&mut raw.events) {
            // Ctrl+C / Ctrl+X / Ctrl+V arrivent sous forme d'événements presse-papiers :
            // on les retransforme en touches pour le terminal.
            // egui-winit ne produit ces événements que si Ctrl est enfoncé : on le force,
            // car `raw.modifiers` reflète l'état en fin de frame (Ctrl peut être déjà relâché).
            let cmd_mods = Modifiers { ctrl: true, command: true, ..mods };
            let as_key = |key: Key| egui::Event::Key { key, physical_key: None, pressed: true, repeat: false, modifiers: cmd_mods };
            let ev = if text_focus {
                ev
            } else {
                match ev {
                    egui::Event::Copy => as_key(Key::C),
                    egui::Event::Cut => as_key(Key::X),
                    egui::Event::Paste(text) => {
                        if self.config.ctrl_v_paste && !mods.shift && !capturing {
                            egui::Event::Paste(text)
                        } else {
                            as_key(Key::V)
                        }
                    }
                    other => other,
                }
            };

            match &ev {
                egui::Event::Key { key, physical_key, pressed, modifiers, .. } => {
                    let (key, physical, pressed, kmods) = (*key, *physical_key, *pressed, *modifiers);
                    if pressed {
                        swallow_text = !text_focus || capturing || modal;
                    }
                    if !pressed {
                        if text_focus {
                            kept.push(ev);
                        }
                        continue;
                    }
                    // Enregistrement d'un raccourci (Paramètres → Raccourcis).
                    if let Some(action) = self.settings_ui.capture {
                        if key == Key::Escape && !kmods.any() {
                            self.settings_ui.capture = None;
                        } else {
                            self.settings_ui.captured = Some((action, KeyCombo::new(kmods.ctrl, kmods.shift, kmods.alt, key)));
                            self.settings_ui.capture = None;
                        }
                        continue;
                    }
                    if modal {
                        match key {
                            Key::Enter => self.pending.push(Pending::Ui(UiCmd::ConfirmYes)),
                            Key::Escape => self.pending.push(Pending::Ui(UiCmd::ConfirmNo)),
                            _ => {}
                        }
                        continue;
                    }
                    if in_history {
                        let cmd = match key {
                            Key::ArrowDown => Some(UiCmd::HistoryMove(1)),
                            Key::ArrowUp => Some(UiCmd::HistoryMove(-1)),
                            Key::Tab => Some(UiCmd::HistoryMove(if kmods.shift { -1 } else { 1 })),
                            Key::PageDown => Some(UiCmd::HistoryMove(10)),
                            Key::PageUp => Some(UiCmd::HistoryMove(-10)),
                            Key::Enter => Some(UiCmd::HistoryRun(kmods.shift || kmods.ctrl)),
                            Key::Escape => Some(UiCmd::HistoryClose),
                            _ => None,
                        };
                        if let Some(cmd) = cmd {
                            self.pending.push(Pending::Ui(cmd));
                            swallow_text = true;
                            continue;
                        }
                    }
                    if in_palette {
                        let cmd = match key {
                            Key::ArrowDown => Some(UiCmd::PaletteMove(1)),
                            Key::ArrowUp => Some(UiCmd::PaletteMove(-1)),
                            Key::Tab => Some(UiCmd::PaletteMove(if kmods.shift { -1 } else { 1 })),
                            Key::PageDown => Some(UiCmd::PaletteMove(8)),
                            Key::PageUp => Some(UiCmd::PaletteMove(-8)),
                            Key::Enter => Some(UiCmd::PaletteRun),
                            Key::Escape => Some(UiCmd::PaletteClose),
                            _ => None,
                        };
                        if let Some(cmd) = cmd {
                            self.pending.push(Pending::Ui(cmd));
                            continue;
                        }
                    }
                    if in_rename {
                        match key {
                            Key::Enter => {
                                self.pending.push(Pending::Ui(UiCmd::RenameApply));
                                continue;
                            }
                            Key::Escape => {
                                self.pending.push(Pending::Ui(UiCmd::RenameCancel));
                                continue;
                            }
                            Key::Tab => continue,
                            _ => {}
                        }
                    }
                    if in_search {
                        match key {
                            Key::Enter => {
                                self.pending.push(Pending::Ui(UiCmd::SearchNext(kmods.shift)));
                                continue;
                            }
                            Key::Escape => {
                                self.pending.push(Pending::Ui(UiCmd::SearchClose));
                                continue;
                            }
                            Key::Tab => continue,
                            _ => {}
                        }
                    }
                    if let Some(action) = self.bindings.find(key, physical, &kmods, &key_ctx) {
                        if !text_focus || Self::allowed_in_text_field(action, &kmods, key) {
                            self.pending.push(Pending::Action(action));
                            swallow_text = true;
                            continue;
                        }
                    }
                    if text_focus {
                        // Tab ne doit jamais sortir d'un champ vers un bouton.
                        if key != Key::Tab {
                            kept.push(ev);
                        }
                        continue;
                    }
                    // Touche « simple » : son texte (événement suivant) sera tapé normalement.
                    swallow_text = kmods.ctrl || kmods.alt || kmods.command;
                    self.pending.push(Pending::Key { key, mods: kmods });
                }
                egui::Event::Text(text) => {
                    if std::mem::take(&mut swallow_text) {
                        if text_focus && !capturing && !modal {
                            kept.push(ev);
                        }
                    } else if text_focus {
                        kept.push(ev);
                    } else if !capturing && !modal {
                        self.pending.push(Pending::Text(text.clone()));
                    }
                }
                egui::Event::Ime(egui::ImeEvent::Commit(text)) if !text_focus => {
                    if !modal && !text.is_empty() {
                        self.pending.push(Pending::Text(text.clone()));
                    }
                }
                egui::Event::Paste(text) => {
                    if text_focus {
                        kept.push(ev);
                    } else if !modal {
                        self.pending.push(Pending::Paste(text.clone()));
                    }
                }
                egui::Event::Copy | egui::Event::Cut => {
                    if text_focus {
                        kept.push(ev);
                    }
                }
                _ => kept.push(ev),
            }
        }
        raw.events = kept;
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let time = ctx.input(|i| i.time);

        // Fermeture de la fenêtre (bouton du système, Alt+F4…) : confirmation si besoin.
        if ctx.input(|i| i.viewport().close_requested()) && !self.allow_close {
            let ids: Vec<u64> = self.panes.keys().copied().collect();
            let busy = if self.config.confirm_close { self.busy(&ids) } else { Vec::new() };
            if !busy.is_empty() {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                self.ask_confirm(ConfirmKind::CloseWindow, busy);
            }
        }

        self.frames += 1;
        if self.guard_frames.is_some_and(|n| self.frames >= n) {
            guard::disarm();
            self.guard_frames = None;
        }
        if !self.pending.is_empty() {
            self.last_input_time = time;
        }
        let focused = self.focused_id();
        let blink_alive = ctx.input(|i| i.focused)
            && (self.config.cursor_blink_timeout == 0 || time - self.last_input_time < self.config.cursor_blink_timeout as f64);
        if self.config.cursor_blink && focused.is_some() && blink_alive {
            let interval = match self.config.cursor_blink_style {
                BlinkStyle::Blink => 250,
                _ => 30,
            };
            ctx.request_repaint_after(Duration::from_millis(interval));
        }
        if time < self.bell_until {
            ctx.request_repaint_after(Duration::from_millis(100));
        }
        if !self.toasts.is_empty() {
            ctx.request_repaint_after(Duration::from_millis(16));
        }

        let mut changed = false;
        let mut bell = false;
        for pane in self.panes.values_mut() {
            if pane.drain(&self.theme) {
                changed = true;
            }
            if pane.bell {
                pane.bell = false;
                bell = true;
            }
        }
        if bell {
            match self.config.bell {
                BellMode::None => {}
                BellMode::Visual => self.bell_until = time + 0.25,
                BellMode::Urgent => {
                    self.bell_until = time + 0.25;
                    if !ctx.input(|i| i.focused) {
                        ctx.send_viewport_cmd(egui::ViewportCommand::RequestUserAttention(egui::UserAttentionType::Informational));
                    }
                }
            }
        }

        // Fermeture automatique des panneaux dont le shell est terminé.
        if self.config.close_on_exit {
            let exited: Vec<u64> = self.panes.iter().filter(|(_, p)| p.exited).map(|(id, _)| *id).collect();
            for id in exited {
                self.remove_pane(id);
            }
        }
        if self.tabs.is_empty() {
            return;
        }

        self.metrics = render::compute_metrics(ctx, &self.config);
        self.run_demo(ctx, time);
        self.process_pending();
        if self.tabs.is_empty() {
            return;
        }
        self.apply_config_changes();
        self.update_suggestions();
        self.report_focus(ctx);
        self.process_finished(ctx);
        self.update_auto_theme(time);
        self.update_quake(ctx);
        if let Some(tab) = self.tabs.get_mut(self.active) {
            tab.activity = None;
        }
        if self.config.autosave_secs > 0 && time - self.last_autosave > self.config.autosave_secs as f64 {
            self.last_autosave = time;
            if self.frames > 10 {
                self.save_session();
            }
        }

        // Taille de fenêtre mémorisée (hors maximisé / plein écran).
        if !self.is_maximized() {
            if let Some(r) = ctx.input(|i| i.viewport().inner_rect) {
                self.window_size = r.size();
            }
        }

        let window_title = if self.config.custom_title.is_empty() {
            format!("NovaTerm — {}", self.active_pane_title())
        } else {
            self.config.custom_title.clone()
        };
        if window_title != self.window_title {
            self.window_title = window_title.clone();
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(window_title));
        }

        self.render_window_bg(ctx);
        if self.config.custom_decorations && !self.fullscreen && !self.quake {
            self.render_title_bar(ctx);
        }
        self.render_banner(ctx);
        self.render_appimage_offer(ctx);
        self.render_tab_bar(ctx);
        self.render_status_bar(ctx, time);
        self.render_central(ctx, time);
        self.render_resize_handles(ctx);
        self.render_toasts(ctx, time);

        let mut out = settings::SettingsOut::default();
        if self.settings_open {
            let before = format!("{:?}", self.config);
            if self.settings_ui.font_scan_requested && self.font_catalog.is_none() {
                self.font_catalog = Some(fonts::discover());
            }
            let info = {
                let fp = self.focused_id().and_then(|id| self.panes.get(&id));
                settings::SettingsInfo {
                    history_len: self.history.len(),
                    history_loading: self.history_rx.is_some(),
                    integration_active: fp.is_some_and(|p| p.integration),
                    suggest_conflict: fp.is_some_and(|p| p.suggest_conflict),
                    shell_name: if self.config.shell.trim().is_empty() {
                        platform::default_shell()
                    } else {
                        self.config.shell.clone()
                    },
                    tabs: self.tabs.len(),
                }
            };
            settings::show(
                ctx,
                &mut self.config,
                &mut self.settings_open,
                &mut self.settings_ui,
                &self.bindings,
                self.font_catalog.as_ref(),
                &info,
                &mut out,
            );
            if before != format!("{:?}", self.config) {
                self.config_dirty = true;
            }
        } else {
            self.settings_ui.capture = None;
        }
        if out.browse {
            self.file_browser = Some(settings::FileBrowser::new());
        }
        if out.help {
            self.help_open = true;
        }
        if let Some(a) = out.action {
            self.run_action(a);
        }
        if let Some(i) = out.open_profile {
            self.open_profile(i);
        }
        if let Some(i) = out.run_snippet {
            self.run_snippet(i);
        }
        if out.install_quake_shortcut {
            let note = match quake::install_shortcut(&self.config.quake_hotkey) {
                Ok(m) => m,
                Err(e) => e,
            };
            self.settings_ui.quake_note = Some(note);
        }
        if self.file_browser.is_some() && settings::show_file_browser(ctx, &mut self.file_browser, &mut self.config) {
            self.config_dirty = true;
        }
        if self.help_open {
            let mut run = None;
            settings::show_help(ctx, &mut self.help_open, &self.bindings, &mut self.settings_ui.help_filter, &mut run);
            if let Some(a) = run {
                self.run_action(a);
            }
        }
        self.render_palette(ctx);
        self.render_save_profile(ctx);
        if self.theme_import.is_some() {
            if let Some(path) = settings::show_file_picker(ctx, &mut self.theme_import, "Importer un thème", settings::is_theme_file) {
                match theme::import_file(&path) {
                    Ok(mut t) => {
                        let base = t.name.clone();
                        let mut n = 2;
                        while self.config.theme_names().contains(&t.name) {
                            t.name = format!("{base} {n}");
                            n += 1;
                        }
                        self.config.theme_name = t.name.clone();
                        self.config.custom_theme = None;
                        self.toast(format!("Thème « {} » importé", t.name));
                        self.config.user_themes.push(t);
                        self.config_dirty = true;
                    }
                    Err(e) => self.toast(format!("Import impossible : {e}")),
                }
            }
        }
        self.render_history_popup(ctx);
        self.render_rename(ctx);
        self.render_confirm(ctx);

        if changed {
            ctx.request_repaint();
        }
        if self.config.cursor_smooth && self.panes.values().any(|p| p.cursor_moving) {
            ctx.request_repaint();
        }
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.save_session();
        guard::disarm();
        if self.quake {
            return;
        }
        if self.config.remember_window_size && self.window_size.x > 100.0 && self.window_size.y > 100.0 {
            self.config.window_width = self.window_size.x.round();
            self.config.window_height = self.window_size.y.round();
        }
        self.config.save();
    }

    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        if self.config.custom_decorations {
            // Fond transparent : le rect arrondi peint dans update() fait le fond.
            return [0.0; 4];
        }
        let b = self.theme.background;
        Color32::from_rgba_unmultiplied(b[0], b[1], b[2], (self.config.opacity * 255.0) as u8).to_normalized_gamma_f32()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fuzzy() {
        assert!(fuzzy_match("div dro", "Diviser à droite"));
        assert!(!fuzzy_match("xyz", "Diviser à droite"));
    }
}
