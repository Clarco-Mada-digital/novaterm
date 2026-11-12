//! Fonctions dépendantes du système (Linux, Windows, macOS).

use std::path::{Path, PathBuf};
use std::process::Command;

/// Cherche un exécutable dans le PATH.
pub fn which(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    let exts: Vec<String> = if cfg!(windows) {
        std::env::var("PATHEXT")
            .unwrap_or_else(|_| ".EXE;.CMD;.BAT".into())
            .split(';')
            .map(|e| e.to_ascii_lowercase())
            .chain(std::iter::once(String::new()))
            .collect()
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
pub fn shell_presets() -> Vec<(String, String)> {
    let mut out = Vec::new();
    if cfg!(windows) {
        for (label, cmd) in [
            ("PowerShell 7", "pwsh.exe"),
            ("Windows PowerShell", "powershell.exe"),
            ("Invite de commandes (cmd)", "cmd.exe"),
            ("WSL (Linux)", "wsl.exe"),
        ] {
            if which(cmd.trim_end_matches(".exe")).is_some() {
                out.push((label.to_string(), cmd.to_string()));
            }
        }
        for git_bash in ["C:\\Program Files\\Git\\bin\\bash.exe", "C:\\Program Files (x86)\\Git\\bin\\bash.exe"] {
            if Path::new(git_bash).exists() {
                out.push(("Git Bash".into(), format!("{git_bash} --login -i")));
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

/// Ouvre un lien, un fichier ou un dossier avec l'application par défaut.
pub fn open(target: &str) {
    let result = if cfg!(windows) {
        Command::new("cmd").args(["/C", "start", "", target]).spawn()
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
    Command::new(program).args(parts).spawn().map(|_| ())
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

/// Notification du bureau.
pub fn notify(title: &str, body: &str) {
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
        let ps = format!(
            "[Windows.UI.Notifications.ToastNotificationManager, Windows.UI.Notifications, ContentType = WindowsRuntime] > $null; \
             $t = [Windows.UI.Notifications.ToastNotificationManager]::GetTemplateContent(1); \
             $x = $t.GetElementsByTagName('text'); $x.Item(0).AppendChild($t.CreateTextNode('{}')) > $null; \
             $x.Item(1).AppendChild($t.CreateTextNode('{}')) > $null; \
             [Windows.UI.Notifications.ToastNotificationManager]::CreateToastNotifier('NovaTerm').Show([Windows.UI.Notifications.ToastNotification]::new($t))",
            title.replace('\'', "''"),
            body.replace('\'', "''")
        );
        hidden_command("powershell").args(["-NoProfile", "-Command", &ps]).spawn().map(|_| ())
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
    } else if cfg!(windows) {
        let _ = hidden_command("powershell").args(["-NoProfile", "-Command", "[System.Media.SystemSounds]::Asterisk.Play()"]).spawn();
    }
}

#[cfg(windows)]
fn hidden_command(program: &str) -> Command {
    use std::os::windows::process::CommandExt;
    let mut c = Command::new(program);
    c.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    c
}

#[cfg(not(windows))]
fn hidden_command(program: &str) -> Command {
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
