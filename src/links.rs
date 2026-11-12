//! Liens cliquables : URL, chemins de fichiers (`src/app.rs:120:5`), traces d'erreur
//! (Python `File "x.py", line 3`, Rust `--> src/main.rs:4:2`, JS `(app.js:10:3)`).

use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Target {
    Url(String),
    File { path: PathBuf, line: Option<u32>, col: Option<u32> },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hit {
    pub target: Target,
    pub start: usize,
    pub end: usize,
}

const SCHEMES: [&str; 6] = ["https://", "http://", "file://", "ftp://", "mailto:", "ssh://"];

/// URL couvrant la colonne `col`.
pub fn find_url(chars: &[char], col: usize) -> Option<Hit> {
    let line: String = chars.iter().collect();
    let mut start_byte = 0;
    while start_byte < line.len() {
        let rest = &line[start_byte..];
        let found = SCHEMES.iter().filter_map(|s| rest.find(s)).min()?;
        let abs = start_byte + found;
        let start_col = line[..abs].chars().count();
        let mut end_col = start_col;
        for (i, ch) in chars.iter().enumerate().skip(start_col) {
            if ch.is_whitespace() || matches!(ch, '<' | '>' | '"' | '`' | '\'' | '│' | '\0') {
                break;
            }
            end_col = i + 1;
        }
        while end_col > start_col && matches!(chars[end_col - 1], '.' | ',' | ';' | ':' | '!' | '?' | ')' | ']' | '}') {
            let candidate: String = chars[start_col..end_col].iter().collect();
            if chars[end_col - 1] == ')' && candidate.matches('(').count() >= candidate.matches(')').count() {
                break;
            }
            end_col -= 1;
        }
        let url: String = chars[start_col..end_col].iter().collect();
        if col >= start_col && col < end_col && SCHEMES.iter().all(|s| url.len() > s.len()) {
            return Some(Hit { target: Target::Url(url), start: start_col, end: end_col });
        }
        start_byte = abs + 1;
        while !line.is_char_boundary(start_byte) {
            start_byte += 1;
        }
    }
    None
}

fn resolve(raw: &str, cwd: Option<&Path>) -> Option<PathBuf> {
    let p = if let Some(rest) = raw.strip_prefix("~/") {
        dirs::home_dir()?.join(rest)
    } else {
        let p = PathBuf::from(raw);
        if p.is_absolute() {
            p
        } else {
            cwd?.join(p)
        }
    };
    p.exists().then_some(p)
}

/// Sépare « chemin:ligne:colonne » (sans couper « C:\… » sous Windows).
fn split_location(token: &str) -> (&str, Option<u32>, Option<u32>) {
    let mut path = token;
    let mut nums: Vec<u32> = Vec::new();
    while nums.len() < 2 {
        match path.rfind(':') {
            Some(i) if i > 1 => match path[i + 1..].parse::<u32>() {
                Ok(n) => {
                    nums.insert(0, n);
                    path = &path[..i];
                }
                Err(_) => break,
            },
            _ => break,
        }
    }
    // « fichier.rs(12,5) » (MSVC, TypeScript).
    if nums.is_empty() {
        if let (Some(o), true) = (path.rfind('('), path.ends_with(')')) {
            let inner: Vec<u32> = path[o + 1..path.len() - 1].split(',').filter_map(|n| n.trim().parse().ok()).collect();
            if !inner.is_empty() {
                return (&path[..o], inner.first().copied(), inner.get(1).copied());
            }
        }
    }
    (path, nums.first().copied(), nums.get(1).copied())
}

/// Chemin de fichier (existant) couvrant la colonne `col`.
pub fn find_path(chars: &[char], col: usize, cwd: Option<&Path>) -> Option<Hit> {
    if col >= chars.len() {
        return None;
    }
    let line: String = chars.iter().collect();
    // Python : File "chemin", line N
    if let Some(fpos) = line.find("File \"") {
        let start = line[..fpos].chars().count() + 6;
        if let Some(q) = chars[start..].iter().position(|c| *c == '"') {
            let end = start + q;
            if col >= start.saturating_sub(6) && col < end {
                let raw: String = chars[start..end].iter().collect();
                let after: String = chars[end..].iter().collect();
                let ln = after.split("line ").nth(1).and_then(|s| s.split(|c: char| !c.is_ascii_digit()).next()).and_then(|n| n.parse().ok());
                if let Some(path) = resolve(&raw, cwd) {
                    return Some(Hit { target: Target::File { path, line: ln, col: None }, start, end });
                }
            }
        }
    }
    let is_sep = |c: char| c.is_whitespace() || matches!(c, '"' | '\'' | '`' | '<' | '>' | '|' | '[' | ']' | '{' | '}' | '│' | '\0' | ',');
    if is_sep(chars[col]) {
        return None;
    }
    let mut start = col;
    while start > 0 && !is_sep(chars[start - 1]) {
        start -= 1;
    }
    let mut end = col + 1;
    while end < chars.len() && !is_sep(chars[end]) {
        end += 1;
    }
    // Retirer parenthèses / ponctuation autour (« (src/a.js:1:2) », « a.c:3: »).
    while start < end && matches!(chars[start], '(' | '=' ) {
        start += 1;
    }
    while end > start && matches!(chars[end - 1], ')' | '.' | ';' | ':' | '!' | '?') {
        end -= 1;
    }
    if start >= end || col < start || col >= end {
        return None;
    }
    let token: String = chars[start..end].iter().collect();
    if token.len() < 2 || token.contains("://") {
        return None;
    }
    let (raw, line_no, col_no) = split_location(&token);
    let looks_like_path = raw.contains('/') || raw.contains('\\') || raw.rsplit('.').next().is_some_and(|e| e != raw && e.len() <= 5);
    if !looks_like_path {
        return None;
    }
    let path = resolve(raw, cwd)?;
    Some(Hit { target: Target::File { path, line: line_no, col: col_no }, start, end })
}

/// Lien sous la colonne : URL d'abord, puis chemin.
pub fn find_at(chars: &[char], col: usize, cwd: Option<&Path>) -> Option<Hit> {
    find_url(chars, col).or_else(|| find_path(chars, col, cwd))
}

/// Éditeurs proposés : (libellé, modèle de commande). `{file}`, `{line}`, `{col}`.
/// Un modèle commençant par `@tab ` s'ouvre dans un nouvel onglet NovaTerm.
pub fn editor_presets() -> Vec<(&'static str, &'static str)> {
    vec![
        ("Application par défaut du système", ""),
        ("VS Code", "code -g {file}:{line}:{col}"),
        ("VSCodium", "codium -g {file}:{line}:{col}"),
        ("Zed", "zed {file}:{line}:{col}"),
        ("Sublime Text", "subl {file}:{line}:{col}"),
        ("Xed (Linux Mint)", "xed +{line} {file}"),
        ("Gedit", "gedit +{line} {file}"),
        ("Kate", "kate --line {line} {file}"),
        ("JetBrains (IDEA, PyCharm…)", "idea --line {line} {file}"),
        ("Dans NovaTerm (nano)", "@tab nano +{line} {file}"),
        ("Dans NovaTerm (vim)", "@tab vim +{line} {file}"),
        ("Dans NovaTerm (nvim)", "@tab nvim +{line} {file}"),
    ]
}

/// Premier éditeur graphique installé (pour la valeur par défaut).
pub fn detect_editor() -> String {
    for (bin, tpl) in [
        ("code", "code -g {file}:{line}:{col}"),
        ("codium", "codium -g {file}:{line}:{col}"),
        ("zed", "zed {file}:{line}:{col}"),
        ("subl", "subl {file}:{line}:{col}"),
    ] {
        if crate::platform::which(bin).is_some() {
            return tpl.to_string();
        }
    }
    String::new()
}

/// Commande à lancer pour un fichier (None = ouverture système).
pub fn editor_command(template: &str, path: &Path, line: Option<u32>, col: Option<u32>) -> Option<String> {
    let t = template.trim();
    if t.is_empty() || path.is_dir() {
        return None;
    }
    let quoted = {
        let p = path.display().to_string();
        if p.contains(' ') {
            format!("\"{p}\"")
        } else {
            p
        }
    };
    Some(
        t.replace("{file}", &quoted)
            .replace("{line}", &line.unwrap_or(1).to_string())
            .replace("{col}", &col.unwrap_or(1).to_string()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chars(s: &str) -> Vec<char> {
        s.chars().collect()
    }

    #[test]
    fn urls() {
        let l = chars("voir https://exemple.fr/a(b). fin");
        let h = find_url(&l, 8).unwrap();
        assert_eq!(h.target, Target::Url("https://exemple.fr/a(b)".into()));
        assert_eq!((h.start, h.end), (5, 28));
        assert!(find_url(&l, 1).is_none());
        assert_eq!(find_url(&chars("http://localhost:8000/."), 3).unwrap().target, Target::Url("http://localhost:8000/".into()));
    }

    #[test]
    fn paths_and_traces() {
        let dir = std::env::temp_dir().join(format!("novaterm-links-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("src")).unwrap();
        std::fs::write(dir.join("src/app.rs"), "x").unwrap();
        std::fs::write(dir.join("x.py"), "x").unwrap();
        let cwd = Some(dir.as_path());
        let rust = chars("  --> src/app.rs:12:5");
        let h = find_path(&rust, 10, cwd).unwrap();
        assert_eq!(h.target, Target::File { path: dir.join("src/app.rs"), line: Some(12), col: Some(5) });
        let py = chars("  File \"x.py\", line 42, in <module>");
        let h = find_path(&py, 9, cwd).unwrap();
        assert_eq!(h.target, Target::File { path: dir.join("x.py"), line: Some(42), col: None });
        let js = chars("    at main (src/app.rs:3:9)");
        assert!(matches!(find_path(&js, 16, cwd).unwrap().target, Target::File { line: Some(3), col: Some(9), .. }));
        assert!(find_path(&chars("missing/file.rs:1"), 3, cwd).is_none());
        assert!(find_path(&chars("hello world"), 2, cwd).is_none());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn editor() {
        let cmd = editor_command("code -g {file}:{line}:{col}", Path::new("/a b/c.rs"), Some(3), None).unwrap();
        assert_eq!(cmd, "code -g \"/a b/c.rs\":3:1");
        assert_eq!(split_location("C:\\x\\a.rs:4"), ("C:\\x\\a.rs", Some(4), None));
        assert_eq!(split_location("a.ts(12,5)"), ("a.ts", Some(12), Some(5)));
    }
}
