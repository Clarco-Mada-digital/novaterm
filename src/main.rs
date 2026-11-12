//! NovaTerm — terminal moderne pour Linux, Windows et macOS.

// Pas de fenêtre console noire derrière l'application sous Windows (en release).
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod app;
mod appimage;
mod blocks;
mod completions;
mod config;
mod fonts;
mod guard;
mod history;
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

use app::{NovaApp, StartOptions};

const HELP: &str = "Usage : novaterm [options]

Options :
  --cwd DOSSIER        ouvrir un onglet dans ce dossier
  --profile NOM        ouvrir un profil de projet
  --quake              lancer le terminal rapide (fenêtre qui descend du haut)
  --toggle             afficher / masquer le terminal rapide (à lier à un raccourci)
  --quake-shortcut TOUCHE   créer le raccourci du bureau (ex. F12, Ctrl+Alt+T)
  --quake-shortcut-remove   supprimer ce raccourci
  --imgcat FICHIER     afficher une image dans le terminal (--width N|N%|Npx)
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
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned()
}

fn main() -> eframe::Result<()> {
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
            eprintln!("Usage : imgcat [--width 40|50%|300px] image…  (PNG, JPEG, GIF, WebP, BMP, ICO, SVG)");
            std::process::exit(2);
        }
        let mut failed = false;
        for file in files {
            match std::fs::read(&file) {
                Ok(bytes) => {
                    use std::io::Write;
                    let name = std::path::Path::new(&file).file_name().and_then(|n| n.to_str()).unwrap_or("image");
                    let _ = std::io::stdout().write_all(images::iterm_sequence(&bytes, name, width.as_deref()).as_bytes());
                }
                Err(e) => {
                    eprintln!("imgcat : {file} : {e}");
                    failed = true;
                }
            }
        }
        std::process::exit(if failed { 1 } else { 0 });
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
        cwd: arg_value(&args, "--cwd").map(std::path::PathBuf::from).filter(|p| p.is_dir()),
    };
    let result = eframe::run_native("NovaTerm", options, Box::new(|cc| Ok(Box::new(NovaApp::new(cc, start)))));
    if quake_mode {
        quake::cleanup();
    }
    result
}
