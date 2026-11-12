//! Historique des commandes (zsh, bash, PSReadLine, session) pour les suggestions.
//!
//! Tout reste en mémoire, dans NovaTerm : rien n'est envoyé ni réécrit sur disque.

use crate::i18n::t;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};

/// Tri des suggestions.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Ranking {
    Recent,
    Frequent,
}

impl Ranking {
    pub fn label(self) -> &'static str {
        match self {
            Self::Recent => t("La plus récente"),
            Self::Frequent => t("La plus utilisée"),
        }
    }
}

#[derive(Default)]
pub struct History {
    /// Commandes uniques, de la plus ancienne à la plus récente.
    entries: Vec<String>,
    /// Index dans `entries` et nombre d'utilisations.
    index: HashMap<String, (usize, u32)>,
}

const MAX_ENTRIES: usize = 50_000;

impl History {
    /// Nombre de commandes distinctes.
    pub fn len(&self) -> usize {
        self.index.len()
    }

    /// Ajoute une commande (la place en tête des plus récentes).
    pub fn push(&mut self, cmd: &str) {
        let cmd = cmd.trim_end();
        if cmd.trim().is_empty() || cmd.len() > 4096 {
            return;
        }
        let count = match self.index.remove(cmd) {
            Some((i, n)) => {
                self.entries[i].clear(); // tombstone, compacté plus tard
                n + 1
            }
            None => 1,
        };
        self.entries.push(cmd.to_string());
        self.index.insert(cmd.to_string(), (self.entries.len() - 1, count));
        if self.entries.len() > MAX_ENTRIES * 2 {
            self.compact();
        }
    }

    fn compact(&mut self) {
        let start = self.entries.len().saturating_sub(MAX_ENTRIES);
        let kept: Vec<String> = self.entries.drain(..).skip(start).filter(|e| !e.is_empty()).collect();
        let old = std::mem::take(&mut self.index);
        for (i, e) in kept.iter().enumerate() {
            let n = old.get(e).map(|(_, n)| *n).unwrap_or(1);
            self.index.insert(e.clone(), (i, n));
        }
        self.entries = kept;
    }

    fn ignored(cmd: &str, ignore: &[String]) -> bool {
        ignore.iter().any(|p| !p.is_empty() && cmd.starts_with(p.as_str()))
    }

    /// Commandes qui prolongent `prefix`, de la meilleure à la moins bonne.
    pub fn suggest(&self, prefix: &str, max: usize, ranking: Ranking, ignore: &[String]) -> Vec<String> {
        if prefix.is_empty() || max == 0 {
            return Vec::new();
        }
        let candidates = self
            .entries
            .iter()
            .rev()
            .filter(|e| e.len() > prefix.len() && e.starts_with(prefix) && !e.contains('\n'))
            .filter(|e| !Self::ignored(e, ignore));
        match ranking {
            Ranking::Recent => candidates.take(max).cloned().collect(),
            Ranking::Frequent => {
                let mut v: Vec<(&String, u32, usize)> = candidates
                    .take(2000)
                    .map(|e| {
                        let (i, n) = self.index[e.as_str()];
                        (e, n, i)
                    })
                    .collect();
                v.sort_by(|a, b| b.1.cmp(&a.1).then(b.2.cmp(&a.2)));
                v.into_iter().take(max).map(|(e, ..)| e.clone()).collect()
            }
        }
    }

    /// Recherche floue (tous les mots présents), plus récentes d'abord.
    pub fn search(&self, query: &str, max: usize) -> Vec<String> {
        let words: Vec<String> = query.to_lowercase().split_whitespace().map(String::from).collect();
        self.entries
            .iter()
            .rev()
            .filter(|e| !e.is_empty())
            .filter(|e| {
                let l = e.to_lowercase();
                words.iter().all(|w| l.contains(w.as_str()))
            })
            .take(max)
            .cloned()
            .collect()
    }

    pub fn count(&self, cmd: &str) -> u32 {
        self.index.get(cmd).map(|(_, n)| *n).unwrap_or(0)
    }
}

/// zsh « métafie » les octets ≥ 0x80 : 0x83 suivi de (octet ^ 0x20).
fn unmetafy(bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(bytes.len());
    let mut it = bytes.iter();
    while let Some(&b) = it.next() {
        if b == 0x83 {
            if let Some(&n) = it.next() {
                out.push(n ^ 0x20);
            }
        } else {
            out.push(b);
        }
    }
    out
}

/// Analyse un historique zsh (format simple ou étendu « : ts:durée;cmd »,
/// lignes continuées par « \ »).
pub fn parse_zsh(bytes: &[u8]) -> Vec<String> {
    let text = String::from_utf8_lossy(&unmetafy(bytes)).into_owned();
    let mut out = Vec::new();
    let mut current: Option<String> = None;
    for line in text.lines() {
        let body = match current.take() {
            Some(mut prev) => {
                prev.push('\n');
                prev.push_str(line);
                prev
            }
            None => {
                if let Some(rest) = line.strip_prefix(": ") {
                    rest.split_once(';').map(|(_, c)| c.to_string()).unwrap_or_default()
                } else {
                    line.to_string()
                }
            }
        };
        if let Some(stripped) = body.strip_suffix('\\') {
            current = Some(stripped.to_string());
        } else {
            out.push(body);
        }
    }
    if let Some(rest) = current {
        out.push(rest);
    }
    out
}

/// Analyse un historique bash (lignes « #timestamp » ignorées).
pub fn parse_bash(bytes: &[u8]) -> Vec<String> {
    String::from_utf8_lossy(bytes)
        .lines()
        .filter(|l| !(l.starts_with('#') && l[1..].chars().all(|c| c.is_ascii_digit())))
        .map(String::from)
        .collect()
}

/// Analyse l'historique de PSReadLine (PowerShell) : une commande par ligne, les
/// lignes terminées par un accent grave se poursuivent sur la suivante.
pub fn parse_powershell(bytes: &[u8]) -> Vec<String> {
    // Le fichier est en UTF-8 (avec BOM éventuel) et en fins de ligne CRLF.
    let text = String::from_utf8_lossy(bytes).trim_start_matches('\u{feff}').replace("\r\n", "\n");
    let mut out = Vec::new();
    let mut current: Option<String> = None;
    for line in text.lines() {
        let body = match current.take() {
            Some(mut prev) => {
                prev.push('\n');
                prev.push_str(line);
                prev
            }
            None => line.to_string(),
        };
        match body.strip_suffix('`') {
            Some(stripped) => current = Some(stripped.to_string()),
            None => out.push(body),
        }
    }
    if let Some(rest) = current {
        out.push(rest);
    }
    out
}

/// Fichier d'historique de PSReadLine (Windows PowerShell **et** PowerShell 7).
fn powershell_history_files() -> Vec<PathBuf> {
    let Some(data) = dirs::data_dir() else { return Vec::new() };
    let base = data.join("Microsoft").join("Windows").join("PowerShell").join("PSReadLine");
    // ConsoleHost_history.txt (pwsh et powershell partagent le même dossier).
    let mut out = vec![base.join("ConsoleHost_history.txt")];
    // Hôtes personnalisés (Visual Studio Code, etc.).
    if let Ok(rd) = std::fs::read_dir(&base) {
        for e in rd.flatten() {
            let p = e.path();
            if p.extension().is_some_and(|x| x == "txt") && !out.contains(&p) {
                out.push(p);
            }
        }
    }
    out
}

fn read(path: &Path) -> Option<Vec<u8>> {
    std::fs::read(path).ok()
}

/// Charge les historiques en arrière-plan (sans bloquer l'interface).
pub fn load_async(zsh: bool, bash: bool, powershell: bool) -> Receiver<History> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut h = History::default();
        let home = dirs::home_dir().unwrap_or_default();
        if powershell {
            for path in powershell_history_files() {
                if let Some(b) = read(&path) {
                    for c in parse_powershell(&b) {
                        h.push(&c);
                    }
                }
            }
        }
        if bash {
            let path = std::env::var("HISTFILE")
                .ok()
                .filter(|p| p.contains("bash"))
                .map(Into::into)
                .unwrap_or_else(|| home.join(".bash_history"));
            if let Some(b) = read(&path) {
                for c in parse_bash(&b) {
                    h.push(&c);
                }
            }
        }
        if zsh {
            let zdot = std::env::var("ZDOTDIR").map(Into::into).unwrap_or_else(|_| home.clone());
            for path in [zdot.join(".zsh_history"), home.join(".histfile")] {
                if let Some(b) = read(&path) {
                    for c in parse_zsh(&b) {
                        h.push(&c);
                    }
                    break;
                }
            }
        }
        let _ = tx.send(h);
    });
    rx
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zsh_extended_and_continuation() {
        let raw = b": 1:0;git status\n: 2:0;echo a\\\nb\n: 3:0;ls -la\nplain cmd\n";
        assert_eq!(parse_zsh(raw), vec!["git status", "echo a\nb", "ls -la", "plain cmd"]);
    }

    #[test]
    fn zsh_metafied() {
        // « é » = C3 A9 ; métafié : C3 (0x83, 0xA9^0x20) → 0x83 0x89.
        let raw = [b'e', b'c', b'h', b'o', b' ', 0xC3, 0x83, 0x89];
        assert_eq!(parse_zsh(&raw), vec!["echo é"]);
    }

    #[test]
    fn bash_timestamps() {
        assert_eq!(parse_bash(b"#1700000000\nls\n#comment\n"), vec!["ls", "#comment"]);
    }

    #[test]
    fn powershell_history() {
        // BOM, CRLF et continuation par accent grave.
        let raw = "\u{feff}git status\r\nGet-ChildItem -Recurse `\r\n  -Filter *.rs\r\nnpm run dev\r\n";
        assert_eq!(
            parse_powershell(raw.as_bytes()),
            vec!["git status", "Get-ChildItem -Recurse \n  -Filter *.rs", "npm run dev"]
        );
    }

    #[test]
    fn suggest_recent_and_frequent() {
        let mut h = History::default();
        for c in ["git status", "git push", "git status", "git status", "git pull", "ls"] {
            h.push(c);
        }
        assert_eq!(h.suggest("git", 1, Ranking::Recent, &[]), vec!["git pull"]);
        assert_eq!(h.suggest("git", 1, Ranking::Frequent, &[]), vec!["git status"]);
        assert_eq!(h.suggest("git p", 5, Ranking::Recent, &[]), vec!["git pull", "git push"]);
        assert!(h.suggest("git status", 5, Ranking::Recent, &[]).is_empty());
        assert_eq!(h.suggest("git", 5, Ranking::Recent, &["git pu".into()]), vec!["git status"]);
        assert_eq!(h.count("git status"), 3);
        assert_eq!(h.len(), 4);
        assert_eq!(h.search("GIT pu", 5), vec!["git pull", "git push"]);
    }
}
