//! Langue de l'interface. Le français est la langue de référence du code ; la
//! traduction anglaise est un dictionnaire « français → anglais »
//! (`assets/i18n/en.tsv`, une ligne par texte, séparateur tabulation).

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

/// Choix de langue dans les paramètres.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Language {
    /// Langue du système (LANG / LC_ALL…).
    Auto,
    French,
    English,
}

impl Language {
    pub fn label(self) -> &'static str {
        match self {
            Language::Auto => "Automatique (langue du système)",
            Language::French => "Français",
            Language::English => "English",
        }
    }
}

static ENGLISH: AtomicBool = AtomicBool::new(false);

/// La langue du système est-elle le français ?
fn system_is_french() -> bool {
    for var in ["LC_ALL", "LC_MESSAGES", "LANG", "LANGUAGE"] {
        if let Ok(v) = std::env::var(var) {
            if !v.is_empty() {
                return v.to_ascii_lowercase().starts_with("fr");
            }
        }
    }
    // Windows / macOS : pas de variable LANG en général.
    #[cfg(windows)]
    {
        if let Ok(out) = std::process::Command::new("powershell").args(["-NoProfile", "-Command", "(Get-Culture).Name"]).output() {
            return String::from_utf8_lossy(&out.stdout).to_ascii_lowercase().starts_with("fr");
        }
    }
    #[cfg(target_os = "macos")]
    {
        if let Ok(out) = std::process::Command::new("defaults").args(["read", "-g", "AppleLanguages"]).output() {
            return String::from_utf8_lossy(&out.stdout).to_ascii_lowercase().contains("fr");
        }
    }
    false
}

/// Applique le choix de langue (effet immédiat : les textes sont relus à chaque image).
pub fn set(lang: Language) {
    let english = match lang {
        Language::English => true,
        Language::French => false,
        Language::Auto => !system_is_french(),
    };
    ENGLISH.store(english, Ordering::Relaxed);
}

/// Applique le choix seulement s'il a changé (appelé à chaque image).
pub fn set_if_changed(lang: Language) {
    static LAST: std::sync::Mutex<Option<Language>> = std::sync::Mutex::new(None);
    let mut last = LAST.lock().unwrap_or_else(|e| e.into_inner());
    if *last != Some(lang) {
        *last = Some(lang);
        set(lang);
    }
}

pub fn is_english() -> bool {
    ENGLISH.load(Ordering::Relaxed)
}

fn table() -> &'static HashMap<&'static str, &'static str> {
    static T: OnceLock<HashMap<&'static str, &'static str>> = OnceLock::new();
    T.get_or_init(|| {
        include_str!("../assets/i18n/en.tsv")
            .lines()
            .filter(|l| !l.is_empty() && !l.starts_with('#'))
            .filter_map(|l| l.split_once('\t'))
            // « \n » dans le fichier = retour à la ligne dans le texte.
            .map(|(fr, en)| -> (&'static str, &'static str) {
                if fr.contains("\\n") {
                    (Box::leak(fr.replace("\\n", "\n").into_boxed_str()), Box::leak(en.replace("\\n", "\n").into_boxed_str()))
                } else {
                    (fr, en)
                }
            })
            .collect()
    })
}

/// Traduit un texte fixe de l'interface (le français est rendu tel quel).
pub fn t(fr: &'static str) -> &'static str {
    if !is_english() {
        return fr;
    }
    table().get(fr).copied().unwrap_or(fr)
}

/// Traduit un modèle puis insère les valeurs à la place des « {} », dans l'ordre.
pub fn tf(template: &'static str, args: &[&dyn std::fmt::Display]) -> String {
    let tpl = t(template);
    let mut out = String::with_capacity(tpl.len() + 16);
    let mut args = args.iter();
    let mut rest = tpl;
    while let Some(i) = rest.find("{}") {
        out.push_str(&rest[..i]);
        if let Some(a) = args.next() {
            out.push_str(&a.to_string());
        }
        rest = &rest[i + 2..];
    }
    out.push_str(rest);
    out
}

/// Traduit un texte calculé (même dictionnaire), pour les textes non fixes.
pub fn tr(fr: &str) -> String {
    if !is_english() {
        return fr.to_string();
    }
    table().get(fr).map(|s| s.to_string()).unwrap_or_else(|| fr.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dictionary_is_well_formed() {
        let text = include_str!("../assets/i18n/en.tsv");
        let mut seen = std::collections::HashSet::new();
        for (n, l) in text.lines().enumerate().filter(|(_, l)| !l.is_empty() && !l.starts_with('#')) {
            let (fr, en) = l.split_once('\t').unwrap_or_else(|| panic!("ligne {} sans tabulation : {l}", n + 1));
            assert!(!fr.trim().is_empty() && !en.trim().is_empty(), "ligne {} vide", n + 1);
            assert!(seen.insert(fr), "ligne {} : doublon « {fr} »", n + 1);
        }
    }

    #[test]
    fn templates() {
        set(Language::French);
        assert_eq!(tf("Ouverture de {}…", &[&"a.rs"]), "Ouverture de a.rs…");
    }

    #[test]
    fn switch() {
        set(Language::English);
        assert_eq!(t("Apparence"), "Appearance");
        assert_eq!(t("texte inconnu"), "texte inconnu");
        set(Language::French);
        assert_eq!(t("Apparence"), "Apparence");
    }
}
