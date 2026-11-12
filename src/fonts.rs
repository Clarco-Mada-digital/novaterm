//! Polices : JetBrains Mono embarquée + polices monospaces du système.
//!
//! Chaque fichier est **validé** (lecture avec ab_glyph, présence des glyphes de base)
//! avant d'être donné à egui : une police cassée est ignorée au lieu de faire planter
//! l'application.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use ab_glyph::{Font, FontRef};
use egui::{FontData, FontDefinitions, FontFamily};

/// Nom affiché de la police intégrée.
pub const BUILTIN: &str = "JetBrains Mono (intégrée)";

/// Polices système utilisées en repli (symboles, flèches, box drawing) si présentes.
#[cfg(target_os = "linux")]
const FALLBACKS: &[&str] = &[
    "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf",
    "/usr/share/fonts/truetype/freefont/FreeMono.ttf",
    "/usr/share/fonts/truetype/noto/NotoSansSymbols-Regular.ttf",
    "/usr/share/fonts/truetype/noto/NotoSansSymbols2-Regular.ttf",
    "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
    "/usr/share/fonts/TTF/DejaVuSansMono.ttf",
    "/usr/share/fonts/noto/NotoSansSymbols2-Regular.ttf",
];
#[cfg(target_os = "windows")]
const FALLBACKS: &[&str] = &[
    "C:\\Windows\\Fonts\\consola.ttf",
    "C:\\Windows\\Fonts\\seguisym.ttf",
    "C:\\Windows\\Fonts\\segoeui.ttf",
];
#[cfg(target_os = "macos")]
const FALLBACKS: &[&str] = &[
    "/System/Library/Fonts/Menlo.ttc",
    "/System/Library/Fonts/Apple Symbols.ttf",
    "/System/Library/Fonts/Supplemental/Arial Unicode.ttf",
];
#[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
const FALLBACKS: &[&str] = &[];

#[derive(Clone, Debug, Default)]
pub struct FontFace {
    pub path: String,
    pub index: u32,
}

/// Une famille et ses variantes trouvées sur le système.
#[derive(Clone, Debug, Default)]
pub struct FontEntry {
    pub regular: Option<FontFace>,
    pub bold: Option<FontFace>,
    pub italic: Option<FontFace>,
    pub bold_italic: Option<FontFace>,
}

/// Vérifie qu'un fichier de police est lisible et utilisable pour un terminal.
pub fn validate(bytes: &[u8], index: u32) -> Result<(), String> {
    let font = FontRef::try_from_slice_and_index(bytes, index).map_err(|e| format!("fichier illisible ({e})"))?;
    for ch in ['M', 'a', '0'] {
        if font.glyph_id(ch).0 == 0 {
            return Err(format!("le caractère « {ch} » est absent"));
        }
    }
    let m = font.h_advance_unscaled(font.glyph_id('M'));
    if !m.is_finite() || m <= 0.0 || font.height_unscaled() <= 0.0 {
        return Err("métriques invalides".into());
    }
    Ok(())
}

fn is_monospace(bytes: &[u8], index: u32) -> bool {
    let Ok(font) = FontRef::try_from_slice_and_index(bytes, index) else { return false };
    let i = font.h_advance_unscaled(font.glyph_id('i'));
    let m = font.h_advance_unscaled(font.glyph_id('M'));
    m > 0.0 && (i - m).abs() < 0.5
}

fn add_face(out: &mut BTreeMap<String, FontEntry>, family: &str, style: &str, face: FontFace) {
    let st = style.to_ascii_lowercase();
    let bold = st.contains("bold") && !st.contains("semibold") && !st.contains("extrabold");
    let italic = st.contains("italic") || st.contains("oblique");
    let plain = matches!(st.as_str(), "regular" | "book" | "normal" | "roman" | "medium" | "");
    let entry = out.entry(family.to_string()).or_default();
    match (bold, italic) {
        (true, true) => entry.bold_italic = entry.bold_italic.take().or(Some(face)),
        (true, false) => entry.bold = entry.bold.take().or(Some(face)),
        (false, true) => {
            if !st.contains("light") && !st.contains("thin") {
                entry.italic = entry.italic.take().or(Some(face));
            }
        }
        (false, false) => {
            if st == "regular" || entry.regular.is_none() && (plain || !st.contains("light")) {
                entry.regular = Some(face);
            }
        }
    }
}

/// Familles monospaces via fontconfig (Linux, BSD, parfois macOS).
fn discover_fontconfig(out: &mut BTreeMap<String, FontEntry>) -> bool {
    let mut found = false;
    for spacing in ["mono", "dual", "charcell"] {
        let Ok(o) = std::process::Command::new("fc-list")
            .args(["-f", "%{family[0]}\t%{style[0]}\t%{index}\t%{file}\n", &format!(":spacing={spacing}")])
            .output()
        else {
            return false;
        };
        for line in String::from_utf8_lossy(&o.stdout).lines() {
            let mut it = line.splitn(4, '\t');
            let (Some(family), Some(style), Some(index), Some(path)) = (it.next(), it.next(), it.next(), it.next())
            else {
                continue;
            };
            let lower = path.to_ascii_lowercase();
            if !(lower.ends_with(".ttf") || lower.ends_with(".otf") || lower.ends_with(".ttc")) || family.trim().is_empty() {
                continue;
            }
            found = true;
            let face = FontFace { path: path.to_string(), index: index.trim().parse().unwrap_or(0) };
            add_face(out, family.trim(), style, face);
        }
    }
    found
}

fn font_dirs() -> Vec<PathBuf> {
    let mut dirs_list = Vec::new();
    if cfg!(windows) {
        let windir = std::env::var("WINDIR").unwrap_or_else(|_| "C:\\Windows".into());
        dirs_list.push(PathBuf::from(windir).join("Fonts"));
        if let Some(local) = dirs::data_local_dir() {
            dirs_list.push(local.join("Microsoft").join("Windows").join("Fonts"));
        }
    } else if cfg!(target_os = "macos") {
        dirs_list.extend(["/System/Library/Fonts", "/Library/Fonts"].map(PathBuf::from));
        if let Some(h) = dirs::home_dir() {
            dirs_list.push(h.join("Library/Fonts"));
        }
    } else {
        dirs_list.extend(["/usr/share/fonts", "/usr/local/share/fonts"].map(PathBuf::from));
        if let Some(h) = dirs::home_dir() {
            dirs_list.push(h.join(".fonts"));
            dirs_list.push(h.join(".local/share/fonts"));
        }
    }
    dirs_list
}

/// Repli sans fontconfig : parcourt les dossiers de polices et garde les chasses fixes.
fn discover_scan(out: &mut BTreeMap<String, FontEntry>) {
    fn walk(dir: &Path, depth: usize, files: &mut Vec<PathBuf>) {
        let Ok(rd) = std::fs::read_dir(dir) else { return };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() && depth < 4 {
                walk(&p, depth + 1, files);
            } else if let Some(ext) = p.extension().and_then(|e| e.to_str()) {
                if matches!(ext.to_ascii_lowercase().as_str(), "ttf" | "otf") {
                    files.push(p);
                }
            }
        }
    }
    let mut files = Vec::new();
    for d in font_dirs() {
        walk(&d, 0, &mut files);
    }
    for path in files.into_iter().take(3000) {
        let Ok(bytes) = std::fs::read(&path) else { continue };
        if !is_monospace(&bytes, 0) {
            continue;
        }
        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or_default().to_string();
        // « JetBrainsMono-BoldItalic » → famille « JetBrainsMono », style « BoldItalic ».
        let (family, style) = stem.split_once('-').map(|(f, s)| (f.to_string(), s.to_string())).unwrap_or((stem.clone(), "Regular".into()));
        add_face(out, &family, &style, FontFace { path: path.display().to_string(), index: 0 });
    }
}

/// Familles monospaces disponibles (triées par nom).
pub fn discover() -> BTreeMap<String, FontEntry> {
    let mut out: BTreeMap<String, FontEntry> = BTreeMap::new();
    if !discover_fontconfig(&mut out) {
        discover_scan(&mut out);
    }
    out.retain(|_, e| e.regular.is_some());
    out
}

fn load(face: &FontFace) -> Option<FontData> {
    let bytes = std::fs::read(&face.path).ok()?;
    if let Err(e) = validate(&bytes, face.index) {
        eprintln!("NovaTerm: police ignorée {} : {e}", face.path);
        return None;
    }
    let mut data = FontData::from_owned(bytes);
    data.index = face.index;
    Some(data)
}

/// Installe les familles « Mono », « MonoBold », « MonoItalic », « MonoBoldItalic ».
/// `family` vide, introuvable ou invalide = JetBrains Mono intégrée.
/// Retourne `Err` (avec la raison) si la famille demandée n'a pas pu être utilisée.
pub fn install(ctx: &egui::Context, family: &str, catalog: Option<&BTreeMap<String, FontEntry>>) -> Result<(), String> {
    let mut fonts = FontDefinitions::default();
    let embedded: [(&str, &'static [u8]); 4] = [
        ("JBMono", include_bytes!("../assets/fonts/JetBrainsMono-Regular.ttf")),
        ("JBMonoBold", include_bytes!("../assets/fonts/JetBrainsMono-Bold.ttf")),
        ("JBMonoItalic", include_bytes!("../assets/fonts/JetBrainsMono-Italic.ttf")),
        ("JBMonoBoldItalic", include_bytes!("../assets/fonts/JetBrainsMono-BoldItalic.ttf")),
    ];
    for (name, bytes) in embedded {
        fonts.font_data.insert(name.into(), Arc::new(FontData::from_static(bytes)));
    }

    let mut regular = vec!["JBMono".to_string()];
    let mut bold = vec!["JBMonoBold".to_string(), "JBMono".to_string()];
    let mut italic = vec!["JBMonoItalic".to_string(), "JBMono".to_string()];
    let mut bold_italic = vec!["JBMonoBoldItalic".to_string(), "JBMonoBold".to_string(), "JBMono".to_string()];
    let mut result = Ok(());

    if !family.is_empty() {
        match catalog.and_then(|c| c.get(family)) {
            None => result = Err(format!("police « {family} » introuvable")),
            Some(entry) => match entry.regular.as_ref().and_then(load) {
                None => result = Err(format!("police « {family} » invalide ou illisible")),
                Some(r) => {
                    fonts.font_data.insert("UserRegular".into(), Arc::new(r));
                    let mut variant = |key: &str, face: &Option<FontFace>| -> bool {
                        match face.as_ref().and_then(load) {
                            Some(d) => {
                                fonts.font_data.insert(key.into(), Arc::new(d));
                                true
                            }
                            None => false,
                        }
                    };
                    let has_b = variant("UserBold", &entry.bold);
                    let has_i = variant("UserItalic", &entry.italic);
                    let has_bi = variant("UserBoldItalic", &entry.bold_italic);
                    let pick = |cands: &[(&str, bool)]| -> Vec<String> {
                        let mut v: Vec<String> =
                            cands.iter().filter(|(_, ok)| *ok).map(|(n, _)| n.to_string()).collect();
                        v.push("UserRegular".into());
                        v.push("JBMono".into());
                        v
                    };
                    regular = vec!["UserRegular".into(), "JBMono".into()];
                    bold = pick(&[("UserBold", has_b)]);
                    italic = pick(&[("UserItalic", has_i)]);
                    bold_italic = pick(&[("UserBoldItalic", has_bi), ("UserBold", has_b)]);
                }
            },
        }
    }

    // Replis pour les symboles absents, dans le terminal comme dans l'interface.
    let mut proportional = fonts.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
    proportional.push("JBMono".into());
    for (i, path) in FALLBACKS.iter().enumerate() {
        let Ok(bytes) = std::fs::read(path) else { continue };
        if validate(&bytes, 0).is_err() {
            continue;
        }
        let name = format!("Fallback{i}");
        fonts.font_data.insert(name.clone(), Arc::new(FontData::from_owned(bytes)));
        for list in [&mut regular, &mut bold, &mut italic, &mut bold_italic, &mut proportional] {
            list.push(name.clone());
        }
    }
    fonts.families.insert(FontFamily::Proportional, proportional);
    fonts.families.insert(FontFamily::Name("Mono".into()), regular.clone());
    fonts.families.insert(FontFamily::Name("MonoBold".into()), bold);
    fonts.families.insert(FontFamily::Name("MonoItalic".into()), italic);
    fonts.families.insert(FontFamily::Name("MonoBoldItalic".into()), bold_italic);
    fonts.families.insert(FontFamily::Monospace, regular);
    ctx.set_fonts(fonts);
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_embedded_and_rejects_garbage() {
        assert!(validate(include_bytes!("../assets/fonts/JetBrainsMono-Regular.ttf"), 0).is_ok());
        assert!(validate(b"not a font at all", 0).is_err());
        let mut broken = include_bytes!("../assets/fonts/JetBrainsMono-Regular.ttf")[..2000].to_vec();
        broken[0] ^= 0xff;
        assert!(validate(&broken, 0).is_err());
        assert!(is_monospace(include_bytes!("../assets/fonts/JetBrainsMono-Regular.ttf"), 0));
    }
}
