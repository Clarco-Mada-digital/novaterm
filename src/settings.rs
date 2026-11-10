//! Fenêtre de paramètres (par pages), éditeur de raccourcis, aide et sélecteur d'image.

use std::collections::BTreeMap;
use std::path::PathBuf;

use egui::{Color32, Key, RichText};

use crate::config::{AcceptKey, BellMode, BlinkStyle, Config, CursorShape, RightClick, StartDir};
use crate::history::Ranking;
use crate::fonts::{self, FontEntry};
use crate::keys::{Action, Bindings, KeyCombo, ALL_ACTIONS};
use crate::theme;

#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum Page {
    #[default]
    Appearance,
    Font,
    Window,
    Cursor,
    Terminal,
    Mouse,
    Suggestions,
    Shortcuts,
}

impl Page {
    const ALL: [Page; 8] =
        [Page::Appearance, Page::Font, Page::Window, Page::Cursor, Page::Terminal, Page::Mouse, Page::Suggestions, Page::Shortcuts];

    fn label(self) -> &'static str {
        match self {
            Page::Appearance => "🎨 Apparence",
            Page::Font => "🔤 Police",
            Page::Window => "🖥 Fenêtre",
            Page::Cursor => "✏ Curseur",
            Page::Terminal => ">_ Terminal",
            Page::Mouse => "🖱 Souris",
            Page::Suggestions => "✨ Suggestions",
            Page::Shortcuts => "⌨ Raccourcis",
        }
    }
}

/// État de l'interface des paramètres (non sauvegardé).
#[derive(Default)]
pub struct SettingsUi {
    pub page: Page,
    /// Action dont on enregistre le prochain raccourci.
    pub capture: Option<Action>,
    /// Raccourci capturé par `raw_input_hook`, appliqué à la frame suivante.
    pub captured: Option<(Action, KeyCombo)>,
    pub shortcut_filter: String,
    pub font_filter: String,
    pub help_filter: String,
    pub font_scan_requested: bool,
    pub note: Option<String>,
    confirm_reset: bool,
}

/// Informations d'état affichées dans les paramètres.
pub struct SettingsInfo {
    pub history_len: usize,
    pub history_loading: bool,
    /// Intégration shell active dans le panneau actif.
    pub integration_active: bool,
    pub suggest_conflict: bool,
    pub shell_name: String,
}

/// Demandes de la fenêtre de paramètres à l'application.
#[derive(Default)]
pub struct SettingsOut {
    pub browse: bool,
    pub help: bool,
    pub action: Option<Action>,
}

fn hint(ui: &mut egui::Ui, text: &str) {
    ui.label(RichText::new(text).small().color(Color32::GRAY));
}

fn color_row(ui: &mut egui::Ui, label: &str, c: &mut [u8; 3]) {
    ui.horizontal(|ui| {
        egui::widgets::color_picker::color_edit_button_srgb(ui, c);
        ui.label(label);
    });
}

fn section(ui: &mut egui::Ui, title: &str) {
    ui.add_space(8.0);
    ui.label(RichText::new(title).strong().size(15.0));
    ui.add_space(2.0);
}

#[allow(clippy::too_many_arguments)]
pub fn show(
    ctx: &egui::Context,
    config: &mut Config,
    open: &mut bool,
    st: &mut SettingsUi,
    bindings: &Bindings,
    catalog: Option<&BTreeMap<String, FontEntry>>,
    info: &SettingsInfo,
    out: &mut SettingsOut,
) {
    apply_captured(config, st, bindings);
    let screen = ctx.screen_rect();
    egui::Window::new("Paramètres")
        .open(open)
        .resizable(true)
        .collapsible(false)
        .default_width(620.0_f32.min(screen.width() - 40.0))
        .default_height(560.0_f32.min(screen.height() - 60.0))
        .show(ctx, |ui| {
            ui.horizontal_wrapped(|ui| {
                for p in Page::ALL {
                    if ui.selectable_label(st.page == p, p.label()).clicked() {
                        st.page = p;
                        st.capture = None;
                    }
                }
            });
            ui.separator();
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                match st.page {
                    Page::Appearance => page_appearance(ui, config, out),
                    Page::Font => page_font(ui, config, st, catalog),
                    Page::Window => page_window(ui, config),
                    Page::Cursor => page_cursor(ui, config),
                    Page::Terminal => page_terminal(ui, config),
                    Page::Mouse => page_mouse(ui, config),
                    Page::Suggestions => page_suggestions(ui, config, info, out, bindings),
                    Page::Shortcuts => page_shortcuts(ui, config, st, bindings),
                }
                ui.add_space(12.0);
                ui.separator();
                ui.horizontal_wrapped(|ui| {
                    if ui.button("Aide des raccourcis").clicked() {
                        out.help = true;
                    }
                    if ui.button("Ouvrir config.toml").clicked() {
                        out.action = Some(Action::OpenConfig);
                    }
                    if st.confirm_reset {
                        if ui.button(RichText::new("Confirmer la réinitialisation").color(Color32::from_rgb(230, 100, 90))).clicked() {
                            *config = Config::default();
                            st.confirm_reset = false;
                        }
                        if ui.button("Annuler").clicked() {
                            st.confirm_reset = false;
                        }
                    } else if ui.button("Tout réinitialiser…").clicked() {
                        st.confirm_reset = true;
                    }
                });
                hint(
                    ui,
                    &format!(
                        "Sauvegarde automatique : {}",
                        Config::path().map(|p| p.display().to_string()).unwrap_or_default()
                    ),
                );
            });
        });
}

fn page_appearance(ui: &mut egui::Ui, config: &mut Config, out: &mut SettingsOut) {
    section(ui, "Thème");
    let presets = theme::presets();
    egui::ComboBox::from_id_salt("theme_combo")
        .width(220.0)
        .selected_text(if config.custom_theme.is_some() { "Personnalisé".to_string() } else { config.theme_name.clone() })
        .show_ui(ui, |ui| {
            for p in &presets {
                let active = config.custom_theme.is_none() && config.theme_name == p.name;
                if ui.selectable_label(active, &p.name).clicked() {
                    config.theme_name = p.name.clone();
                    config.custom_theme = None;
                }
            }
        });
    ui.horizontal(|ui| {
        if config.custom_theme.is_none() {
            if ui.button("Personnaliser les couleurs…").clicked() {
                config.custom_theme = Some(config.theme());
            }
        } else if ui.button("Revenir au thème prédéfini").clicked() {
            config.custom_theme = None;
        }
    });
    if let Some(custom) = &mut config.custom_theme {
        ui.add_space(4.0);
        ui.columns(2, |cols| {
            color_row(&mut cols[0], "Texte", &mut custom.foreground);
            color_row(&mut cols[0], "Fond", &mut custom.background);
            color_row(&mut cols[1], "Curseur", &mut custom.cursor);
            color_row(&mut cols[1], "Sélection", &mut custom.selection);
        });
        egui::CollapsingHeader::new("Couleurs ANSI (16)").show(ui, |ui| {
            let names = [
                "Noir", "Rouge", "Vert", "Jaune", "Bleu", "Magenta", "Cyan", "Blanc", "Noir clair", "Rouge clair",
                "Vert clair", "Jaune clair", "Bleu clair", "Magenta clair", "Cyan clair", "Blanc clair",
            ];
            ui.columns(2, |cols| {
                for (i, name) in names.iter().enumerate() {
                    color_row(&mut cols[i / 8], name, &mut custom.ansi[i]);
                }
            });
        });
    }

    section(ui, "Transparence");
    ui.add(egui::Slider::new(&mut config.opacity, 0.15..=1.0).text("Opacité du fond"));
    hint(ui, "Nécessite un compositeur (activé par défaut sur Cinnamon).");
    ui.add(egui::Slider::new(&mut config.dim_inactive, 0.0..=0.7).text("Assombrir les panneaux inactifs"));

    section(ui, "Image de fond");
    ui.horizontal(|ui| {
        if ui.button("Parcourir…").clicked() {
            out.browse = true;
        }
        if config.bg_image.is_some() && ui.button("Retirer").clicked() {
            config.bg_image = None;
        }
    });
    if let Some(path) = &config.bg_image {
        ui.label(RichText::new(path).small().monospace());
    }
    ui.add_enabled(
        config.bg_image.is_some(),
        egui::Slider::new(&mut config.bg_image_opacity, 0.0..=1.0).text("Opacité de l'image"),
    );
    ui.add_enabled(config.bg_image.is_some(), egui::Checkbox::new(&mut config.bg_image_stretch, "Étirer l'image (sinon couvrir)"));

    section(ui, "Animations");
    ui.checkbox(&mut config.toasts, "Notifications animées (copie, split, thème…)");
}

fn page_font(ui: &mut egui::Ui, config: &mut Config, st: &mut SettingsUi, catalog: Option<&BTreeMap<String, FontEntry>>) {
    section(ui, "Famille");
    let current = if config.font_family.is_empty() { fonts::BUILTIN.to_string() } else { config.font_family.clone() };
    match catalog {
        None => {
            ui.label(format!("Police actuelle : {current}"));
            if ui.button("Choisir une police du système…").clicked() {
                st.font_scan_requested = true;
            }
            hint(ui, "Recherche les polices à chasse fixe installées (fontconfig).");
        }
        Some(cat) => {
            ui.horizontal(|ui| {
                ui.label("Filtrer");
                ui.add(egui::TextEdit::singleline(&mut st.font_filter).desired_width(180.0).hint_text("ex. fira, mono…"));
            });
            egui::ComboBox::from_id_salt("font_family")
                .width(300.0)
                .height(320.0)
                .selected_text(&current)
                .show_ui(ui, |ui| {
                    if ui.selectable_label(config.font_family.is_empty(), fonts::BUILTIN).clicked() {
                        config.font_family.clear();
                    }
                    let f = st.font_filter.to_lowercase();
                    for (name, entry) in cat {
                        if !f.is_empty() && !name.to_lowercase().contains(&f) {
                            continue;
                        }
                        let mut label = name.clone();
                        if entry.bold.is_none() {
                            label.push_str("  (sans gras)");
                        }
                        if ui.selectable_label(config.font_family == *name, label).clicked() {
                            config.font_family = name.clone();
                        }
                    }
                });
            hint(ui, &format!("{} polices à chasse fixe trouvées.", cat.len()));
        }
    }

    section(ui, "Taille");
    ui.horizontal(|ui| {
        if ui.button(" − ").clicked() {
            config.font_size = (config.font_size - 1.0).max(6.0);
        }
        ui.add(egui::DragValue::new(&mut config.font_size).range(6.0..=48.0).speed(0.25).suffix(" px"));
        if ui.button(" + ").clicked() {
            config.font_size = (config.font_size + 1.0).min(48.0);
        }
        if ui.button("Par défaut").clicked() {
            config.font_size = Config::default().font_size;
        }
    });
    ui.add(egui::Slider::new(&mut config.font_size, 6.0..=48.0).text("Taille").step_by(0.5));
    hint(ui, "Raccourcis : Ctrl+Maj++ / Ctrl+Maj+- / Ctrl+0, ou Ctrl+molette.");

    section(ui, "Espacement");
    ui.add(egui::Slider::new(&mut config.line_spacing, 0.9..=2.0).text("Interligne"));
    ui.add(egui::Slider::new(&mut config.letter_spacing, -2.0..=6.0).text("Espacement des lettres (px)"));
    ui.add(egui::Slider::new(&mut config.padding, 0.0..=40.0).text("Marges intérieures (px)"));

    section(ui, "Aperçu");
    let sample = "AaBbCc 0123456789 Il1| O0 {}[]() <= => != ~ @#$%\nâêîôû éèàç ── │ ├─┤ ✓ ✗ → ← ▲ ▼  λ π";
    egui::Frame::NONE
        .fill(ui.visuals().extreme_bg_color)
        .corner_radius(6)
        .inner_margin(8)
        .show(ui, |ui| {
            ui.label(RichText::new(sample).family(egui::FontFamily::Name("Mono".into())).size(config.font_size));
        });
}

fn page_window(ui: &mut egui::Ui, config: &mut Config) {
    section(ui, "Taille au démarrage");
    ui.horizontal(|ui| {
        ui.label("Largeur");
        ui.add(egui::DragValue::new(&mut config.window_width).range(420.0..=7680.0).suffix(" px"));
        ui.label("Hauteur");
        ui.add(egui::DragValue::new(&mut config.window_height).range(260.0..=4320.0).suffix(" px"));
    });
    ui.horizontal(|ui| {
        if ui.button("Utiliser la taille actuelle").clicked() {
            if let Some(r) = ui.ctx().input(|i| i.viewport().inner_rect) {
                config.window_width = r.width().round();
                config.window_height = r.height().round();
            }
        }
        for (w, h) in [(900.0, 560.0), (1100.0, 700.0), (1400.0, 860.0)] {
            if ui.button(format!("{w:.0}×{h:.0}")).clicked() {
                config.window_width = w;
                config.window_height = h;
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(w, h)));
            }
        }
    });
    ui.checkbox(&mut config.remember_window_size, "Mémoriser la taille à la fermeture");
    ui.checkbox(&mut config.start_maximized, "Démarrer agrandie");
    ui.checkbox(&mut config.start_fullscreen, "Démarrer en plein écran");
    ui.checkbox(&mut config.always_on_top, "Toujours au premier plan");

    section(ui, "Barres");
    ui.checkbox(&mut config.show_tab_bar, "Barre d'onglets");
    ui.add_enabled(config.show_tab_bar, egui::Checkbox::new(&mut config.hide_single_tab, "Masquer s'il n'y a qu'un onglet"));
    ui.checkbox(&mut config.show_status_bar, "Barre de statut");

    section(ui, "Décorations");
    ui.checkbox(&mut config.custom_decorations, "Décorations personnalisées");
    hint(ui, "Barre de titre, boutons et coins arrondis dessinés par NovaTerm (sinon ceux du système).");
    ui.add_enabled_ui(config.custom_decorations, |ui| {
        ui.horizontal(|ui| {
            ui.label("Titre");
            ui.add(egui::TextEdit::singleline(&mut config.custom_title).hint_text("Automatique").desired_width(220.0));
        });
        ui.add(egui::Slider::new(&mut config.corner_radius, 0.0..=24.0).text("Arrondi des coins"));
    });

    section(ui, "Fermeture");
    ui.checkbox(&mut config.confirm_close, "Confirmer si un programme tourne encore (serveur, éditeur…)");
}

fn page_cursor(ui: &mut egui::Ui, config: &mut Config) {
    section(ui, "Forme");
    ui.horizontal(|ui| {
        for shape in [CursorShape::Block, CursorShape::Beam, CursorShape::Underline] {
            ui.selectable_value(&mut config.cursor_shape, shape, shape.label());
        }
    });
    ui.checkbox(&mut config.cursor_app_shape, "Laisser les applications changer la forme (vim…)");
    section(ui, "Animation");
    ui.checkbox(&mut config.cursor_blink, "Animer le curseur");
    ui.add_enabled_ui(config.cursor_blink, |ui| {
        ui.horizontal(|ui| {
            for style in [BlinkStyle::Blink, BlinkStyle::Fade, BlinkStyle::Pulse, BlinkStyle::Flip] {
                ui.selectable_value(&mut config.cursor_blink_style, style, style.label());
            }
        });
    });
    ui.checkbox(&mut config.cursor_smooth, "Déplacement fluide (traînée)");
    hint(ui, "S'applique immédiatement à tous les panneaux.");
}

fn page_terminal(ui: &mut egui::Ui, config: &mut Config) {
    section(ui, "Shell");
    ui.horizontal(|ui| {
        ui.label("Programme");
        ui.add(egui::TextEdit::singleline(&mut config.shell).hint_text("$SHELL").desired_width(220.0));
    });
    hint(ui, "Vide = shell par défaut. S'applique aux nouveaux panneaux.");
    ui.horizontal(|ui| {
        ui.label("Dossier de départ");
        egui::ComboBox::from_id_salt("start_dir").selected_text(config.start_dir.label()).show_ui(ui, |ui| {
            for d in [StartDir::Inherit, StartDir::Home, StartDir::Custom] {
                ui.selectable_value(&mut config.start_dir, d, d.label());
            }
        });
    });
    if config.start_dir == StartDir::Custom {
        ui.add(egui::TextEdit::singleline(&mut config.custom_start_dir).hint_text("~/projets").desired_width(280.0));
    }
    ui.checkbox(&mut config.close_on_exit, "Fermer le panneau quand le shell se termine");
    hint(ui, "Sinon le panneau reste ouvert : Entrée pour relancer un shell.");

    section(ui, "Historique");
    ui.add(egui::Slider::new(&mut config.scrollback, 0..=200_000).text("Lignes d'historique").logarithmic(true));

    section(ui, "Clavier");
    ui.checkbox(&mut config.smart_copy, "Ctrl+C copie s'il y a une sélection (sinon interrompt)");
    ui.checkbox(&mut config.ctrl_v_paste, "Ctrl+V colle (au lieu d'envoyer ^V)");
    ui.checkbox(&mut config.warn_multiline_paste, "Confirmer avant de coller plusieurs lignes");

    section(ui, "Sélection");
    ui.horizontal(|ui| {
        ui.label("Séparateurs de mots (double-clic)");
        ui.add(egui::TextEdit::singleline(&mut config.word_separators).font(egui::TextStyle::Monospace).desired_width(180.0));
    });

    section(ui, "Cloche (BEL)");
    ui.horizontal(|ui| {
        for b in [BellMode::None, BellMode::Visual, BellMode::Urgent] {
            ui.selectable_value(&mut config.bell, b, b.label());
        }
    });
}

fn page_mouse(ui: &mut egui::Ui, config: &mut Config) {
    section(ui, "Focus");
    ui.checkbox(&mut config.focus_follows_mouse, "Le focus suit la souris (panneau survolé = actif)");
    section(ui, "Sélection & presse-papiers");
    ui.checkbox(&mut config.copy_on_select, "Copier automatiquement la sélection");
    ui.checkbox(&mut config.middle_click_paste, "Clic du milieu : coller la sélection");
    ui.horizontal(|ui| {
        ui.label("Clic droit");
        egui::ComboBox::from_id_salt("right_click").selected_text(config.right_click.label()).show_ui(ui, |ui| {
            for r in [RightClick::Menu, RightClick::Paste, RightClick::CopyOrPaste] {
                ui.selectable_value(&mut config.right_click, r, r.label());
            }
        });
    });
    hint(ui, "Double-clic : mot · triple-clic : ligne · Alt+glisser : sélection rectangulaire.");
    section(ui, "Défilement");
    ui.add(egui::Slider::new(&mut config.scroll_multiplier, 1.0..=10.0).text("Lignes par cran de molette"));
    ui.checkbox(&mut config.ctrl_scroll_zoom, "Ctrl+molette : taille de la police");
    section(ui, "Liens & applications");
    ui.checkbox(&mut config.ctrl_click_links, "Ctrl+clic ouvre les liens (http, https, file…)");
    ui.checkbox(&mut config.mouse_reporting, "Transmettre la souris aux programmes (htop, vim, mc…)");
    hint(ui, "Maintenir Maj pour sélectionner du texte malgré tout.");
}

fn page_suggestions(ui: &mut egui::Ui, config: &mut Config, info: &SettingsInfo, out: &mut SettingsOut, bindings: &Bindings) {
    section(ui, "Suggestions de commandes");
    hint(
        ui,
        "Pendant que vous tapez, NovaTerm propose la suite de la commande d'après votre historique \
         (texte grisé après le curseur). Tout reste local sur votre machine.",
    );
    ui.checkbox(&mut config.suggestions, "Activer les suggestions (texte fantôme)");
    ui.add_enabled_ui(config.suggestions, |ui| {
        ui.checkbox(&mut config.suggest_list, "Afficher aussi une liste de propositions sous le curseur");
        ui.add_enabled(
            config.suggest_list,
            egui::Slider::new(&mut config.suggest_max, 2..=12).text("Propositions dans la liste"),
        );
        ui.horizontal(|ui| {
            ui.label("Accepter avec");
            egui::ComboBox::from_id_salt("suggest_accept").selected_text(config.suggest_accept.label()).show_ui(ui, |ui| {
                for k in [AcceptKey::RightOrEnd, AcceptKey::Right, AcceptKey::Tab] {
                    ui.selectable_value(&mut config.suggest_accept, k, k.label());
                }
            });
        });
        ui.checkbox(&mut config.suggest_word_accept, "Ctrl+→ accepte un mot à la fois");
        ui.horizontal(|ui| {
            ui.label("Proposer");
            for r in [Ranking::Recent, Ranking::Frequent] {
                ui.selectable_value(&mut config.suggest_ranking, r, r.label());
            }
        });
        ui.add(egui::Slider::new(&mut config.suggest_min_chars, 1..=5).text("Caractères avant de suggérer"));
        ui.add(egui::Slider::new(&mut config.suggest_opacity, 0.15..=0.9).text("Opacité du texte fantôme"));
    });

    section(ui, "Sources");
    ui.checkbox(&mut config.history_zsh, "Historique zsh (~/.zsh_history)");
    ui.checkbox(&mut config.history_bash, "Historique bash (~/.bash_history)");
    hint(ui, "Les commandes lancées dans NovaTerm sont ajoutées automatiquement.");
    if info.history_loading {
        ui.label("Chargement de l'historique…");
    } else {
        ui.label(format!("{} commandes connues.", info.history_len));
    }
    if ui.button(format!("Rechercher dans l'historique ({})", bindings.first_shortcut(Action::HistorySearch))).clicked() {
        out.action = Some(Action::HistorySearch);
    }
    ui.label("Ne jamais suggérer les commandes commençant par (une par ligne) :");
    ui.add(
        egui::TextEdit::multiline(&mut config.suggest_ignore)
            .font(egui::TextStyle::Monospace)
            .desired_rows(3)
            .desired_width(320.0),
    );

    section(ui, "Intégration shell");
    ui.checkbox(&mut config.shell_integration, "Activer l'intégration shell (zsh)");
    hint(
        ui,
        "Nécessaire aux suggestions. NovaTerm charge un petit script en plus de votre ~/.zshrc, \
         sans le modifier. S'applique aux nouveaux onglets/panneaux. bash : bientôt.",
    );
    let (txt, color) = if info.suggest_conflict {
        ("⚠ zsh-autosuggestions est déjà actif : NovaTerm le laisse faire dans ce panneau.", Color32::from_rgb(230, 180, 90))
    } else if info.integration_active {
        ("✔ Active dans le panneau actuel.", Color32::from_rgb(110, 200, 120))
    } else {
        ("✖ Inactive dans le panneau actuel (shell non pris en charge, ou ouvert avant l'activation).", Color32::GRAY)
    };
    ui.label(RichText::new(txt).color(color));
    hint(ui, &format!("Shell : {}", info.shell_name));
}

/// Ajoute à une action le raccourci capturé (et le retire des autres actions).
fn apply_captured(config: &mut Config, st: &mut SettingsUi, bindings: &Bindings) {
    let Some((action, combo)) = st.captured.take() else { return };
    let printable_alone = !combo.ctrl
        && !combo.alt
        && !matches!(
            combo.key,
            Key::F1 | Key::F2 | Key::F3 | Key::F4 | Key::F5 | Key::F6 | Key::F7 | Key::F8 | Key::F9 | Key::F10
                | Key::F11 | Key::F12 | Key::PageUp | Key::PageDown | Key::Insert | Key::Home | Key::End
        );
    if printable_alone {
        st.note = Some(format!(
            "« {} » refusé : sans Ctrl ni Alt, cette touche ne pourrait plus être tapée dans le terminal.",
            combo.display()
        ));
        return;
    }
    for other in bindings.conflicts(&combo, action) {
        let list: Vec<String> = bindings.map[&other]
            .iter()
            .filter(|c| **c != combo)
            .map(|c| c.to_config_string())
            .collect();
        config.keybindings.insert(other.id().to_string(), list);
        st.note = Some(format!("{} retiré de « {} ».", combo.display(), other.label()));
    }
    let mut list: Vec<String> = bindings.map[&action].iter().map(|c| c.to_config_string()).collect();
    let s = combo.to_config_string();
    if !list.contains(&s) {
        list.push(s);
    }
    config.keybindings.insert(action.id().to_string(), list);
}

fn page_shortcuts(ui: &mut egui::Ui, config: &mut Config, st: &mut SettingsUi, bindings: &Bindings) {
    section(ui, "Raccourcis clavier");
    hint(ui, "« + » puis appuyez sur la combinaison voulue (Échap pour annuler). « × » retire un raccourci.");
    ui.horizontal(|ui| {
        ui.label("Filtrer");
        ui.add(egui::TextEdit::singleline(&mut st.shortcut_filter).desired_width(200.0).hint_text("ex. onglet, split…"));
        if ui.button("Tout réinitialiser").clicked() {
            config.keybindings.clear();
            st.note = Some("Raccourcis par défaut restaurés.".into());
        }
    });
    if let Some(note) = &st.note {
        ui.label(RichText::new(note).color(Color32::from_rgb(230, 180, 90)));
    }
    let filter = st.shortcut_filter.to_lowercase();
    let mut category = "";
    egui::Grid::new("shortcut_grid").num_columns(2).striped(true).spacing([12.0, 6.0]).show(ui, |ui| {
        for &a in ALL_ACTIONS {
            let combos = &bindings.map[&a];
            let text = format!("{} {} {}", a.label(), a.category(), bindings.shortcut_text(a)).to_lowercase();
            if !filter.is_empty() && !text.contains(&filter) {
                continue;
            }
            if a.category() != category {
                category = a.category();
                ui.label(RichText::new(category).strong().color(ui.visuals().hyperlink_color));
                ui.end_row();
            }
            ui.label(a.label());
            ui.horizontal_wrapped(|ui| {
                let mut remove: Option<usize> = None;
                for (i, c) in combos.iter().enumerate() {
                    egui::Frame::NONE
                        .fill(ui.visuals().extreme_bg_color)
                        .corner_radius(4)
                        .inner_margin(egui::Margin::symmetric(5, 1))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = 2.0;
                                ui.label(RichText::new(c.display()).monospace());
                                if ui.small_button("×").on_hover_text("Retirer").clicked() {
                                    remove = Some(i);
                                }
                            });
                        });
                }
                if combos.is_empty() {
                    ui.label(RichText::new("—").color(Color32::GRAY));
                }
                if let Some(i) = remove {
                    let list: Vec<String> = combos
                        .iter()
                        .enumerate()
                        .filter(|(j, _)| *j != i)
                        .map(|(_, c)| c.to_config_string())
                        .collect();
                    config.keybindings.insert(a.id().to_string(), list);
                }
                if st.capture == Some(a) {
                    ui.label(RichText::new("Appuyez sur une combinaison…").color(Color32::from_rgb(120, 200, 255)));
                } else if ui.small_button("+").on_hover_text("Ajouter un raccourci").clicked() {
                    st.capture = Some(a);
                    st.note = None;
                }
                if config.keybindings.contains_key(a.id()) && ui.small_button("↺").on_hover_text("Par défaut").clicked() {
                    config.keybindings.remove(a.id());
                }
            });
            ui.end_row();
        }
    });
}

/// Aide : tous les raccourcis, groupés et filtrables. Un clic exécute l'action.
pub fn show_help(ctx: &egui::Context, open: &mut bool, bindings: &Bindings, filter: &mut String, run: &mut Option<Action>) {
    let screen = ctx.screen_rect();
    egui::Window::new("Raccourcis clavier")
        .open(open)
        .resizable(true)
        .collapsible(false)
        .default_width(520.0_f32.min(screen.width() - 40.0))
        .default_height(520.0_f32.min(screen.height() - 60.0))
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label("🔍");
                ui.add(egui::TextEdit::singleline(filter).desired_width(240.0).hint_text("Filtrer…"));
            });
            hint(ui, "Cliquez sur une ligne pour exécuter l'action. Modifiables dans Paramètres → Raccourcis.");
            ui.separator();
            let f = filter.to_lowercase();
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                let mut category = "";
                for &a in ALL_ACTIONS {
                    let sc = bindings.shortcut_text(a);
                    if !f.is_empty() && !format!("{} {} {}", a.label(), a.category(), sc).to_lowercase().contains(&f) {
                        continue;
                    }
                    if a.category() != category {
                        category = a.category();
                        ui.add_space(6.0);
                        ui.label(RichText::new(category).strong().color(ui.visuals().hyperlink_color));
                    }
                    let resp = ui
                        .horizontal(|ui| {
                            ui.add_sized([230.0, 18.0], egui::Label::new(a.label()).truncate());
                            ui.label(RichText::new(if sc.is_empty() { "—".to_string() } else { sc }).monospace().strong());
                        })
                        .response
                        .interact(egui::Sense::click());
                    if resp.hovered() {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                    }
                    if resp.clicked() && !a.only_when_searching() && a != Action::Help {
                        *run = Some(a);
                    }
                }
                ui.add_space(8.0);
                ui.label(RichText::new("Souris").strong().color(ui.visuals().hyperlink_color));
                for (k, d) in [
                    ("Double-clic / triple-clic", "Sélectionner un mot / une ligne"),
                    ("Alt + glisser", "Sélection rectangulaire"),
                    ("Maj + glisser", "Sélectionner même si le programme capture la souris"),
                    ("Clic du milieu", "Coller la sélection"),
                    ("Ctrl + clic", "Ouvrir un lien"),
                    ("Ctrl + molette", "Taille de la police"),
                    ("Double-clic sur un séparateur", "Remettre le split à 50 %"),
                    ("Double-clic / clic milieu sur un onglet", "Renommer / fermer"),
                ] {
                    ui.horizontal(|ui| {
                        ui.add_sized([230.0, 18.0], egui::Label::new(d).truncate());
                        ui.label(RichText::new(k).monospace().strong());
                    });
                }
            });
        });
}

pub struct FileBrowser {
    pub dir: PathBuf,
}

impl FileBrowser {
    pub fn new() -> Self {
        let dir = dirs::picture_dir().or_else(dirs::home_dir).unwrap_or_else(|| PathBuf::from("/"));
        Self { dir }
    }
}

fn is_image(path: &std::path::Path) -> bool {
    matches!(
        path.extension().and_then(|e| e.to_str()).map(|e| e.to_ascii_lowercase()).as_deref(),
        Some("png" | "jpg" | "jpeg" | "webp" | "bmp" | "gif")
    )
}

/// Retourne true si une image a été choisie (config.bg_image mis à jour).
pub fn show_file_browser(ctx: &egui::Context, browser: &mut Option<FileBrowser>, config: &mut Config) -> bool {
    let mut chosen = false;
    let mut close = false;
    let mut still_open = browser.is_some();
    if let Some(b) = browser {
        egui::Window::new("Choisir une image de fond")
            .open(&mut still_open)
            .resizable(true)
            .default_width(460.0)
            .default_height(420.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if ui.button("⬆ Dossier parent").clicked() {
                        if let Some(parent) = b.dir.parent() {
                            b.dir = parent.to_path_buf();
                        }
                    }
                    if ui.button("🏠").on_hover_text("Dossier personnel").clicked() {
                        if let Some(h) = dirs::home_dir() {
                            b.dir = h;
                        }
                    }
                    ui.label(RichText::new(b.dir.display().to_string()).monospace().small());
                });
                ui.separator();
                let entries = std::fs::read_dir(&b.dir).ok();
                egui::ScrollArea::vertical().show(ui, |ui| {
                    let mut dirs = Vec::new();
                    let mut files = Vec::new();
                    if let Some(entries) = entries {
                        for entry in entries.flatten() {
                            let path = entry.path();
                            let hidden = path.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with('.'));
                            if hidden {
                                continue;
                            }
                            if path.is_dir() {
                                dirs.push(path);
                            } else if is_image(&path) {
                                files.push(path);
                            }
                        }
                    }
                    dirs.sort();
                    files.sort();
                    for d in dirs {
                        let name = d.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                        if ui.selectable_label(false, format!("📁 {name}")).clicked() {
                            b.dir = d;
                        }
                    }
                    for f in files {
                        let name = f.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                        if ui.selectable_label(false, format!("🖼 {name}")).clicked() {
                            config.bg_image = Some(f.display().to_string());
                            chosen = true;
                            close = true;
                        }
                    }
                });
            });
    }
    if close || !still_open {
        *browser = None;
    }
    chosen
}
