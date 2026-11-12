//! Blocs de commandes et marqueurs invisibles.
//!
//! Les marqueurs sont des caractères Unicode « tag » (U+E0000…U+E007F, largeur nulle)
//! attachés à une cellule de la grille. Ils suivent le texte quand il défile, est
//! redimensionné ou sort de l'historique, et disparaissent si la cellule est effacée.

use std::time::{Duration, Instant};

const TAG_START: char = '\u{E0001}';
const TAG_BASE: u32 = 0xE0000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MarkKind {
    Block,
    Image,
}

pub fn encode(kind: MarkKind, id: u32) -> Vec<char> {
    let k = match kind {
        MarkKind::Block => 'B',
        MarkKind::Image => 'I',
    };
    let mut v = vec![TAG_START, char::from_u32(TAG_BASE + k as u32).unwrap_or(TAG_START)];
    for d in id.to_string().chars() {
        v.push(char::from_u32(TAG_BASE + d as u32).unwrap_or(TAG_START));
    }
    v
}

/// Décode tous les marqueurs d'une liste de caractères « zero-width ».
pub fn decode_all(chars: &[char]) -> Vec<(MarkKind, u32)> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] != TAG_START {
            i += 1;
            continue;
        }
        let kind = match chars.get(i + 1).map(|c| *c as u32 - TAG_BASE.min(*c as u32)) {
            Some(x) if x == 'B' as u32 => MarkKind::Block,
            Some(x) if x == 'I' as u32 => MarkKind::Image,
            _ => {
                i += 1;
                continue;
            }
        };
        let mut j = i + 2;
        let mut id: u32 = 0;
        let mut digits = 0;
        while let Some(c) = chars.get(j) {
            let v = (*c as u32).wrapping_sub(TAG_BASE);
            if (b'0' as u32..=b'9' as u32).contains(&v) {
                id = id.saturating_mul(10).saturating_add(v - b'0' as u32);
                digits += 1;
                j += 1;
            } else {
                break;
            }
        }
        if digits > 0 {
            out.push((kind, id));
        }
        i = j;
    }
    out
}

/// Retire les marqueurs d'un texte copié.
pub fn strip_tags(s: &str) -> String {
    s.chars().filter(|c| !('\u{E0000}'..='\u{E007F}').contains(c)).collect()
}

/// Une commande lancée depuis le prompt.
#[derive(Clone, Debug)]
pub struct Block {
    pub id: u32,
    pub command: String,
    pub started: Instant,
    /// (code de retour, durée) une fois terminée.
    pub finished: Option<(i32, Duration)>,
    /// Lignes du prompt + de la commande (pour retrouver le haut du bloc).
    pub prompt_rows: usize,
}

impl Block {
    pub fn elapsed(&self) -> Duration {
        self.finished.map(|(_, d)| d).unwrap_or_else(|| self.started.elapsed())
    }
}

/// Commande terminée, à signaler (notification, onglet).
#[derive(Clone, Debug)]
pub struct Finished {
    pub id: u32,
    pub command: String,
    pub exit: i32,
    pub duration: Duration,
}

/// Bloc visible à l'écran (lignes relatives au haut du panneau, peuvent être < 0).
#[derive(Clone, Debug)]
pub struct VisibleBlock {
    pub id: u32,
    pub top_row: i32,
    pub cmd_row: i32,
    pub end_row: i32,
}

/// « 850 ms », « 2,3 s », « 1 min 12 s », « 1 h 03 ».
pub fn fmt_duration(d: Duration) -> String {
    let ms = d.as_millis();
    if ms < 1000 {
        format!("{ms} ms")
    } else if ms < 60_000 {
        format!("{:.1} s", d.as_secs_f64()).replace('.', ",")
    } else if ms < 3_600_000 {
        format!("{} min {:02} s", ms / 60_000, (ms / 1000) % 60)
    } else {
        format!("{} h {:02}", ms / 3_600_000, (ms / 60_000) % 60)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_marks() {
        let mut chars = encode(MarkKind::Block, 42);
        chars.extend(encode(MarkKind::Image, 7));
        chars.push('\u{301}');
        assert_eq!(decode_all(&chars), vec![(MarkKind::Block, 42), (MarkKind::Image, 7)]);
        let s: String = format!("ls{}x", encode(MarkKind::Block, 3).into_iter().collect::<String>());
        assert_eq!(strip_tags(&s), "lsx");
    }

    #[test]
    fn durations() {
        assert_eq!(fmt_duration(Duration::from_millis(850)), "850 ms");
        assert_eq!(fmt_duration(Duration::from_millis(2300)), "2,3 s");
        assert_eq!(fmt_duration(Duration::from_secs(72)), "1 min 12 s");
        assert_eq!(fmt_duration(Duration::from_secs(3780)), "1 h 03");
    }
}
