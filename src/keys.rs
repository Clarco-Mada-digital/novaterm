//! Raccourcis clavier configurables : actions, combinaisons et liaisons par défaut.

use std::collections::BTreeMap;

use egui::{Key, Modifiers};

/// Toutes les actions déclenchables par un raccourci (ou la palette de commandes).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Action {
    // Onglets
    NewTab,
    CloseTab,
    NextTab,
    PrevTab,
    MoveTabLeft,
    MoveTabRight,
    RenameTab,
    LastTab,
    GotoTab1,
    GotoTab2,
    GotoTab3,
    GotoTab4,
    GotoTab5,
    GotoTab6,
    GotoTab7,
    GotoTab8,
    GotoTab9,
    // Panneaux
    SplitRight,
    SplitDown,
    ClosePane,
    FocusLeft,
    FocusRight,
    FocusUp,
    FocusDown,
    FocusNext,
    FocusPrev,
    ResizeLeft,
    ResizeRight,
    ResizeUp,
    ResizeDown,
    EqualizeSplits,
    ToggleZoom,
    PaneToTab,
    // Édition
    Copy,
    Paste,
    PasteSelection,
    SelectAll,
    Find,
    FindNext,
    FindPrev,
    Clear,
    ResetTerminal,
    // Défilement
    ScrollLineUp,
    ScrollLineDown,
    ScrollPageUp,
    ScrollPageDown,
    ScrollTop,
    ScrollBottom,
    // Police
    FontInc,
    FontDec,
    FontReset,
    // Fenêtre & application
    NewWindow,
    Fullscreen,
    ToggleMaximize,
    ToggleAlwaysOnTop,
    ToggleTabBar,
    ToggleStatusBar,
    NextTheme,
    OpacityUp,
    OpacityDown,
    CommandPalette,
    Settings,
    Help,
    OpenConfig,
    ReloadConfig,
    Quit,
    // Suggestions
    HistorySearch,
    ToggleSuggestions,
    // Blocs de commandes
    PrevCommand,
    NextCommand,
    CopyLastOutput,
    CopyLastCommand,
    RerunLast,
    // Outils
    ToggleBroadcast,
    Snippets,
    Profiles,
    SaveProfile,
    ImportTheme,
    QuickTerminal,
    // Aides
    FixLastCommand,
    ExplainCommand,
    CheckUpdates,
}

pub const ALL_ACTIONS: &[Action] = &[
    Action::NewTab,
    Action::CloseTab,
    Action::NextTab,
    Action::PrevTab,
    Action::MoveTabLeft,
    Action::MoveTabRight,
    Action::RenameTab,
    Action::LastTab,
    Action::GotoTab1,
    Action::GotoTab2,
    Action::GotoTab3,
    Action::GotoTab4,
    Action::GotoTab5,
    Action::GotoTab6,
    Action::GotoTab7,
    Action::GotoTab8,
    Action::GotoTab9,
    Action::SplitRight,
    Action::SplitDown,
    Action::ClosePane,
    Action::FocusLeft,
    Action::FocusRight,
    Action::FocusUp,
    Action::FocusDown,
    Action::FocusNext,
    Action::FocusPrev,
    Action::ResizeLeft,
    Action::ResizeRight,
    Action::ResizeUp,
    Action::ResizeDown,
    Action::EqualizeSplits,
    Action::ToggleZoom,
    Action::PaneToTab,
    Action::Copy,
    Action::Paste,
    Action::PasteSelection,
    Action::SelectAll,
    Action::Find,
    Action::FindNext,
    Action::FindPrev,
    Action::Clear,
    Action::ResetTerminal,
    Action::ScrollLineUp,
    Action::ScrollLineDown,
    Action::ScrollPageUp,
    Action::ScrollPageDown,
    Action::ScrollTop,
    Action::ScrollBottom,
    Action::FontInc,
    Action::FontDec,
    Action::FontReset,
    Action::NewWindow,
    Action::Fullscreen,
    Action::ToggleMaximize,
    Action::ToggleAlwaysOnTop,
    Action::ToggleTabBar,
    Action::ToggleStatusBar,
    Action::NextTheme,
    Action::OpacityUp,
    Action::OpacityDown,
    Action::CommandPalette,
    Action::Settings,
    Action::Help,
    Action::OpenConfig,
    Action::ReloadConfig,
    Action::Quit,
    Action::HistorySearch,
    Action::ToggleSuggestions,
    Action::PrevCommand,
    Action::NextCommand,
    Action::CopyLastOutput,
    Action::CopyLastCommand,
    Action::RerunLast,
    Action::ToggleBroadcast,
    Action::Snippets,
    Action::Profiles,
    Action::SaveProfile,
    Action::ImportTheme,
    Action::QuickTerminal,
    Action::FixLastCommand,
    Action::ExplainCommand,
    Action::CheckUpdates,
];

impl Action {
    /// Identifiant stable utilisé dans config.toml.
    pub fn id(self) -> &'static str {
        use Action::*;
        match self {
            NewTab => "new_tab",
            CloseTab => "close_tab",
            NextTab => "next_tab",
            PrevTab => "prev_tab",
            MoveTabLeft => "move_tab_left",
            MoveTabRight => "move_tab_right",
            RenameTab => "rename_tab",
            LastTab => "last_tab",
            GotoTab1 => "goto_tab_1",
            GotoTab2 => "goto_tab_2",
            GotoTab3 => "goto_tab_3",
            GotoTab4 => "goto_tab_4",
            GotoTab5 => "goto_tab_5",
            GotoTab6 => "goto_tab_6",
            GotoTab7 => "goto_tab_7",
            GotoTab8 => "goto_tab_8",
            GotoTab9 => "goto_tab_9",
            SplitRight => "split_right",
            SplitDown => "split_down",
            ClosePane => "close_pane",
            FocusLeft => "focus_left",
            FocusRight => "focus_right",
            FocusUp => "focus_up",
            FocusDown => "focus_down",
            FocusNext => "focus_next",
            FocusPrev => "focus_prev",
            ResizeLeft => "resize_left",
            ResizeRight => "resize_right",
            ResizeUp => "resize_up",
            ResizeDown => "resize_down",
            EqualizeSplits => "equalize_splits",
            ToggleZoom => "toggle_zoom",
            PaneToTab => "pane_to_tab",
            Copy => "copy",
            Paste => "paste",
            PasteSelection => "paste_selection",
            SelectAll => "select_all",
            Find => "find",
            FindNext => "find_next",
            FindPrev => "find_prev",
            Clear => "clear",
            ResetTerminal => "reset_terminal",
            ScrollLineUp => "scroll_line_up",
            ScrollLineDown => "scroll_line_down",
            ScrollPageUp => "scroll_page_up",
            ScrollPageDown => "scroll_page_down",
            ScrollTop => "scroll_top",
            ScrollBottom => "scroll_bottom",
            FontInc => "font_inc",
            FontDec => "font_dec",
            FontReset => "font_reset",
            NewWindow => "new_window",
            Fullscreen => "fullscreen",
            ToggleMaximize => "toggle_maximize",
            ToggleAlwaysOnTop => "toggle_always_on_top",
            ToggleTabBar => "toggle_tab_bar",
            ToggleStatusBar => "toggle_status_bar",
            NextTheme => "next_theme",
            OpacityUp => "opacity_up",
            OpacityDown => "opacity_down",
            CommandPalette => "command_palette",
            Settings => "settings",
            Help => "help",
            OpenConfig => "open_config",
            ReloadConfig => "reload_config",
            Quit => "quit",
            HistorySearch => "history_search",
            ToggleSuggestions => "toggle_suggestions",
            PrevCommand => "prev_command",
            NextCommand => "next_command",
            CopyLastOutput => "copy_last_output",
            CopyLastCommand => "copy_last_command",
            RerunLast => "rerun_last",
            ToggleBroadcast => "toggle_broadcast",
            Snippets => "snippets",
            Profiles => "profiles",
            SaveProfile => "save_profile",
            ImportTheme => "import_theme",
            QuickTerminal => "quick_terminal",
            FixLastCommand => "fix_last_command",
            ExplainCommand => "explain_command",
            CheckUpdates => "check_updates",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        ALL_ACTIONS.iter().copied().find(|a| a.id() == id)
    }

    /// Libellé affiché (aide, palette, paramètres).
    pub fn label(self) -> &'static str {
        use Action::*;
        match self {
            NewTab => "Nouvel onglet",
            CloseTab => "Fermer l'onglet",
            NextTab => "Onglet suivant",
            PrevTab => "Onglet précédent",
            MoveTabLeft => "Déplacer l'onglet à gauche",
            MoveTabRight => "Déplacer l'onglet à droite",
            RenameTab => "Renommer l'onglet",
            LastTab => "Dernier onglet",
            GotoTab1 => "Aller à l'onglet 1",
            GotoTab2 => "Aller à l'onglet 2",
            GotoTab3 => "Aller à l'onglet 3",
            GotoTab4 => "Aller à l'onglet 4",
            GotoTab5 => "Aller à l'onglet 5",
            GotoTab6 => "Aller à l'onglet 6",
            GotoTab7 => "Aller à l'onglet 7",
            GotoTab8 => "Aller à l'onglet 8",
            GotoTab9 => "Aller à l'onglet 9",
            SplitRight => "Diviser à droite",
            SplitDown => "Diviser en bas",
            ClosePane => "Fermer le panneau",
            FocusLeft => "Panneau de gauche",
            FocusRight => "Panneau de droite",
            FocusUp => "Panneau du haut",
            FocusDown => "Panneau du bas",
            FocusNext => "Panneau suivant",
            FocusPrev => "Panneau précédent",
            ResizeLeft => "Agrandir/réduire vers la gauche",
            ResizeRight => "Agrandir/réduire vers la droite",
            ResizeUp => "Agrandir/réduire vers le haut",
            ResizeDown => "Agrandir/réduire vers le bas",
            EqualizeSplits => "Égaliser les panneaux",
            ToggleZoom => "Zoomer le panneau (plein onglet)",
            PaneToTab => "Déplacer le panneau dans un nouvel onglet",
            Copy => "Copier",
            Paste => "Coller",
            PasteSelection => "Coller la sélection",
            SelectAll => "Tout sélectionner",
            Find => "Rechercher",
            FindNext => "Résultat suivant",
            FindPrev => "Résultat précédent",
            Clear => "Effacer l'écran et l'historique",
            ResetTerminal => "Réinitialiser le terminal",
            ScrollLineUp => "Défiler d'une ligne vers le haut",
            ScrollLineDown => "Défiler d'une ligne vers le bas",
            ScrollPageUp => "Page précédente de l'historique",
            ScrollPageDown => "Page suivante de l'historique",
            ScrollTop => "Début de l'historique",
            ScrollBottom => "Fin de l'historique",
            FontInc => "Agrandir la police",
            FontDec => "Réduire la police",
            FontReset => "Taille de police par défaut",
            NewWindow => "Nouvelle fenêtre",
            Fullscreen => "Plein écran",
            ToggleMaximize => "Agrandir / restaurer la fenêtre",
            ToggleAlwaysOnTop => "Toujours au premier plan",
            ToggleTabBar => "Afficher / masquer la barre d'onglets",
            ToggleStatusBar => "Afficher / masquer la barre de statut",
            NextTheme => "Thème suivant",
            OpacityUp => "Augmenter l'opacité",
            OpacityDown => "Diminuer l'opacité",
            CommandPalette => "Palette de commandes",
            Settings => "Paramètres",
            Help => "Aide des raccourcis",
            OpenConfig => "Ouvrir le fichier de configuration",
            ReloadConfig => "Recharger la configuration",
            Quit => "Quitter NovaTerm",
            HistorySearch => "Rechercher dans l'historique des commandes",
            ToggleSuggestions => "Activer / désactiver les suggestions",
            PrevCommand => "Commande précédente (remonter)",
            NextCommand => "Commande suivante (descendre)",
            CopyLastOutput => "Copier la sortie de la dernière commande",
            CopyLastCommand => "Copier la dernière commande",
            RerunLast => "Relancer la dernière commande",
            ToggleBroadcast => "Taper dans tous les panneaux (diffusion)",
            Snippets => "Commandes favorites (snippets)",
            Profiles => "Ouvrir un profil de projet",
            SaveProfile => "Enregistrer la disposition comme profil",
            ImportTheme => "Importer un thème…",
            QuickTerminal => "Terminal rapide (afficher / masquer)",
            FixLastCommand => "Corriger la dernière commande (astuce)",
            ExplainCommand => "Expliquer la commande (hors ligne)",
            CheckUpdates => "Rechercher une mise à jour",
        }
    }

    pub fn category(self) -> &'static str {
        use Action::*;
        match self {
            NewTab | CloseTab | NextTab | PrevTab | MoveTabLeft | MoveTabRight | RenameTab | LastTab
            | GotoTab1 | GotoTab2 | GotoTab3 | GotoTab4 | GotoTab5 | GotoTab6 | GotoTab7 | GotoTab8
            | GotoTab9 => "Onglets",
            SplitRight | SplitDown | ClosePane | FocusLeft | FocusRight | FocusUp | FocusDown | FocusNext
            | FocusPrev | ResizeLeft | ResizeRight | ResizeUp | ResizeDown | EqualizeSplits | ToggleZoom
            | PaneToTab => "Panneaux",
            Copy | Paste | PasteSelection | SelectAll | Find | FindNext | FindPrev | Clear
            | ResetTerminal => "Édition",
            ScrollLineUp | ScrollLineDown | ScrollPageUp | ScrollPageDown | ScrollTop | ScrollBottom => {
                "Défilement"
            }
            FontInc | FontDec | FontReset => "Police",
            HistorySearch | ToggleSuggestions => "Suggestions",
            PrevCommand | NextCommand | CopyLastOutput | CopyLastCommand | RerunLast => "Blocs de commandes",
            ToggleBroadcast | Snippets | Profiles | SaveProfile | ImportTheme | QuickTerminal | CheckUpdates => "Outils",
            FixLastCommand | ExplainCommand => "Aides",
            _ => "Fenêtre",
        }
    }

    /// Actions qui ne doivent être interceptées que lorsque la recherche est ouverte
    /// (sinon la touche est transmise au programme : F3 dans mc, etc.).
    pub fn only_when_searching(self) -> bool {
        matches!(self, Action::FindNext | Action::FindPrev)
    }

    /// L'action s'applique-t-elle dans ce contexte ? Sinon la touche part au programme.
    pub fn context_ok(self, ctx: &KeyContext) -> bool {
        match self {
            Action::FindNext | Action::FindPrev => ctx.searching,
            Action::PrevCommand | Action::NextCommand => ctx.prompt_nav,
            _ => true,
        }
    }
}

/// État utile pour décider si un raccourci « contextuel » s'applique.
#[derive(Clone, Copy, Debug, Default)]
pub struct KeyContext {
    /// La barre de recherche est ouverte.
    pub searching: bool,
    /// Le panneau a des blocs de commandes et n'est pas en plein écran (vim, htop…).
    pub prompt_nav: bool,
}

/// Une combinaison de touches (ex. Ctrl+Maj+T).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct KeyCombo {
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
    pub key: Key,
}

impl KeyCombo {
    pub fn new(ctrl: bool, shift: bool, alt: bool, key: Key) -> Self {
        Self { ctrl, shift, alt, key }
    }

    /// Analyse « Ctrl+Shift+T », « Alt+1 », « F11 »…
    pub fn parse(s: &str) -> Option<Self> {
        let mut combo = Self { ctrl: false, shift: false, alt: false, key: Key::Escape };
        let mut key = None;
        for part in s.split('+').map(str::trim) {
            if part.is_empty() {
                // « Ctrl++ » : la touche est « + ».
                key = Some(Key::Plus);
                continue;
            }
            match part.to_ascii_lowercase().as_str() {
                "ctrl" | "control" => combo.ctrl = true,
                "shift" | "maj" => combo.shift = true,
                "alt" => combo.alt = true,
                _ => key = Some(Key::from_name(part).or_else(|| Key::from_name(&part.to_ascii_uppercase()))?),
            }
        }
        combo.key = key?;
        Some(combo)
    }

    /// Forme sérialisée (stable, en anglais) pour config.toml.
    pub fn to_config_string(self) -> String {
        let mut s = String::new();
        if self.ctrl {
            s.push_str("Ctrl+");
        }
        if self.shift {
            s.push_str("Shift+");
        }
        if self.alt {
            s.push_str("Alt+");
        }
        s.push_str(self.key.name());
        s
    }

    /// Forme lisible en français.
    pub fn display(self) -> String {
        let mut s = String::new();
        if self.ctrl {
            s.push_str("Ctrl+");
        }
        if self.shift {
            s.push_str("Maj+");
        }
        if self.alt {
            s.push_str("Alt+");
        }
        let k = match self.key {
            Key::ArrowLeft => "←",
            Key::ArrowRight => "→",
            Key::ArrowUp => "↑",
            Key::ArrowDown => "↓",
            Key::PageUp => "PagePréc",
            Key::PageDown => "PageSuiv",
            Key::Home => "Début",
            Key::End => "Fin",
            Key::Insert => "Inser",
            Key::Delete => "Suppr",
            Key::Escape => "Échap",
            Key::Enter => "Entrée",
            Key::Space => "Espace",
            Key::Backspace => "Retour",
            Key::Plus => "+",
            Key::Minus => "-",
            Key::Equals => "=",
            Key::Comma => ",",
            Key::Period => ".",
            Key::Semicolon => ";",
            Key::Colon => ":",
            Key::Slash => "/",
            Key::Backslash => "\\",
            Key::Questionmark => "?",
            Key::Exclamationmark => "!",
            Key::OpenBracket => "[",
            Key::CloseBracket => "]",
            Key::Quote => "'",
            Key::Backtick => "`",
            other => other.name(),
        };
        s.push_str(k);
        s
    }

    /// Correspondance avec un événement clavier. Les chiffres sont aussi comparés à la
    /// touche physique (claviers AZERTY : Alt+& = Alt+1).
    pub fn matches(&self, key: Key, physical: Option<Key>, mods: &Modifiers) -> bool {
        if self.ctrl != mods.ctrl || self.shift != mods.shift || self.alt != mods.alt {
            return false;
        }
        if self.key == key {
            return true;
        }
        let is_digit = |k: Key| {
            matches!(
                k,
                Key::Num0 | Key::Num1 | Key::Num2 | Key::Num3 | Key::Num4 | Key::Num5 | Key::Num6 | Key::Num7
                    | Key::Num8 | Key::Num9
            )
        };
        is_digit(self.key) && physical == Some(self.key)
    }
}

fn c(s: &str) -> KeyCombo {
    KeyCombo::parse(s).unwrap_or_else(|| panic!("raccourci par défaut invalide : {s}"))
}

/// Raccourcis par défaut.
pub fn default_bindings(action: Action) -> Vec<KeyCombo> {
    use Action::*;
    let list: &[&str] = match action {
        NewTab => &["Ctrl+Shift+T"],
        CloseTab => &["Ctrl+Alt+W"],
        NextTab => &["Ctrl+Tab", "Ctrl+PageDown"],
        PrevTab => &["Ctrl+Shift+Tab", "Ctrl+PageUp"],
        MoveTabLeft => &["Ctrl+Shift+PageUp"],
        MoveTabRight => &["Ctrl+Shift+PageDown"],
        RenameTab => &["Ctrl+Shift+R", "F2"],
        LastTab => &["Alt+0"],
        GotoTab1 => &["Alt+1"],
        GotoTab2 => &["Alt+2"],
        GotoTab3 => &["Alt+3"],
        GotoTab4 => &["Alt+4"],
        GotoTab5 => &["Alt+5"],
        GotoTab6 => &["Alt+6"],
        GotoTab7 => &["Alt+7"],
        GotoTab8 => &["Alt+8"],
        GotoTab9 => &["Alt+9"],
        SplitRight => &["Ctrl+Shift+E"],
        SplitDown => &["Ctrl+Shift+O"],
        ClosePane => &["Ctrl+Shift+W"],
        FocusLeft => &["Ctrl+Shift+Left", "Alt+Left"],
        FocusRight => &["Ctrl+Shift+Right", "Alt+Right"],
        FocusUp => &["Ctrl+Shift+Up", "Alt+Up"],
        FocusDown => &["Ctrl+Shift+Down", "Alt+Down"],
        FocusNext => &["Ctrl+Shift+J"],
        FocusPrev => &["Ctrl+Shift+H"],
        ResizeLeft => &["Ctrl+Shift+Alt+Left"],
        ResizeRight => &["Ctrl+Shift+Alt+Right"],
        ResizeUp => &["Ctrl+Shift+Alt+Up"],
        ResizeDown => &["Ctrl+Shift+Alt+Down"],
        EqualizeSplits => &["Ctrl+Alt+E"],
        ToggleZoom => &["Ctrl+Shift+X"],
        PaneToTab => &["Ctrl+Shift+B"],
        Copy => &["Ctrl+Shift+C", "Ctrl+Insert"],
        Paste => &["Ctrl+Shift+V", "Shift+Insert"],
        PasteSelection => &["Ctrl+Shift+S"],
        SelectAll => &["Ctrl+Shift+A"],
        Find => &["Ctrl+Shift+F"],
        FindNext => &["F3"],
        FindPrev => &["Shift+F3"],
        Clear => &["Ctrl+Shift+K"],
        ResetTerminal => &["Ctrl+Alt+R"],
        ScrollLineUp => &["Ctrl+Shift+Y"],
        ScrollLineDown => &["Ctrl+Shift+D"],
        ScrollPageUp => &["Shift+PageUp"],
        ScrollPageDown => &["Shift+PageDown"],
        ScrollTop => &["Ctrl+Shift+Home"],
        ScrollBottom => &["Ctrl+Shift+End"],
        FontInc => &["Ctrl+Shift+Plus", "Ctrl+Shift+Equals", "Ctrl+Plus", "Ctrl+Equals"],
        FontDec => &["Ctrl+Shift+Minus", "Ctrl+Minus"],
        FontReset => &["Ctrl+Shift+0", "Ctrl+0"],
        NewWindow => &["Ctrl+Shift+N"],
        Fullscreen => &["F11"],
        ToggleMaximize => &["Ctrl+Shift+M"],
        ToggleAlwaysOnTop => &[],
        ToggleTabBar => &[],
        ToggleStatusBar => &[],
        NextTheme => &["Ctrl+Shift+G"],
        OpacityUp => &["Ctrl+Shift+Alt+Plus", "Ctrl+Shift+Alt+Equals"],
        OpacityDown => &["Ctrl+Shift+Alt+Minus"],
        CommandPalette => &["Ctrl+Shift+P"],
        Settings => &["Ctrl+Comma", "Ctrl+Shift+Comma"],
        Help => &["F1", "Ctrl+Shift+Questionmark"],
        OpenConfig => &[],
        ReloadConfig => &["Ctrl+Shift+F5"],
        Quit => &["Ctrl+Shift+Q"],
        HistorySearch => &["Ctrl+Alt+H"],
        ToggleSuggestions => &[],
        PrevCommand => &["Ctrl+Up"],
        NextCommand => &["Ctrl+Down"],
        CopyLastOutput => &["Ctrl+Alt+C"],
        CopyLastCommand => &[],
        RerunLast => &[],
        ToggleBroadcast => &["Ctrl+Shift+I"],
        Snippets => &["Ctrl+Alt+S"],
        Profiles => &["Ctrl+Alt+P"],
        SaveProfile => &[],
        ImportTheme => &[],
        QuickTerminal => &[],
        FixLastCommand => &["Ctrl+Alt+F"],
        ExplainCommand => &["Ctrl+Alt+X"],
        CheckUpdates => &[],
    };
    list.iter().map(|s| c(s)).collect()
}

/// Table résolue : action → combinaisons (défauts + surcharges de la config).
#[derive(Clone)]
pub struct Bindings {
    pub map: BTreeMap<Action, Vec<KeyCombo>>,
}

impl Bindings {
    /// `overrides` : action_id → liste de combinaisons (liste vide = désactivé).
    pub fn build(overrides: &BTreeMap<String, Vec<String>>) -> Self {
        for id in overrides.keys() {
            if Action::from_id(id).is_none() {
                eprintln!("NovaTerm: action inconnue dans [keybindings] : {id}");
            }
        }
        let mut map = BTreeMap::new();
        for &a in ALL_ACTIONS {
            let combos = match overrides.get(a.id()) {
                Some(list) => list.iter().filter_map(|s| KeyCombo::parse(s)).collect(),
                None => default_bindings(a),
            };
            map.insert(a, combos);
        }
        Self { map }
    }

    pub fn find(&self, key: Key, physical: Option<Key>, mods: &Modifiers, ctx: &KeyContext) -> Option<Action> {
        // Priorité à la touche logique exacte, puis aux chiffres physiques.
        let exact = self
            .map
            .iter()
            .find(|(a, combos)| a.context_ok(ctx) && combos.iter().any(|c| c.key == key && c.matches(key, None, mods)));
        if let Some((a, _)) = exact {
            return Some(*a);
        }
        self.map
            .iter()
            .find(|(a, combos)| a.context_ok(ctx) && combos.iter().any(|c| c.matches(key, physical, mods)))
            .map(|(a, _)| *a)
    }

    pub fn shortcut_text(&self, action: Action) -> String {
        self.map
            .get(&action)
            .map(|v| v.iter().map(|c| c.display()).collect::<Vec<_>>().join("  /  "))
            .unwrap_or_default()
    }

    pub fn first_shortcut(&self, action: Action) -> String {
        self.map
            .get(&action)
            .and_then(|v| v.first())
            .map(|c| c.display())
            .unwrap_or_default()
    }

    /// Actions utilisant déjà cette combinaison (pour signaler les conflits).
    pub fn conflicts(&self, combo: &KeyCombo, except: Action) -> Vec<Action> {
        self.map
            .iter()
            .filter(|(a, v)| **a != except && v.contains(combo))
            .map(|(a, _)| *a)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_parse() {
        for &a in ALL_ACTIONS {
            let _ = default_bindings(a);
            assert_eq!(Action::from_id(a.id()), Some(a));
        }
    }

    #[test]
    fn roundtrip() {
        let combo = KeyCombo::parse("Ctrl+Shift+T").unwrap();
        assert!(combo.ctrl && combo.shift && !combo.alt && combo.key == Key::T);
        assert_eq!(KeyCombo::parse(&combo.to_config_string()), Some(combo));
        assert_eq!(KeyCombo::parse("alt+1").unwrap().key, Key::Num1);
        assert_eq!(KeyCombo::parse("Ctrl++").unwrap().key, Key::Plus);
    }

    #[test]
    fn find_and_azerty_digits() {
        let b = Bindings::build(&BTreeMap::new());
        let m = Modifiers { ctrl: true, shift: true, ..Default::default() };
        assert_eq!(b.find(Key::T, None, &m, &KeyContext::default()), Some(Action::NewTab));
        // Alt+& (AZERTY) : touche logique inconnue → touche physique 1.
        let alt = Modifiers { alt: true, ..Default::default() };
        assert_eq!(b.find(Key::Quote, Some(Key::Num3), &alt, &KeyContext::default()), Some(Action::GotoTab3));
        // F3 n'est intercepté que pendant une recherche.
        assert_eq!(b.find(Key::F3, None, &Modifiers::default(), &KeyContext::default()), None);
        assert_eq!(b.find(Key::F3, None, &Modifiers::default(), &KeyContext { searching: true, prompt_nav: false }), Some(Action::FindNext));
        // Ctrl+C seul n'est pas un raccourci : il doit aller au terminal (SIGINT).
        let ctrl = Modifiers { ctrl: true, ..Default::default() };
        assert_eq!(b.find(Key::C, None, &ctrl, &KeyContext::default()), None);
    }
}
