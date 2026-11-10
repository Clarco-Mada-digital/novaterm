//! NovaTerm — terminal moderne pour Linux.

mod app;
mod icon;
mod config;
mod fonts;
mod history;
mod input;
mod keys;
mod layout;
mod render;
mod settings;
mod shell;
mod terminal;
mod theme;

use app::NovaApp;

fn main() -> eframe::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--help" || a == "-h") {
        println!(
            "NovaTerm {} — terminal moderne (onglets, splits, thèmes, transparence, image de fond)\n\n\
             Usage : novaterm\n\n\
             Raccourcis principaux : Ctrl+Maj+P (palette de commandes), Ctrl+, (paramètres), F1 (aide),\n\
             Ctrl+Maj+T (onglet), Ctrl+Tab (onglet suivant), Ctrl+Maj+E/O (split), Alt+Flèches (panneaux),\n\
             Ctrl+Maj+C/V (copier/coller), Ctrl+Maj+F (recherche), Ctrl+Maj+Q (quitter).\n\
             Tous les raccourcis sont modifiables dans Paramètres → Raccourcis.\n\n\
             Configuration : ~/.config/novaterm/config.toml",
            env!("CARGO_PKG_VERSION")
        );
        return Ok(());
    }
    if args.iter().any(|a| a == "--version" || a == "-V") {
        println!("NovaTerm {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }

    if let Some(pos) = args.iter().position(|a| a == "--export-icon") {
        let dir = args.get(pos + 1).map(std::path::PathBuf::from).unwrap_or_else(|| "novaterm-icons".into());
        if let Err(e) = icon::export(&dir) {
            eprintln!("Export de l'icône impossible : {e}");
            std::process::exit(1);
        }
        return Ok(());
    }

    let config = config::Config::load();
    let mut viewport = egui::ViewportBuilder::default()
        .with_inner_size([config.window_width.max(420.0), config.window_height.max(260.0)])
        .with_min_inner_size([420.0, 260.0])
        .with_transparent(true)
        .with_decorations(!config.custom_decorations)
        .with_maximized(config.start_maximized)
        .with_fullscreen(config.start_fullscreen)
        .with_title("NovaTerm")
        .with_app_id("novaterm")
        .with_icon(std::sync::Arc::new(icon::window_icon()));
    if config.always_on_top {
        viewport = viewport.with_always_on_top();
    }
    let options = eframe::NativeOptions { viewport, ..Default::default() };

    eframe::run_native(
        "NovaTerm",
        options,
        Box::new(|cc| Ok(Box::new(NovaApp::new(cc)))),
    )
}