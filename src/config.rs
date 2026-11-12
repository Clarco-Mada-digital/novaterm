//! Configuration persistante (TOML) et préférences utilisateur.

use crate::i18n::t;
use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::history::Ranking;
use crate::session::Profile;
use crate::theme::{self, Theme};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CursorShape {
    Block,
    Beam,
    Underline,
}

impl CursorShape {
    pub fn label(self) -> &'static str {
        match self {
            Self::Block => t("Bloc"),
            Self::Beam => t("Barre"),
            Self::Underline => t("Souligné"),
        }
    }

    pub fn to_alacritty(self) -> alacritty_terminal::vte::ansi::CursorShape {
        use alacritty_terminal::vte::ansi::CursorShape as A;
        match self {
            Self::Block => A::Block,
            Self::Beam => A::Beam,
            Self::Underline => A::Underline,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum BlinkStyle {
    Blink,
    Fade,
    Pulse,
    Flip,
}

impl BlinkStyle {
    pub fn label(self) -> &'static str {
        match self {
            Self::Blink => t("Clignotement"),
            Self::Fade => t("Fondu"),
            Self::Pulse => t("Pulsation"),
            Self::Flip => t("Rotation verticale"),
        }
    }
}

/// Action du clic droit dans un panneau.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RightClick {
    Menu,
    Paste,
    CopyOrPaste,
}

impl RightClick {
    pub fn label(self) -> &'static str {
        match self {
            Self::Menu => t("Menu contextuel"),
            Self::Paste => t("Coller"),
            Self::CopyOrPaste => t("Copier la sélection, sinon coller"),
        }
    }
}

/// Signal sonore du terminal (BEL).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum BellMode {
    None,
    Visual,
    Urgent,
}

impl BellMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::None => t("Désactivé"),
            Self::Visual => t("Flash visuel"),
            Self::Urgent => t("Flash + attention de la fenêtre"),
        }
    }
}

/// Badge de statut / durée sur les blocs de commandes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum BadgeMode {
    Always,
    Notable,
    Never,
}

impl BadgeMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::Always => t("Toujours"),
            Self::Notable => t("Échecs et commandes de plus de 2 s"),
            Self::Never => t("Jamais"),
        }
    }
}

/// Restauration de la session au démarrage.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RestoreMode {
    Off,
    Layout,
    LayoutAndContent,
}

impl RestoreMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::Off => t("Non (nouvelle session vide)"),
            Self::Layout => t("Onglets, splits, dossiers et noms"),
            Self::LayoutAndContent => t("Tout, avec le texte affiché"),
        }
    }
}

/// Changement automatique de thème clair / sombre.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AutoTheme {
    Off,
    System,
    Schedule,
}

impl AutoTheme {
    pub fn label(self) -> &'static str {
        match self {
            Self::Off => t("Désactivé"),
            Self::System => t("Suivre le système"),
            Self::Schedule => t("Selon l'heure"),
        }
    }
}

/// Portée de la diffusion du clavier.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum BroadcastScope {
    Tab,
    All,
}

impl BroadcastScope {
    pub fn label(self) -> &'static str {
        match self {
            Self::Tab => t("Panneaux de l'onglet"),
            Self::All => t("Tous les panneaux de tous les onglets"),
        }
    }
}

/// Commande favorite (palette de commandes).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Snippet {
    pub name: String,
    pub command: String,
    /// Exécuter directement (sinon simplement insérer).
    #[serde(default)]
    pub run: bool,
}

/// Touche(s) pour accepter une suggestion.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AcceptKey {
    RightOrEnd,
    Right,
    Tab,
}

impl AcceptKey {
    pub fn label(self) -> &'static str {
        match self {
            Self::RightOrEnd => t("→ ou Fin"),
            Self::Right => t("→ seulement"),
            Self::Tab => t("Tab (remplace la complétion du shell)"),
        }
    }
}

/// Répertoire de départ des nouveaux onglets / panneaux.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum StartDir {
    Inherit,
    Home,
    Custom,
}

impl StartDir {
    pub fn label(self) -> &'static str {
        match self {
            Self::Inherit => t("Celui du panneau actif"),
            Self::Home => t("Dossier personnel"),
            Self::Custom => t("Dossier personnalisé"),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Famille de police (vide = JetBrains Mono intégrée).
    pub font_family: String,
    pub font_size: f32,
    /// Espacement horizontal supplémentaire entre les caractères (px).
    pub letter_spacing: f32,
    pub line_spacing: f32,
    pub padding: f32,
    /// Opacité du fond (0.15 = très transparent, 1.0 = opaque).
    pub opacity: f32,
    pub bg_image: Option<String>,
    pub bg_image_opacity: f32,
    /// true = étiré, false = couvrir (recadré).
    pub bg_image_stretch: bool,
    pub theme_name: String,
    /// Thème personnalisé (prioritaire sur theme_name si présent).
    pub custom_theme: Option<Theme>,
    pub cursor_shape: CursorShape,
    pub cursor_blink: bool,
    /// Style d'animation du curseur (si cursor_blink).
    pub cursor_blink_style: BlinkStyle,
    /// Déplacement fluide du curseur (effet « trail/smear »).
    pub cursor_smooth: bool,
    /// Laisser les applications (vim…) changer la forme du curseur.
    pub cursor_app_shape: bool,
    /// Notifications animées en bas de la fenêtre (toasts).
    pub toasts: bool,
    pub scrollback: usize,
    /// Vide = shell par défaut ($SHELL).
    pub shell: String,
    pub show_status_bar: bool,
    /// true = barre de titre dessinée par NovaTerm, false = décorations du système.
    pub custom_decorations: bool,
    /// Titre de la barre personnalisée (vide = titre du panneau actif).
    pub custom_title: String,
    /// Rayon des coins arrondis de la fenêtre (décorations personnalisées).
    pub corner_radius: f32,

    // --- Fenêtre ---
    pub window_width: f32,
    pub window_height: f32,
    /// Mémoriser la taille de la fenêtre à la fermeture.
    pub remember_window_size: bool,
    pub start_maximized: bool,
    pub start_fullscreen: bool,
    pub always_on_top: bool,
    pub show_tab_bar: bool,
    /// Masquer la barre d'onglets lorsqu'il n'y a qu'un seul onglet.
    pub hide_single_tab: bool,
    /// Assombrir les panneaux inactifs (0 = non).
    pub dim_inactive: f32,
    /// Demander confirmation avant de fermer si un programme tourne encore.
    pub confirm_close: bool,

    // --- Souris ---
    /// Le panneau sous la souris reçoit le focus.
    pub focus_follows_mouse: bool,
    /// Copier automatiquement la sélection.
    pub copy_on_select: bool,
    /// Clic du milieu = coller.
    pub middle_click_paste: bool,
    pub right_click: RightClick,
    /// Multiplicateur de défilement à la molette.
    pub scroll_multiplier: f32,
    /// Ctrl+molette = zoom de la police.
    pub ctrl_scroll_zoom: bool,
    /// Ctrl+clic ouvre les liens (http, https, file…).
    pub ctrl_click_links: bool,
    /// Transmettre la souris aux programmes qui la demandent (htop, vim, mc…).
    pub mouse_reporting: bool,

    // --- Terminal ---
    pub bell: BellMode,
    pub start_dir: StartDir,
    pub custom_start_dir: String,
    /// Fermer le panneau automatiquement à la fin du shell.
    pub close_on_exit: bool,
    /// Ctrl+C copie s'il y a une sélection (sinon interrompt le programme).
    pub smart_copy: bool,
    /// Ctrl+V colle (au lieu d'envoyer ^V au programme).
    pub ctrl_v_paste: bool,
    /// Caractères qui délimitent un mot pour le double-clic.
    pub word_separators: String,
    /// Avertir avant de coller plusieurs lignes.
    pub warn_multiline_paste: bool,

    // --- Suggestions ---
    /// Injecter l'intégration shell (zsh) : nécessaire aux suggestions.
    pub shell_integration: bool,
    /// Texte fantôme : suggestion de fin de commande depuis l'historique.
    pub suggestions: bool,
    /// Liste déroulante de suggestions sous le curseur.
    pub suggest_list: bool,
    pub suggest_accept: AcceptKey,
    /// Ctrl+→ accepte mot par mot.
    pub suggest_word_accept: bool,
    pub suggest_ranking: Ranking,
    /// Nombre de lignes de la liste.
    pub suggest_max: usize,
    /// Nombre de caractères tapés avant de suggérer.
    pub suggest_min_chars: usize,
    /// Opacité du texte fantôme.
    pub suggest_opacity: f32,
    pub history_zsh: bool,
    pub history_bash: bool,
    /// Débuts de commandes à ne jamais suggérer (un par ligne).
    pub suggest_ignore: String,

    /// Complétion intelligente (sous-commandes, options, branches git, scripts npm…).
    pub completions: bool,

    // --- Blocs de commandes & notifications ---
    pub blocks_enabled: bool,
    /// Barre colorée à gauche de chaque commande (vert / rouge / en cours).
    pub blocks_gutter: bool,
    pub blocks_badges: BadgeMode,
    /// Fine ligne de séparation entre les commandes.
    pub blocks_separator: bool,
    /// Boutons au survol d'un bloc (copier la sortie, relancer…).
    pub blocks_actions: bool,
    pub notify_long_commands: bool,
    pub notify_threshold_secs: u32,
    /// Notifier seulement si la fenêtre n'est pas au premier plan (ou l'onglet caché).
    pub notify_only_unfocused: bool,
    pub notify_sound: bool,
    /// Autoriser les programmes à envoyer des notifications (OSC 9 / 777).
    pub notify_from_apps: bool,
    /// Pastille sur les onglets en arrière-plan qui ont du nouveau.
    pub tab_activity: bool,

    // --- Images ---
    pub images_enabled: bool,
    /// Protocole Kitty : autoriser la lecture d'images locales par chemin.
    pub images_allow_files: bool,

    // --- Session & profils ---
    pub restore_session: RestoreMode,
    pub restore_lines: usize,
    /// Sauvegarde automatique de la session (secondes, 0 = seulement à la fermeture).
    pub autosave_secs: u32,
    pub profiles: Vec<Profile>,
    /// Profil ouvert au démarrage (vide = aucun).
    pub startup_profile: String,

    // --- Liens & éditeur ---
    /// Commande d'ouverture des fichiers ({file}, {line}, {col} ; vide = système).
    pub editor_command: String,
    /// Détecter les chemins de fichiers et traces d'erreur.
    pub detect_paths: bool,
    /// Ouvrir les liens d'un simple clic (sinon Ctrl+clic).
    pub link_plain_click: bool,

    // --- Terminal rapide ---
    pub quake_height: f32,
    pub quake_width: f32,
    pub quake_hide_on_unfocus: bool,
    pub quake_hotkey: String,

    // --- Performances ---
    /// Arrêter le clignotement du curseur après N secondes d'inactivité (0 = jamais).
    pub cursor_blink_timeout: u32,

    /// Langue de l'interface.
    pub language: crate::i18n::Language,

    // --- Mises à jour ---
    /// Vérifier régulièrement si une nouvelle version est publiée (GitHub).
    pub update_check: bool,
    /// Proposer aussi les versions bêta.
    pub update_prerelease: bool,
    /// Version ignorée par l'utilisateur.
    pub update_skip: String,

    // --- Aides sans IA ---
    /// Proposer une correction quand une commande est mal tapée.
    pub help_typos: bool,
    /// Astuces sur les commandes en échec (port occupé, module manquant…).
    pub help_error_tips: bool,
    /// « Expliquer la commande » (pages man, tldr).
    pub help_explain: bool,

    /// Proposer d'ajouter l'AppImage au menu au lancement.
    pub appimage_prompt: bool,

    // --- Sécurité ---
    /// Les programmes peuvent copier dans le presse-papiers (OSC 52, vim, tmux…).
    pub osc52_write: bool,
    /// Les programmes peuvent lire le presse-papiers (OSC 52) — risqué via SSH.
    pub osc52_read: bool,

    // --- Divers ---
    pub broadcast_scope: BroadcastScope,
    pub snippets: Vec<Snippet>,
    pub user_themes: Vec<Theme>,
    pub auto_theme: AutoTheme,
    pub light_theme: String,
    pub dark_theme: String,
    pub day_hour: u32,
    pub night_hour: u32,

    /// Raccourcis personnalisés : action → combinaisons (liste vide = désactivé).
    pub keybindings: BTreeMap<String, Vec<String>>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            font_size: 14.0,
            line_spacing: 1.2,
            padding: 10.0,
            opacity: 0.94,
            bg_image: None,
            bg_image_opacity: 0.35,
            bg_image_stretch: false,
            theme_name: "Nova Dark".to_string(),
            custom_theme: None,
            cursor_shape: CursorShape::Block,
            cursor_blink: true,
            cursor_blink_style: BlinkStyle::Blink,
            cursor_smooth: true,
            cursor_app_shape: false,
            toasts: true,
            scrollback: 10_000,
            shell: String::new(),
            show_status_bar: true,
            custom_decorations: true,
            custom_title: String::new(),
            corner_radius: 12.0,
            font_family: String::new(),
            letter_spacing: 0.0,
            window_width: 1100.0,
            window_height: 700.0,
            remember_window_size: true,
            start_maximized: false,
            start_fullscreen: false,
            always_on_top: false,
            show_tab_bar: true,
            hide_single_tab: false,
            dim_inactive: 0.25,
            confirm_close: true,
            focus_follows_mouse: false,
            copy_on_select: false,
            middle_click_paste: true,
            right_click: RightClick::Menu,
            scroll_multiplier: 3.0,
            ctrl_scroll_zoom: true,
            ctrl_click_links: true,
            mouse_reporting: true,
            bell: BellMode::Visual,
            start_dir: StartDir::Inherit,
            custom_start_dir: String::new(),
            close_on_exit: true,
            smart_copy: false,
            ctrl_v_paste: false,
            word_separators: ",│`|:\"' ()[]{}<>\t".to_string(),
            warn_multiline_paste: false,
            keybindings: BTreeMap::new(),
            shell_integration: true,
            suggestions: true,
            suggest_list: false,
            suggest_accept: AcceptKey::RightOrEnd,
            suggest_word_accept: true,
            suggest_ranking: Ranking::Recent,
            suggest_max: 6,
            suggest_min_chars: 1,
            suggest_opacity: 0.45,
            history_zsh: true,
            history_bash: true,
            suggest_ignore: "rm -rf\n".to_string(),
            completions: true,
            blocks_enabled: true,
            blocks_gutter: true,
            blocks_badges: BadgeMode::Notable,
            blocks_separator: true,
            blocks_actions: true,
            notify_long_commands: true,
            notify_threshold_secs: 10,
            notify_only_unfocused: true,
            notify_sound: false,
            notify_from_apps: true,
            tab_activity: true,
            images_enabled: true,
            images_allow_files: true,
            restore_session: RestoreMode::Layout,
            restore_lines: 300,
            autosave_secs: 60,
            profiles: Vec::new(),
            startup_profile: String::new(),
            editor_command: crate::links::detect_editor(),
            detect_paths: true,
            link_plain_click: false,
            quake_height: 0.45,
            quake_width: 1.0,
            quake_hide_on_unfocus: false,
            quake_hotkey: "F12".to_string(),
            cursor_blink_timeout: 15,
            appimage_prompt: true,
            language: crate::i18n::Language::Auto,
            update_check: true,
            update_prerelease: true,
            update_skip: String::new(),
            help_typos: true,
            help_error_tips: true,
            help_explain: true,
            osc52_write: true,
            osc52_read: false,
            broadcast_scope: BroadcastScope::Tab,
            snippets: vec![
                Snippet { name: "Espace disque".into(), command: "df -h".into(), run: true },
                Snippet { name: "Processus les plus gourmands".into(), command: "ps aux --sort=-%cpu | head -15".into(), run: true },
                Snippet { name: "Ports en écoute".into(), command: "ss -tulpn".into(), run: true },
            ],
            user_themes: Vec::new(),
            auto_theme: AutoTheme::Off,
            light_theme: "Solarized Light".to_string(),
            dark_theme: "Nova Dark".to_string(),
            day_hour: 7,
            night_hour: 19,
        }
    }
}

impl Config {
    pub fn path() -> Option<PathBuf> {
        dirs::config_dir().map(|d| d.join("novaterm").join("config.toml"))
    }

    pub fn load() -> Self {
        Self::load_checked().0
    }

    /// Charge la configuration. Un fichier invalide n'est jamais écrasé en silence :
    /// il est mis de côté (`config.toml.invalide`) et un message est retourné.
    pub fn load_checked() -> (Self, Option<String>) {
        let Some(p) = Self::path() else { return (Self::default(), None) };
        let Ok(text) = std::fs::read_to_string(&p) else { return (Self::default(), None) };
        match toml::from_str(&text) {
            Ok(c) => (c, None),
            Err(e) => {
                let backup = p.with_extension("toml.invalide");
                let _ = std::fs::copy(&p, &backup);
                let first = e.to_string().lines().next().unwrap_or_default().to_string();
                (
                    Self::default(),
                    Some(format!(
                        "Configuration invalide ({}) : réglages par défaut utilisés. Votre fichier est conservé dans « {} ».",
                        first.replace("TOML parse error at", "erreur"),
                        backup.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()
                    )),
                )
            }
        }
    }

    pub fn save(&self) {
        if let Some(p) = Self::path() {
            if let Some(parent) = p.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            if let Ok(s) = toml::to_string_pretty(self) {
                // Écriture atomique : jamais de fichier à moitié écrit.
                let tmp = p.with_extension("toml.tmp");
                if std::fs::write(&tmp, s).is_ok() {
                    let _ = std::fs::rename(tmp, p);
                }
            }
        }
    }

    /// Thème actif : personnalisé, sinon importé, sinon prédéfini.
    pub fn theme(&self) -> Theme {
        self.custom_theme.clone().unwrap_or_else(|| self.theme_named(&self.theme_name))
    }

    pub fn theme_named(&self, name: &str) -> Theme {
        self.user_themes.iter().find(|t| t.name == name).cloned().unwrap_or_else(|| theme::by_name(name))
    }

    /// Noms de tous les thèmes disponibles (prédéfinis puis importés).
    pub fn theme_names(&self) -> Vec<String> {
        theme::presets().into_iter().map(|t| t.name).chain(self.user_themes.iter().map(|t| t.name.clone())).collect()
    }
}