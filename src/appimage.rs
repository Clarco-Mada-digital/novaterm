//! AppImage : un seul fichier, à garder quelque part. NovaTerm propose de le ranger
//! dans ~/Applications et de l'ajouter au menu des applications (icône comprise),
//! et sait retirer cette intégration.

use std::path::{Path, PathBuf};

/// Chemin de l'AppImage en cours d'exécution (variable posée par le runtime AppImage).
pub fn current() -> Option<PathBuf> {
    if !cfg!(target_os = "linux") {
        return None;
    }
    std::env::var_os("APPIMAGE").map(PathBuf::from).filter(|p| p.is_file())
}

/// Chemin durable de l'exécutable : l'AppImage elle-même si on tourne depuis une
/// AppImage (son dossier `/tmp/.mount_…` disparaît à la fermeture), sinon le binaire.
pub fn stable_exe() -> Option<PathBuf> {
    current().or_else(|| std::env::current_exe().ok())
}

fn desktop_file() -> Option<PathBuf> {
    Some(dirs::data_dir()?.join("applications").join("novaterm.desktop"))
}

/// Dossier conventionnel des AppImage.
pub fn applications_dir() -> Option<PathBuf> {
    Some(dirs::home_dir()?.join("Applications"))
}

/// L'entrée de menu pointe-t-elle vers cette AppImage ?
pub fn is_integrated(app: &Path) -> bool {
    desktop_file()
        .and_then(|d| std::fs::read_to_string(d).ok())
        .is_some_and(|t| t.contains(&app.display().to_string()))
}

/// Contenu du fichier .desktop pour une AppImage donnée.
pub fn desktop_entry(app: &Path) -> String {
    let exec = format!("\"{}\"", app.display());
    include_str!("../assets/novaterm.desktop")
        .lines()
        .map(|l| match l.split_once('=') {
            Some(("Exec", rest)) => format!("Exec={}", rest.replacen("novaterm", &exec, 1)),
            _ => l.to_string(),
        })
        .chain([format!("X-AppImage-Path={}", app.display()), format!("TryExec={}", app.display())])
        .collect::<Vec<_>>()
        .join("\n")
        + "\n"
}

/// Ajoute au menu ; avec `move_to_apps`, range d'abord le fichier dans ~/Applications.
/// Retourne l'emplacement final de l'AppImage.
pub fn integrate(move_to_apps: bool) -> Result<PathBuf, String> {
    let src = current().ok_or("NovaTerm n'est pas lancé depuis une AppImage")?;
    let mut app = src.clone();
    if move_to_apps {
        let dir = applications_dir().ok_or("dossier personnel introuvable")?;
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let dest = dir.join("NovaTerm.AppImage");
        if dest != src {
            // Renommer si possible (même disque), sinon copier puis supprimer l'original.
            if std::fs::rename(&src, &dest).is_err() {
                std::fs::copy(&src, &dest).map_err(|e| format!("copie impossible : {e}"))?;
                let _ = std::fs::remove_file(&src);
            }
            app = dest;
        }
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&app, std::fs::Permissions::from_mode(0o755));
    }
    // Icônes (dessinées par NovaTerm) et entrée de menu.
    let icons = dirs::data_dir().ok_or("dossier de données introuvable")?.join("icons/hicolor");
    for size in [16u32, 32, 48, 64, 128, 256] {
        let dir = icons.join(format!("{size}x{size}/apps"));
        let _ = std::fs::create_dir_all(&dir);
        let _ = crate::icon::render(size).save(dir.join("novaterm.png"));
    }
    let _ = std::fs::create_dir_all(icons.join("scalable/apps"));
    let _ = std::fs::write(icons.join("scalable/apps/novaterm.svg"), crate::icon::SVG);
    let desktop = desktop_file().ok_or("dossier des applications introuvable")?;
    if let Some(parent) = desktop.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::write(&desktop, desktop_entry(&app)).map_err(|e| e.to_string())?;
    refresh_menus();
    // Le raccourci du terminal rapide doit suivre le nouvel emplacement.
    let _ = crate::quake::repair_shortcut(Some(&app));
    Ok(app)
}

/// Retire l'entrée de menu (le fichier AppImage n'est pas supprimé).
pub fn unintegrate() -> Result<(), String> {
    if let Some(d) = desktop_file() {
        if d.exists() {
            std::fs::remove_file(d).map_err(|e| e.to_string())?;
        }
    }
    refresh_menus();
    Ok(())
}

fn refresh_menus() {
    if let Some(data) = dirs::data_dir() {
        let _ = std::process::Command::new("update-desktop-database").arg(data.join("applications")).output();
        let _ = std::process::Command::new("gtk-update-icon-cache").args(["-q", "-t"]).arg(data.join("icons/hicolor")).output();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn desktop_points_to_appimage() {
        let d = desktop_entry(Path::new("/home/a/Applications/NovaTerm.AppImage"));
        assert!(d.contains("Exec=\"/home/a/Applications/NovaTerm.AppImage\""));
        assert!(d.contains("X-AppImage-Path=/home/a/Applications/NovaTerm.AppImage"));
        assert!(d.contains("Icon=novaterm"));
        assert!(!d.lines().any(|l| l == "Exec=novaterm"));
    }
}
