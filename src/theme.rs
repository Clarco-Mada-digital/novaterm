//! Palettes de couleurs et thèmes prédéfinis.

use egui::Color32;
use serde::{Deserialize, Serialize};

pub type Rgb = [u8; 3];

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Theme {
    pub name: String,
    pub foreground: Rgb,
    pub background: Rgb,
    pub cursor: Rgb,
    pub selection: Rgb,
    pub ansi: [Rgb; 16],
}

pub const fn rgb(c: Rgb) -> Color32 {
    Color32::from_rgb(c[0], c[1], c[2])
}

pub fn dim(c: Rgb) -> Rgb {
    [
        (c[0] as f32 * 0.66) as u8,
        (c[1] as f32 * 0.66) as u8,
        (c[2] as f32 * 0.66) as u8,
    ]
}

/// Couleur par défaut d'une couleur indexée (cube xterm 256 couleurs).
pub fn indexed_default(i: usize, theme: &Theme) -> Rgb {
    match i {
        0..=15 => theme.ansi[i],
        16..=231 => {
            let i = i - 16;
            let r = i / 36;
            let g = (i % 36) / 6;
            let b = i % 6;
            let conv = |v: usize| if v == 0 { 0 } else { 55 + v * 40 };
            [conv(r) as u8, conv(g) as u8, conv(b) as u8]
        }
        232..=255 => {
            let v = (8 + (i - 232) * 10) as u8;
            [v, v, v]
        }
        _ => theme.foreground,
    }
}

macro_rules! theme {
    ($name:expr, $fg:expr, $bg:expr, $cur:expr, $sel:expr, [$($a:expr),* $(,)?]) => {
        Theme {
            name: $name.to_string(),
            foreground: $fg,
            background: $bg,
            cursor: $cur,
            selection: $sel,
            ansi: [$($a),*],
        }
    };
}

pub fn presets() -> Vec<Theme> {
    vec![
        theme!("Nova Dark", [0xe6, 0xe6, 0xeb], [0x12, 0x14, 0x1c], [0x7a, 0xa2, 0xf7], [0x2d, 0x4f, 0x8e], [
            [0x1a, 0x1c, 0x26], [0xf7, 0x76, 0x8e], [0x9e, 0xce, 0x6a], [0xe0, 0xaf, 0x68],
            [0x7a, 0xa2, 0xf7], [0xbb, 0x9a, 0xf7], [0x7d, 0xcf, 0xff], [0xc0, 0xc7, 0xd4],
            [0x41, 0x48, 0x68], [0xff, 0x9e, 0xb0], [0xb9, 0xe0, 0x8a], [0xff, 0xc7, 0x85],
            [0x9d, 0xb8, 0xff], [0xd0, 0xb5, 0xff], [0xa5, 0xe0, 0xff], [0xff, 0xff, 0xff],
        ]),
        theme!("Dracula", [0xf8, 0xf8, 0xf2], [0x28, 0x2a, 0x36], [0xf8, 0xf8, 0xf2], [0x44, 0x47, 0x5a], [
            [0x21, 0x22, 0x2c], [0xff, 0x55, 0x55], [0x50, 0xfa, 0x7b], [0xf1, 0xfa, 0x8c],
            [0xbd, 0x93, 0xf9], [0xff, 0x79, 0xc6], [0x8b, 0xe9, 0xfd], [0xf8, 0xf8, 0xf2],
            [0x62, 0x72, 0xa4], [0xff, 0x6e, 0x6e], [0x69, 0xff, 0x94], [0xff, 0xff, 0xa5],
            [0xd6, 0xac, 0xff], [0xff, 0x92, 0xdf], [0xa4, 0xff, 0xff], [0xff, 0xff, 0xff],
        ]),
        theme!("Nord", [0xd8, 0xde, 0xe9], [0x2e, 0x34, 0x40], [0xd8, 0xde, 0xe9], [0x43, 0x4c, 0x5e], [
            [0x3b, 0x42, 0x52], [0xbf, 0x61, 0x6a], [0xa3, 0xbe, 0x8c], [0xeb, 0xcb, 0x8b],
            [0x81, 0xa1, 0xc1], [0xb4, 0x8e, 0xad], [0x88, 0xc0, 0xd0], [0xe5, 0xe9, 0xf0],
            [0x4c, 0x56, 0x6a], [0xbf, 0x61, 0x6a], [0xa3, 0xbe, 0x8c], [0xeb, 0xcb, 0x8b],
            [0x81, 0xa1, 0xc1], [0xb4, 0x8e, 0xad], [0x8f, 0xbc, 0xbb], [0xec, 0xef, 0xf4],
        ]),
        theme!("Gruvbox", [0xeb, 0xdb, 0xb2], [0x28, 0x28, 0x28], [0xeb, 0xdb, 0xb2], [0x50, 0x49, 0x45], [
            [0x28, 0x28, 0x28], [0xcc, 0x24, 0x1d], [0x98, 0x97, 0x1a], [0xd7, 0x99, 0x21],
            [0x45, 0x85, 0x88], [0xb1, 0x62, 0x86], [0x68, 0x9d, 0x6a], [0xa8, 0x99, 0x84],
            [0x92, 0x83, 0x74], [0xfb, 0x49, 0x34], [0xb8, 0xbb, 0x26], [0xfa, 0xbd, 0x2f],
            [0x83, 0xa5, 0x98], [0xd3, 0x86, 0x9b], [0x8e, 0xc0, 0x7c], [0xeb, 0xdb, 0xb2],
        ]),
        theme!("Solarized Dark", [0x83, 0x94, 0x96], [0x00, 0x2b, 0x36], [0x93, 0xa1, 0xa1], [0x07, 0x36, 0x42], [
            [0x07, 0x36, 0x42], [0xdc, 0x32, 0x2f], [0x85, 0x99, 0x00], [0xb5, 0x89, 0x00],
            [0x26, 0x8b, 0xd2], [0xd3, 0x36, 0x82], [0x2a, 0xa1, 0x98], [0xee, 0xe8, 0xd5],
            [0x00, 0x2b, 0x36], [0xcb, 0x4b, 0x16], [0x58, 0x6e, 0x75], [0x65, 0x7b, 0x83],
            [0x83, 0x94, 0x96], [0x6c, 0x71, 0xc4], [0x93, 0xa1, 0xa1], [0xfd, 0xf6, 0xe3],
        ]),
        theme!("Catppuccin Mocha", [0xcd, 0xd6, 0xf4], [0x1e, 0x1e, 0x2e], [0xf5, 0xe0, 0xdc], [0x45, 0x47, 0x5a], [
            [0x45, 0x47, 0x5a], [0xf3, 0x8b, 0xa8], [0xa6, 0xe3, 0xa1], [0xf9, 0xe2, 0xaf],
            [0x89, 0xb4, 0xfa], [0xf5, 0xc2, 0xe7], [0x94, 0xe2, 0xd5], [0xba, 0xc2, 0xde],
            [0x58, 0x5b, 0x70], [0xf3, 0x8b, 0xa8], [0xa6, 0xe3, 0xa1], [0xf9, 0xe2, 0xaf],
            [0x89, 0xb4, 0xfa], [0xf5, 0xc2, 0xe7], [0x94, 0xe2, 0xd5], [0xa6, 0xad, 0xc8],
        ]),
        theme!("Tokyo Night", [0xa9, 0xb1, 0xd6], [0x1a, 0x1b, 0x26], [0xc0, 0xca, 0xf5], [0x33, 0x46, 0x7a], [
            [0x32, 0x34, 0x4a], [0xf7, 0x76, 0x8e], [0x9e, 0xce, 0x6a], [0xe0, 0xaf, 0x68],
            [0x7a, 0xa2, 0xf7], [0xbb, 0x9a, 0xf7], [0x7d, 0xcf, 0xff], [0x78, 0x7c, 0x99],
            [0x44, 0x4b, 0x6a], [0xff, 0x7a, 0x93], [0xb9, 0xf2, 0x7c], [0xff, 0x9e, 0x64],
            [0x7d, 0xa6, 0xff], [0xbb, 0x9a, 0xf7], [0x0d, 0xb9, 0xd7], [0xac, 0xb0, 0xd0],
        ]),
        theme!("Monokai", [0xf8, 0xf8, 0xf2], [0x27, 0x28, 0x22], [0xf8, 0xf8, 0xf0], [0x49, 0x48, 0x3e], [
            [0x27, 0x28, 0x22], [0xf9, 0x26, 0x72], [0xa6, 0xe2, 0x2e], [0xf4, 0xbf, 0x75],
            [0x66, 0xd9, 0xef], [0xae, 0x81, 0xff], [0xa1, 0xef, 0xe4], [0xf8, 0xf8, 0xf2],
            [0x75, 0x71, 0x5e], [0xf9, 0x26, 0x72], [0xa6, 0xe2, 0x2e], [0xf4, 0xbf, 0x75],
            [0x66, 0xd9, 0xef], [0xae, 0x81, 0xff], [0xa1, 0xef, 0xe4], [0xf9, 0xf8, 0xf5],
        ]),
        theme!("Solarized Light", [0x65, 0x7b, 0x83], [0xfd, 0xf6, 0xe3], [0x58, 0x6e, 0x75], [0xee, 0xe8, 0xd5], [
            [0x07, 0x36, 0x42], [0xdc, 0x32, 0x2f], [0x85, 0x99, 0x00], [0xb5, 0x89, 0x00],
            [0x26, 0x8b, 0xd2], [0xd3, 0x36, 0x82], [0x2a, 0xa1, 0x98], [0xee, 0xe8, 0xd5],
            [0x00, 0x2b, 0x36], [0xcb, 0x4b, 0x16], [0x58, 0x6e, 0x75], [0x65, 0x7b, 0x83],
            [0x83, 0x94, 0x96], [0x6c, 0x71, 0xc4], [0x93, 0xa1, 0xa1], [0xfd, 0xf6, 0xe3],
        ]),
    ]
}

pub fn by_name(name: &str) -> Theme {
    presets()
        .into_iter()
        .find(|t| t.name == name)
        .unwrap_or_else(|| presets().into_iter().next().unwrap())
}
/// Couleur « #rrggbb », « rrggbb », « 0xrrggbb » ou « #rgb ».
pub fn parse_hex(s: &str) -> Option<Rgb> {
    let h = s.trim().trim_matches(|c| c == '\'' || c == '"').trim_start_matches('#').trim_start_matches("0x");
    let h = h.get(..6).filter(|_| h.len() >= 6).map(str::to_string).or_else(|| {
        (h.len() == 3).then(|| h.chars().flat_map(|c| [c, c]).collect())
    })?;
    let v = u32::from_str_radix(&h, 16).ok()?;
    Some([(v >> 16) as u8, (v >> 8) as u8, v as u8])
}

const ANSI_NAMES: [&str; 8] = ["black", "red", "green", "yellow", "blue", "magenta", "cyan", "white"];

fn finish(name: String, fg: Option<Rgb>, bg: Option<Rgb>, cur: Option<Rgb>, sel: Option<Rgb>, ansi: [Option<Rgb>; 16]) -> Result<Theme, String> {
    let base = by_name("Nova Dark");
    let found = ansi.iter().filter(|c| c.is_some()).count();
    if fg.is_none() && bg.is_none() && found < 8 {
        return Err("aucune couleur reconnue dans ce fichier".into());
    }
    let mut out = [[0u8; 3]; 16];
    for i in 0..16 {
        out[i] = ansi[i].or(ansi[i % 8]).unwrap_or(base.ansi[i]);
    }
    let fg = fg.unwrap_or(out[7]);
    let bg = bg.unwrap_or(out[0]);
    Ok(Theme { name, foreground: fg, background: bg, cursor: cur.unwrap_or(fg), selection: sel.unwrap_or(out[8]), ansi: out })
}

/// Importe un thème : iTerm2 (.itermcolors), Alacritty (.toml/.yml), Kitty (.conf),
/// Windows Terminal (.json), Xresources.
pub fn import_file(path: &std::path::Path) -> Result<Theme, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("lecture impossible : {e}"))?;
    let name = path.file_stem().and_then(|s| s.to_str()).unwrap_or("Importé").replace(['_', '-'], " ");
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    let (mut fg, mut bg, mut cur, mut sel) = (None, None, None, None);
    let mut ansi: [Option<Rgb>; 16] = [None; 16];

    if ext == "itermcolors" || text.contains("<plist") {
        // <key>Ansi 0 Color</key><dict>… Red Component … <real>0.5</real> …</dict>
        let mut rest = text.as_str();
        while let Some(k) = rest.find("<key>") {
            rest = &rest[k + 5..];
            let Some(e) = rest.find("</key>") else { break };
            let key = rest[..e].to_string();
            let Some(dict_end) = rest.find("</dict>") else { break };
            let dict = &rest[e..dict_end];
            let comp = |n: &str| -> Option<u8> {
                let p = dict.find(&format!("<key>{n} Component</key>"))?;
                let r = &dict[p..];
                let s = r.find("<real>")? + 6;
                let v: f32 = r[s..s + r[s..].find("</real>")?].trim().parse().ok()?;
                Some((v.clamp(0.0, 1.0) * 255.0).round() as u8)
            };
            if let (Some(r), Some(g), Some(b)) = (comp("Red"), comp("Green"), comp("Blue")) {
                let c = Some([r, g, b]);
                match key.as_str() {
                    "Foreground Color" => fg = c,
                    "Background Color" => bg = c,
                    "Cursor Color" => cur = c,
                    "Selection Color" => sel = c,
                    k => {
                        if let Some(n) = k.strip_prefix("Ansi ").and_then(|s| s.strip_suffix(" Color")).and_then(|n| n.parse::<usize>().ok()) {
                            if n < 16 {
                                ansi[n] = c;
                            }
                        }
                    }
                }
                rest = &rest[dict_end..];
            }
        }
    } else if ext == "json" || text.trim_start().starts_with('{') {
        let get = |k: &str| -> Option<Rgb> {
            let p = text.find(&format!("\"{k}\""))?;
            let r = &text[p + k.len() + 2..];
            let q = r.find('"')? + 1;
            parse_hex(&r[q..q + r[q..].find('"')?])
        };
        fg = get("foreground");
        bg = get("background");
        cur = get("cursorColor");
        sel = get("selectionBackground");
        for (i, n) in ANSI_NAMES.iter().enumerate() {
            ansi[i] = get(n).or_else(|| if *n == "magenta" { get("purple") } else { None });
            let bright = format!("bright{}{}", n[..1].to_uppercase(), &n[1..]);
            ansi[i + 8] = get(&bright).or_else(|| if *n == "magenta" { get("brightPurple") } else { None });
        }
    } else {
        // Alacritty (toml / yaml), Kitty, Xresources : analyse ligne par ligne.
        let mut section = String::new();
        for raw in text.lines() {
            let line = raw.trim();
            if line.is_empty() || line.starts_with('#') || line.starts_with('!') {
                continue;
            }
            if line.starts_with('[') {
                section = line.trim_matches(['[', ']']).to_ascii_lowercase();
                continue;
            }
            let (k, v) = match line.split_once(['=', ':']) {
                Some((k, v)) if !k.contains(' ') || line.contains('=') => (k.trim(), v.trim()),
                _ => match line.split_once(char::is_whitespace) {
                    Some((k, v)) => (k.trim(), v.trim()),
                    None => continue,
                },
            };
            let key = k.trim_start_matches("*.").trim_start_matches('*').to_ascii_lowercase();
            if v.is_empty() {
                // Section YAML (« primary: », « normal: »…).
                section = key.clone();
                continue;
            }
            let Some(c) = parse_hex(v) else { continue };
            let in_sec = |s: &str| section.ends_with(s);
            match key.as_str() {
                "foreground" if !in_sec("cursor") && !in_sec("selection") => fg = Some(c),
                "background" if in_sec("selection") => sel = Some(c),
                "background" if !in_sec("cursor") => bg = Some(c),
                "cursor" | "cursorcolor" => cur = Some(c),
                "selection_background" => sel = Some(c),
                _ => {
                    if let Some(n) = key.strip_prefix("color").and_then(|n| n.parse::<usize>().ok()) {
                        if n < 16 {
                            ansi[n] = Some(c);
                        }
                    } else if let Some(i) = ANSI_NAMES.iter().position(|a| *a == key) {
                        let idx = if in_sec("bright") { i + 8 } else { i };
                        ansi[idx] = Some(c);
                    }
                }
            }
        }
    }
    finish(name, fg, bg, cur, sel, ansi)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str, content: &str) -> std::path::PathBuf {
        let p = std::env::temp_dir().join(format!("novaterm-theme-{}-{name}", std::process::id()));
        std::fs::write(&p, content).unwrap();
        p
    }

    #[test]
    fn hex() {
        assert_eq!(parse_hex("#1a2b3c"), Some([0x1a, 0x2b, 0x3c]));
        assert_eq!(parse_hex("'0xffffff'"), Some([255, 255, 255]));
        assert_eq!(parse_hex("#fff"), Some([255, 255, 255]));
    }

    #[test]
    fn import_formats() {
        let alacritty = tmp("a.toml", "[colors.primary]\nbackground = '#101010'\nforeground = '#e0e0e0'\n[colors.normal]\nblack='#000000'\nred='#ff0000'\ngreen='#00ff00'\nyellow='#ffff00'\nblue='#0000ff'\nmagenta='#ff00ff'\ncyan='#00ffff'\nwhite='#ffffff'\n[colors.bright]\nred='#ff8080'\n");
        let t = import_file(&alacritty).unwrap();
        assert_eq!(t.background, [0x10, 0x10, 0x10]);
        assert_eq!(t.ansi[1], [255, 0, 0]);
        assert_eq!(t.ansi[9], [0xff, 0x80, 0x80]);
        let kitty = tmp("k.conf", "foreground #dddddd\nbackground #222222\ncolor1 #aa0000\ncolor9 #ff5555\nselection_background #444444\n");
        let t = import_file(&kitty).unwrap();
        assert_eq!((t.foreground, t.ansi[1], t.selection), ([0xdd; 3], [0xaa, 0, 0], [0x44; 3]));
        let wt = tmp("w.json", r##"{"name":"X","background":"#0C0C0C","foreground":"#CCCCCC","red":"#C50F1F","brightRed":"#E74856","purple":"#881798"}"##);
        let t = import_file(&wt).unwrap();
        assert_eq!((t.ansi[1], t.ansi[9], t.ansi[5]), ([0xc5, 0x0f, 0x1f], [0xe7, 0x48, 0x56], [0x88, 0x17, 0x98]));
        let iterm = tmp("i.itermcolors", "<plist><dict><key>Ansi 1 Color</key><dict><key>Blue Component</key><real>0</real><key>Green Component</key><real>0</real><key>Red Component</key><real>1</real></dict><key>Background Color</key><dict><key>Blue Component</key><real>0.1</real><key>Green Component</key><real>0.1</real><key>Red Component</key><real>0.1</real></dict></dict></plist>");
        let t = import_file(&iterm).unwrap();
        assert_eq!((t.ansi[1], t.background), ([255, 0, 0], [26, 26, 26]));
        assert!(import_file(&tmp("e.txt", "hello")).is_err());
        for p in [alacritty, kitty, wt, iterm] {
            let _ = std::fs::remove_file(p);
        }
    }
}
