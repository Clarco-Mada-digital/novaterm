//! Terminal rapide (« quake ») : une fenêtre NovaTerm qui descend du haut de l'écran.
//!
//! `novaterm --toggle` contacte l'instance rapide déjà ouverte (canal local TCP sur
//! 127.0.0.1, port noté dans un fichier) ou la lance si elle n'existe pas.
//! Le raccourci global est créé dans les réglages du bureau (Cinnamon / GNOME).

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::process::Command;
use std::sync::mpsc::{self, Receiver};
use std::time::Duration;

fn port_file() -> Option<PathBuf> {
    let dir = dirs::runtime_dir().or_else(dirs::cache_dir)?.join("novaterm");
    std::fs::create_dir_all(&dir).ok()?;
    Some(dir.join("quake.port"))
}

/// Jeton aléatoire : seul un processus capable de lire le fichier (même utilisateur)
/// peut piloter le terminal rapide.
fn new_token() -> String {
    use std::hash::{BuildHasher, Hasher};
    let mut out = String::new();
    for i in 0..2u64 {
        let mut h = std::collections::hash_map::RandomState::new().build_hasher();
        h.write_u64(i ^ std::process::id() as u64);
        h.write_u128(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0));
        out.push_str(&format!("{:016x}", h.finish()));
    }
    out
}

fn write_private(path: &std::path::Path, content: &str) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        let mut f = std::fs::OpenOptions::new().write(true).create(true).truncate(true).mode(0o600).open(path)?;
        f.write_all(content.as_bytes())
    }
    #[cfg(not(unix))]
    {
        std::fs::write(path, content)
    }
}

/// Client : demande à l'instance rapide de basculer ; la lance si nécessaire.
pub fn toggle_or_launch() {
    let info = port_file().and_then(|p| std::fs::read_to_string(p).ok()).unwrap_or_default();
    let mut parts = info.split_whitespace();
    if let (Some(port), Some(token)) = (parts.next().and_then(|p| p.parse::<u16>().ok()), parts.next()) {
        if let Ok(mut s) = TcpStream::connect_timeout(&([127, 0, 0, 1], port).into(), Duration::from_millis(300)) {
            if s.write_all(format!("toggle {token}\n").as_bytes()).is_ok() {
                return;
            }
        }
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Err(e) = Command::new(exe).arg("--quake").spawn() {
            eprintln!("NovaTerm: lancement du terminal rapide impossible: {e}");
        }
    }
}

/// Serveur (instance rapide) : chaque message « toggle » est transmis à l'interface.
pub fn listen(ctx: egui::Context) -> Option<Receiver<()>> {
    let listener = TcpListener::bind(("127.0.0.1", 0)).ok()?;
    let port = listener.local_addr().ok()?.port();
    let token = new_token();
    write_private(&port_file()?, &format!("{port} {token}")).ok()?;
    let expected = format!("toggle {token}");
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let _ = stream.set_read_timeout(Some(Duration::from_millis(500)));
            let mut line = String::new();
            let _ = BufReader::new(stream).take(256).read_line(&mut line);
            if line.trim() == expected {
                if tx.send(()).is_err() {
                    break;
                }
                ctx.request_repaint();
            }
        }
    });
    Some(rx)
}

pub fn cleanup() {
    if let Some(p) = port_file() {
        let _ = std::fs::remove_file(p);
    }
}

/// « Ctrl+Alt+T » / « F12 » → format GTK « <Primary><Alt>t ».
pub fn to_gtk_accel(s: &str) -> String {
    let mut out = String::new();
    let parts: Vec<&str> = s.split('+').map(str::trim).filter(|p| !p.is_empty()).collect();
    for (i, p) in parts.iter().enumerate() {
        let last = i + 1 == parts.len();
        match (p.to_ascii_lowercase().as_str(), last) {
            ("ctrl" | "control", false) => out.push_str("<Primary>"),
            ("alt", false) => out.push_str("<Alt>"),
            ("shift" | "maj", false) => out.push_str("<Shift>"),
            ("super" | "win" | "meta", false) => out.push_str("<Super>"),
            (_, true) if p.len() == 1 => out.push_str(&p.to_ascii_lowercase()),
            (_, true) => out.push_str(p),
            _ => {}
        }
    }
    out
}

fn gsettings(args: &[&str]) -> Result<String, String> {
    let out = Command::new("gsettings").args(args).output().map_err(|e| format!("gsettings introuvable : {e}"))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

fn add_to_list(list: &str, item: &str) -> String {
    let mut items: Vec<String> = list
        .trim_start_matches("@as")
        .trim()
        .trim_start_matches('[')
        .trim_end_matches(']')
        .split(',')
        .map(|s| s.trim().trim_matches('\'').to_string())
        .filter(|s| !s.is_empty())
        .collect();
    if !items.iter().any(|i| i == item) {
        items.push(item.to_string());
    }
    format!("[{}]", items.iter().map(|i| format!("'{i}'")).collect::<Vec<_>>().join(", "))
}

/// Crée (ou met à jour) le raccourci global du bureau qui lance `novaterm --toggle`.
pub fn install_shortcut(combo: &str) -> Result<String, String> {
    if !cfg!(target_os = "linux") {
        return Err("Sur ce système, créez un raccourci clavier qui lance « novaterm --toggle ».".into());
    }
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let command = format!("{} --toggle", exe.display());
    let accel = to_gtk_accel(combo);
    let desktop = std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default().to_ascii_lowercase();
    if desktop.contains("cinnamon") || desktop.contains("x-cinnamon") {
        let base = "/org/cinnamon/desktop/keybindings/custom-keybindings/novaterm-quake/";
        let schema = format!("org.cinnamon.desktop.keybindings.custom-keybinding:{base}");
        gsettings(&["set", &schema, "name", "NovaTerm (terminal rapide)"])?;
        gsettings(&["set", &schema, "command", &command])?;
        gsettings(&["set", &schema, "binding", &format!("['{accel}']")])?;
        let list = gsettings(&["get", "org.cinnamon.desktop.keybindings", "custom-list"])?;
        gsettings(&["set", "org.cinnamon.desktop.keybindings", "custom-list", &add_to_list(&list, "novaterm-quake")])?;
        Ok(format!("Raccourci {combo} créé dans Cinnamon."))
    } else if desktop.contains("gnome") || desktop.contains("unity") || desktop.contains("budgie") {
        let base = "/org/gnome/settings-daemon/plugins/media-keys/custom-keybindings/novaterm-quake/";
        let schema = format!("org.gnome.settings-daemon.plugins.media-keys.custom-keybinding:{base}");
        gsettings(&["set", &schema, "name", "NovaTerm (terminal rapide)"])?;
        gsettings(&["set", &schema, "command", &command])?;
        gsettings(&["set", &schema, "binding", &accel])?;
        let key = "org.gnome.settings-daemon.plugins.media-keys";
        let list = gsettings(&["get", key, "custom-keybindings"])?;
        gsettings(&["set", key, "custom-keybindings", &add_to_list(&list, base)])?;
        Ok(format!("Raccourci {combo} créé dans GNOME."))
    } else {
        Err(format!(
            "Bureau « {desktop} » non reconnu : créez un raccourci clavier qui lance « {command} »."
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accel_and_list() {
        assert_eq!(to_gtk_accel("F12"), "F12");
        assert_eq!(to_gtk_accel("Ctrl+Alt+T"), "<Primary><Alt>t");
        assert_eq!(add_to_list("@as []", "a"), "['a']");
        assert_eq!(add_to_list("['custom0', 'a']", "a"), "['custom0', 'a']");
        assert_eq!(add_to_list("['custom0']", "b"), "['custom0', 'b']");
    }
}
