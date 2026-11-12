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
    if let Some(exe) = crate::appimage::stable_exe() {
        if let Err(e) = crate::platform::hidden_command(&exe.display().to_string()).arg("--quake").spawn() {
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
#[cfg(any(not(windows), test))]
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
            (_, true) => out.push_str(match p.to_ascii_lowercase().as_str() {
                "space" | "espace" => "space",
                "enter" | "entrée" | "return" => "Return",
                "pageup" => "Page_Up",
                "pagedown" => "Page_Down",
                "plus" => "plus",
                "minus" => "minus",
                "equals" => "equal",
                "comma" => "comma",
                "period" => "period",
                "slash" => "slash",
                "backslash" => "backslash",
                "backtick" => "grave",
                "quote" => "apostrophe",
                "semicolon" => "semicolon",
                "openbracket" => "bracketleft",
                "closebracket" => "bracketright",
                "escape" => "Escape",
                "tab" => "Tab",
                "insert" => "Insert",
                "delete" => "Delete",
                "home" => "Home",
                "end" => "End",
                "up" => "Up",
                "down" => "Down",
                "left" => "Left",
                "right" => "Right",
                _ => p,
            }),
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

fn parse_list(list: &str) -> Vec<String> {
    list.trim_start_matches("@as")
        .trim()
        .trim_start_matches('[')
        .trim_end_matches(']')
        .split(',')
        .map(|s| s.trim().trim_matches('\'').to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

fn format_list(items: &[String]) -> String {
    if items.is_empty() {
        "@as []".to_string()
    } else {
        format!("[{}]", items.iter().map(|i| format!("'{i}'")).collect::<Vec<_>>().join(", "))
    }
}

#[cfg(test)]
fn add_to_list(list: &str, item: &str) -> String {
    let mut items = parse_list(list);
    if !items.iter().any(|i| i == item) {
        items.push(item.to_string());
    }
    format_list(&items)
}

/// Valeur texte au format GVariant pour `gsettings set` (« 'texte' »).
fn gvariant_str(s: &str) -> String {
    format!("'{}'", s.replace('\\', "\\\\").replace('\'', "\\'"))
}

fn toggle_command(exe: &std::path::Path) -> String {
    format!("\"{}\" --toggle", exe.display())
}

/// Programme lancé par une commande gsettings (« '"/chemin/x" --toggle' » → /chemin/x).
#[cfg(any(not(windows), test))]
fn program_of(command: &str) -> Option<std::path::PathBuf> {
    let c = command.trim().trim_matches('\'');
    let prog = if let Some(rest) = c.strip_prefix('"') {
        rest.split('"').next()?
    } else {
        c.split_whitespace().next()?
    };
    (!prog.is_empty()).then(|| std::path::PathBuf::from(prog))
}

// ---------------------------------------------------------------- Windows
//
// Windows n'a pas de table de raccourcis personnalisés comme GNOME ou Cinnamon.
// En revanche, un **raccourci (.lnk) placé dans le menu Démarrer** peut porter une
// touche d'accès rapide gérée par l'explorateur : c'est la seule méthode qui
// fonctionne sans laisser un programme résident. Contrainte de Windows : la
// combinaison doit comporter Ctrl et/ou Alt (une touche F seule est ignorée).

/// Raccourci du menu Démarrer qui lance `novaterm --toggle`.
#[cfg(windows)]
fn win_lnk_path() -> Option<PathBuf> {
    let dir = dirs::data_dir()?.join("Microsoft").join("Windows").join("Start Menu").join("Programs");
    std::fs::create_dir_all(&dir).ok()?;
    Some(dir.join("NovaTerm (terminal rapide).lnk"))
}

/// Mémo de la touche installée (pour l'afficher sans relire le .lnk).
#[cfg(windows)]
fn win_keys_path() -> Option<PathBuf> {
    let dir = dirs::cache_dir()?.join("novaterm");
    std::fs::create_dir_all(&dir).ok()?;
    Some(dir.join("quake.hotkey"))
}

/// « Ctrl+Alt+T » → « CTRL+ALT+T » (format attendu par WScript.Shell).
#[cfg(any(windows, test))]
fn to_win_hotkey(s: &str) -> Result<String, String> {
    let parts: Vec<&str> = s.split('+').map(str::trim).filter(|p| !p.is_empty()).collect();
    let (Some(key), mods) = (parts.last(), &parts[..parts.len().saturating_sub(1)]) else {
        return Err("Combinaison vide".into());
    };
    let mut out = String::new();
    for m in mods {
        out.push_str(match m.to_ascii_lowercase().as_str() {
            "ctrl" | "control" => "CTRL+",
            "alt" => "ALT+",
            "shift" | "maj" => "SHIFT+",
            // Windows ne sait pas associer la touche Windows à un raccourci .lnk.
            "super" | "win" | "meta" => {
                return Err("Windows réserve la touche Windows : utilisez Ctrl+Alt+une touche.".into())
            }
            _ => return Err(format!("Modificateur inconnu : {m}")),
        });
    }
    if !out.contains("CTRL") && !out.contains("ALT") {
        return Err("Windows exige Ctrl et/ou Alt dans un raccourci global (ex. Ctrl+Alt+T).".into());
    }
    let key = key.to_ascii_uppercase();
    let named = match key.as_str() {
        "SPACE" | "ESPACE" => "SPACE".to_string(),
        "ENTER" | "ENTRÉE" | "RETURN" => "ENTER".to_string(),
        "PAGEUP" => "PGUP".to_string(),
        "PAGEDOWN" => "PGDN".to_string(),
        k => k.to_string(),
    };
    Ok(out + &named)
}

/// Crée ou remplace le raccourci .lnk via WScript.Shell (aucune fenêtre ouverte).
#[cfg(windows)]
fn win_write_lnk(hotkey: &str, exe: &std::path::Path) -> Result<(), String> {
    let lnk = win_lnk_path().ok_or("dossier du menu Démarrer introuvable")?;
    let q = |s: String| s.replace('\'', "''");
    let script = format!(
        "$ErrorActionPreference='Stop'; \
         $s=(New-Object -ComObject WScript.Shell).CreateShortcut('{lnk}'); \
         $s.TargetPath='{exe}'; $s.Arguments='--toggle'; $s.Hotkey='{hotkey}'; \
         $s.IconLocation='{exe},0'; $s.Description='NovaTerm (terminal rapide)'; $s.Save()",
        lnk = q(lnk.display().to_string()),
        exe = q(exe.display().to_string()),
        hotkey = q(hotkey.to_string()),
    );
    let out = crate::platform::hidden_command("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .output()
        .map_err(|e| format!("PowerShell introuvable : {e}"))?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        return Err(format!("création du raccourci impossible : {}", err.lines().next().unwrap_or("erreur inconnue")));
    }
    if let Some(p) = win_keys_path() {
        let _ = std::fs::write(p, hotkey);
    }
    Ok(())
}

#[cfg(windows)]
fn win_install(combo: &str) -> Result<String, String> {
    let exe = crate::appimage::stable_exe().ok_or("exécutable introuvable")?;
    let hotkey = to_win_hotkey(combo)?;
    win_write_lnk(&hotkey, &exe)?;
    Ok(format!(
        "✔ {hotkey} active (raccourci « NovaTerm (terminal rapide) » dans le menu Démarrer). \
         Si rien ne se passe, ouvrez une session puis réessayez : l'explorateur relit les raccourcis au démarrage."
    ))
}

#[cfg(windows)]
fn win_status() -> Option<ShortcutStatus> {
    let lnk = win_lnk_path()?;
    if !lnk.is_file() {
        return None;
    }
    let keys = win_keys_path().and_then(|p| std::fs::read_to_string(p).ok()).unwrap_or_else(|| "?".into());
    let program = crate::appimage::stable_exe();
    let program_ok = program.as_ref().is_some_and(|p| p.exists());
    Some(ShortcutStatus { keys, program, program_ok, legacy: false })
}

#[cfg(windows)]
fn win_remove() -> Result<String, String> {
    if let Some(p) = win_lnk_path() {
        let _ = std::fs::remove_file(p);
    }
    if let Some(p) = win_keys_path() {
        let _ = std::fs::remove_file(p);
    }
    Ok("Raccourci du terminal rapide supprimé.".into())
}

/// Bureau dont on sait gérer les raccourcis personnalisés.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Desk {
    Cinnamon,
    Gnome,
}

/// Identifiant du raccourci. Cinnamon ne retire au rechargement que les raccourcis
/// dont le nom contient « custom » : l'identifiant doit donc le contenir.
const ID: &str = "custom-novaterm-quake";
/// Ancien identifiant (v0.0.1), migré automatiquement.
const LEGACY_ID: &str = "novaterm-quake";

fn desk() -> Option<Desk> {
    if !cfg!(target_os = "linux") {
        return None;
    }
    let d = std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default().to_ascii_lowercase();
    if d.contains("cinnamon") {
        Some(Desk::Cinnamon)
    } else if d.contains("gnome") || d.contains("unity") || d.contains("budgie") {
        Some(Desk::Gnome)
    } else {
        None
    }
}

impl Desk {
    fn list_schema(self) -> &'static str {
        match self {
            Desk::Cinnamon => "org.cinnamon.desktop.keybindings",
            Desk::Gnome => "org.gnome.settings-daemon.plugins.media-keys",
        }
    }
    fn list_key(self) -> &'static str {
        match self {
            Desk::Cinnamon => "custom-list",
            Desk::Gnome => "custom-keybindings",
        }
    }
    fn path(self, id: &str) -> String {
        match self {
            Desk::Cinnamon => format!("/org/cinnamon/desktop/keybindings/custom-keybindings/{id}/"),
            Desk::Gnome => format!("/org/gnome/settings-daemon/plugins/media-keys/custom-keybindings/{id}/"),
        }
    }
    /// Élément de la liste : le nom (Cinnamon) ou le chemin (GNOME).
    fn item(self, id: &str) -> String {
        match self {
            Desk::Cinnamon => id.to_string(),
            Desk::Gnome => self.path(id),
        }
    }
    fn entry(self, id: &str) -> String {
        let schema = match self {
            Desk::Cinnamon => "org.cinnamon.desktop.keybindings.custom-keybinding",
            Desk::Gnome => "org.gnome.settings-daemon.plugins.media-keys.custom-keybinding",
        };
        format!("{schema}:{}", self.path(id))
    }
    fn binding_value(self, accel: &str) -> String {
        match self {
            Desk::Cinnamon => format!("[{}]", gvariant_str(accel)),
            Desk::Gnome => gvariant_str(accel),
        }
    }
    fn read_list(self) -> Vec<String> {
        gsettings(&["get", self.list_schema(), self.list_key()]).map(|l| parse_list(&l)).unwrap_or_default()
    }
    /// Écrit la liste en forçant le bureau à recharger les raccourcis : Cinnamon ne
    /// relit les commandes et touches que lorsque la liste change.
    fn write_list_reload(self, items: &[String], ours: Option<&str>) -> Result<(), String> {
        gsettings(&["set", self.list_schema(), self.list_key(), &format_list(items)])?;
        if let Some(o) = ours {
            let mut with = items.to_vec();
            with.push(o.to_string());
            std::thread::sleep(Duration::from_millis(150));
            gsettings(&["set", self.list_schema(), self.list_key(), &format_list(&with)])?;
        }
        Ok(())
    }
    fn reset_entry(self, id: &str) {
        for key in ["name", "command", "binding"] {
            let _ = gsettings(&["reset", &self.entry(id), key]);
        }
    }
}

/// « <Primary><Alt>t » → « Ctrl+Alt+T ».
pub fn accel_label(accel: &str) -> String {
    let mut out = String::new();
    let mut rest = accel;
    while let Some(start) = rest.find('<') {
        let Some(end) = rest[start..].find('>') else { break };
        let m = &rest[start + 1..start + end];
        out.push_str(match m.to_ascii_lowercase().as_str() {
            "primary" | "control" | "ctrl" => "Ctrl+",
            "alt" | "mod1" => "Alt+",
            "shift" => "Maj+",
            "super" | "mod4" => "Super+",
            _ => "",
        });
        rest = &rest[start + end + 1..];
    }
    let key = if rest.chars().count() == 1 { rest.to_uppercase() } else { rest.to_string() };
    out + &key
}

/// État du raccourci global, pour l'afficher dans les paramètres.
pub struct ShortcutStatus {
    pub keys: String,
    pub program: Option<std::path::PathBuf>,
    pub program_ok: bool,
    /// Ancien identifiant encore présent (raccourci bloqué jusqu'à la migration).
    pub legacy: bool,
}

pub fn shortcut_status() -> Option<ShortcutStatus> {
    #[cfg(windows)]
    return win_status();
    #[cfg(not(windows))]
    status_desk()
}

#[cfg(not(windows))]
fn status_desk() -> Option<ShortcutStatus> {
    let d = desk()?;
    let list = d.read_list();
    let legacy = list.contains(&d.item(LEGACY_ID));
    let id = if list.contains(&d.item(ID)) {
        ID
    } else if legacy {
        LEGACY_ID
    } else {
        return None;
    };
    let binding = gsettings(&["get", &d.entry(id), "binding"]).unwrap_or_default();
    let accel = parse_list(&binding).into_iter().next().unwrap_or_else(|| binding.trim_matches('\'').to_string());
    let program = gsettings(&["get", &d.entry(id), "command"]).ok().and_then(|c| program_of(&c));
    let program_ok = program.as_ref().is_some_and(|p| p.exists() && !p.starts_with("/tmp/.mount_"));
    Some(ShortcutStatus { keys: accel_label(&accel), program, program_ok, legacy })
}

/// Autres raccourcis du bureau qui utilisent déjà cette combinaison.
#[cfg(not(windows))]
pub fn conflicts(accel: &str) -> Vec<String> {
    let Some(d) = desk() else { return Vec::new() };
    let needle = format!("'{}'", accel.to_ascii_lowercase());
    let mut out = Vec::new();
    let schemas: &[&str] = match d {
        Desk::Cinnamon => &["org.cinnamon.desktop.keybindings.wm", "org.cinnamon.desktop.keybindings.media-keys", "org.cinnamon.desktop.keybindings"],
        Desk::Gnome => &["org.gnome.desktop.wm.keybindings", "org.gnome.settings-daemon.plugins.media-keys", "org.gnome.shell.keybindings"],
    };
    for schema in schemas {
        let Ok(all) = gsettings(&["list-recursively", schema]) else { continue };
        for line in all.lines() {
            let mut parts = line.splitn(3, ' ');
            let (_, key, value) = (parts.next(), parts.next().unwrap_or(""), parts.next().unwrap_or(""));
            if value.to_ascii_lowercase().contains(&needle) && key != d.list_key() {
                out.push(key.to_string());
            }
        }
    }
    for item in d.read_list() {
        let id = item.trim_end_matches('/').rsplit('/').next().unwrap_or(&item).to_string();
        if id == ID || id == LEGACY_ID {
            continue;
        }
        let b = gsettings(&["get", &d.entry(&id), "binding"]).unwrap_or_default();
        if b.to_ascii_lowercase().contains(&needle) {
            let name = gsettings(&["get", &d.entry(&id), "name"]).unwrap_or_default();
            let name = name.trim_matches('\'').trim();
            out.push(if name.is_empty() { format!("raccourci personnalisé « {id} »") } else { name.to_string() });
        }
    }
    out.dedup();
    out
}

/// Écrit le raccourci (touches GTK) et force son rechargement ; migre l'ancien.
fn write_shortcut(d: Desk, accel: &str, exe: &std::path::Path) -> Result<(), String> {
    let mut items: Vec<String> = d.read_list().into_iter().filter(|i| *i != d.item(ID) && *i != d.item(LEGACY_ID)).collect();
    d.reset_entry(LEGACY_ID);
    let entry = d.entry(ID);
    gsettings(&["set", &entry, "name", &gvariant_str("NovaTerm (terminal rapide)")])?;
    gsettings(&["set", &entry, "command", &gvariant_str(&toggle_command(exe))])?;
    gsettings(&["set", &entry, "binding", &d.binding_value(accel)])?;
    items.retain(|i| !i.is_empty());
    d.write_list_reload(&items, Some(&d.item(ID)))
}

/// Répare un raccourci existant : ancien identifiant (migré) ou exécutable disparu
/// (dossier temporaire d'une AppImage, ancien emplacement). `exe` = nouvel emplacement.
pub fn repair_shortcut(exe: Option<&std::path::Path>) -> Option<String> {
    let d = desk()?;
    let exe = exe.map(std::path::Path::to_path_buf).or_else(crate::appimage::stable_exe)?;
    if exe.starts_with("/tmp/.mount_") || !exe.exists() {
        return None;
    }
    let status = shortcut_status()?;
    if status.program_ok && !status.legacy {
        return None;
    }
    let id = if status.legacy && !d.read_list().contains(&d.item(ID)) { LEGACY_ID } else { ID };
    let binding = gsettings(&["get", &d.entry(id), "binding"]).ok()?;
    let accel = parse_list(&binding).into_iter().next().unwrap_or_else(|| binding.trim_matches('\'').to_string());
    if accel.is_empty() {
        return None;
    }
    write_shortcut(d, &accel, &exe).ok()?;
    Some(format!("Raccourci {} du terminal rapide réparé", accel_label(&accel)))
}

/// Supprime le raccourci global (et l'ancien identifiant).
pub fn remove_shortcut() -> Result<String, String> {
    #[cfg(windows)]
    return win_remove();
    #[cfg(not(windows))]
    remove_shortcut_desk()
}

#[cfg(not(windows))]
fn remove_shortcut_desk() -> Result<String, String> {
    let d = desk().ok_or("bureau non pris en charge")?;
    let items: Vec<String> = d.read_list().into_iter().filter(|i| *i != d.item(ID) && *i != d.item(LEGACY_ID)).collect();
    d.write_list_reload(&items, None)?;
    d.reset_entry(ID);
    d.reset_entry(LEGACY_ID);
    Ok("Raccourci du terminal rapide supprimé.".into())
}

/// Crée (ou remplace) le raccourci global qui lance `novaterm --toggle`.
/// L'ancienne touche est retirée : il n'y a jamais deux raccourcis.
pub fn install_shortcut(combo: &str) -> Result<String, String> {
    #[cfg(windows)]
    return win_install(combo);
    #[cfg(not(windows))]
    install_shortcut_desk(combo)
}

#[cfg(not(windows))]
fn install_shortcut_desk(combo: &str) -> Result<String, String> {
    let exe = crate::appimage::stable_exe().ok_or("exécutable introuvable")?;
    let Some(d) = desk() else {
        return Err(format!(
            "Sur ce bureau, créez vous-même un raccourci clavier qui lance : {}",
            toggle_command(&exe)
        ));
    };
    let accel = to_gtk_accel(combo);
    if accel.is_empty() {
        return Err("Combinaison vide".into());
    }
    write_shortcut(d, &accel, &exe)?;
    let others = conflicts(&accel);
    let name = if d == Desk::Cinnamon { "Cinnamon" } else { "GNOME" };
    if others.is_empty() {
        Ok(format!("✔ {} active dans {name}.", accel_label(&accel)))
    } else {
        Ok(format!("⚠ {} est aussi utilisé par : {} — choisissez une autre touche si rien ne se passe.", accel_label(&accel), others.join(", ")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn program_parsing() {
        assert_eq!(program_of("'\"/home/a/Applications/NovaTerm.AppImage\" --toggle'").unwrap(), std::path::PathBuf::from("/home/a/Applications/NovaTerm.AppImage"));
        assert_eq!(program_of("'/tmp/.mount_X/usr/bin/novaterm --toggle'").unwrap(), std::path::PathBuf::from("/tmp/.mount_X/usr/bin/novaterm"));
        assert_eq!(toggle_command(std::path::Path::new("/a b/n")), "\"/a b/n\" --toggle");
        assert_eq!(gvariant_str("\"/a'b\" --toggle"), "'\"/a\\'b\" --toggle'");
    }

    #[test]
    fn labels_and_lists() {
        assert_eq!(accel_label("<Primary><Alt>t"), "Ctrl+Alt+T");
        assert_eq!(accel_label("F12"), "F12");
        assert_eq!(to_gtk_accel("Ctrl+Space"), "<Primary>space");
        assert_eq!(format_list(&[]), "@as []");
        assert_eq!(parse_list("['custom0', 'custom-novaterm-quake']"), vec!["custom0", "custom-novaterm-quake"]);
        assert!(ID.contains("custom"), "Cinnamon ne retire que les raccourcis nommés « custom… »");
    }

    #[test]
    fn accel_and_list() {
        assert_eq!(to_gtk_accel("F12"), "F12");
        assert_eq!(to_gtk_accel("Ctrl+Alt+T"), "<Primary><Alt>t");
        // Windows : Ctrl et/ou Alt obligatoires, touche Windows refusée.
        assert_eq!(to_win_hotkey("Ctrl+Alt+T").unwrap(), "CTRL+ALT+T");
        assert_eq!(to_win_hotkey("Ctrl+Shift+F12").unwrap(), "CTRL+SHIFT+F12");
        assert!(to_win_hotkey("F12").is_err());
        assert!(to_win_hotkey("Super+T").is_err());
        assert!(to_win_hotkey("").is_err());
        assert_eq!(add_to_list("@as []", "a"), "['a']");
        assert_eq!(add_to_list("['custom0', 'a']", "a"), "['custom0', 'a']");
        assert_eq!(add_to_list("['custom0']", "b"), "['custom0', 'b']");
    }
}
