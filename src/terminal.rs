//! Un panneau de terminal : émulation VT (alacritty_terminal) + PTY (portable-pty).

use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;

use alacritty_terminal::event::{Event, EventListener, WindowSize};
use alacritty_terminal::grid::{Dimensions, Scroll};
use alacritty_terminal::term::color::Colors;
use alacritty_terminal::term::{Config as TermConfig, Term};
use alacritty_terminal::vte::ansi::{CursorStyle, Processor, Rgb};
use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtySize};

use crate::config::Config;
use crate::shell::{self, Chunk, OscFilter, ShellMsg};
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

pub struct Pane {
    pub term: Term<Proxy>,
    parser: Processor,
    writer: Box<dyn Write + Send>,
    child: Box<dyn Child + Send + Sync>,
    master: Box<dyn MasterPty + Send>,
    rx: Receiver<Msg>,
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
    /// Formes mises en cache, reconstruites uniquement quand `dirty`.
    pub cached: Vec<egui::Shape>,
    /// Position pixel affichée du curseur (animée), et état du déplacement fluide.
    pub cursor_display: egui::Pos2,
    pub cursor_from: egui::Pos2,
    pub cursor_move_start: f64,
    pub cursor_prev_point: Option<(i32, usize)>,
    pub cursor_moving: bool,
    /// Séquences privées de l'intégration shell.
    osc: OscFilter,
    /// Ligne en cours d'édition au prompt : (position du curseur, texte).
    /// `None` hors édition ou dès qu'une touche est envoyée (en attente du shell).
    pub cmdline: Option<(usize, String)>,
    /// L'intégration shell a répondu au moins une fois.
    pub integration: bool,
    /// Un autre plugin de suggestions est actif dans ce shell.
    pub suggest_conflict: bool,
    /// Commandes exécutées depuis le dernier passage (pour l'historique).
    pub executed: Vec<String>,
}

impl Pane {
    pub fn spawn(
        cols: usize,
        rows: usize,
        config: &Config,
        cwd: Option<PathBuf>,
        ctx: egui::Context,
    ) -> anyhow::Result<Self> {
        let system = native_pty_system();
        let size = PtySize {
            rows: rows as u16,
            cols: cols as u16,
            pixel_width: 0,
            pixel_height: 0,
        };
        let pair = system.openpty(size)?;

        let shell = config.shell.trim();
        let shell_path = if shell.is_empty() { std::env::var("SHELL").unwrap_or_default() } else { shell.to_string() };
        let mut cmd = if shell.is_empty() {
            CommandBuilder::new_default_prog()
        } else {
            CommandBuilder::new(shell)
        };
        cmd.env("TERM", "xterm-256color");
        cmd.env("COLORTERM", "truecolor");
        cmd.env("TERM_PROGRAM", "NovaTerm");
        cmd.env("TERM_PROGRAM_VERSION", env!("CARGO_PKG_VERSION"));
        if config.shell_integration {
            for (k, v) in shell::env_for(&shell_path) {
                cmd.env(k, v);
            }
        }
        if let Some(dir) = cwd {
            cmd.cwd(dir);
        }

        let child = pair.slave.spawn_command(cmd)?;
        // Fermer le slave côté parent pour propager le EOF quand l'enfant meurt.
        drop(pair.slave);

        let reader = pair.master.try_clone_reader()?;
        let writer = pair.master.take_writer()?;

        let (tx, rx) = mpsc::channel();
        let proxy = Proxy { tx: tx.clone(), ctx: ctx.clone() };
        let term_config = term_config(config);
        let dims = Dims { columns: cols, screen_lines: rows };
        let term = Term::new(term_config, &dims, proxy);

        let thread_ctx = ctx.clone();
        thread::spawn(move || {
            let mut reader = reader;
            let mut buf = [0u8; 8192];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) => {
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
                    Err(_) => {
                        let _ = tx.send(Msg::Eof);
                        thread_ctx.request_repaint();
                        break;
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
            cached: Vec::new(),
            cursor_display: egui::Pos2::ZERO,
            cursor_from: egui::Pos2::ZERO,
            cursor_move_start: 0.0,
            cursor_prev_point: None,
            cursor_moving: false,
            osc: OscFilter::default(),
            cmdline: None,
            integration: false,
            suggest_conflict: false,
            executed: Vec::new(),
        })
    }

    /// Consomme les messages du PTY et du terminal. Retourne true si quelque chose a changé.
    pub fn drain(&mut self, theme: &Theme) -> bool {
        let mut changed = false;
        while let Ok(msg) = self.rx.try_recv() {
            changed = true;
            match msg {
                Msg::Bytes(bytes) => {
                    let mut msgs = Vec::new();
                    let (parser, term) = (&mut self.parser, &mut self.term);
                    self.osc.feed(&bytes, |chunk| match chunk {
                        Chunk::Data(d) => parser.advance(term, d),
                        Chunk::Msg(m) => msgs.push(m),
                    });
                    for m in msgs {
                        self.handle_shell_msg(&m);
                    }
                    self.dirty = true;
                }
                Msg::Eof => {
                    self.exited = true;
                    self.dirty = true;
                }
                Msg::Term(event) => self.handle_event(event, theme),
            }
        }
        changed
    }

    fn handle_shell_msg(&mut self, payload: &str) {
        self.integration = true;
        match shell::parse(payload) {
            Some(ShellMsg::Line { cursor, buffer }) => self.cmdline = Some((cursor, buffer)),
            Some(ShellMsg::EndLine) => self.cmdline = None,
            Some(ShellMsg::Executed(cmd)) => {
                self.cmdline = None;
                if self.executed.len() < 256 {
                    self.executed.push(cmd);
                }
            }
            Some(ShellMsg::Conflict) => self.suggest_conflict = true,
            None => {}
        }
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
                if let Ok(mut clipboard) = arboard::Clipboard::new() {
                    let _ = clipboard.set_text(text);
                }
            }
            Event::ClipboardLoad(_, f) => {
                let text = arboard::Clipboard::new()
                    .ok()
                    .and_then(|mut c| c.get_text().ok())
                    .unwrap_or_default();
                let response = f(&text);
                let _ = self.writer.write_all(response.as_bytes());
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
        self.write_input(b"\x0c");
        self.dirty = true;
    }

    /// Applique les options modifiables à chaud (historique, séparateurs de mots…).
    pub fn apply_options(&mut self, config: &Config) {
        self.term.set_options(term_config(config));
        self.dirty = true;
    }

    /// Nom du programme au premier plan s'il ne s'agit pas du shell lui-même
    /// (ex. « python3 » pour un serveur lancé dans le terminal).
    pub fn foreground_process(&self) -> Option<String> {
        if self.exited {
            return None;
        }
        let pgid = self.master.process_group_leader()?;
        let shell_pid = self.child.process_id()? as i32;
        if pgid == shell_pid {
            return None;
        }
        std::fs::read_to_string(format!("/proc/{pgid}/comm"))
            .ok()
            .map(|s| s.trim().to_string())
            .or_else(|| Some(format!("pid {pgid}")))
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

    /// Répertoire courant du processus enfant (pour les nouveaux splits/onglets).
    pub fn cwd(&self) -> Option<PathBuf> {
        let pid = self.child.process_id()?;
        std::fs::read_link(format!("/proc/{pid}/cwd")).ok()
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
        default_cursor_style: CursorStyle {
            shape: config.cursor_shape.to_alacritty(),
            blinking: config.cursor_blink,
        },
        ..Default::default()
    }
}

impl Drop for Pane {
    fn drop(&mut self) {
        let _ = self.child.kill();
    }
}