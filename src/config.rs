//! Configuration persistante (TOML) et préférences utilisateur.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::history::Ranking;
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
            Self::Block => "Bloc",
            Self::Beam => "Barre",
            Self::Underline => "Souligné",
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
            Self::Blink => "Clignotement",
            Self::Fade => "Fondu",
            Self::Pulse => "Pulsation",
            Self::Flip => "Rotation verticale",
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
            Self::Menu => "Menu contextuel",
            Self::Paste => "Coller",
            Self::CopyOrPaste => "Copier la sélection, sinon coller",
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
            Self::None => "Désactivé",
            Self::Visual => "Flash visuel",
            Self::Urgent => "Flash + attention de la fenêtre",
        }
    }
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
            Self::RightOrEnd => "→ ou Fin",
            Self::Right => "→ seulement",
            Self::Tab => "Tab (remplace la complétion du shell)",
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
            Self::Inherit => "Celui du panneau actif",
            Self::Home => "Dossier personnel",
            Self::Custom => "Dossier personnalisé",
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
        }
    }
}

impl Config {
    pub fn path() -> Option<PathBuf> {
        dirs::config_dir().map(|d| d.join("novaterm").join("config.toml"))
    }

    pub fn load() -> Self {
        Self::path()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|s| toml::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) {
        if let Some(p) = Self::path() {
            if let Some(parent) = p.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            if let Ok(s) = toml::to_string_pretty(self) {
                let _ = std::fs::write(p, s);
            }
        }
    }

    /// Thème actif : personnalisé sinon prédéfini.
    pub fn theme(&self) -> Theme {
        self.custom_theme
            .clone()
            .unwrap_or_else(|| theme::by_name(&self.theme_name))
    }
}