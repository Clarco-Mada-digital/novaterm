//! Fonctions dépendantes du système (Linux, Windows, macOS).

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

/// Cherche un exécutable dans le PATH.
pub fn which(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    let exts: Vec<String> = if cfg!(windows) {
        let pathext: Vec<String> = std::env::var("PATHEXT")
            .unwrap_or_else(|_| ".EXE;.CMD;.BAT".into())
            .split(';')
            .map(|e| e.to_ascii_lowercase())
            .collect();
        // Un nom déjà suffixé (« pwsh.exe ») est cherché tel quel, sans doubler
        // l'extension (« pwsh.exe.exe »).
        let lower = name.to_ascii_lowercase();
        if pathext.iter().any(|e| !e.is_empty() && lower.ends_with(e)) {
            vec![String::new()]
        } else {
            pathext.into_iter().chain(std::iter::once(String::new())).collect()
        }
    } else {
        vec![String::new()]
    };
    for dir in std::env::split_paths(&path) {
        for ext in &exts {
            let p = dir.join(format!("{name}{ext}"));
            if p.is_file() {
                return Some(p);
            }
        }
    }
    None
}

/// Shell par défaut de l'utilisateur.
pub fn default_shell() -> String {
    if cfg!(windows) {
        if which("pwsh").is_some() {
            "pwsh.exe".into()
        } else {
            "powershell.exe".into()
        }
    } else {
        std::env::var("SHELL").ok().filter(|s| !s.is_empty()).unwrap_or_else(|| {
            if cfg!(target_os = "macos") {
                "/bin/zsh".into()
            } else {
                "/bin/bash".into()
            }
        })
    }
}

/// Shells proposés dans les paramètres : (libellé, commande).
/// Résultat mis en cache : la liste est demandée à chaque image tant que le menu
/// déroulant des paramètres est ouvert.
pub fn shell_presets() -> Vec<(String, String)> {
    static CACHE: OnceLock<Vec<(String, String)>> = OnceLock::new();
    CACHE.get_or_init(detect_shells).clone()
}

fn detect_shells() -> Vec<(String, String)> {
    let mut out = Vec::new();
    if cfg!(windows) {
        for (label, cmd) in
            [("PowerShell 7", "pwsh.exe"), ("Windows PowerShell", "powershell.exe"), ("Invite de commandes (cmd)", "cmd.exe")]
        {
            if which(cmd).is_some() {
                out.push((label.to_string(), cmd.to_string()));
            }
        }
        // wsl.exe est présent dans System32 même sans distribution installée :
        // proposer « WSL » mènerait à un onglet qui se ferme aussitôt.
        if which("wsl.exe").is_some() && wsl_has_distro() {
            out.push(("WSL (Linux)".into(), "wsl.exe".into()));
        }
        for git_bash in ["C:\\Program Files\\Git\\bin\\bash.exe", "C:\\Program Files (x86)\\Git\\bin\\bash.exe"] {
            if Path::new(git_bash).exists() {
                out.push(("Git Bash".into(), format!("\"{git_bash}\" --login -i")));
                break;
            }
        }
    } else {
        for name in ["zsh", "bash", "fish", "nu", "sh"] {
            if let Some(p) = which(name) {
                out.push((name.to_string(), p.display().to_string()));
            }
        }
    }
    out
}

/// Au moins une distribution WSL est-elle installée ?
fn wsl_has_distro() -> bool {
    if !cfg!(windows) {
        return false;
    }
    let Ok(out) = hidden_command("wsl.exe").args(["-l", "-q"]).output() else { return false };
    // wsl.exe répond en UTF-16 : on cherche simplement un caractère imprimable.
    out.stdout.iter().any(|b| b.is_ascii_alphanumeric())
}

/// Binaire console compagnon (`novaterm-cli.exe`), à côté de l'exécutable.
///
/// Sous Windows, NovaTerm est une application fenêtrée : elle n'a pas de sortie
/// standard utilisable et le shell ne l'attend pas. `novaterm-cli.exe` sert donc
/// à `imgcat` et aux options en ligne de commande.
pub fn cli_exe(exe: &Path) -> Option<PathBuf> {
    if !cfg!(windows) {
        return None;
    }
    let p = exe.with_file_name("novaterm-cli.exe");
    p.is_file().then_some(p)
}

/// Ouvre un lien, un fichier ou un dossier avec l'application par défaut.
pub fn open(target: &str) {
    #[cfg(windows)]
    {
        if crate::win::open(target) {
            return;
        }
    }
    let result = if cfg!(windows) {
        // Repli : « start » a besoin d'un titre vide avant la cible.
        hidden_command("cmd").args(["/C", "start", "", target]).spawn()
    } else if cfg!(target_os = "macos") {
        Command::new("open").arg(target).spawn()
    } else {
        Command::new("xdg-open").arg(target).spawn()
    };
    if let Err(e) = result {
        eprintln!("NovaTerm: impossible d'ouvrir {target}: {e}");
    }
}

/// Lance une commande « à la shell » (éditeur configuré, etc.) sans attendre.
pub fn spawn_command_line(line: &str) -> std::io::Result<()> {
    let mut parts = split_args(line);
    if parts.is_empty() {
        return Ok(());
    }
    let program = parts.remove(0);
    // hidden_command : sans cela, un éditeur console (vim, nano…) lancé depuis
    // NovaTerm ouvre une fenêtre noire sous Windows.
    hidden_command(&program).args(parts).spawn().map(|_| ())
}

/// Découpe une ligne de commande en arguments (guillemets simples et doubles).
pub fn split_args(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quote: Option<char> = None;
    let mut has = false;
    for c in line.chars() {
        match (quote, c) {
            (Some(q), c) if c == q => quote = None,
            (Some(_), c) => cur.push(c),
            (None, '"' | '\'') => {
                quote = Some(c);
                has = true;
            }
            (None, c) if c.is_whitespace() => {
                if has || !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                    has = false;
                }
            }
            (None, c) => cur.push(c),
        }
    }
    if has || !cur.is_empty() {
        out.push(cur);
    }
    out
}

/// Notification du bureau. Sous Windows elle passe par PowerShell (~500 ms) :
/// l'appel est fait dans un fil pour ne pas figer l'affichage.
pub fn notify(title: &str, body: &str) {
    if cfg!(windows) {
        let (t, b) = (title.to_string(), body.to_string());
        std::thread::spawn(move || notify_blocking(&t, &b));
    } else {
        notify_blocking(title, body);
    }
}

fn notify_blocking(title: &str, body: &str) {
    let result = if cfg!(target_os = "linux") || cfg!(target_os = "freebsd") {
        Command::new("notify-send").args(["-a", "NovaTerm", "-i", "novaterm", title, body]).spawn().map(|_| ())
    } else if cfg!(target_os = "macos") {
        let script = format!(
            "display notification \"{}\" with title \"{}\"",
            body.replace('"', "'"),
            title.replace('"', "'")
        );
        Command::new("osascript").args(["-e", &script]).spawn().map(|_| ())
    } else if cfg!(windows) {
        // L'identifiant d'application doit être enregistré auprès de Windows, sinon
        // la notification est ignorée **sans erreur**. « NovaTerm » ne l'est pas
        // (application non installée en paquet MSIX) : on se replie sur celui de
        // PowerShell, toujours présent — c'est la méthode employée par BurntToast.
        let ps = format!(
            "[Windows.UI.Notifications.ToastNotificationManager, Windows.UI.Notifications, ContentType = WindowsRuntime] > $null; \
             $t = [Windows.UI.Notifications.ToastNotificationManager]::GetTemplateContent(1); \
             $x = $t.GetElementsByTagName('text'); $x.Item(0).AppendChild($t.CreateTextNode('{}')) > $null; \
             $x.Item(1).AppendChild($t.CreateTextNode('{}')) > $null; \
             $n = [Windows.UI.Notifications.ToastNotificationManager]::CreateToastNotifier('{{1AC14E77-02E7-4E5D-B744-2EB1AE5198B7}}\\WindowsPowerShell\\v1.0\\powershell.exe'); \
             $n.Show([Windows.UI.Notifications.ToastNotification]::new($t))",
            title.replace('\'', "''"),
            body.replace('\'', "''")
        );
        hidden_command("powershell").args(["-NoProfile", "-NonInteractive", "-Command", &ps]).spawn().map(|_| ())
    } else {
        Ok(())
    };
    if let Err(e) = result {
        eprintln!("NovaTerm: notification impossible: {e}");
    }
}

/// Petit son de fin de tâche (si disponible).
pub fn play_sound() {
    if cfg!(target_os = "linux") {
        for f in [
            "/usr/share/sounds/freedesktop/stereo/complete.oga",
            "/usr/share/sounds/freedesktop/stereo/bell.oga",
        ] {
            if Path::new(f).exists() {
                let _ = Command::new("paplay").arg(f).spawn();
                return;
            }
        }
    } else if cfg!(target_os = "macos") {
        let _ = Command::new("afplay").arg("/System/Library/Sounds/Glass.aiff").spawn();
    }
    #[cfg(windows)]
    crate::win::beep();
}

/// Programme lancé en arrière-plan. Sous Windows, NovaTerm est une application
/// fenêtrée : sans CREATE_NO_WINDOW, chaque programme console (git, powershell…)
/// ouvrirait une fenêtre noire le temps de son exécution.
#[cfg(windows)]
pub fn hidden_command(program: &str) -> Command {
    use std::os::windows::process::CommandExt;
    let mut c = Command::new(program);
    c.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    c
}

#[cfg(not(windows))]
pub fn hidden_command(program: &str) -> Command {
    Command::new(program)
}

/// Dossier courant d'un processus.
pub fn process_cwd(pid: u32) -> Option<PathBuf> {
    if cfg!(target_os = "linux") {
        std::fs::read_link(format!("/proc/{pid}/cwd")).ok()
    } else if cfg!(target_os = "macos") {
        let out = Command::new("lsof").args(["-a", "-d", "cwd", "-Fn", "-p", &pid.to_string()]).output().ok()?;
        String::from_utf8_lossy(&out.stdout).lines().find_map(|l| l.strip_prefix('n')).map(PathBuf::from)
    } else {
        None
    }
}

/// Nom d'un processus (pour « python3 est en cours »).
#[cfg(unix)]
pub fn process_name(pid: i32) -> Option<String> {
    if cfg!(target_os = "linux") {
        std::fs::read_to_string(format!("/proc/{pid}/comm")).ok().map(|s| s.trim().to_string())
    } else {
        let out = Command::new("ps").args(["-o", "comm=", "-p", &pid.to_string()]).output().ok()?;
        let name = String::from_utf8_lossy(&out.stdout).trim().to_string();
        (!name.is_empty()).then(|| name.rsplit('/').next().unwrap_or(&name).to_string())
    }
}

/// Thème clair/sombre du bureau (None = inconnu).
pub fn desktop_prefers_dark() -> Option<bool> {
    if !cfg!(target_os = "linux") {
        return None;
    }
    let get = |schema: &str, key: &str| -> Option<String> {
        let o = Command::new("gsettings").args(["get", schema, key]).output().ok()?;
        o.status.success().then(|| String::from_utf8_lossy(&o.stdout).trim().trim_matches('\'').to_string())
    };
    if let Some(v) = get("org.x.apps.portal", "color-scheme").or_else(|| get("org.gnome.desktop.interface", "color-scheme")) {
        if v.contains("dark") {
            return Some(true);
        }
        if v.contains("light") {
            return Some(false);
        }
    }
    for schema in ["org.cinnamon.desktop.interface", "org.gnome.desktop.interface"] {
        if let Some(t) = get(schema, "gtk-theme") {
            return Some(t.to_ascii_lowercase().contains("dark"));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split() {
        assert_eq!(split_args(r#"code -g "my file.rs:3""#), vec!["code", "-g", "my file.rs:3"]);
        assert_eq!(split_args("  a  'b c' \"\" "), vec!["a", "b c", ""]);
    }
}
