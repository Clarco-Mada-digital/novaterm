//! Un panneau de terminal : émulation VT (alacritty_terminal) + PTY (portable-pty),
//! blocs de commandes, images en ligne et intégration shell.

use std::collections::{HashMap, VecDeque};
use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::thread;
use std::time::Instant;

use alacritty_terminal::event::{Event, EventListener, WindowSize};
use alacritty_terminal::grid::{Dimensions, Scroll};
use alacritty_terminal::index::{Column, Line, Point};
use alacritty_terminal::term::color::Colors;
use alacritty_terminal::term::{Config as TermConfig, Term};
use alacritty_terminal::vte::ansi::{CursorStyle, Processor, Rgb};
use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtySize};

use crate::blocks::{self, Block, Finished, MarkKind, VisibleBlock};
use crate::config::Config;
use crate::images::{self, KittyState, Placement};
use crate::platform;
use crate::shell::{self, ShellMsg};
use crate::stream::{Chunk, SeqFilter, SeqKind};
use crate::theme::{dim, indexed_default, Theme};

struct Dims {
    columns: usize,
    screen_lines: usize,
}

impl Dimensions for Dims {
    fn total_lines(&self) -> usize {
        self.screen_lines
    }
    fn screen_lines(&self) -> usize {
        self.screen_lines
    }
    fn columns(&self) -> usize {
        self.columns
    }
}

enum Msg {
    Bytes(Vec<u8>),
    Term(Event),
    Eof,
}

#[derive(Clone)]
pub struct Proxy {
    tx: Sender<Msg>,
    ctx: egui::Context,
}

impl EventListener for Proxy {
    fn send_event(&self, event: Event) {
        let _ = self.tx.send(Msg::Term(event));
        self.ctx.request_repaint();
    }
}

/// Options du panneau modifiables à chaud.
#[derive(Clone, Copy)]
pub struct PaneOpts {
    pub images: bool,
    pub image_files: bool,
    pub app_notify: bool,
    /// Les programmes peuvent écrire dans le presse-papiers (OSC 52).
    pub clipboard_write: bool,
    /// Les programmes peuvent LIRE le presse-papiers (OSC 52) : désactivé par défaut.
    pub clipboard_read: bool,
}

impl PaneOpts {
    pub fn from(config: &Config) -> Self {
        Self {
            images: config.images_enabled,
            image_files: config.images_allow_files,
            app_notify: config.notify_from_apps,
            clipboard_write: config.osc52_write,
            clipboard_read: config.osc52_read,
        }
    }
}

/// Mémoire maximale des images d'un panneau (en pixels, ≈ 256 Mo).
const MAX_PANE_IMAGE_PIXELS: usize = 64 * 1024 * 1024;

/// Image affichée dans le terminal.
pub struct InlineImage {
    pub image: Arc<egui::ColorImage>,
    pub cols: usize,
    pub rows: usize,
    pub texture: Option<egui::TextureHandle>,
}

pub struct Pane {
    pub term: Term<Proxy>,
    parser: Processor,
    writer: Box<dyn Write + Send>,
    child: Box<dyn Child + Send + Sync>,
    master: Box<dyn MasterPty + Send>,
    rx: Receiver<Msg>,
    ctx: egui::Context,
    pub title: String,
    pub exited: bool,
    /// Code de sortie du shell (si connu).
    pub exit_code: Option<i32>,
    pub dirty: bool,
    pub bell: bool,
    pub cols: usize,
    pub rows: usize,
    pub cell_w: f32,
    pub cell_h: f32,
    pub scroll_accum: f32,
    pub selecting: bool,
    /// Formes mises en cache par ligne : (empreinte, formes).
    pub row_cache: Vec<(u64, Vec<egui::Shape>)>,
    /// Clé de validité du cache (position, police, thème…).
    pub cache_key: u64,
    /// Formes de l'écran complet (concaténation du cache par ligne).
    pub cached: Vec<egui::Shape>,
    /// Position pixel affichée du curseur (animée), et état du déplacement fluide.
    pub cursor_display: egui::Pos2,
    pub cursor_from: egui::Pos2,
    pub cursor_move_start: f64,
    pub cursor_prev_point: Option<(i32, usize)>,
    pub cursor_moving: bool,
    pub opts: PaneOpts,
    filter: SeqFilter,
    /// Ligne en cours d'édition au prompt : (position du curseur, texte).
    /// `None` hors édition ou dès qu'une touche est envoyée (en attente du shell).
    pub cmdline: Option<(usize, String)>,
    /// L'intégration shell a répondu au moins une fois.
    pub integration: bool,
    /// Un autre plugin de suggestions est actif dans ce shell.
    pub suggest_conflict: bool,
    /// Commandes exécutées depuis le dernier passage (pour l'historique).
    pub executed: Vec<String>,
    /// Dossier courant signalé par le shell.
    pub cwd_reported: Option<PathBuf>,
    // --- Blocs de commandes ---
    pub blocks: Vec<Block>,
    pub running: Option<u32>,
    /// Ligne absolue du début du prompt courant.
    prompt_abs: Option<i64>,
    next_mark: u32,
    /// Commandes terminées à signaler (notifications).
    pub finished: Vec<Finished>,
    /// Notifications demandées par les programmes (titre, texte).
    pub notifications: Vec<(String, String)>,
    pub vis_blocks: Vec<VisibleBlock>,
    // --- Images ---
    pub images: HashMap<u32, InlineImage>,
    image_order: VecDeque<u32>,
    kitty: KittyState,
    /// Images visibles : (ligne écran, colonne, id).
    pub vis_images: Vec<(i32, usize, u32)>,
    /// Dernière activité (sortie du programme), pour l'indicateur d'onglet.
    pub last_output: Instant,
    /// Astuces calculées pour les blocs en échec (id du bloc → astuces).
    pub tips: HashMap<u32, Vec<crate::helpers::Tip>>,
    /// Dernière notification demandée par un programme (anti-spam).
    last_app_notify: Option<Instant>,
}

impl Pane {
    pub fn spawn(cols: usize, rows: usize, config: &Config, cwd: Option<PathBuf>, ctx: egui::Context) -> anyhow::Result<Self> {
        let system = native_pty_system();
        let size = PtySize { rows: rows as u16, cols: cols as u16, pixel_width: 0, pixel_height: 0 };
        let pair = system.openpty(size)?;

        let custom = config.shell.trim();
        let mut words = platform::split_args(custom);
        let shell_path = if words.is_empty() { platform::default_shell() } else { words.remove(0) };
        let launch = if config.shell_integration { shell::launch_for(&shell_path) } else { shell::Launch::default() };
        let mut cmd = if !launch.args.is_empty() {
            let mut c = CommandBuilder::new(&shell_path);
            c.args(&launch.args);
            c
        } else if !custom.is_empty() {
            let mut c = CommandBuilder::new(&shell_path);
            c.args(&words);
            c
        } else if cfg!(unix) {
            CommandBuilder::new_default_prog()
        } else {
            CommandBuilder::new(&shell_path)
        };
        cmd.env("TERM", "xterm-256color");
        cmd.env("COLORTERM", "truecolor");
        cmd.env("TERM_PROGRAM", "NovaTerm");
        cmd.env("TERM_PROGRAM_VERSION", env!("CARGO_PKG_VERSION"));
        for (k, v) in &launch.env {
            cmd.env(k, v);
        }
        if let Some(dir) = cwd {
            cmd.cwd(dir);
        } else if let Some(home) = dirs::home_dir() {
            cmd.cwd(home);
        }

        let child = pair.slave.spawn_command(cmd)?;
        // Fermer le slave côté parent pour propager le EOF quand l'enfant meurt.
        drop(pair.slave);

        let reader = pair.master.try_clone_reader()?;
        let writer = pair.master.take_writer()?;

        let (tx, rx) = mpsc::channel();
        let proxy = Proxy { tx: tx.clone(), ctx: ctx.clone() };
        let dims = Dims { columns: cols, screen_lines: rows };
        let term = Term::new(term_config(config), &dims, proxy);

        let thread_ctx = ctx.clone();
        thread::spawn(move || {
            let mut reader = reader;
            let mut buf = vec![0u8; 64 * 1024];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) | Err(_) => {
                        let _ = tx.send(Msg::Eof);
                        thread_ctx.request_repaint();
                        break;
                    }
                    Ok(n) => {
                        if tx.send(Msg::Bytes(buf[..n].to_vec())).is_err() {
                            break;
                        }
                        thread_ctx.request_repaint();
                    }
                }
            }
        });

        Ok(Self {
            term,
            parser: Processor::new(),
            writer,
            child,
            master: pair.master,
            rx,
            ctx,
            title: "shell".to_string(),
            exited: false,
            exit_code: None,
            dirty: true,
            bell: false,
            cols,
            rows,
            cell_w: 8.0,
            cell_h: 16.0,
            scroll_accum: 0.0,
            selecting: false,
            row_cache: Vec::new(),
            cache_key: 0,
            cached: Vec::new(),
            cursor_display: egui::Pos2::ZERO,
            cursor_from: egui::Pos2::ZERO,
            cursor_move_start: 0.0,
            cursor_prev_point: None,
            cursor_moving: false,
            opts: PaneOpts::from(config),
            filter: SeqFilter::default(),
            cmdline: None,
            integration: false,
            suggest_conflict: false,
            executed: Vec::new(),
            cwd_reported: None,
            blocks: Vec::new(),
            running: None,
            prompt_abs: None,
            next_mark: 1,
            finished: Vec::new(),
            notifications: Vec::new(),
            vis_blocks: Vec::new(),
            images: HashMap::new(),
            image_order: VecDeque::new(),
            kitty: KittyState::default(),
            vis_images: Vec::new(),
            last_output: Instant::now(),
            last_app_notify: None,
            tips: HashMap::new(),
        })
    }

    /// Affiche un texte restauré (session précédente) avant la sortie du shell.
    pub fn inject_restored(&mut self, text: &str) {
        let body = text.replace("\r\n", "\n").replace('\n', "\r\n");
        let payload = format!("\x1b[2m{body}\r\n\x1b[0;2m─── session restaurée ───\x1b[0m\r\n");
        self.parser.advance(&mut self.term, payload.as_bytes());
        self.dirty = true;
    }

    /// Consomme les messages du PTY et du terminal. Retourne true si quelque chose a changé.
    pub fn drain(&mut self, theme: &Theme) -> bool {
        let mut changed = false;
        // Borne le travail par frame pour garder l'interface fluide sous un flot de sortie.
        let mut budget = 4 * 1024 * 1024usize;
        while budget > 0 {
            let Ok(msg) = self.rx.try_recv() else { break };
            changed = true;
            match msg {
                Msg::Bytes(bytes) => {
                    budget = budget.saturating_sub(bytes.len());
                    for chunk in self.filter.feed(&bytes) {
                        match chunk {
                            Chunk::Data(d) => self.parser.advance(&mut self.term, &d),
                            Chunk::Seq(kind, body) => self.handle_seq(kind, &body),
                        }
                    }
                    self.last_output = Instant::now();
                    self.dirty = true;
                }
                Msg::Eof => {
                    self.exited = true;
                    self.dirty = true;
                }
                Msg::Term(event) => self.handle_event(event, theme),
            }
        }
        if budget == 0 {
            self.ctx.request_repaint();
        }
        changed
    }

    fn handle_seq(&mut self, kind: SeqKind, body: &[u8]) {
        match kind {
            SeqKind::Nova => {
                let s = String::from_utf8_lossy(body).into_owned();
                self.handle_shell_msg(&s);
            }
            SeqKind::Cwd => {
                if let Some(p) = shell::parse_osc7(&String::from_utf8_lossy(body)) {
                    self.cwd_reported = Some(PathBuf::from(p));
                }
            }
            SeqKind::Notify777 => {
                if self.opts.app_notify {
                    let s = String::from_utf8_lossy(body);
                    let mut parts = s.splitn(3, ';');
                    if parts.next() == Some("notify") {
                        let title = parts.next().unwrap_or("NovaTerm").to_string();
                        let text = parts.next().unwrap_or("").to_string();
                        self.push_app_notification(title, text);
                    }
                }
            }
            SeqKind::Notify9 => {
                let s = String::from_utf8_lossy(body);
                // « 9;4;… » = barre de progression (ConEmu), pas une notification.
                if self.opts.app_notify && !s.starts_with("4;") && !s.is_empty() {
                    self.push_app_notification("NovaTerm".into(), s.into_owned());
                }
            }
            SeqKind::ITerm => {
                if self.opts.images {
                    match images::parse_iterm(body) {
                        Ok(Some(p)) => self.place_image(p),
                        Ok(None) => {}
                        Err(e) => self.print_error(&format!("image : {e}")),
                    }
                }
            }
            SeqKind::Kitty => {
                if self.opts.images {
                    let r = self.kitty.handle(body, self.opts.image_files);
                    if r.delete_all {
                        self.images.clear();
                        self.image_order.clear();
                    }
                    if let Some(p) = r.place {
                        self.place_image(p);
                    }
                    if let Some(reply) = r.reply {
                        self.write_raw(reply.as_bytes());
                    }
                } else if body.windows(3).any(|w| w == b"a=q") {
                    // Pas de réponse = images non prises en charge (les outils basculent en texte).
                }
            }
        }
    }

    /// Au plus une notification de programme toutes les 2 secondes.
    fn push_app_notification(&mut self, title: String, body: String) {
        if self.last_app_notify.is_some_and(|t| t.elapsed().as_secs_f32() < 2.0) {
            return;
        }
        self.last_app_notify = Some(Instant::now());
        let clip = |s: String, n: usize| s.chars().filter(|c| !c.is_control()).take(n).collect::<String>();
        self.notifications.push((clip(title, 80), clip(body, 300)));
    }

    fn print_error(&mut self, msg: &str) {
        let s = format!("\r\n\x1b[2;31m[NovaTerm] {msg}\x1b[0m\r\n");
        self.parser.advance(&mut self.term, s.as_bytes());
    }

    fn abs_line(&self, line: Line) -> i64 {
        self.term.grid().history_size() as i64 + line.0 as i64
    }

    fn mark(&mut self, line: Line, col: usize, kind: MarkKind, id: u32) {
        let cols = self.term.columns();
        let grid = self.term.grid_mut();
        if line < grid.topmost_line() || line > grid.bottommost_line() {
            return;
        }
        let cell = &mut grid[line][Column(col.min(cols.saturating_sub(1)))];
        for c in blocks::encode(kind, id) {
            cell.push_zerowidth(c);
        }
    }

    fn handle_shell_msg(&mut self, payload: &str) {
        self.integration = true;
        match shell::parse(payload) {
            Some(ShellMsg::Line { cursor, buffer }) => self.cmdline = Some((cursor, buffer)),
            Some(ShellMsg::EndLine) => self.cmdline = None,
            Some(ShellMsg::PromptStart) => {
                self.cmdline = None;
                let line = self.term.grid().cursor.point.line;
                self.prompt_abs = Some(self.abs_line(line));
            }
            Some(ShellMsg::Executed(cmd)) => {
                self.cmdline = None;
                // La commande a été validée : le curseur est en début de ligne suivante.
                let cur = self.term.grid().cursor.point;
                let line = if cur.column.0 == 0 { cur.line - 1 } else { cur.line };
                let id = self.next_mark;
                self.next_mark += 1;
                let prompt_rows = self
                    .prompt_abs
                    .map(|a| (self.abs_line(line) - a + 1).clamp(1, 20) as usize)
                    .unwrap_or(1);
                self.mark(line, 0, MarkKind::Block, id);
                self.blocks.push(Block { id, command: cmd.clone(), started: Instant::now(), finished: None, prompt_rows });
                if self.blocks.len() > 5000 {
                    self.blocks.drain(..1000);
                }
                self.running = Some(id);
                self.prompt_abs = None;
                if self.executed.len() < 256 {
                    self.executed.push(cmd);
                }
            }
            Some(ShellMsg::Done(code)) => {
                if let Some(id) = self.running.take() {
                    if let Some(b) = self.blocks.iter_mut().rev().find(|b| b.id == id) {
                        let d = b.started.elapsed();
                        b.finished = Some((code, d));
                        self.finished.push(Finished { id, command: b.command.clone(), exit: code, duration: d });
                    }
                }
            }
            Some(ShellMsg::Cwd(p)) => self.cwd_reported = Some(PathBuf::from(p)),
            Some(ShellMsg::Conflict) => self.suggest_conflict = true,
            None => {}
        }
    }

    fn place_image(&mut self, p: Placement) {
        let max_cols = self.cols.max(1);
        let (cols, rows) = images::cells_for(&p, self.cell_w, self.cell_h, max_cols, self.rows);
        let id = self.next_mark;
        self.next_mark += 1;
        let cur = self.term.grid().cursor.point;
        self.mark(cur.line, cur.column.0, MarkKind::Image, id);
        self.images.insert(id, InlineImage { image: p.image, cols, rows, texture: None });
        self.image_order.push_back(id);
        // Plafonds : nombre d'images et mémoire totale (les plus anciennes partent).
        let pixels = |imgs: &HashMap<u32, InlineImage>| imgs.values().map(|i| i.image.size[0] * i.image.size[1]).sum::<usize>();
        while self.image_order.len() > 100 || (self.image_order.len() > 1 && pixels(&self.images) > MAX_PANE_IMAGE_PIXELS) {
            if let Some(old) = self.image_order.pop_front() {
                self.images.remove(&old);
            }
        }
        if p.keep_cursor {
            return;
        }
        let moves = if p.iterm_cursor {
            format!("{}\r", "\n".repeat(rows))
        } else {
            format!("{}\x1b[{}C", "\n".repeat(rows.saturating_sub(1)), cols)
        };
        self.parser.advance(&mut self.term, moves.as_bytes());
    }

    pub fn block(&self, id: u32) -> Option<&Block> {
        self.blocks.binary_search_by_key(&id, |b| b.id).ok().map(|i| &self.blocks[i])
    }

    fn block_marks_in(&self, from: Line, to: Line) -> Vec<(Line, u32)> {
        let grid = self.term.grid();
        let mut out = Vec::new();
        let mut l = from.max(grid.topmost_line());
        let to = to.min(grid.bottommost_line());
        while l <= to {
            if let Some(z) = grid[l][Column(0)].zerowidth() {
                for (k, id) in blocks::decode_all(z) {
                    if k == MarkKind::Block {
                        out.push((l, id));
                    }
                }
            }
            l += 1;
        }
        out
    }

    /// Haut du prompt courant (pour terminer le dernier bloc).
    fn current_prompt_top(&self) -> Line {
        let cur = self.term.grid().cursor.point.line;
        match self.prompt_abs {
            Some(a) => Line((a - self.term.grid().history_size() as i64) as i32).min(cur),
            None => cur,
        }
    }

    fn block_end(&self, id: u32, next: Option<(Line, u32)>) -> Line {
        match next {
            Some((l, nid)) => {
                let rows = self.block(nid).map(|b| b.prompt_rows).unwrap_or(1) as i32;
                l - rows
            }
            None if self.running == Some(id) => self.term.grid().cursor.point.line,
            None => self.current_prompt_top() - 1,
        }
    }

    /// Recalcule les blocs et images visibles (appelé quand l'écran change).
    pub fn refresh_marks(&mut self) {
        let offset = self.term.grid().display_offset() as i32;
        let rows = self.term.screen_lines() as i32;
        let view_top = Line(-offset);
        let view_bottom = Line(rows - 1 - offset);
        self.vis_blocks.clear();
        if !self.blocks.is_empty() {
            // Le bloc qui couvre le haut de l'écran peut commencer bien plus haut.
            let mut marks = Vec::new();
            let topmost = self.term.grid().topmost_line();
            let mut l = view_top - 1;
            let mut scanned = 0;
            // Borné : pendant un gros affichage, ne pas parcourir tout l'historique à chaque frame.
            while l >= topmost && scanned < 5_000 {
                if let Some(m) = self.block_marks_in(l, l).pop() {
                    marks.push(m);
                    break;
                }
                l -= 1;
                scanned += 1;
            }
            marks.extend(self.block_marks_in(view_top, view_bottom));
            // Le suivant après l'écran sert à connaître la fin du dernier visible.
            let after = self.block_marks_in(view_bottom + 1, view_bottom + 60).into_iter().next();
            for i in 0..marks.len() {
                let (line, id) = marks[i];
                let Some(b) = self.block(id) else { continue };
                let next = marks.get(i + 1).copied().or(after);
                let end = self.block_end(id, next);
                let top = line - (b.prompt_rows as i32 - 1);
                self.vis_blocks.push(VisibleBlock {
                    id,
                    top_row: top.0 + offset,
                    cmd_row: line.0 + offset,
                    end_row: end.0.max(line.0) + offset,
                });
            }
        }
        self.vis_images.clear();
        if !self.images.is_empty() {
            let max_rows = self.images.values().map(|i| i.rows).max().unwrap_or(0) as i32;
            let grid = self.term.grid();
            let cols = self.term.columns();
            let mut l = (view_top - max_rows).max(grid.topmost_line());
            while l <= view_bottom {
                for c in 0..cols {
                    if let Some(z) = grid[l][Column(c)].zerowidth() {
                        for (k, id) in blocks::decode_all(z) {
                            if k == MarkKind::Image && self.images.contains_key(&id) {
                                self.vis_images.push((l.0 + offset, c, id));
                            }
                        }
                    }
                }
                l += 1;
            }
        }
    }

    /// Lignes du haut de chaque bloc (pour naviguer entre les commandes).
    pub fn block_tops(&self) -> Vec<Line> {
        let grid = self.term.grid();
        self.block_marks_in(grid.topmost_line(), grid.bottommost_line())
            .into_iter()
            .map(|(l, id)| l - (self.block(id).map(|b| b.prompt_rows).unwrap_or(1) as i32 - 1))
            .collect()
    }

    /// Fait défiler pour placer `line` en haut de l'écran.
    pub fn scroll_to_line(&mut self, line: Line) {
        let history = self.term.grid().history_size() as i32;
        let desired = (-line.0).clamp(0, history);
        let current = self.term.grid().display_offset() as i32;
        self.term.scroll_display(Scroll::Delta(desired - current));
        self.dirty = true;
    }

    fn lines_text(&self, start: Line, end: Line) -> String {
        let cols = self.term.columns();
        if end < start {
            return String::new();
        }
        let raw = self.term.bounds_to_string(Point::new(start, Column(0)), Point::new(end, Column(cols.saturating_sub(1))));
        let mut lines: Vec<String> = blocks::strip_tags(&raw).lines().map(|l| l.trim_end().to_string()).collect();
        while lines.last().is_some_and(|l| l.is_empty()) {
            lines.pop();
        }
        while lines.first().is_some_and(|l| l.is_empty()) {
            lines.remove(0);
        }
        lines.join("\n")
    }

    /// Sortie d'un bloc (texte), si son marqueur est encore dans l'historique.
    pub fn block_output(&self, id: u32) -> Option<String> {
        let grid = self.term.grid();
        let marks = self.block_marks_in(grid.topmost_line(), grid.bottommost_line());
        let i = marks.iter().position(|(_, m)| *m == id)?;
        let (line, _) = marks[i];
        let end = self.block_end(id, marks.get(i + 1).copied());
        Some(self.lines_text(line + 1, end))
    }

    /// Dernières lignes de l'écran (pour la restauration de session).
    pub fn tail_text(&self, max_lines: usize) -> String {
        let grid = self.term.grid();
        let end = grid.cursor.point.line;
        let start = (end - max_lines as i32).max(grid.topmost_line());
        self.lines_text(start, end)
    }

    fn handle_event(&mut self, event: Event, theme: &Theme) {
        match event {
            Event::Title(title) => {
                self.title = title;
                self.dirty = true;
            }
            Event::ResetTitle => {
                self.title = "shell".to_string();
                self.dirty = true;
            }
            Event::PtyWrite(text) => {
                let _ = self.writer.write_all(text.as_bytes());
                let _ = self.writer.flush();
            }
            Event::TextAreaSizeRequest(f) => {
                let size = WindowSize {
                    num_lines: self.rows as u16,
                    num_cols: self.cols as u16,
                    cell_width: self.cell_w as u16,
                    cell_height: self.cell_h as u16,
                };
                let response = f(size);
                let _ = self.writer.write_all(response.as_bytes());
            }
            Event::ColorRequest(index, f) => {
                let response = f(self.resolve_index(index, theme));
                let _ = self.writer.write_all(response.as_bytes());
            }
            Event::ClipboardStore(_, text) => {
                if self.opts.clipboard_write {
                    if let Ok(mut clipboard) = arboard::Clipboard::new() {
                        let _ = clipboard.set_text(text);
                    }
                }
            }
            Event::ClipboardLoad(_, f) => {
                // Sécurité : un programme (même distant, via SSH) ne lit le presse-papiers
                // que si l'utilisateur l'a autorisé.
                if self.opts.clipboard_read {
                    let text = arboard::Clipboard::new().ok().and_then(|mut c| c.get_text().ok()).unwrap_or_default();
                    let response = f(&text);
                    let _ = self.writer.write_all(response.as_bytes());
                }
            }
            Event::Bell => {
                self.bell = true;
                self.dirty = true;
            }
            Event::Exit => self.exited = true,
            Event::ChildExit(code) => {
                self.exited = true;
                self.exit_code = Some(code);
            }
            Event::Wakeup | Event::MouseCursorDirty | Event::CursorBlinkingChange => {
                self.dirty = true;
            }
        }
    }

    fn resolve_index(&self, index: usize, theme: &Theme) -> Rgb {
        let colors: &Colors = self.term.colors();
        let c = if let Some(rgb) = colors[index] {
            [rgb.r, rgb.g, rgb.b]
        } else {
            match index {
                0..=15 => theme.ansi[index],
                256 => theme.foreground,
                257 => theme.background,
                258 => theme.cursor,
                259..=266 => dim(theme.ansi[index - 259]),
                267 => theme.ansi[15],
                268 => dim(theme.background),
                _ => indexed_default(index, theme),
            }
        };
        Rgb { r: c[0], g: c[1], b: c[2] }
    }

    pub fn write_input(&mut self, bytes: &[u8]) {
        if self.exited {
            return;
        }
        let _ = self.writer.write_all(bytes);
        let _ = self.writer.flush();
        // La ligne va changer : on attend le prochain rapport du shell.
        self.cmdline = None;
        self.term.scroll_display(Scroll::Bottom);
        self.dirty = true;
    }

    /// Écrit des octets sans ramener la vue en bas (rapports souris, focus…).
    pub fn write_raw(&mut self, bytes: &[u8]) {
        if self.exited {
            return;
        }
        let _ = self.writer.write_all(bytes);
        let _ = self.writer.flush();
    }

    /// Réinitialisation complète (RIS) puis rafraîchissement du shell.
    pub fn reset(&mut self) {
        self.parser.advance(&mut self.term, b"\x1bc");
        self.term.grid_mut().clear_history();
        self.images.clear();
        self.image_order.clear();
        self.write_input(b"\x0c");
        self.dirty = true;
    }

    /// Applique les options modifiables à chaud (historique, séparateurs de mots…).
    pub fn apply_options(&mut self, config: &Config) {
        self.term.set_options(term_config(config));
        self.opts = PaneOpts::from(config);
        self.dirty = true;
    }

    /// Nom du programme au premier plan s'il ne s'agit pas du shell lui-même
    /// (ex. « python3 » pour un serveur lancé dans le terminal).
    #[cfg(unix)]
    pub fn foreground_process(&self) -> Option<String> {
        if self.exited {
            return None;
        }
        let pgid = self.master.process_group_leader()?;
        let shell_pid = self.child.process_id()? as i32;
        if pgid == shell_pid {
            return None;
        }
        platform::process_name(pgid).or_else(|| Some(format!("pid {pgid}")))
    }

    /// Windows : on se fie à l'intégration shell (commande en cours).
    #[cfg(not(unix))]
    pub fn foreground_process(&self) -> Option<String> {
        if self.exited {
            return None;
        }
        let id = self.running?;
        self.block(id).map(|b| b.command.split_whitespace().next().unwrap_or("programme").to_string())
    }

    /// Efface l'historique puis demande au shell de rafraîchir l'écran (Ctrl+L).
    pub fn clear_all(&mut self) {
        self.term.grid_mut().clear_history();
        self.write_input(b"\x0c");
    }

    pub fn resize(&mut self, cols: usize, rows: usize, cell_w: f32, cell_h: f32) {
        self.cell_w = cell_w;
        self.cell_h = cell_h;
        let cols = cols.max(2);
        let rows = rows.max(1);
        if cols != self.cols || rows != self.rows {
            self.cols = cols;
            self.rows = rows;
            self.term.resize(Dims { columns: cols, screen_lines: rows });
            let _ = self.master.resize(PtySize {
                rows: rows as u16,
                cols: cols as u16,
                pixel_width: (cols as f32 * cell_w) as u16,
                pixel_height: (rows as f32 * cell_h) as u16,
            });
            self.dirty = true;
        }
    }

    /// Répertoire courant du shell (pour les nouveaux splits/onglets).
    pub fn cwd(&self) -> Option<PathBuf> {
        let pid = self.child.process_id();
        pid.and_then(platform::process_cwd).or_else(|| self.cwd_reported.clone())
    }

    pub fn close(&mut self) {
        let _ = self.child.kill();
        self.exited = true;
    }
}

fn term_config(config: &Config) -> TermConfig {
    let mut separators = config.word_separators.replace("\\t", "\t");
    if separators.is_empty() {
        separators = " ".into();
    }
    TermConfig {
        scrolling_history: config.scrollback,
        semantic_escape_chars: separators,
        default_cursor_style: CursorStyle { shape: config.cursor_shape.to_alacritty(), blinking: config.cursor_blink },
        ..Default::default()
    }
}

impl Drop for Pane {
    fn drop(&mut self) {
        let _ = self.child.kill();
    }
}
