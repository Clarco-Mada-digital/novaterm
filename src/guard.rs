//! Garde-fou contre les plantages au démarrage (police cassée, etc.).
//!
//! Un marqueur est écrit avant une opération risquée (démarrage, changement de
//! police) et effacé quand l'affichage s'est bien déroulé. S'il est encore présent
//! au lancement suivant, l'opération a planté : NovaTerm démarre alors en mode
//! sans échec (police intégrée) et l'explique à l'utilisateur.

use std::path::PathBuf;

fn marker() -> Option<PathBuf> {
    let dir = dirs::cache_dir()?.join("novaterm");
    std::fs::create_dir_all(&dir).ok()?;
    Some(dir.join("startup.pending"))
}

/// Raison du plantage précédent, s'il y en a eu un (« startup », « font:Nom »…).
pub fn previous_crash() -> Option<String> {
    let path = marker()?;
    let reason = std::fs::read_to_string(&path).ok()?;
    let _ = std::fs::remove_file(path);
    Some(reason.trim().to_string())
}

/// Pose le marqueur avant une opération risquée.
pub fn arm(reason: &str) {
    if let Some(p) = marker() {
        let _ = std::fs::write(p, reason);
    }
}

/// L'opération s'est bien passée.
pub fn disarm() {
    if let Some(p) = marker() {
        let _ = std::fs::remove_file(p);
    }
}

/// Message lisible pour l'utilisateur.
pub fn explain(reason: &str) -> String {
    match reason.strip_prefix("font:") {
        Some(font) => format!(
            "La police « {font} » a fait planter NovaTerm. Mode sans échec : retour à la police intégrée."
        ),
        None => "NovaTerm ne s'est pas lancé correctement la dernière fois. Mode sans échec : police intégrée.".into(),
    }
}
