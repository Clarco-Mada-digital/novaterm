//! Fenêtre de l'assistant IA (sous-module de `app`).

use super::*;
use crate::ai::{self, Segment, Task};

pub(super) fn ai_input_id() -> Id {
    Id::new("nova_ai_input")
}

pub(super) enum AiPhase {
    /// Saisie d'une question.
    Input,
    /// Contenu capturé à confirmer avant l'envoi.
    Preview,
    Streaming,
    Done,
    Error(String),
}

pub(super) struct AiPanel {
    pane: Option<u64>,
    title: &'static str,
    pub(super) phase: AiPhase,
    input: String,
    focus: bool,
    /// Message (déjà masqué) en attente de confirmation.
    pending: Option<String>,
    masked: usize,
    system: String,
    endpoint: ai::Endpoint,
    history: Vec<ai::Message>,
    /// Libellé de chaque message de l'utilisateur (le contexte capturé est replié).
    labels: Vec<String>,
    /// Libellé du prochain message envoyé.
    next_label: String,
    response: String,
    job: Option<ai::Job>,
}

impl AiPanel {
    /// Mode démo : saisie dans le champ de l'assistant.
    pub(super) fn demo_type(&mut self, text: &str) {
        self.input.push_str(text);
    }
}

impl NovaApp {
    /// L'assistant est-il activé ? Sinon, explique comment l'activer.
    fn ai_ready(&mut self) -> bool {
        if self.config.ai_enabled {
            return true;
        }
        self.toast(t("L'assistant IA est désactivé : activez-le dans Paramètres → Assistant IA"));
        self.settings_open = true;
        self.settings_ui.page = settings::Page::Ai;
        false
    }

    fn ai_context(&self, pane: Option<u64>) -> ai::Context {
        let shell = if self.config.shell.trim().is_empty() { platform::default_shell() } else { self.config.shell.clone() };
        let shell = std::path::Path::new(shell.split_whitespace().next().unwrap_or(""))
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        let cwd = pane.and_then(|id| self.panes.get(&id)).and_then(|p| p.cwd()).map(|p| p.display().to_string());
        ai::Context { shell, cwd }
    }

    /// Ouvre l'assistant pour une tâche (question vide = champ de saisie).
    pub(super) fn ai_open(&mut self, task: Task) {
        if !self.ai_ready() {
            return;
        }
        let pane = self.focused_id();
        let ctx = self.ai_context(pane);
        let endpoint = ai::Endpoint::new(self.config.ai_provider, &self.config.ai_base_url, &self.config.ai_model);
        let mut panel = AiPanel {
            pane,
            title: task.title(),
            phase: AiPhase::Input,
            input: String::new(),
            focus: true,
            pending: None,
            masked: 0,
            system: ai::system_prompt(&ctx, crate::i18n::is_english()),
            endpoint,
            history: Vec::new(),
            labels: Vec::new(),
            next_label: self.ai_mask(&task.label()).0,
            response: String::new(),
            job: None,
        };
        if let Some(p) = &mut self.ai {
            if let Some(j) = p.job.take() {
                j.cancel();
            }
        }
        let empty_question = matches!(&task, Task::Ask { question } if question.trim().is_empty());
        if !empty_question {
            let (msg, masked) = self.ai_mask(&ai::user_message(&task));
            panel.masked = masked;
            if self.config.ai_confirm && task.has_captured() {
                panel.pending = Some(msg);
                panel.phase = AiPhase::Preview;
            } else {
                self.ai = Some(panel);
                self.ai_send(msg);
                return;
            }
        }
        self.ai = Some(panel);
    }

    fn ai_mask(&self, text: &str) -> (String, usize) {
        if self.config.ai_mask_secrets {
            ai::mask_secrets(text)
        } else {
            (text.to_string(), 0)
        }
    }

    fn ai_send(&mut self, msg: String) {
        let ctx = self.ctx.clone();
        let Some(p) = self.ai.as_mut() else { return };
        let label = if p.next_label.is_empty() { msg.clone() } else { std::mem::take(&mut p.next_label) };
        p.labels.push(label);
        p.history.push(ai::Message { role: ai::Role::User, content: msg });
        p.response.clear();
        p.phase = AiPhase::Streaming;
        p.job = Some(ai::start(p.endpoint.clone(), p.system.clone(), p.history.clone(), move || ctx.request_repaint()));
    }

    /// Entrée : envoyer la question, confirmer l'aperçu.
    pub(super) fn ai_submit(&mut self) {
        let Some(p) = self.ai.as_mut() else { return };
        match p.phase {
            AiPhase::Preview => {
                if let Some(msg) = p.pending.take() {
                    self.ai_send(msg);
                }
            }
            AiPhase::Input | AiPhase::Done | AiPhase::Error(_) => {
                let q = std::mem::take(&mut p.input);
                if q.trim().is_empty() {
                    return;
                }
                if matches!(p.phase, AiPhase::Error(_)) && p.history.last().is_some_and(|m| m.role == ai::Role::User) {
                    // La question précédente n'a pas eu de réponse : on la remplace.
                    p.history.pop();
                    p.labels.pop();
                }
                let (msg, masked) = self.ai_mask(q.trim());
                if let Some(p) = self.ai.as_mut() {
                    p.masked += masked;
                }
                self.ai_send(msg);
            }
            AiPhase::Streaming => {}
        }
    }

    /// Échap : arrêter la réponse en cours, sinon fermer.
    pub(super) fn ai_escape(&mut self) {
        if let Some(p) = self.ai.as_mut() {
            if let Some(job) = p.job.take() {
                job.cancel();
                if !p.response.is_empty() {
                    p.history.push(ai::Message { role: ai::Role::Assistant, content: std::mem::take(&mut p.response) });
                }
                p.phase = AiPhase::Done;
                p.focus = true;
                return;
            }
        }
        self.ai = None;
        self.clear_egui_focus();
    }

    /// Commandes proposées dans la dernière réponse.
    fn ai_commands(&self) -> Vec<String> {
        let Some(p) = &self.ai else { return Vec::new() };
        let last = p.history.iter().rev().find(|m| m.role == ai::Role::Assistant).map(|m| m.content.as_str()).unwrap_or("");
        ai::segments(last)
            .into_iter()
            .filter_map(|s| match s {
                Segment::Code { lang, code } if ai::is_command(&lang) => Some(code),
                _ => None,
            })
            .collect()
    }

    /// Place une commande proposée sur la ligne du terminal, sans l'exécuter.
    pub(super) fn ai_insert(&mut self, cmd: Option<String>) {
        let Some(cmd) = cmd.or_else(|| self.ai_commands().into_iter().next()) else {
            self.toast(t("Aucune commande proposée"));
            return;
        };
        let pane = self.ai.as_ref().and_then(|p| p.pane).filter(|id| self.panes.contains_key(id)).or_else(|| self.focused_id());
        if let Some(id) = pane {
            // Plusieurs lignes : jointes par « && » plutôt qu'exécutées une à une.
            let one = if cmd.lines().count() > 1 && !cmd.contains('\\') {
                cmd.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')).collect::<Vec<_>>().join(" && ")
            } else {
                cmd
            };
            self.insert_command(id, &one, false);
            self.ai = None;
            self.clear_egui_focus();
            if one.contains("[masqué]") {
                self.toast(t("⚠ La commande contient « [masqué] » : remplacez-le par la vraie valeur avant de l'exécuter"));
            } else {
                self.toast(t("Commande insérée : vérifiez-la, puis Entrée pour l'exécuter"));
            }
        }
    }

    /// Réception des morceaux de réponse.
    pub(super) fn ai_poll(&mut self) {
        let Some(p) = self.ai.as_mut() else { return };
        let Some(job) = &p.job else { return };
        let mut end: Option<Option<String>> = None;
        while let Ok(ev) = job.rx.try_recv() {
            match ev {
                ai::Event::Chunk(c) => p.response.push_str(&c),
                ai::Event::Done => end = Some(None),
                ai::Event::Error(e) => end = Some(Some(e)),
            }
        }
        if let Some(err) = end {
            p.job = None;
            if !p.response.is_empty() {
                p.history.push(ai::Message { role: ai::Role::Assistant, content: std::mem::take(&mut p.response) });
            }
            p.phase = match err {
                Some(e) => AiPhase::Error(e),
                None => AiPhase::Done,
            };
            p.focus = true;
        }
    }

    // ------------------------------------------------------------ tâches

    pub(super) fn ai_analyze_block(&mut self, pane_id: u64, block: u32) {
        let Some(pane) = self.panes.get(&pane_id) else { return };
        let Some(b) = pane.block(block) else { return };
        let command = b.command.clone();
        let exit = b.finished.map(|(c, _)| c);
        let output = pane.block_output(block).unwrap_or_default();
        let output = ai::tail(&output, self.config.ai_max_lines, 24_000);
        self.set_focus(pane_id);
        match exit {
            Some(code) if code != 0 => self.ai_open(Task::ExplainError { command, output, exit: code }),
            _ => self.ai_open(Task::Summarize { command, output }),
        }
    }

    pub(super) fn ai_analyze_last(&mut self) {
        let last = self.focused_id().and_then(|id| {
            let p = self.panes.get(&id)?;
            p.blocks.iter().rev().find(|b| b.finished.is_some()).map(|b| (id, b.id))
        });
        match last {
            Some((p, b)) => self.ai_analyze_block(p, b),
            None => match self.selection_text().filter(|s| !s.trim().is_empty()) {
                // Sans intégration shell : la sélection sert de sortie.
                Some(sel) => {
                    let output = ai::tail(&sel, self.config.ai_max_lines, 24_000);
                    self.ai_open(Task::Summarize { command: String::new(), output })
                }
                None => self.toast(t("Aucune commande terminée (intégration shell inactive ?) : sélectionnez la sortie à analyser")),
            },
        }
    }

    pub(super) fn ai_explain_command(&mut self, cmd: Option<String>) {
        let target = cmd
            .or_else(|| self.selection_text().filter(|t| !t.contains('\n')))
            .or_else(|| self.focused_id().and_then(|id| self.panes.get(&id)).and_then(|p| p.cmdline.as_ref()).map(|(_, b)| b.clone()))
            .filter(|t| !t.trim().is_empty())
            .or_else(|| self.last_block().and_then(|(p, b)| self.block_command(p, b)));
        match target {
            Some(command) => self.ai_open(Task::ExplainCommand { command }),
            None => self.toast(t("Rien à expliquer : tapez ou sélectionnez une commande")),
        }
    }

    pub(super) fn ai_commit_message(&mut self) {
        if !self.ai_ready() {
            return;
        }
        let cwd = self.focused_id().and_then(|id| self.panes.get(&id)).and_then(|p| p.cwd());
        let Some(cwd) = cwd else {
            self.toast(t("Dossier courant inconnu (intégration shell inactive ?)"));
            return;
        };
        match ai::staged_diff(&cwd, 20_000) {
            Ok(diff) => self.ai_open(Task::CommitMessage { diff }),
            Err(e) => self.toast(e),
        }
    }

    // ------------------------------------------------------------ affichage

    pub(super) fn render_ai(&mut self, ctx: &egui::Context) {
        if self.ai.is_none() {
            return;
        }
        let accent = rgb(self.theme.cursor);
        let surface = surface_color(&self.theme);
        let insert_key = "Ctrl+Entrée";
        let mut open = true;
        let mut insert: Option<String> = None;
        let mut submit = false;
        let mut retry = false;
        let screen = ctx.screen_rect();
        let Some(p) = self.ai.as_mut() else { return };
        let title = format!("✨ {} — {}", t("Assistant IA"), p.title);
        egui::Window::new(title)
            .id(Id::new("nova_ai_window"))
            .open(&mut open)
            .collapsible(false)
            .resizable(true)
            .default_width(640.0_f32.min(screen.width() - 40.0))
            .default_height(460.0_f32.min(screen.height() - 80.0))
            .anchor(egui::Align2::CENTER_TOP, Vec2::new(0.0, 50.0))
            .show(ctx, |ui| {
                // Service et confidentialité.
                ui.horizontal(|ui| {
                    let (icon, where_) = if p.endpoint.is_local() { ("🔒", t("local")) } else { ("🌐", t("en ligne")) };
                    ui.label(egui::RichText::new(format!("{icon} {} · {} · {where_}", p.endpoint.provider.label(), p.endpoint.model)).small().color(Color32::GRAY));
                    if p.masked > 0 {
                        ui.label(egui::RichText::new(crate::i18n::tf("· {} secret(s) masqué(s)", &[&p.masked])).small().color(Color32::GRAY));
                    }
                });
                ui.separator();
                let bottom = 64.0;
                egui::ScrollArea::vertical()
                    .max_height((ui.available_height() - bottom).max(120.0))
                    .auto_shrink([false, true])
                    .stick_to_bottom(true)
                    .show(ui, |ui| {
                        let mut user_n = 0;
                        for (i, m) in p.history.iter().enumerate() {
                            match m.role {
                                ai::Role::User => {
                                    let label = p.labels.get(user_n).cloned().unwrap_or_default();
                                    user_n += 1;
                                    if label != m.content {
                                        egui::CollapsingHeader::new(egui::RichText::new(format!("› {label}")).strong())
                                            .id_salt(("ai_user", i))
                                            .show(ui, |ui| {
                                                ui.add(egui::Label::new(egui::RichText::new(&m.content).monospace().small()).wrap());
                                            });
                                    } else {
                                        ui.label(egui::RichText::new(format!("› {}", m.content)).strong());
                                    }
                                }
                                ai::Role::Assistant => answer(ui, &m.content, accent, surface, &mut insert),
                            }
                            ui.add_space(6.0);
                        }
                        match &p.phase {
                            AiPhase::Preview => {
                                ui.label(egui::RichText::new(t("Ceci sera envoyé :")).strong());
                                egui::Frame::NONE.fill(surface).corner_radius(6).inner_margin(egui::Margin::same(8)).show(ui, |ui| {
                                    ui.set_width(ui.available_width());
                                    if let Some(msg) = &p.pending {
                                        ui.add(egui::Label::new(egui::RichText::new(msg).monospace()).wrap().selectable(true));
                                    }
                                });
                            }
                            AiPhase::Streaming => {
                                if !p.response.is_empty() {
                                    answer(ui, &p.response, accent, surface, &mut None);
                                }
                                ui.horizontal(|ui| {
                                    ui.spinner();
                                    ui.label(egui::RichText::new(t("Réponse en cours…  (Échap pour arrêter)")).color(Color32::GRAY));
                                });
                            }
                            AiPhase::Error(e) => {
                                ui.label(egui::RichText::new(format!("✖ {e}")).color(Color32::from_rgb(230, 110, 100)));
                                if p.history.last().is_some_and(|m| m.role == ai::Role::User) && ui.button(t("Réessayer")).clicked() {
                                    retry = true;
                                }
                            }
                            AiPhase::Input | AiPhase::Done => {}
                        }
                    });
                ui.separator();
                match p.phase {
                    AiPhase::Preview => {
                        ui.horizontal(|ui| {
                            if ui.button(egui::RichText::new(t("Envoyer  (Entrée)")).strong()).clicked() {
                                submit = true;
                            }
                            if ui.button(t("Annuler  (Échap)")).clicked() {
                                p.pending = None;
                                p.phase = AiPhase::Input;
                                p.focus = true;
                            }
                            ui.label(egui::RichText::new(t("Paramètres → Assistant IA pour ne plus confirmer")).small().color(Color32::GRAY));
                        });
                    }
                    AiPhase::Streaming => {}
                    _ => {
                        let hint = if p.history.is_empty() {
                            t("Décrivez ce que vous voulez faire, ex. : trouver les fichiers de plus de 100 Mo")
                        } else {
                            t("Question de suivi…  (Entrée pour envoyer, Échap pour fermer)")
                        };
                        let r = ui.add(egui::TextEdit::singleline(&mut p.input).id(ai_input_id()).desired_width(f32::INFINITY).hint_text(hint));
                        if std::mem::take(&mut p.focus) {
                            r.request_focus();
                        }
                        let has_cmd = p.history.last().is_some_and(|m| {
                            m.role == ai::Role::Assistant && ai::segments(&m.content).iter().any(|s| matches!(s, Segment::Code { lang, .. } if ai::is_command(lang)))
                        });
                        let mut tip = String::from(t("Les commandes ne sont jamais exécutées : « Insérer » les place sur la ligne, vous validez avec Entrée."));
                        if has_cmd {
                            tip = format!("{insert_key} : {}  ·  {tip}", t("insérer la première commande"));
                        }
                        ui.label(egui::RichText::new(tip).small().color(Color32::GRAY));
                    }
                }
            });
        if retry {
            if let Some(p) = self.ai.as_mut() {
                if let Some(last) = p.history.pop() {
                    p.next_label = p.labels.pop().unwrap_or_default();
                    self.ai_send(last.content);
                }
            }
        }
        if submit {
            self.ai_submit();
        }
        if let Some(cmd) = insert {
            self.ai_insert(Some(cmd));
        }
        if !open {
            if let Some(p) = self.ai.take() {
                if let Some(j) = p.job {
                    j.cancel();
                }
            }
        }
    }
}

/// Affiche une réponse : texte, et blocs de code avec « Insérer » / « Copier ».
fn answer(ui: &mut egui::Ui, text: &str, accent: Color32, surface: Color32, insert: &mut Option<String>) {
    for seg in ai::segments(text) {
        match seg {
            Segment::Text(s) => {
                for para in s.split("\n\n") {
                    ui.add(egui::Label::new(para.replace("**", "")).wrap().selectable(true));
                }
            }
            Segment::Code { lang, code } => {
                egui::Frame::NONE
                    .fill(surface)
                    .stroke(Stroke::new(1.0_f32, accent.gamma_multiply(0.35)))
                    .corner_radius(6)
                    .inner_margin(egui::Margin::same(8))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal_top(|ui| {
                            ui.add(egui::Label::new(egui::RichText::new(&code).monospace().color(accent)).wrap().selectable(true));
                        });
                        ui.horizontal(|ui| {
                            if ai::is_command(&lang) && ui.small_button(t("⏎ Insérer")).on_hover_text(t("Placer la commande sur la ligne, sans l'exécuter")).clicked() {
                                *insert = Some(code.clone());
                            }
                            if ui.small_button(t("📋 Copier")).clicked() {
                                ui.ctx().copy_text(code.clone());
                            }
                            if !lang.is_empty() {
                                ui.label(egui::RichText::new(&lang).small().color(Color32::GRAY));
                            }
                        });
                    });
            }
        }
    }
}
