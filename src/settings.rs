//! Fenêtre de paramètres (par pages), éditeur de raccourcis, aide et sélecteur d'image.

use std::collections::BTreeMap;
use std::path::PathBuf;

use egui::{Color32, Key, RichText};

use crate::config::{
    AcceptKey, AutoTheme, BadgeMode, BellMode, BlinkStyle, BroadcastScope, Config, CursorShape, RestoreMode, RightClick, Snippet,
    StartDir,
};
use crate::links;
use crate::platform;
use crate::session::Node;
use crate::history::Ranking;
use crate::fonts::{self, FontEntry};
use crate::keys::{Action, Bindings, KeyCombo, ALL_ACTIONS};

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
    Blocks,
    Links,
    Images,
    Session,
    Quake,
    Snippets,
    Shortcuts,
    Advanced,
}

impl Page {
    /// Pages regroupées par thème (barre latérale).
    const GROUPS: [(&'static str, &'static [Page]); 5] = [
        ("APPARENCE", &[Page::Appearance, Page::Font, Page::Cursor, Page::Window]),
        ("TERMINAL", &[Page::Terminal, Page::Mouse, Page::Shortcuts]),
        ("PRODUCTIVITÉ", &[Page::Suggestions, Page::Blocks, Page::Links, Page::Snippets, Page::Images]),
        ("ORGANISATION", &[Page::Session, Page::Quake]),
        ("SYSTÈME", &[Page::Advanced]),
    ];

    fn icon(self) -> &'static str {
        match self {
            Page::Appearance => "🎨",
            Page::Font => "🔤",
            Page::Window => "🖥",
            Page::Cursor => "✏",
            Page::Terminal => ">_",
            Page::Mouse => "🖱",
            Page::Suggestions => "✨",
            Page::Blocks => "▤",
            Page::Links => "🔗",
            Page::Images => "🖼",
            Page::Session => "💾",
            Page::Quake => "⬇",
            Page::Snippets => "▶",
            Page::Shortcuts => "⌨",
            Page::Advanced => "⚙",
        }
    }

    fn title(self) -> &'static str {
        match self {
            Page::Appearance => "Apparence",
            Page::Font => "Police",
            Page::Window => "Fenêtre",
            Page::Cursor => "Curseur",
            Page::Terminal => "Terminal",
            Page::Mouse => "Souris",
            Page::Suggestions => "Suggestions",
            Page::Blocks => "Blocs & alertes",
            Page::Links => "Liens & éditeur",
            Page::Images => "Images",
            Page::Session => "Session & profils",
            Page::Quake => "Terminal rapide",
            Page::Snippets => "Snippets",
            Page::Shortcuts => "Raccourcis",
            Page::Advanced => "Avancé",
        }
    }

    fn subtitle(self) -> &'static str {
        match self {
            Page::Appearance => "Thème, couleurs, transparence et image de fond.",
            Page::Font => "Police, taille et espacements du texte.",
            Page::Window => "Taille, barres, décorations et comportement de la fenêtre.",
            Page::Cursor => "Forme et animation du curseur.",
            Page::Terminal => "Shell, historique, clavier et sélection.",
            Page::Mouse => "Focus, presse-papiers, défilement et souris dans les programmes.",
            Page::Suggestions => "Suite de commande proposée pendant la frappe et complétion intelligente.",
            Page::Blocks => "Chaque commande devient un bloc avec son statut ; alertes de fin de tâche.",
            Page::Links => "URL, fichiers et erreurs cliquables, et l'éditeur qui les ouvre.",
            Page::Images => "Afficher des images directement dans le terminal.",
            Page::Session => "Retrouver ses onglets au démarrage et ouvrir des espaces de travail tout prêts.",
            Page::Quake => "Un terminal qui descend du haut de l'écran avec une seule touche.",
            Page::Snippets => "Vos commandes favorites, accessibles depuis la palette.",
            Page::Shortcuts => "Toutes les actions et leurs raccourcis, modifiables.",
            Page::Advanced => "Dépannage et informations.",
        }
    }

    /// Mots-clés pour la recherche dans les paramètres.
    fn keywords(self) -> &'static str {
        match self {
            Page::Appearance => "thème couleur theme transparence opacité image fond clair sombre auto importer iterm alacritty kitty animation notification toast",
            Page::Font => "police font taille interligne espacement lettre marge nerd fira jetbrains",
            Page::Window => "fenêtre taille démarrage agrandie plein écran premier plan barre onglets statut décoration titre coins arrondis fermeture confirmer",
            Page::Cursor => "curseur bloc barre souligné clignotement fondu pulsation animation traînée batterie",
            Page::Terminal => "shell zsh bash powershell cmd wsl dossier historique scrollback clavier ctrl+c ctrl+v coller sélection mots diffusion broadcast cloche bell",
            Page::Mouse => "souris focus suit copier sélection clic milieu droit molette défilement zoom lien htop vim",
            Page::Suggestions => "suggestion autosuggestion historique complétion completion git docker npm intégration shell zsh",
            Page::Blocks => "bloc commande statut durée badge notification alerte son fin tâche onglet pastille",
            Page::Links => "lien url fichier chemin erreur éditeur vscode code vim nano ctrl+clic",
            Page::Images => "image imgcat kitty iterm photo svg png",
            Page::Session => "session restaurer démarrage profil projet espace de travail sauvegarde",
            Page::Quake => "quake rapide déroulant f12 raccourci global toggle",
            Page::Snippets => "snippet favori commande palette",
            Page::Shortcuts => "raccourci clavier touche keybinding",
            Page::Advanced => "dépannage police réinitialiser config version",
        }
    }

    fn matches(self, query: &str) -> bool {
        let hay = format!("{} {}", self.title(), self.keywords()).to_lowercase();
        query.to_lowercase().split_whitespace().all(|w| hay.contains(w))
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
    pub quake_note: Option<String>,
    /// En attente de la touche du terminal rapide (capture au clavier).
    pub quake_capture: bool,
    pub quake_captured: Option<KeyCombo>,
    /// État du raccourci dans le bureau (lu à la demande, pas à chaque image).
    pub quake_status: Option<Option<crate::quake::ShortcutStatus>>,
    pub appimage_note: Option<String>,
    /// Recherche dans les paramètres.
    pub search: String,
}

/// Informations d'état affichées dans les paramètres.
pub struct SettingsInfo {
    pub history_len: usize,
    pub history_loading: bool,
    /// Intégration shell active dans le panneau actif.
    pub integration_active: bool,
    pub suggest_conflict: bool,
    pub shell_name: String,
    pub tabs: usize,
}

/// Demandes de la fenêtre de paramètres à l'application.
#[derive(Default)]
pub struct SettingsOut {
    pub browse: bool,
    pub help: bool,
    pub action: Option<Action>,
    pub open_profile: Option<usize>,
    pub install_quake_shortcut: bool,
    pub remove_quake_shortcut: bool,
    pub run_snippet: Option<usize>,
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

/// Carte : bloc arrondi contenant une section de réglages.
fn card(ui: &mut egui::Ui, title: &str, add: impl FnOnce(&mut egui::Ui)) {
    let v = ui.visuals().clone();
    let fill = if v.dark_mode { Color32::from_white_alpha(6) } else { Color32::from_black_alpha(8) };
    egui::Frame::NONE
        .fill(fill)
        .stroke(egui::Stroke::new(1.0, v.widgets.noninteractive.bg_stroke.color.gamma_multiply(0.6)))
        .corner_radius(10)
        .inner_margin(egui::Margin::symmetric(16, 12))
        .outer_margin(egui::Margin { left: 0, right: 0, top: 0, bottom: 10 })
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(RichText::new(title).strong().size(15.0).color(v.strong_text_color()));
            ui.add_space(6.0);
            ui.spacing_mut().item_spacing.y = 6.0;
            add(ui);
        });
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
        .title_bar(true)
        .default_width(900.0_f32.min(screen.width() - 40.0))
        .default_height(640.0_f32.min(screen.height() - 60.0))
        .show(ctx, |ui| {
            ui.spacing_mut().slider_width = 220.0;
            let narrow = ui.available_width() < 600.0;
            let height = ui.available_height().max(300.0);
            if narrow {
                // Fenêtre étroite : recherche et pages en haut.
                ui.horizontal_wrapped(|ui| {
                    search_box(ui, st, 160.0);
                    for (_, pages) in Page::GROUPS {
                        for &p in pages {
                            if (st.search.is_empty() || p.matches(&st.search))
                                && ui.selectable_label(st.page == p, format!("{} {}", p.icon(), p.title())).clicked()
                            {
                                st.page = p;
                                st.capture = None;
                            }
                        }
                    }
                });
                ui.separator();
                body(ui, config, st, bindings, catalog, info, out);
            } else {
                ui.horizontal_top(|ui| {
                    ui.vertical(|ui| {
                        ui.set_width(190.0);
                        ui.set_min_height(height);
                        sidebar(ui, st);
                    });
                    ui.add_space(6.0);
                    ui.vertical(|ui| body(ui, config, st, bindings, catalog, info, out));
                });
            }
        });
}

fn search_box(ui: &mut egui::Ui, st: &mut SettingsUi, width: f32) {
    let r = ui.add(egui::TextEdit::singleline(&mut st.search).hint_text("🔍 Rechercher un réglage").desired_width(width));
    // Entrée : aller à la première page trouvée.
    if r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
        if let Some(p) = Page::GROUPS.iter().flat_map(|(_, p)| p.iter()).find(|p| p.matches(&st.search)) {
            st.page = *p;
        }
    }
}

/// Barre latérale : recherche + pages groupées.
fn sidebar(ui: &mut egui::Ui, st: &mut SettingsUi) {
    search_box(ui, st, ui.available_width());
    ui.add_space(6.0);
    egui::ScrollArea::vertical().id_salt("settings_nav").auto_shrink([false, false]).show(ui, |ui| {
        let accent = ui.visuals().selection.bg_fill;
        let text = ui.visuals().text_color();
        let mut any = false;
        for (group, pages) in Page::GROUPS {
            let visible: Vec<Page> = pages.iter().copied().filter(|p| st.search.is_empty() || p.matches(&st.search)).collect();
            if visible.is_empty() {
                continue;
            }
            any = true;
            ui.add_space(8.0);
            ui.label(RichText::new(group).small().strong().color(text.gamma_multiply(0.5)));
            for p in visible {
                let selected = st.page == p;
                let (rect, resp) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 30.0), egui::Sense::click());
                let painter = ui.painter();
                if selected {
                    painter.rect_filled(rect, 7.0, accent.gamma_multiply(0.55));
                    painter.rect_filled(egui::Rect::from_min_size(rect.min + egui::vec2(0.0, 7.0), egui::vec2(3.0, 16.0)), 2.0, accent);
                } else if resp.hovered() {
                    painter.rect_filled(rect, 7.0, text.gamma_multiply(0.06));
                }
                painter.text(rect.left_center() + egui::vec2(12.0, 0.0), egui::Align2::LEFT_CENTER, p.icon(), egui::FontId::proportional(14.0), text);
                painter.text(
                    rect.left_center() + egui::vec2(38.0, 0.0),
                    egui::Align2::LEFT_CENTER,
                    p.title(),
                    egui::FontId::proportional(14.0),
                    if selected { ui.visuals().strong_text_color() } else { text },
                );
                if resp.clicked() {
                    st.page = p;
                    st.capture = None;
                }
            }
        }
        if !any {
            ui.label(RichText::new("Aucun réglage trouvé").color(Color32::GRAY));
        }
    });
}

#[allow(clippy::too_many_arguments)]
fn body(
    ui: &mut egui::Ui,
    config: &mut Config,
    st: &mut SettingsUi,
    bindings: &Bindings,
    catalog: Option<&BTreeMap<String, FontEntry>>,
    info: &SettingsInfo,
    out: &mut SettingsOut,
) {
    // En-tête de page.
    ui.add_space(2.0);
    ui.label(RichText::new(format!("{}  {}", st.page.icon(), st.page.title())).size(22.0).strong().color(ui.visuals().strong_text_color()));
    ui.label(RichText::new(st.page.subtitle()).color(Color32::GRAY));
    ui.add_space(8.0);
    let scroll_h = (ui.available_height() - 46.0).max(120.0);
    egui::ScrollArea::vertical().id_salt(("settings_body", st.page as u8)).max_height(scroll_h).auto_shrink([false, false]).show(ui, |ui| {
        ui.set_width(ui.available_width() - 8.0);
        match st.page {
            Page::Appearance => page_appearance(ui, config, out),
            Page::Font => page_font(ui, config, st, catalog),
            Page::Window => page_window(ui, config),
            Page::Cursor => page_cursor(ui, config),
            Page::Terminal => page_terminal(ui, config),
            Page::Mouse => page_mouse(ui, config),
            Page::Suggestions => page_suggestions(ui, config, info, out, bindings),
            Page::Blocks => page_blocks(ui, config, bindings),
            Page::Links => page_links(ui, config),
            Page::Images => page_images(ui, config),
            Page::Session => page_session(ui, config, info, out),
            Page::Quake => page_quake(ui, config, st, out),
            Page::Snippets => page_snippets(ui, config, bindings, out),
            Page::Shortcuts => page_shortcuts(ui, config, st, bindings),
            Page::Advanced => page_advanced(ui, config, st, out),
        }
    });
    // Pied de page fixe.
    ui.separator();
    ui.horizontal(|ui| {
        if ui.button("⌨ Aide des raccourcis").clicked() {
            out.help = true;
        }
        if ui.button("📄 Ouvrir config.toml").on_hover_text(Config::path().map(|p| p.display().to_string()).unwrap_or_default()).clicked() {
            out.action = Some(Action::OpenConfig);
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if st.confirm_reset {
                if ui.button("Annuler").clicked() {
                    st.confirm_reset = false;
                }
                if ui.button(RichText::new("Confirmer : tout réinitialiser").color(Color32::from_rgb(230, 100, 90))).clicked() {
                    *config = Config::default();
                    st.confirm_reset = false;
                }
            } else if ui.button("Tout réinitialiser…").clicked() {
                st.confirm_reset = true;
            }
            ui.label(RichText::new("✔ Enregistré automatiquement").small().color(Color32::GRAY));
        });
    });
}

fn page_appearance(ui: &mut egui::Ui, config: &mut Config, out: &mut SettingsOut) {
    card(ui, "Thème", |ui| {
        let names = config.theme_names();
        ui.horizontal(|ui| {
            egui::ComboBox::from_id_salt("theme_combo")
                .width(220.0)
                .selected_text(if config.custom_theme.is_some() { "Personnalisé".to_string() } else { config.theme_name.clone() })
                .show_ui(ui, |ui| {
                    for name in &names {
                        let active = config.custom_theme.is_none() && config.theme_name == *name;
                        if ui.selectable_label(active, name).clicked() {
                            config.theme_name = name.clone();
                            config.custom_theme = None;
                        }
                    }
                });
            if ui.button("Importer…").on_hover_text("iTerm2, Alacritty, Kitty, Windows Terminal, Xresources").clicked() {
                out.action = Some(Action::ImportTheme);
            }
        });
        if !config.user_themes.is_empty() {
            egui::CollapsingHeader::new(format!("Thèmes importés ({})", config.user_themes.len())).show(ui, |ui| {
                let mut remove = None;
                for (i, t) in config.user_themes.iter().enumerate() {
                    ui.horizontal(|ui| {
                        ui.label(&t.name);
                        if ui.small_button("Supprimer").clicked() {
                            remove = Some(i);
                        }
                    });
                }
                if let Some(i) = remove {
                    let t = config.user_themes.remove(i);
                    if config.theme_name == t.name {
                        config.theme_name = "Nova Dark".into();
                    }
                }
            });
        }
        ui.horizontal(|ui| {
            ui.label("Clair / sombre automatique");
            egui::ComboBox::from_id_salt("auto_theme").selected_text(config.auto_theme.label()).show_ui(ui, |ui| {
                for m in [AutoTheme::Off, AutoTheme::System, AutoTheme::Schedule] {
                    ui.selectable_value(&mut config.auto_theme, m, m.label());
                }
            });
        });
        if config.auto_theme != AutoTheme::Off {
            let pick = |ui: &mut egui::Ui, id: &str, label: &str, value: &mut String| {
                ui.horizontal(|ui| {
                    ui.label(label);
                    egui::ComboBox::from_id_salt(id).selected_text(value.clone()).show_ui(ui, |ui| {
                        for n in &names {
                            ui.selectable_value(value, n.clone(), n);
                        }
                    });
                });
            };
            pick(ui, "light_theme", "Thème clair", &mut config.light_theme);
            pick(ui, "dark_theme", "Thème sombre", &mut config.dark_theme);
            if config.auto_theme == AutoTheme::Schedule {
                ui.horizontal(|ui| {
                    ui.label("Jour à partir de");
                    ui.add(egui::DragValue::new(&mut config.day_hour).range(0..=23).suffix(" h"));
                    ui.label("nuit à partir de");
                    ui.add(egui::DragValue::new(&mut config.night_hour).range(0..=23).suffix(" h"));
                });
            }
        }
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

    });
    card(ui, "Transparence", |ui| {
        ui.add(egui::Slider::new(&mut config.opacity, 0.15..=1.0).text("Opacité du fond"));
        hint(ui, "Nécessite un compositeur (activé par défaut sur Cinnamon).");
        ui.add(egui::Slider::new(&mut config.dim_inactive, 0.0..=0.7).text("Assombrir les panneaux inactifs"));

    });
    card(ui, "Image de fond", |ui| {
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

    });
    card(ui, "Animations", |ui| {
        ui.checkbox(&mut config.toasts, "Notifications animées (copie, split, thème…)");
    });
}

fn page_font(ui: &mut egui::Ui, config: &mut Config, st: &mut SettingsUi, catalog: Option<&BTreeMap<String, FontEntry>>) {
    card(ui, "Famille", |ui| {
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

    });
    card(ui, "Taille", |ui| {
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

    });
    card(ui, "Espacement", |ui| {
        ui.add(egui::Slider::new(&mut config.line_spacing, 0.9..=2.0).text("Interligne"));
        ui.add(egui::Slider::new(&mut config.letter_spacing, -2.0..=6.0).text("Espacement des lettres (px)"));
        ui.add(egui::Slider::new(&mut config.padding, 0.0..=40.0).text("Marges intérieures (px)"));

    });
    card(ui, "Aperçu", |ui| {
        let sample = "AaBbCc 0123456789 Il1| O0 {}[]() <= => != ~ @#$%\nâêîôû éèàç ── │ ├─┤ ✓ ✗ → ← ▲ ▼  λ π";
        egui::Frame::NONE
            .fill(ui.visuals().extreme_bg_color)
            .corner_radius(6)
            .inner_margin(8)
            .show(ui, |ui| {
                ui.label(RichText::new(sample).family(egui::FontFamily::Name("Mono".into())).size(config.font_size));
            });
    });
}

fn page_window(ui: &mut egui::Ui, config: &mut Config) {
    card(ui, "Taille au démarrage", |ui| {
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

    });
    card(ui, "Barres", |ui| {
        ui.checkbox(&mut config.show_tab_bar, "Barre d'onglets");
        ui.add_enabled(config.show_tab_bar, egui::Checkbox::new(&mut config.hide_single_tab, "Masquer s'il n'y a qu'un onglet"));
        ui.checkbox(&mut config.show_status_bar, "Barre de statut");

    });
    card(ui, "Décorations", |ui| {
        ui.checkbox(&mut config.custom_decorations, "Décorations personnalisées");
        hint(ui, "Barre de titre, boutons et coins arrondis dessinés par NovaTerm (sinon ceux du système).");
        ui.add_enabled_ui(config.custom_decorations, |ui| {
            ui.horizontal(|ui| {
                ui.label("Titre");
                ui.add(egui::TextEdit::singleline(&mut config.custom_title).hint_text("Automatique").desired_width(220.0));
            });
            ui.add(egui::Slider::new(&mut config.corner_radius, 0.0..=24.0).text("Arrondi des coins"));
        });

    });
    card(ui, "Fermeture", |ui| {
        ui.checkbox(&mut config.confirm_close, "Confirmer si un programme tourne encore (serveur, éditeur…)");
    });
}

fn page_cursor(ui: &mut egui::Ui, config: &mut Config) {
    card(ui, "Forme", |ui| {
        ui.horizontal(|ui| {
            for shape in [CursorShape::Block, CursorShape::Beam, CursorShape::Underline] {
                ui.selectable_value(&mut config.cursor_shape, shape, shape.label());
            }
        });
        ui.checkbox(&mut config.cursor_app_shape, "Laisser les applications changer la forme (vim…)");
    });
    card(ui, "Animation", |ui| {
        ui.checkbox(&mut config.cursor_blink, "Animer le curseur");
        ui.add_enabled_ui(config.cursor_blink, |ui| {
            ui.horizontal(|ui| {
                for style in [BlinkStyle::Blink, BlinkStyle::Fade, BlinkStyle::Pulse, BlinkStyle::Flip] {
                    ui.selectable_value(&mut config.cursor_blink_style, style, style.label());
                }
            });
        });
        ui.checkbox(&mut config.cursor_smooth, "Déplacement fluide (traînée)");
        ui.add(
            egui::Slider::new(&mut config.cursor_blink_timeout, 0..=120)
                .text("Arrêter l'animation après (s d'inactivité, 0 = jamais)"),
        );
        hint(ui, "Économise la batterie : sans animation, NovaTerm ne consomme rien au repos.");
    });
}

fn page_terminal(ui: &mut egui::Ui, config: &mut Config) {
    card(ui, "Shell", |ui| {
        ui.horizontal(|ui| {
            ui.label("Programme");
            ui.add(egui::TextEdit::singleline(&mut config.shell).hint_text(platform::default_shell()).desired_width(220.0));
            egui::ComboBox::from_id_salt("shell_presets").selected_text("Choisir…").show_ui(ui, |ui| {
                if ui.selectable_label(config.shell.is_empty(), "Par défaut du système").clicked() {
                    config.shell.clear();
                }
                for (label, cmd) in platform::shell_presets() {
                    if ui.selectable_label(config.shell == cmd, label).clicked() {
                        config.shell = cmd;
                    }
                }
            });
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

    });
    card(ui, "Historique", |ui| {
        ui.add(egui::Slider::new(&mut config.scrollback, 0..=200_000).text("Lignes d'historique").logarithmic(true));

    });
    card(ui, "Clavier", |ui| {
        ui.checkbox(&mut config.smart_copy, "Ctrl+C copie s'il y a une sélection (sinon interrompt)");
        ui.checkbox(&mut config.ctrl_v_paste, "Ctrl+V colle (au lieu d'envoyer ^V)");

    });
    card(ui, "Sélection", |ui| {
        ui.horizontal(|ui| {
            ui.label("Séparateurs de mots (double-clic)");
            ui.add(egui::TextEdit::singleline(&mut config.word_separators).font(egui::TextStyle::Monospace).desired_width(180.0));
        });

    });
    card(ui, "Diffusion du clavier", |ui| {
        ui.horizontal(|ui| {
            ui.label("Taper dans plusieurs panneaux à la fois :");
            for s in [BroadcastScope::Tab, BroadcastScope::All] {
                ui.selectable_value(&mut config.broadcast_scope, s, s.label());
            }
        });
        hint(ui, "Activez / désactivez avec Ctrl+Maj+I (cadre rouge pendant la diffusion).");

    });
    card(ui, "Sécurité", |ui| {
        ui.checkbox(&mut config.osc52_write, "Les programmes peuvent copier dans le presse-papiers (vim, tmux, OSC 52)");
        ui.checkbox(&mut config.osc52_read, "Les programmes peuvent LIRE le presse-papiers (OSC 52)");
        hint(ui, "Lecture déconseillée : un programme distant (SSH) pourrait récupérer un mot de passe copié.");
        ui.checkbox(&mut config.warn_multiline_paste, "Confirmer avant de coller plusieurs lignes");
    });
    card(ui, "Cloche (BEL)", |ui| {
        ui.horizontal(|ui| {
            for b in [BellMode::None, BellMode::Visual, BellMode::Urgent] {
                ui.selectable_value(&mut config.bell, b, b.label());
            }
        });
    });
}

fn page_mouse(ui: &mut egui::Ui, config: &mut Config) {
    card(ui, "Focus", |ui| {
        ui.checkbox(&mut config.focus_follows_mouse, "Le focus suit la souris (panneau survolé = actif)");
    });
    card(ui, "Sélection & presse-papiers", |ui| {
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
    });
    card(ui, "Défilement", |ui| {
        ui.add(egui::Slider::new(&mut config.scroll_multiplier, 1.0..=10.0).text("Lignes par cran de molette"));
        ui.checkbox(&mut config.ctrl_scroll_zoom, "Ctrl+molette : taille de la police");
    });
    card(ui, "Liens & applications", |ui| {
        ui.checkbox(&mut config.ctrl_click_links, "Ctrl+clic ouvre les liens (http, https, file…)");
        ui.checkbox(&mut config.mouse_reporting, "Transmettre la souris aux programmes (htop, vim, mc…)");
        hint(ui, "Maintenir Maj pour sélectionner du texte malgré tout.");
    });
}

fn page_suggestions(ui: &mut egui::Ui, config: &mut Config, info: &SettingsInfo, out: &mut SettingsOut, bindings: &Bindings) {
    card(ui, "Suggestions de commandes", |ui| {
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
            ui.checkbox(&mut config.completions, "Complétion intelligente (git, docker, cargo, npm, make, ssh, Django…)");
            hint(ui, "Sous-commandes et options avec leur description, branches git, scripts npm, cibles make, hôtes SSH.");
            ui.horizontal(|ui| {
                ui.label("Proposer");
                for r in [Ranking::Recent, Ranking::Frequent] {
                    ui.selectable_value(&mut config.suggest_ranking, r, r.label());
                }
            });
            ui.add(egui::Slider::new(&mut config.suggest_min_chars, 1..=5).text("Caractères avant de suggérer"));
            ui.add(egui::Slider::new(&mut config.suggest_opacity, 0.15..=0.9).text("Opacité du texte fantôme"));
        });

    });
    card(ui, "Sources", |ui| {
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

    });
    card(ui, "Intégration shell", |ui| {
        ui.checkbox(&mut config.shell_integration, "Activer l'intégration shell (zsh, bash)");
        hint(
            ui,
            "Nécessaire aux suggestions et aux blocs de commandes. NovaTerm charge un petit script \
             en plus de votre ~/.zshrc ou ~/.bashrc, sans les modifier. S'applique aux nouveaux onglets. \
             (bash : blocs et notifications ; suggestions en direct avec zsh uniquement)",
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
    });
}

fn page_blocks(ui: &mut egui::Ui, config: &mut Config, bindings: &Bindings) {
    card(ui, "Blocs de commandes", |ui| {
        hint(ui, "Chaque commande et sa sortie forment un bloc (nécessite l'intégration shell, page Suggestions).");
        ui.checkbox(&mut config.blocks_enabled, "Activer les blocs de commandes");
        ui.add_enabled_ui(config.blocks_enabled, |ui| {
            ui.checkbox(&mut config.blocks_gutter, "Barre de couleur à gauche (vert = succès, rouge = échec, bleu = en cours)");
            ui.checkbox(&mut config.blocks_separator, "Ligne de séparation entre les commandes");
            ui.checkbox(&mut config.blocks_actions, "Boutons au survol (copier la commande / la sortie, relancer, remonter)");
            ui.horizontal(|ui| {
                ui.label("Badge statut et durée");
                egui::ComboBox::from_id_salt("badges").selected_text(config.blocks_badges.label()).show_ui(ui, |ui| {
                    for m in [BadgeMode::Always, BadgeMode::Notable, BadgeMode::Never] {
                        ui.selectable_value(&mut config.blocks_badges, m, m.label());
                    }
                });
            });
        });
        hint(
            ui,
            &format!(
                "Navigation : {} / {} · copier la dernière sortie : {}",
                bindings.first_shortcut(Action::PrevCommand),
                bindings.first_shortcut(Action::NextCommand),
                bindings.first_shortcut(Action::CopyLastOutput)
            ),
        );

    });
    card(ui, "Notifications de fin de tâche", |ui| {
        ui.checkbox(&mut config.notify_long_commands, "Prévenir quand une longue commande se termine");
        ui.add_enabled_ui(config.notify_long_commands, |ui| {
            ui.add(egui::Slider::new(&mut config.notify_threshold_secs, 2..=600).text("Durée minimale (s)").logarithmic(true));
            ui.checkbox(&mut config.notify_only_unfocused, "Seulement si je suis sur une autre fenêtre ou un autre onglet");
            ui.checkbox(&mut config.notify_sound, "Jouer un son");
        });
        ui.checkbox(&mut config.notify_from_apps, "Autoriser les programmes à envoyer des notifications (OSC 9 / 777)");
        ui.checkbox(&mut config.tab_activity, "Pastille sur les onglets en arrière-plan (● activité, ✔ / ✖ commande terminée)");
    });
}

fn page_links(ui: &mut egui::Ui, config: &mut Config) {
    card(ui, "Liens cliquables", |ui| {
        ui.checkbox(&mut config.ctrl_click_links, "Activer les liens (URL, liens OSC 8)");
        ui.add_enabled_ui(config.ctrl_click_links, |ui| {
            ui.checkbox(&mut config.detect_paths, "Détecter aussi les fichiers et erreurs (src/app.rs:120, File \"x.py\", line 3…)");
            ui.checkbox(&mut config.link_plain_click, "Ouvrir d'un simple clic (sinon Ctrl+clic)");
        });
    });
    card(ui, "Éditeur pour les fichiers", |ui| {
        egui::ComboBox::from_id_salt("editor_preset")
            .width(300.0)
            .selected_text(
                links::editor_presets()
                    .into_iter()
                    .find(|(_, t)| *t == config.editor_command)
                    .map(|(l, _)| l.to_string())
                    .unwrap_or_else(|| "Personnalisé".into()),
            )
            .show_ui(ui, |ui| {
                for (label, tpl) in links::editor_presets() {
                    if ui.selectable_label(config.editor_command == tpl, label).clicked() {
                        config.editor_command = tpl.to_string();
                    }
                }
            });
        ui.add(
            egui::TextEdit::singleline(&mut config.editor_command)
                .font(egui::TextStyle::Monospace)
                .hint_text("vide = application par défaut")
                .desired_width(360.0),
        );
        hint(ui, "Variables : {file} {line} {col}. « @tab commande » ouvre dans un nouvel onglet NovaTerm.");
    });
}

fn page_images(ui: &mut egui::Ui, config: &mut Config) {
    card(ui, "Images dans le terminal", |ui| {
        ui.checkbox(&mut config.images_enabled, "Afficher les images (protocoles iTerm2 et Kitty)");
        ui.add_enabled_ui(config.images_enabled, |ui| {
            ui.checkbox(&mut config.images_allow_files, "Autoriser l'affichage d'images locales par chemin (Kitty)");
        });
        hint(
            ui,
            "Essayez : imgcat photo.png (fourni par NovaTerm), ou des outils compatibles comme \
             « kitty +kitten icat », chafa --format kitty, viu, ranger, yazi.",
        );
        ui.label(RichText::new("imgcat ~/Images/photo.jpg\nimgcat --width 40 logo.png").monospace());
    });
}

fn page_session(ui: &mut egui::Ui, config: &mut Config, info: &SettingsInfo, out: &mut SettingsOut) {
    card(ui, "Au démarrage", |ui| {
        ui.horizontal(|ui| {
            ui.label("Restaurer la session précédente");
            egui::ComboBox::from_id_salt("restore").selected_text(config.restore_session.label()).show_ui(ui, |ui| {
                for m in [RestoreMode::Off, RestoreMode::Layout, RestoreMode::LayoutAndContent] {
                    ui.selectable_value(&mut config.restore_session, m, m.label());
                }
            });
        });
        ui.add_enabled_ui(config.restore_session == RestoreMode::LayoutAndContent, |ui| {
            ui.add(egui::Slider::new(&mut config.restore_lines, 20..=3000).text("Lignes de texte restaurées par panneau").logarithmic(true));
        });
        ui.add_enabled_ui(config.restore_session != RestoreMode::Off, |ui| {
            ui.add(egui::Slider::new(&mut config.autosave_secs, 0..=600).text("Sauvegarde automatique (s, 0 = à la fermeture)"));
        });
        ui.horizontal(|ui| {
            ui.label("Ouvrir ce profil au démarrage");
            egui::ComboBox::from_id_salt("startup_profile")
                .selected_text(if config.startup_profile.is_empty() { "Aucun".to_string() } else { config.startup_profile.clone() })
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut config.startup_profile, String::new(), "Aucun");
                    for p in &config.profiles {
                        ui.selectable_value(&mut config.startup_profile, p.name.clone(), &p.name);
                    }
                });
        });

    });
    card(ui, "Profils de projet", |ui| {
        hint(ui, "Un profil = des onglets et panneaux nommés, avec un dossier et une commande de démarrage par panneau.");
        if ui.button(format!("Enregistrer la disposition actuelle ({} onglet(s)) comme profil…", info.tabs)).clicked() {
            out.action = Some(Action::SaveProfile);
        }
        let mut remove = None;
        for (pi, profile) in config.profiles.iter_mut().enumerate() {
            egui::CollapsingHeader::new(format!("▣ {}", profile.name)).id_salt(("profile", pi)).show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Nom");
                    ui.text_edit_singleline(&mut profile.name);
                    if ui.button("Ouvrir").clicked() {
                        out.open_profile = Some(pi);
                    }
                    if ui.button("Supprimer").clicked() {
                        remove = Some(pi);
                    }
                });
                for (ti, tab) in profile.tabs.iter_mut().enumerate() {
                    ui.label(RichText::new(format!("Onglet {}", ti + 1)).strong());
                    ui.horizontal(|ui| {
                        ui.label("Titre");
                        let mut t = tab.title.clone().unwrap_or_default();
                        if ui.add(egui::TextEdit::singleline(&mut t).hint_text("automatique").desired_width(160.0)).changed() {
                            tab.title = (!t.is_empty()).then_some(t);
                        }
                    });
                    for (li, leaf) in tab.root.leaves_mut().into_iter().enumerate() {
                        if let Node::Leaf { cwd, command, .. } = leaf {
                            ui.horizontal(|ui| {
                                ui.label(format!("  Panneau {} :", li + 1));
                                let mut d = cwd.clone().unwrap_or_default();
                                let w = ((ui.available_width() - 16.0) / 2.0).max(140.0);
                            if ui.add(egui::TextEdit::singleline(&mut d).hint_text("dossier").desired_width(w)).changed() {
                                    *cwd = (!d.is_empty()).then_some(d);
                                }
                                let mut c = command.clone().unwrap_or_default();
                                if ui
                                    .add(egui::TextEdit::singleline(&mut c).hint_text("commande (ex. npm run dev)").desired_width(w).font(egui::TextStyle::Monospace))
                                    .changed()
                                {
                                    *command = (!c.is_empty()).then_some(c);
                                }
                            });
                        }
                    }
                }
                hint(ui, &format!("En ligne de commande : novaterm --profile \"{}\"", profile.name));
            });
        }
        if let Some(i) = remove {
            let p = config.profiles.remove(i);
            if config.startup_profile == p.name {
                config.startup_profile.clear();
            }
        }
    });
}

fn page_quake(ui: &mut egui::Ui, config: &mut Config, st: &mut SettingsUi, out: &mut SettingsOut) {
    card(ui, "Terminal rapide (« quake »)", |ui| {
        hint(ui, "Un raccourci global fait descendre un terminal depuis le haut de l'écran, et le cache à nouveau.");
        ui.add(egui::Slider::new(&mut config.quake_height, 0.2..=1.0).text("Hauteur (part de l'écran)"));
        ui.add(egui::Slider::new(&mut config.quake_width, 0.3..=1.0).text("Largeur (part de l'écran)"));
        ui.checkbox(&mut config.quake_hide_on_unfocus, "Se cacher quand on clique ailleurs");
    });
    // Touche capturée au clavier : appliquée immédiatement au bureau.
    if let Some(c) = st.quake_captured.take() {
        let alone = !c.ctrl && !c.alt && !c.shift;
        let fkey = matches!(
            c.key,
            Key::F1 | Key::F2 | Key::F3 | Key::F4 | Key::F5 | Key::F6 | Key::F7 | Key::F8 | Key::F9 | Key::F10 | Key::F11 | Key::F12
                | Key::Insert
        );
        if alone && !fkey {
            st.quake_note = Some(format!(
                "« {} » seule bloquerait cette touche partout : ajoutez Ctrl, Alt ou Maj, ou utilisez une touche F.",
                c.display()
            ));
        } else {
            config.quake_hotkey = c.to_config_string();
            out.install_quake_shortcut = true;
        }
    }
    if st.quake_status.is_none() {
        st.quake_status = Some(crate::quake::shortcut_status());
    }
    card(ui, "Raccourci global", |ui| {
        ui.horizontal(|ui| {
            ui.label("Touche :");
            let current = KeyCombo::parse(&config.quake_hotkey).map(|c| c.display()).unwrap_or_else(|| config.quake_hotkey.clone());
            if st.quake_capture {
                ui.label(RichText::new("⌨ Appuyez sur la combinaison voulue… (Échap pour annuler)").color(Color32::from_rgb(120, 200, 255)));
            } else {
                ui.label(RichText::new(current).monospace().strong());
                if ui.button("⌨ Définir au clavier").on_hover_text("Cliquez puis appuyez sur la touche ou la combinaison").clicked() {
                    st.quake_capture = true;
                    st.quake_note = None;
                }
                if ui.button("Appliquer").on_hover_text("Recréer le raccourci du bureau avec cette touche").clicked() {
                    out.install_quake_shortcut = true;
                }
            }
        });
        // État réel dans le bureau.
        match st.quake_status.as_ref().and_then(|s| s.as_ref()) {
            Some(s) => {
                let (txt, color) = if s.legacy {
                    ("⚠ Ancien raccourci (v0.0.1) : cliquez sur « Appliquer » pour le réparer.".to_string(), Color32::from_rgb(230, 180, 90))
                } else if s.program_ok {
                    (format!("✔ Actif dans le bureau : {}", s.keys), Color32::from_rgb(110, 200, 120))
                } else {
                    (
                        format!(
                            "✖ {} pointe vers un programme introuvable ({}) : cliquez sur « Appliquer ».",
                            s.keys,
                            s.program.as_ref().map(|p| p.display().to_string()).unwrap_or_default()
                        ),
                        Color32::from_rgb(230, 110, 110),
                    )
                };
                ui.label(RichText::new(txt).color(color));
                ui.horizontal(|ui| {
                    if ui.small_button("Supprimer le raccourci du bureau").clicked() {
                        out.remove_quake_shortcut = true;
                    }
                    if ui.small_button("Actualiser l'état").clicked() {
                        st.quake_status = None;
                    }
                });
            }
            None => {
                ui.label(RichText::new("Aucun raccourci créé pour l'instant.").color(Color32::GRAY));
            }
        }
        if let Some(n) = &st.quake_note {
            ui.label(RichText::new(n).color(Color32::from_rgb(230, 180, 90)));
        }
        hint(
            ui,
            "Cinnamon et GNOME sont configurés automatiquement ; changer de touche remplace l'ancienne. \
             Ailleurs : liez la commande « novaterm --toggle » à une touche.",
        );
        if ui.button("Essayer maintenant").clicked() {
            out.action = Some(Action::QuickTerminal);
        }
    });
}

fn page_snippets(ui: &mut egui::Ui, config: &mut Config, bindings: &Bindings, out: &mut SettingsOut) {
    ui.horizontal(|ui| {
        if ui.button(RichText::new("+ Nouveau snippet").strong()).clicked() {
            config.snippets.push(Snippet { name: String::new(), command: String::new(), run: false });
        }
        hint(
            ui,
            &format!(
                "Ouverture : {} ou palette ({}). Variables : {{clipboard}} {{selection}} {{cwd}}",
                bindings.first_shortcut(Action::Snippets),
                bindings.first_shortcut(Action::CommandPalette)
            ),
        );
    });
    ui.add_space(8.0);
    if config.snippets.is_empty() {
        card(ui, "Aucun snippet", |ui| {
            ui.label("Ajoutez vos commandes fréquentes (déploiement, logs, docker…) pour les lancer en deux touches.");
        });
    }
    enum Op {
        Up(usize),
        Down(usize),
        Dup(usize),
        Del(usize),
    }
    let mut op = None;
    let n = config.snippets.len();
    for (i, sn) in config.snippets.iter_mut().enumerate() {
        let v = ui.visuals().clone();
        egui::Frame::NONE
            .fill(if v.dark_mode { Color32::from_white_alpha(6) } else { Color32::from_black_alpha(8) })
            .stroke(egui::Stroke::new(1.0, v.widgets.noninteractive.bg_stroke.color.gamma_multiply(0.6)))
            .corner_radius(10)
            .inner_margin(egui::Margin::symmetric(14, 10))
            .outer_margin(egui::Margin { left: 0, right: 0, top: 0, bottom: 8 })
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.label(RichText::new("▶").color(v.selection.bg_fill));
                    let buttons_w = 230.0;
                    ui.add(
                        egui::TextEdit::singleline(&mut sn.name)
                            .hint_text("Nom affiché dans la palette (ex. Logs du serveur)")
                            .font(egui::TextStyle::Body)
                            .desired_width((ui.available_width() - buttons_w).max(120.0)),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.small_button("✕").on_hover_text("Supprimer").clicked() {
                            op = Some(Op::Del(i));
                        }
                        if ui.small_button("⧉").on_hover_text("Dupliquer").clicked() {
                            op = Some(Op::Dup(i));
                        }
                        if ui.add_enabled(i + 1 < n, egui::Button::new("▼").small()).on_hover_text("Descendre").clicked() {
                            op = Some(Op::Down(i));
                        }
                        if ui.add_enabled(i > 0, egui::Button::new("▲").small()).on_hover_text("Monter").clicked() {
                            op = Some(Op::Up(i));
                        }
                        if ui
                            .add_enabled(!sn.command.trim().is_empty(), egui::Button::new("▶ Tester").small())
                            .on_hover_text("Lancer dans le panneau actif")
                            .clicked()
                        {
                            out.run_snippet = Some(i);
                        }
                    });
                });
                let rows = if sn.command.contains('\n') { 3 } else { 1 };
                ui.add(
                    egui::TextEdit::multiline(&mut sn.command)
                        .font(egui::TextStyle::Monospace)
                        .hint_text("Commande (ex. docker compose logs -f --tail 100)")
                        .desired_rows(rows)
                        .desired_width(f32::INFINITY),
                );
                ui.checkbox(&mut sn.run, "Exécuter directement (sinon la commande est seulement insérée, à valider avec Entrée)");
            });
    }
    match op {
        Some(Op::Up(i)) => config.snippets.swap(i, i - 1),
        Some(Op::Down(i)) => config.snippets.swap(i, i + 1),
        Some(Op::Dup(i)) => {
            let mut c = config.snippets[i].clone();
            c.name = format!("{} (copie)", c.name);
            config.snippets.insert(i + 1, c);
        }
        Some(Op::Del(i)) => {
            config.snippets.remove(i);
        }
        None => {}
    }
}

fn page_advanced(ui: &mut egui::Ui, config: &mut Config, st: &mut SettingsUi, out: &mut SettingsOut) {
    card(ui, "Dépannage", |ui| {
        hint(
            ui,
            "Si une police ou un réglage empêche NovaTerm de démarrer, il repasse tout seul en mode sans échec. \
             Vous pouvez aussi lancer : novaterm --safe-mode  ou  novaterm --reset-config",
        );
        if ui.button("Revenir à la police intégrée").clicked() {
            config.font_family.clear();
        }
        if ui.button("Ouvrir config.toml dans l'éditeur").clicked() {
            out.action = Some(Action::OpenConfig);
        }
        if ui.button("Recharger la configuration depuis le fichier").clicked() {
            out.action = Some(Action::ReloadConfig);
        }
    });
    if let Some(app) = crate::appimage::current() {
        card(ui, "AppImage", |ui| {
            ui.label(format!("Fichier : {}", app.display()));
            let integrated = crate::appimage::is_integrated(&app);
            ui.label(if integrated { "✔ Présent dans le menu des applications." } else { "Pas encore dans le menu des applications." });
            ui.horizontal_wrapped(|ui| {
                if ui.button("Ranger dans ~/Applications et ajouter au menu").clicked() {
                    st.appimage_note = Some(match crate::appimage::integrate(true) {
                        Ok(p) => format!("Fait : {}", p.display()),
                        Err(e) => e,
                    });
                }
                if ui.button("Ajouter au menu sans déplacer").clicked() {
                    st.appimage_note = Some(crate::appimage::integrate(false).map(|_| "Fait.".to_string()).unwrap_or_else(|e| e));
                }
                if integrated && ui.button("Retirer du menu").clicked() {
                    st.appimage_note = Some(crate::appimage::unintegrate().map(|_| "Retiré du menu.".to_string()).unwrap_or_else(|e| e));
                }
            });
            ui.checkbox(&mut config.appimage_prompt, "Proposer l'ajout au menu au démarrage");
            if let Some(n) = &st.appimage_note {
                ui.label(RichText::new(n).color(Color32::from_rgb(230, 180, 90)));
            }
        });
    }
    card(ui, "Informations", |ui| {
        ui.label(format!("NovaTerm {} · {} {}", env!("CARGO_PKG_VERSION"), std::env::consts::OS, std::env::consts::ARCH));
        ui.label(format!("Shell : {}", if config.shell.is_empty() { platform::default_shell() } else { config.shell.clone() }));
    });
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
    card(ui, "Raccourcis clavier", |ui| {
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

    pub fn new_any() -> Self {
        let dir = dirs::download_dir().or_else(dirs::home_dir).unwrap_or_else(|| PathBuf::from("/"));
        Self { dir }
    }
}

pub fn is_theme_file(path: &std::path::Path) -> bool {
    matches!(
        path.extension().and_then(|e| e.to_str()).map(|e| e.to_ascii_lowercase()).as_deref(),
        Some("itermcolors" | "toml" | "yml" | "yaml" | "conf" | "json" | "xresources" | "theme")
    ) || path.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with(".Xresources"))
}

/// Sélecteur de fichier générique : retourne le fichier choisi.
pub fn show_file_picker(
    ctx: &egui::Context,
    browser: &mut Option<FileBrowser>,
    title: &str,
    filter: fn(&std::path::Path) -> bool,
) -> Option<PathBuf> {
    let mut chosen = None;
    let mut still_open = browser.is_some();
    if let Some(b) = browser {
        egui::Window::new(title)
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
                let mut dirs_list = Vec::new();
                let mut files = Vec::new();
                if let Ok(entries) = std::fs::read_dir(&b.dir) {
                    for entry in entries.flatten() {
                        let path = entry.path();
                        if path.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with('.') && !filter(&path)) {
                            continue;
                        }
                        if path.is_dir() {
                            dirs_list.push(path);
                        } else if filter(&path) {
                            files.push(path);
                        }
                    }
                }
                dirs_list.sort();
                files.sort();
                egui::ScrollArea::vertical().show(ui, |ui| {
                    for d in dirs_list {
                        let name = d.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                        if ui.selectable_label(false, format!("📁 {name}")).clicked() {
                            b.dir = d;
                        }
                    }
                    for file in files {
                        let name = file.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                        if ui.selectable_label(false, format!("📄 {name}")).clicked() {
                            chosen = Some(file);
                        }
                    }
                });
            });
    }
    if chosen.is_some() || !still_open {
        *browser = None;
    }
    chosen
}

fn is_image(path: &std::path::Path) -> bool {
    matches!(
        path.extension().and_then(|e| e.to_str()).map(|e| e.to_ascii_lowercase()).as_deref(),
        Some("png" | "jpg" | "jpeg" | "webp" | "bmp" | "gif" | "ico" | "svg" | "svgz")
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
