//! Images dans le terminal : protocole iTerm2 (OSC 1337, `imgcat`) et protocole
//! graphique Kitty (APC G, utilisé par `kitty +kitten icat`, chafa, viu, ranger…).

use std::collections::HashMap;
use std::io::Read;
use std::sync::Arc;

use base64::Engine;

/// Taille demandée pour une dimension (iTerm2).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Dim {
    Auto,
    Cells(u32),
    Pixels(u32),
    Percent(f32),
}

fn parse_dim(s: &str) -> Dim {
    let s = s.trim();
    if s.is_empty() || s == "auto" {
        Dim::Auto
    } else if let Some(px) = s.strip_suffix("px") {
        px.parse().map(Dim::Pixels).unwrap_or(Dim::Auto)
    } else if let Some(p) = s.strip_suffix('%') {
        p.parse().map(Dim::Percent).unwrap_or(Dim::Auto)
    } else {
        s.parse().map(Dim::Cells).unwrap_or(Dim::Auto)
    }
}

/// Taille maximale d'une image transmise (octets).
pub const MAX_IMAGE_BYTES: usize = 64 * 1024 * 1024;
/// Nombre maximal de pixels d'une image brute (4096 × 4096).
pub const MAX_PIXELS: usize = 4096 * 4096;

/// Lit une image locale demandée par un programme (protocole Kitty), avec garde-fous :
/// fichier ordinaire, taille limitée ; un fichier « temporaire » n'est supprimé que
/// s'il se trouve dans le dossier temporaire et porte le nom prévu par le protocole.
fn read_image_file(path: &std::path::Path, temporary: bool) -> Result<Vec<u8>, String> {
    let meta = std::fs::metadata(path).map_err(|_| "fichier illisible".to_string())?;
    if !meta.is_file() {
        return Err("pas un fichier ordinaire".into());
    }
    if meta.len() as usize > MAX_IMAGE_BYTES {
        return Err("fichier trop grand".into());
    }
    let bytes = std::fs::read(path).map_err(|_| "fichier illisible".to_string())?;
    if temporary {
        let in_tmp = path
            .canonicalize()
            .ok()
            .zip(std::env::temp_dir().canonicalize().ok())
            .is_some_and(|(p, tmp)| p.starts_with(&tmp) || p.starts_with("/dev/shm"));
        let named = path.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.contains("tty-graphics-protocol"));
        if in_tmp && named {
            let _ = std::fs::remove_file(path);
        }
    }
    Ok(bytes)
}

/// Image décodée prête à afficher.
pub struct Decoded {
    pub image: Arc<egui::ColorImage>,
}

/// SVG (texte) ou SVGZ (gzip) ?
pub fn looks_like_svg(bytes: &[u8]) -> bool {
    if bytes.starts_with(&[0x1f, 0x8b]) {
        return true; // svgz : vérifié par usvg
    }
    let head = String::from_utf8_lossy(&bytes[..bytes.len().min(4096)]);
    let t = head.trim_start_matches('\u{feff}').trim_start();
    t.starts_with("<svg") || (t.starts_with("<?xml") || t.starts_with("<!--") || t.starts_with("<!DOCTYPE")) && head.contains("<svg")
}

/// Options de rendu SVG.
///
/// Parcourir les polices du système prend plusieurs centaines de millisecondes —
/// sur le fil de l'interface, donc visible. Or la très grande majorité des SVG
/// (icônes, logos, graphiques) ne contient aucun texte : on ne charge les polices
/// que pour ceux qui en ont réellement besoin.
fn svg_options(with_text: bool) -> &'static resvg::usvg::Options<'static> {
    static PLAIN: std::sync::OnceLock<resvg::usvg::Options<'static>> = std::sync::OnceLock::new();
    static WITH_FONTS: std::sync::OnceLock<resvg::usvg::Options<'static>> = std::sync::OnceLock::new();
    if with_text {
        WITH_FONTS.get_or_init(|| {
            let mut o = resvg::usvg::Options::default();
            o.fontdb_mut().load_system_fonts();
            o
        })
    } else {
        PLAIN.get_or_init(resvg::usvg::Options::default)
    }
}

/// Le SVG contient-il du texte à mettre en forme ?
fn svg_has_text(bytes: &[u8]) -> bool {
    // Un SVGZ est compressé : on ne peut pas le savoir sans le décompresser,
    // on suppose alors qu'il peut contenir du texte.
    if bytes.starts_with(&[0x1f, 0x8b]) {
        return true;
    }
    memchr::memmem::find(bytes, b"<text").is_some() || memchr::memmem::find(bytes, b"<tspan").is_some()
}

/// Rendu d'un SVG en image : taille d'origine ×2 pour la netteté (entre 256 et 2048 px).
pub fn decode_svg(bytes: &[u8]) -> Result<Decoded, String> {
    let opts = svg_options(svg_has_text(bytes));
    let tree = resvg::usvg::Tree::from_data(bytes, opts).map_err(|e| format!("SVG illisible : {e}"))?;
    let size = tree.size();
    let (w, h) = (size.width().max(1.0), size.height().max(1.0));
    let longest = w.max(h);
    let target = (longest * 2.0).clamp(256.0, 2048.0);
    let scale = target / longest;
    let (pw, ph) = ((w * scale).round().max(1.0) as u32, (h * scale).round().max(1.0) as u32);
    let mut pixmap = resvg::tiny_skia::Pixmap::new(pw, ph).ok_or("SVG trop grand")?;
    resvg::render(&tree, resvg::tiny_skia::Transform::from_scale(scale, scale), &mut pixmap.as_mut());
    let image = egui::ColorImage::from_rgba_premultiplied([pw as usize, ph as usize], pixmap.data());
    Ok(Decoded { image: Arc::new(image) })
}

/// Décode une image : PNG, JPEG, WebP, GIF, BMP, ICO… et SVG / SVGZ.
pub fn decode_bytes(bytes: &[u8]) -> Result<Decoded, String> {
    if looks_like_svg(bytes) {
        if let Ok(d) = decode_svg(bytes) {
            return Ok(d);
        }
    }
    let img = image::load_from_memory(bytes).map_err(|e| format!("image illisible : {e}"))?;
    // Limite raisonnable : les très grandes images sont réduites.
    let img = if img.width() > 4096 || img.height() > 4096 { img.thumbnail(4096, 4096) } else { img };
    let rgba = img.to_rgba8();
    let size = [rgba.width() as usize, rgba.height() as usize];
    Ok(Decoded { image: Arc::new(egui::ColorImage::from_rgba_unmultiplied(size, rgba.as_raw())) })
}

/// Requête d'affichage, calculée en cellules par le panneau.
pub struct Placement {
    pub image: Arc<egui::ColorImage>,
    pub width: Dim,
    pub height: Dim,
    pub keep_ratio: bool,
    /// Kitty C=1 : ne pas déplacer le curseur.
    pub keep_cursor: bool,
    /// iTerm2 : curseur au début de la ligne suivante ; Kitty : après l'image.
    pub iterm_cursor: bool,
}

/// Calcule la taille en cellules (colonnes, lignes) d'une image.
pub fn cells_for(p: &Placement, cell_w: f32, cell_h: f32, max_cols: usize, max_rows: usize) -> (usize, usize) {
    let (iw, ih) = (p.image.size[0] as f32, p.image.size[1] as f32);
    let to_px = |d: Dim, cell: f32, max_cells: usize| -> Option<f32> {
        match d {
            Dim::Auto => None,
            Dim::Cells(n) => Some(n as f32 * cell),
            Dim::Pixels(n) => Some(n as f32),
            Dim::Percent(pc) => Some(max_cells as f32 * cell * pc / 100.0),
        }
    };
    let (mut w, mut h) = match (to_px(p.width, cell_w, max_cols), to_px(p.height, cell_h, max_rows)) {
        (None, None) => (iw, ih),
        (Some(w), None) => (w, if p.keep_ratio { w * ih / iw } else { ih }),
        (None, Some(h)) => (if p.keep_ratio { h * iw / ih } else { iw }, h),
        (Some(w), Some(h)) => {
            if p.keep_ratio {
                let s = (w / iw).min(h / ih);
                (iw * s, ih * s)
            } else {
                (w, h)
            }
        }
    };
    // Ne jamais dépasser la largeur du panneau (ni ~ sa hauteur).
    let max_w = max_cols as f32 * cell_w;
    let max_h = (max_rows.max(4) as f32 - 1.0) * cell_h;
    let s = (max_w / w).min(max_h / h).min(1.0);
    w *= s;
    h *= s;
    ((w / cell_w).ceil().max(1.0) as usize, (h / cell_h).ceil().max(1.0) as usize)
}

/// OSC 1337 ; File=[args] : base64. Retourne None si l'image n'est pas « inline ».
pub fn parse_iterm(body: &[u8]) -> Result<Option<Placement>, String> {
    let body = std::str::from_utf8(body).map_err(|_| "séquence non UTF-8".to_string())?;
    let rest = body.strip_prefix("File=").ok_or("commande 1337 non gérée")?;
    let (args, data) = rest.split_once(':').ok_or("données manquantes")?;
    let mut inline = false;
    let mut width = Dim::Auto;
    let mut height = Dim::Auto;
    let mut keep_ratio = true;
    for kv in args.split(';') {
        let (k, v) = kv.split_once('=').unwrap_or((kv, ""));
        match k {
            "inline" => inline = v == "1",
            "width" => width = parse_dim(v),
            "height" => height = parse_dim(v),
            "preserveAspectRatio" => keep_ratio = v != "0",
            _ => {}
        }
    }
    if !inline {
        return Ok(None);
    }
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(data.trim().as_bytes())
        .map_err(|e| format!("base64 invalide : {e}"))?;
    let d = decode_bytes(&bytes)?;
    Ok(Some(Placement { image: d.image, width, height, keep_ratio, keep_cursor: false, iterm_cursor: true }))
}

/// État du protocole Kitty pour un panneau (transferts en plusieurs morceaux,
/// images transmises puis affichées plus tard).
#[derive(Default)]
pub struct KittyState {
    pending: Option<(HashMap<String, String>, Vec<u8>)>,
    stored: HashMap<u32, Arc<egui::ColorImage>>,
}

/// Résultat d'une commande Kitty : image à afficher et/ou réponse à renvoyer.
#[derive(Default)]
pub struct KittyResult {
    pub place: Option<Placement>,
    pub reply: Option<String>,
    pub delete_all: bool,
}

fn parse_keys(s: &str) -> HashMap<String, String> {
    s.split(',')
        .filter_map(|kv| kv.split_once('='))
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

impl KittyState {
    pub fn handle(&mut self, body: &[u8], allow_files: bool) -> KittyResult {
        let body = String::from_utf8_lossy(body);
        let (ctrl, payload) = body.split_once(';').unwrap_or((&body, ""));
        let keys = parse_keys(ctrl);
        let more = keys.get("m").map(|v| v == "1").unwrap_or(false);

        // Morceaux successifs : on accumule la charge utile.
        let (keys, data) = match self.pending.take() {
            Some((first, mut acc)) => {
                acc.extend_from_slice(payload.as_bytes());
                if more {
                    self.pending = Some((first, acc));
                    return KittyResult::default();
                }
                (first, acc)
            }
            None => {
                if more {
                    self.pending = Some((keys, payload.as_bytes().to_vec()));
                    return KittyResult::default();
                }
                (keys, payload.as_bytes().to_vec())
            }
        };

        let get = |k: &str| keys.get(k).map(String::as_str);
        let num = |k: &str| get(k).and_then(|v| v.parse::<u32>().ok());
        let action = get("a").unwrap_or("t");
        let id = num("i");
        let quiet = num("q").unwrap_or(0);
        let reply = |msg: &str, ok: bool| -> Option<String> {
            let id = id?;
            if (ok && quiet >= 1) || (!ok && quiet >= 2) {
                return None;
            }
            Some(format!("\x1b_Gi={id};{msg}\x1b\\"))
        };

        match action {
            "d" => {
                let what = get("d").unwrap_or("a");
                if matches!(what, "A" | "I") {
                    self.stored.clear();
                }
                KittyResult { delete_all: true, ..Default::default() }
            }
            "q" | "t" | "T" => {
                let image = match self.decode(&keys, &data, allow_files) {
                    Ok(img) => img,
                    Err(e) => return KittyResult { reply: reply(&format!("EINVAL:{e}"), false), ..Default::default() },
                };
                if action == "q" {
                    return KittyResult { reply: reply("OK", true), ..Default::default() };
                }
                if let Some(id) = id {
                    self.stored.insert(id, image.clone());
                    if self.stored.len() > 64 {
                        if let Some(k) = self.stored.keys().find(|k| **k != id).copied() {
                            self.stored.remove(&k);
                        }
                    }
                }
                let place = (action == "T").then(|| self.placement(&keys, image));
                KittyResult { place, reply: reply("OK", true), delete_all: false }
            }
            "p" => match id.and_then(|i| self.stored.get(&i).cloned()) {
                Some(image) => KittyResult { place: Some(self.placement(&keys, image)), reply: reply("OK", true), delete_all: false },
                None => KittyResult { reply: reply("ENOENT:image inconnue", false), ..Default::default() },
            },
            _ => KittyResult::default(),
        }
    }

    fn placement(&self, keys: &HashMap<String, String>, image: Arc<egui::ColorImage>) -> Placement {
        let num = |k: &str| keys.get(k).and_then(|v| v.parse::<u32>().ok());
        Placement {
            image,
            width: num("c").map(Dim::Cells).unwrap_or(Dim::Auto),
            height: num("r").map(Dim::Cells).unwrap_or(Dim::Auto),
            keep_ratio: !(num("c").is_some() && num("r").is_some()),
            keep_cursor: num("C") == Some(1),
            iterm_cursor: false,
        }
    }

    fn decode(&self, keys: &HashMap<String, String>, data: &[u8], allow_files: bool) -> Result<Arc<egui::ColorImage>, String> {
        let get = |k: &str| keys.get(k).map(String::as_str);
        let num = |k: &str| get(k).and_then(|v| v.parse::<usize>().ok());
        let raw = base64::engine::general_purpose::STANDARD
            .decode(data.iter().copied().filter(|b| !b.is_ascii_whitespace()).collect::<Vec<u8>>())
            .map_err(|_| "base64".to_string())?;
        let mut bytes = match get("t").unwrap_or("d") {
            "d" => raw,
            "f" | "t" => {
                if !allow_files {
                    return Err("fichiers refusés".into());
                }
                let path = std::path::PathBuf::from(String::from_utf8_lossy(&raw).to_string());
                read_image_file(&path, get("t") == Some("t"))?
            }
            _ => return Err("transmission non gérée".into()),
        };
        if get("o") == Some("z") {
            // Limite contre les « bombes » de décompression.
            let mut out = Vec::new();
            flate2::read::ZlibDecoder::new(&bytes[..])
                .take(MAX_IMAGE_BYTES as u64 + 1)
                .read_to_end(&mut out)
                .map_err(|_| "zlib".to_string())?;
            if out.len() > MAX_IMAGE_BYTES {
                return Err("image trop grande".into());
            }
            bytes = out;
        }
        match get("f").unwrap_or("32") {
            "100" => decode_bytes(&bytes).map(|d| d.image),
            f @ ("24" | "32") => {
                let (w, h) = (num("s").ok_or("largeur manquante")?, num("v").ok_or("hauteur manquante")?);
                let bpp = if f == "24" { 3 } else { 4 };
                if w == 0 || h == 0 || w * h > MAX_PIXELS || bytes.len() < w * h * bpp {
                    return Err("taille invalide".into());
                }
                let rgba: Vec<u8> = if bpp == 4 {
                    bytes[..w * h * 4].to_vec()
                } else {
                    bytes[..w * h * 3].chunks(3).flat_map(|p| [p[0], p[1], p[2], 255]).collect()
                };
                Ok(Arc::new(egui::ColorImage::from_rgba_unmultiplied([w, h], &rgba)))
            }
            _ => Err("format non géré".into()),
        }
    }
}

/// Séquence iTerm2 pour afficher un fichier (commande `novaterm --imgcat`).
pub fn iterm_sequence(bytes: &[u8], name: &str, width: Option<&str>) -> String {
    let b64 = base64::engine::general_purpose::STANDARD;
    let mut args = format!("name={};size={};inline=1", b64.encode(name), bytes.len());
    if let Some(w) = width {
        args.push_str(&format!(";width={w}"));
    }
    format!("\x1b]1337;File={args}:{}\x07\n", b64.encode(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png_1x1() -> Vec<u8> {
        let mut buf = Vec::new();
        image::RgbaImage::from_pixel(4, 2, image::Rgba([255, 0, 0, 255]))
            .write_to(&mut std::io::Cursor::new(&mut buf), image::ImageFormat::Png)
            .unwrap();
        buf
    }

    #[test]
    fn iterm_roundtrip() {
        let seq = iterm_sequence(&png_1x1(), "a.png", Some("10"));
        let body = seq.trim_start_matches("\x1b]1337;").trim_end_matches("\x07\n");
        let p = parse_iterm(body.as_bytes()).unwrap().unwrap();
        assert_eq!(p.image.size, [4, 2]);
        assert_eq!(p.width, Dim::Cells(10));
        assert_eq!(cells_for(&p, 8.0, 16.0, 80, 24), (10, 3));
    }

    #[test]
    fn kitty_chunks_and_raw() {
        let b64 = base64::engine::general_purpose::STANDARD.encode(png_1x1());
        let (a, b) = b64.split_at(b64.len() / 2);
        let mut k = KittyState::default();
        let r = k.handle(format!("a=T,f=100,i=5,m=1;{a}").as_bytes(), false);
        assert!(r.place.is_none());
        let r = k.handle(format!("m=0;{b}").as_bytes(), false);
        assert_eq!(r.place.unwrap().image.size, [4, 2]);
        assert_eq!(r.reply.as_deref(), Some("\x1b_Gi=5;OK\x1b\\"));
        // Réaffichage par identifiant.
        assert!(k.handle(b"a=p,i=5", false).place.is_some());
        // RGB brut 1×1.
        let raw = base64::engine::general_purpose::STANDARD.encode([1u8, 2, 3]);
        let r = k.handle(format!("a=T,f=24,s=1,v=1,q=2;{raw}").as_bytes(), false);
        assert_eq!(r.place.unwrap().image.size, [1, 1]);
        // Requête de support.
        let q = k.handle(format!("a=q,i=31,s=1,v=1,f=24;{raw}").as_bytes(), false);
        assert_eq!(q.reply.as_deref(), Some("\x1b_Gi=31;OK\x1b\\"));
    }

    #[test]
    fn svg() {
        let svg = br##"<?xml version="1.0"?><svg xmlns="http://www.w3.org/2000/svg" width="40" height="20"><rect width="40" height="20" fill="#ff0000"/></svg>"##;
        assert!(looks_like_svg(svg));
        let d = decode_bytes(svg).unwrap();
        // 40×20 rendu ×2 puis porté à 256 px de large minimum.
        assert_eq!(d.image.size, [256, 128]);
        assert_eq!(d.image.pixels[0], egui::Color32::from_rgb(255, 0, 0));
        // Icône de NovaTerm (fichier réel).
        assert!(decode_bytes(include_bytes!("../assets/icon/novaterm.svg")).is_ok());
        assert!(!looks_like_svg(b"\x89PNG"));
    }

    #[test]
    fn kitty_file_safety() {
        let dir = std::env::temp_dir().join(format!("novaterm-kitty-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        // Un fichier hors du dossier temporaire n'est jamais supprimé.
        let keep = dir.join("garder.png");
        std::fs::write(&keep, png_1x1()).unwrap();
        assert!(read_image_file(&keep, true).is_ok());
        assert!(keep.exists(), "fichier sans le nom du protocole : conservé");
        let tmpfile = dir.join("tty-graphics-protocol-x.png");
        std::fs::write(&tmpfile, png_1x1()).unwrap();
        assert!(read_image_file(&tmpfile, true).is_ok());
        assert!(!tmpfile.exists(), "fichier temporaire du protocole : supprimé");
        assert!(read_image_file(&dir, false).is_err(), "un dossier est refusé");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn dims() {
        assert_eq!(parse_dim("auto"), Dim::Auto);
        assert_eq!(parse_dim("50%"), Dim::Percent(50.0));
        assert_eq!(parse_dim("120px"), Dim::Pixels(120));
        assert_eq!(parse_dim("12"), Dim::Cells(12));
    }
}
