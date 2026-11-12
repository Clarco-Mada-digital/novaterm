//! `novaterm-cli` — compagnon **console** de NovaTerm, utile surtout sous Windows.
//!
//! `novaterm.exe` est une application fenêtrée : elle n'a pas de sortie standard
//! exploitable et le shell ne l'attend pas, ce qui rend `imgcat` inutilisable et
//! muettes les options comme `--help`. Ce petit binaire console comble le trou :
//!
//! - `--imgcat fichier…` écrit lui-même la séquence iTerm2 sur sa sortie standard ;
//! - toute autre option est transmise à `novaterm`, dont la sortie est héritée.
//!
//! Sous Linux et macOS, `novaterm` suffit : ce binaire n'y est qu'un raccourci.

use std::io::Write;
use std::path::PathBuf;

const HELP: &str = "Usage : novaterm-cli --imgcat [--width N|N%|Npx] image…
        novaterm-cli <option de novaterm>

Affiche une image dans le terminal (PNG, JPEG, GIF, WebP, BMP, ICO, SVG).
Les autres options sont transmises à novaterm (--help, --version, --explain…).";

/// Séquence iTerm2 d'affichage d'une image (même format que `novaterm --imgcat`).
fn iterm_sequence(bytes: &[u8], name: &str, width: Option<&str>) -> String {
    use base64::Engine;
    let b64 = base64::engine::general_purpose::STANDARD;
    let mut args = format!("name={};size={};inline=1", b64.encode(name), bytes.len());
    if let Some(w) = width {
        args.push_str(&format!(";width={w}"));
    }
    format!("\x1b]1337;File={args}:{}\x07\n", b64.encode(bytes))
}

/// Chemin de `novaterm`, à côté de ce binaire.
fn novaterm_exe() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let name = if cfg!(windows) { "novaterm.exe" } else { "novaterm" };
    let sibling = exe.with_file_name(name);
    sibling.is_file().then_some(sibling)
}

fn imgcat(args: &[String]) -> i32 {
    let mut width: Option<String> = None;
    let mut files: Vec<&String> = Vec::new();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--width" | "-w" => width = it.next().cloned(),
            s if s.starts_with("--width=") => width = Some(s["--width=".len()..].to_string()),
            _ => files.push(a),
        }
    }
    if files.is_empty() {
        eprintln!("Usage : imgcat [--width 40|50%|300px] image…  (PNG, JPEG, GIF, WebP, BMP, ICO, SVG)");
        return 2;
    }
    let mut code = 0;
    let mut out = std::io::stdout().lock();
    for file in files {
        match std::fs::read(file) {
            Ok(bytes) => {
                let name = std::path::Path::new(file).file_name().and_then(|n| n.to_str()).unwrap_or("image");
                let _ = out.write_all(iterm_sequence(&bytes, name, width.as_deref()).as_bytes());
            }
            Err(e) => {
                eprintln!("imgcat : {file} : {e}");
                code = 1;
            }
        }
    }
    let _ = out.flush();
    code
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        println!("{HELP}");
        return;
    }
    if let Some(pos) = args.iter().position(|a| a == "--imgcat") {
        std::process::exit(imgcat(&args[pos + 1..]));
    }
    // Tout le reste est l'affaire de novaterm : il hérite de notre console, et
        // nous l'attendons — ce que ni cmd ni PowerShell ne font pour une application fenêtrée.
    let Some(exe) = novaterm_exe() else {
        eprintln!("novaterm-cli : novaterm est introuvable à côté de ce programme.");
        std::process::exit(1);
    };
    match std::process::Command::new(exe).args(&args).status() {
        Ok(status) => std::process::exit(status.code().unwrap_or(0)),
        Err(e) => {
            eprintln!("novaterm-cli : lancement impossible : {e}");
            std::process::exit(1);
        }
    }
}
