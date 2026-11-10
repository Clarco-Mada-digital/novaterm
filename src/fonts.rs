//! Polices : JetBrains Mono embarquée + découverte des polices monospaces du système (fontconfig).

use std::collections::BTreeMap;
use std::process::Command;
use std::sync::Arc;

use egui::{FontData, FontDefinitions, FontFamily};

/// Polices système utilisées en repli si elles existent.
const FALLBACKS: &[(&str, &str)] = &[
    ("DejaVuMono", "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf"),
    ("FreeMono", "/usr/share/fonts/truetype/freefont/FreeMono.ttf"),
    ("NotoSymbols", "/usr/share/fonts/truetype/noto/NotoSansSymbols-Regular.ttf"),
    ("NotoSymbols2", "/usr/share/fonts/truetype/noto/NotoSansSymbols2-Regular.ttf"),
    ("DejaVuSans", "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf"),
];

/// Nom affiché de la police intégrée.
pub const BUILTIN: &str = "JetBrains Mono (intégrée)";

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

/// Familles monospaces disponibles (triées par nom).
pub fn discover() -> BTreeMap<String, FontEntry> {
    let mut out: BTreeMap<String, FontEntry> = BTreeMap::new();
    for spacing in ["mono", "dual", "charcell"] {
        let Ok(o) = Command::new("fc-list")
            .args(["-f", "%{family[0]}\t%{style[0]}\t%{index}\t%{file}\n", &format!(":spacing={spacing}")])
            .output()
        else {
            continue;
        };
        for line in String::from_utf8_lossy(&o.stdout).lines() {
            let mut it = line.splitn(4, '\t');
            let (Some(family), Some(style), Some(index), Some(path)) = (it.next(), it.next(), it.next(), it.next())
            else {
                continue;
            };
            let lower = path.to_ascii_lowercase();
            if !(lower.ends_with(".ttf") || lower.ends_with(".otf") || lower.ends_with(".ttc")) {
                continue;
            }
            let family = family.trim();
            if family.is_empty() {
                continue;
            }
            let face = FontFace { path: path.to_string(), index: index.trim().parse().unwrap_or(0) };
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
                    // Préférer « Regular » aux graisses exotiques (Light, Thin…).
                    if st == "regular" || entry.regular.is_none() && (plain || !st.contains("light")) {
                        entry.regular = Some(face);
                    }
                }
            }
        }
    }
    out.retain(|_, e| e.regular.is_some());
    out
}

fn load(face: &FontFace) -> Option<FontData> {
    let bytes = std::fs::read(&face.path).ok()?;
    let mut data = FontData::from_owned(bytes);
    data.index = face.index;
    Some(data)
}

/// Installe les familles « Mono », « MonoBold », « MonoItalic », « MonoBoldItalic ».
/// `family` vide (ou introuvable) = JetBrains Mono intégrée.
pub fn install(ctx: &egui::Context, family: &str, catalog: Option<&BTreeMap<String, FontEntry>>) {
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

    if let Some(entry) = catalog.and_then(|c| c.get(family)) {
        if let Some(r) = entry.regular.as_ref().and_then(load) {
            fonts.font_data.insert("UserRegular".into(), Arc::new(r));
            let b = entry.bold.as_ref().and_then(load);
            let i = entry.italic.as_ref().and_then(load);
            let bi = entry.bold_italic.as_ref().and_then(load);
            let has_b = b.is_some();
            let has_i = i.is_some();
            let has_bi = bi.is_some();
            if let Some(b) = b {
                fonts.font_data.insert("UserBold".into(), Arc::new(b));
            }
            if let Some(i) = i {
                fonts.font_data.insert("UserItalic".into(), Arc::new(i));
            }
            if let Some(bi) = bi {
                fonts.font_data.insert("UserBoldItalic".into(), Arc::new(bi));
            }
            let pick = |cands: &[(&str, bool)]| -> Vec<String> {
                let mut v: Vec<String> = cands.iter().filter(|(_, ok)| *ok).map(|(n, _)| n.to_string()).collect();
                v.push("UserRegular".into());
                v.push("JBMono".into());
                v
            };
            regular = vec!["UserRegular".into(), "JBMono".into()];
            bold = pick(&[("UserBold", has_b)]);
            italic = pick(&[("UserItalic", has_i)]);
            bold_italic = pick(&[("UserBoldItalic", has_bi), ("UserBold", has_b)]);
        }
    }

    // Repli pour les symboles absents (box drawing, flèches, ⏺ ⎿ ✻ ⚠…), dans le
    // terminal comme dans l'interface.
    let mut proportional = fonts.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
    proportional.push("JBMono".into());
    for (name, path) in FALLBACKS {
        if let Ok(bytes) = std::fs::read(path) {
            fonts.font_data.insert((*name).into(), Arc::new(FontData::from_owned(bytes)));
            for list in [&mut regular, &mut bold, &mut italic, &mut bold_italic, &mut proportional] {
                list.push((*name).into());
            }
        }
    }
    fonts.families.insert(FontFamily::Proportional, proportional);

    fonts.families.insert(FontFamily::Name("Mono".into()), regular.clone());
    fonts.families.insert(FontFamily::Name("MonoBold".into()), bold);
    fonts.families.insert(FontFamily::Name("MonoItalic".into()), italic);
    fonts.families.insert(FontFamily::Name("MonoBoldItalic".into()), bold_italic);
    fonts.families.insert(FontFamily::Monospace, regular);
    ctx.set_fonts(fonts);
}
