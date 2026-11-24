//! NovaTerm — terminal moderne pour Linux, Windows et macOS.

// Pas de fenêtre console noire derrière l'application sous Windows (en release).
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod ai;
mod app;
mod appimage;
mod blocks;
mod completions;
mod config;
mod fonts;
mod format;
mod guard;
mod helpers;
mod history;
mod i18n;
mod icon;
mod images;
mod input;
mod keys;
mod layout;
mod links;
mod platform;
mod quake;
mod render;
mod session;
mod settings;
mod shell;
mod stream;
mod terminal;
mod theme;
mod update;
#[cfg(windows)]
mod win;

use app::{NovaApp, StartOptions};

const HELP: &str = "Usage : novaterm [options]

Options :
  --cwd DOSSIER        ouvrir un onglet dans ce dossier
                       (alias : --working-directory, pour les bureaux)
  -e, --command CMD…   exécuter cette commande dans le premier onglet
  --profile NOM        ouvrir un profil de projet
  --quake              lancer le terminal rapide (fenêtre qui descend du haut)
  --toggle             afficher / masquer le terminal rapide (à lier à un raccourci)
  --quake-shortcut TOUCHE   créer le raccourci du bureau (ex. F12, Ctrl+Alt+T)
  --quake-shortcut-remove   supprimer ce raccourci
  --imgcat FICHIER|URL afficher une image dans le terminal (--width N|N%|Npx)
  --explain «COMMANDE» expliquer une commande (pages man, tldr ; hors ligne)
  --check-update       vérifier si une nouvelle version est publiée
  --integrate          AppImage : ranger dans ~/Applications et ajouter au menu
  --unintegrate        AppImage : retirer du menu
  --safe-mode          démarrer avec la police intégrée (en cas de problème)
  --reset-config       sauvegarder puis réinitialiser la configuration
  --export-icon DOSSIER  exporter l'icône (PNG, ICO, SVG)
  -V, --version        version
  -h, --help           cette aide

Raccourcis principaux : Ctrl+Maj+P (palette), Ctrl+, (paramètres), F1 (aide),
Ctrl+Maj+T (onglet), Ctrl+Maj+E/O (split), Alt+Flèches (panneaux), Ctrl+↑/↓ (commandes),
Ctrl+Alt+H (historique), Ctrl+Maj+C/V (copier/coller), Ctrl+Maj+Q (quitter).";

fn arg_value(args: &[String], name: &str) -> Option<String> {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .cloned()
        // Forme « --option=valeur », attendue par plusieurs bureaux.
        .or_else(|| args.iter().find_map(|a| a.strip_prefix(&format!("{name}=")).map(String::from)))
}

/// Télécharge une image (`imgcat https://…`) avec curl, comme le reste du projet.
///
/// Seul `imgcat`, tapé par l'utilisateur, télécharge : un programme qui envoie une
/// séquence d'image ne peut pas faire récupérer une adresse par le terminal.
fn fetch_url(url: &str) -> Result<Vec<u8>, String> {
    let mut cmd = std::process::Command::new(if cfg!(windows) { "curl.exe" } else { "curl" });
    cmd.args([
        "-fsSL",
        // Sans cela, une redirection pourrait emmener curl vers un autre
        // protocole (file://, scp://…) : on s'en tient au web.
        "--proto",
        "=http,https",
        "--proto-redir",
        "=http,https",
        "--max-time",
        "30",
        "--max-filesize",
        &images::MAX_IMAGE_BYTES.to_string(),
        "-H",
        &format!("User-Agent: NovaTerm/{}", env!("CARGO_PKG_VERSION")),
        url,
    ]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    let out = cmd.output().map_err(|_| "curl est introuvable : impossible de télécharger l'image".to_string())?;
    if !out.status.success() {
        return Err(format!("téléchargement impossible ({})", String::from_utf8_lossy(&out.stderr).trim()));
    }
    if out.stdout.is_empty() {
        return Err("réponse vide".into());
    }
    Ok(out.stdout)
}

/// Une adresse web plutôt qu'un fichier ?
fn is_url(arg: &str) -> bool {
    arg.starts_with("https://") || arg.starts_with("http://")
}

/// Nom à annoncer dans la séquence (dernier segment de l'adresse, ou du chemin).
fn image_name(arg: &str) -> &str {
    let name = if is_url(arg) {
        arg.split(['?', '#']).next().unwrap_or(arg).trim_end_matches('/').rsplit('/').next().unwrap_or("")
    } else {
        std::path::Path::new(arg).file_name().and_then(|n| n.to_str()).unwrap_or("")
    };
    if name.is_empty() {
        "image"
    } else {
        name
    }
}

/// Commande à exécuter au démarrage (`-e`, `--command`, `-x`).
///
/// Convention des terminaux : tout ce qui suit l'option est la commande. C'est ce
/// qu'emploient les gestionnaires de fichiers et `xdg-terminal-exec` pour « ouvrir
/// un terminal ici » ou « exécuter dans un terminal ».
fn startup_command(args: &[String]) -> Option<String> {
    let i = args.iter().position(|a| matches!(a.as_str(), "-e" | "--command" | "-x"))?;
    let rest = &args[i + 1..];
    if rest.is_empty() {
        return None;
    }
    // Un seul argument : déjà une ligne de commande complète (« -e "ls -l" »).
    if rest.len() == 1 {
        return Some(rest[0].clone());
    }
    // Plusieurs arguments : on les recolle en citant ceux qui contiennent un espace.
    let line = rest
        .iter()
        .map(|a| if a.contains(' ') && !a.contains('\'') { format!("'{a}'") } else { a.clone() })
        .collect::<Vec<_>>()
        .join(" ");
    Some(line)
}

fn main() -> eframe::Result<()> {
    // Console masquée : les programmes console lancés ensuite s'y rattachent au
    // lieu d'ouvrir une fenêtre noire (shells des onglets, git, éditeur…).
    #[cfg(windows)]
    win::hide_console();
    appimage::take_runtime_env();
    let args: Vec<String> = std::env::args().collect();
    let has = |n: &str| args.iter().any(|a| a == n);
    if has("--help") || has("-h") {
        println!("NovaTerm {} — terminal moderne\n\n{HELP}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    if has("--version") || has("-V") {
        println!("NovaTerm {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    if let Some(dir) = arg_value(&args, "--export-icon").or_else(|| has("--export-icon").then(|| "novaterm-icons".into())) {
        if let Err(e) = icon::export(std::path::Path::new(&dir)) {
            eprintln!("Export de l'icône impossible : {e}");
            std::process::exit(1);
        }
        return Ok(());
    }
    if let Some(pos) = args.iter().position(|a| a == "--imgcat") {
        // imgcat [--width N|N%|Npx] fichier… (options et fichiers dans n'importe quel ordre)
        let mut width: Option<String> = None;
        let mut files = Vec::new();
        let mut it = args[pos + 1..].iter();
        while let Some(a) = it.next() {
            match a.as_str() {
                "--width" | "-w" => width = it.next().cloned(),
                s if s.starts_with("--width=") => width = Some(s["--width=".len()..].to_string()),
                _ => files.push(a.clone()),
            }
        }
        if files.is_empty() {
            eprintln!("Usage : imgcat [--width 40|50%|300px] image…  (fichier ou adresse https://)");
            std::process::exit(2);
        }
        let mut failed = false;
        for file in files {
            let read = if is_url(&file) { fetch_url(&file) } else { std::fs::read(&file).map_err(|e| e.to_string()) };
            match read {
                Ok(bytes) => {
                    use std::io::Write;
                    let _ = std::io::stdout().write_all(images::iterm_sequence(&bytes, image_name(&file), width.as_deref()).as_bytes());
                }
                Err(e) => {
                    eprintln!("imgcat : {file} : {e}");
                    failed = true;
                }
            }
        }
        std::process::exit(if failed { 1 } else { 0 });
    }
    if let Some(cmd) = arg_value(&args, "--explain") {
        let ex = helpers::explain(&cmd);
        println!("{}\n", ex.command);
        for p in &ex.parts {
            println!("  {:<14} {}", p.token, p.text);
        }
        if !ex.examples.is_empty() {
            println!("\n{}", ex.examples);
        }
        return Ok(());
    }
    if has("--check-update") {
        match update::check(true) {
            Ok(Some(u)) => println!("NovaTerm {} est disponible (vous avez {}) : {}", u.version, env!("CARGO_PKG_VERSION"), u.url),
            Ok(None) => println!("NovaTerm {} est à jour.", env!("CARGO_PKG_VERSION")),
            Err(e) => {
                eprintln!("Vérification impossible : {e}");
                std::process::exit(1);
            }
        }
        return Ok(());
    }
    if has("--integrate") {
        match appimage::integrate(true) {
            Ok(p) => println!("NovaTerm ajouté au menu des applications : {}", p.display()),
            Err(e) => {
                eprintln!("Intégration impossible : {e}");
                std::process::exit(1);
            }
        }
        return Ok(());
    }
    if has("--unintegrate") {
        match appimage::unintegrate() {
            Ok(()) => println!("NovaTerm retiré du menu (le fichier AppImage est conservé)."),
            Err(e) => eprintln!("Erreur : {e}"),
        }
        return Ok(());
    }
    if let Some(key) = arg_value(&args, "--quake-shortcut") {
        match quake::install_shortcut(&key) {
            Ok(m) => println!("{m}"),
            Err(e) => {
                eprintln!("{e}");
                std::process::exit(1);
            }
        }
        return Ok(());
    }
    if has("--quake-shortcut-remove") {
        match quake::remove_shortcut() {
            Ok(m) => println!("{m}"),
            Err(e) => eprintln!("{e}"),
        }
        return Ok(());
    }
    if has("--toggle") {
        quake::toggle_or_launch();
        return Ok(());
    }
    if has("--reset-config") {
        if let Some(p) = config::Config::path() {
            let backup = p.with_extension("toml.sauvegarde");
            if std::fs::rename(&p, &backup).is_ok() {
                println!("Configuration réinitialisée. Ancienne version : {}", backup.display());
            } else {
                println!("Aucune configuration à réinitialiser.");
            }
        }
        return Ok(());
    }

    // Garde-fou : si le lancement précédent a planté (police cassée…), mode sans échec.
    let crashed = guard::previous_crash();
    let safe_reason = if has("--safe-mode") { Some("manual".to_string()) } else { crashed };
    let (mut config, config_warning) = config::Config::load_checked();
    i18n::set(config.language);
    if safe_reason.is_some() && !config.font_family.is_empty() {
        config.font_family.clear();
        config.save();
    }
    guard::arm("startup");

    let quake_mode = has("--quake");
    let mut viewport = egui::ViewportBuilder::default()
        .with_min_inner_size([420.0, 200.0])
        .with_transparent(true)
        .with_title("NovaTerm")
        .with_app_id("novaterm")
        .with_icon(std::sync::Arc::new(icon::window_icon()));
    if quake_mode {
        viewport = viewport
            .with_decorations(false)
            .with_taskbar(false)
            .with_always_on_top()
            .with_position([0.0, 0.0])
            .with_inner_size([1200.0, 420.0])
            .with_title("NovaTerm (rapide)");
    } else {
        viewport = viewport
            .with_inner_size([config.window_width.max(420.0), config.window_height.max(260.0)])
            .with_decorations(!config.custom_decorations)
            .with_maximized(config.start_maximized)
            .with_fullscreen(config.start_fullscreen);
        if config.always_on_top {
            viewport = viewport.with_always_on_top();
        }
    }
    let options = eframe::NativeOptions { viewport, ..Default::default() };
    let start = StartOptions {
        config,
        quake: quake_mode,
        safe_reason,
        config_warning,
        profile: arg_value(&args, "--profile"),
        // `--working-directory` : nom employé par les bureaux (GNOME, Cinnamon, XFCE)
        // quand on choisit NovaTerm comme terminal par défaut.
        cwd: arg_value(&args, "--cwd")
            .or_else(|| arg_value(&args, "--working-directory"))
            .and_then(|v| platform::clean_dir_arg(&v)),
        command: startup_command(&args),
        // Le dossier de lancement fait qu'un « ouvrir un terminal ici » depuis un
        // gestionnaire de fichiers ouvre bien le shell dans le dossier affiché.
        // Le terminal rapide, lui, est persistant : son dossier ne doit pas dépendre
        // de la façon dont il a été lancé.
        launch_dir: (!quake_mode).then(platform::launch_cwd).flatten(),
    };
    let result = eframe::run_native("NovaTerm", options, Box::new(|cc| Ok(Box::new(NovaApp::new(cc, start)))));
    if quake_mode {
        quake::cleanup();
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn option_values() {
        let a = args(&["novaterm", "--cwd", "/tmp"]);
        assert_eq!(arg_value(&a, "--cwd").as_deref(), Some("/tmp"));
        // Forme « --option=valeur » employée par certains bureaux.
        let a = args(&["novaterm", "--working-directory=/tmp/x"]);
        assert_eq!(arg_value(&a, "--working-directory").as_deref(), Some("/tmp/x"));
        assert_eq!(arg_value(&a, "--cwd"), None);
    }

    #[test]
    fn urls_and_names() {
        assert!(is_url("https://picsum.photos/seed/1/150"));
        assert!(is_url("http://exemple.fr/a.png"));
        assert!(!is_url("/tmp/photo.png"));
        assert!(!is_url("photo.png"));
        // Nom annoncé dans la séquence : dernier segment, sans la requête.
        assert_eq!(image_name("https://picsum.photos/seed/1/150"), "150");
        assert_eq!(image_name("https://exemple.fr/chat.png?taille=2#x"), "chat.png");
        assert_eq!(image_name("https://exemple.fr/dossier/"), "dossier");
        assert_eq!(image_name("/tmp/photo.png"), "photo.png");
        assert_eq!(image_name("https://exemple.fr"), "exemple.fr");
    }

    #[test]
    fn startup_commands() {
        // Ligne déjà complète.
        assert_eq!(startup_command(&args(&["novaterm", "-e", "ls -l"])).as_deref(), Some("ls -l"));
        // Arguments séparés : recollés, ceux qui contiennent un espace sont cités.
        assert_eq!(
            startup_command(&args(&["novaterm", "--command", "git", "commit", "-m", "mon message"])).as_deref(),
            Some("git commit -m 'mon message'")
        );
        assert_eq!(startup_command(&args(&["novaterm", "--cwd", "/tmp"])), None);
        assert_eq!(startup_command(&args(&["novaterm", "-e"])), None);
    }
}
