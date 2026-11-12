//! Aides intelligentes **sans IA**, entièrement locales :
//! - correction des fautes de frappe (`gti status` → `git status`) ;
//! - astuces pour les erreurs courantes, avec une correction proposée (jamais exécutée) ;
//! - explication hors ligne d'une commande (whatis, page man, tldr).

use std::collections::HashSet;
use std::process::Command;
use std::sync::OnceLock;

/// Astuce affichée sur un bloc en échec.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tip {
    pub title: String,
    pub detail: String,
    /// Commande proposée (insérée sur la ligne, à valider par l'utilisateur).
    pub fix: Option<String>,
}

impl Tip {
    fn new(title: impl Into<String>, detail: impl Into<String>, fix: Option<String>) -> Self {
        Self { title: title.into(), detail: detail.into(), fix }
    }
}

/// Distance de Damerau-Levenshtein (une inversion de deux lettres compte pour 1).
pub fn distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let (n, m) = (a.len(), b.len());
    let mut d = vec![vec![0usize; m + 1]; n + 1];
    for (i, row) in d.iter_mut().enumerate() {
        row[0] = i;
    }
    for (j, cell) in d[0].iter_mut().enumerate() {
        *cell = j;
    }
    for i in 1..=n {
        for j in 1..=m {
            let cost = usize::from(a[i - 1] != b[j - 1]);
            d[i][j] = (d[i - 1][j] + 1).min(d[i][j - 1] + 1).min(d[i - 1][j - 1] + cost);
            if i > 1 && j > 1 && a[i - 1] == b[j - 2] && a[i - 2] == b[j - 1] {
                d[i][j] = d[i][j].min(d[i - 2][j - 2] + 1);
            }
        }
    }
    d[n][m]
}

/// Exécutables du PATH (lus une fois).
fn path_commands() -> &'static HashSet<String> {
    static CMDS: OnceLock<HashSet<String>> = OnceLock::new();
    CMDS.get_or_init(|| {
        let mut set = HashSet::new();
        if let Some(path) = std::env::var_os("PATH") {
            for dir in std::env::split_paths(&path) {
                let Ok(rd) = std::fs::read_dir(dir) else { continue };
                for e in rd.flatten() {
                    if let Some(name) = e.file_name().to_str() {
                        let name = name.strip_suffix(".exe").unwrap_or(name);
                        if !name.starts_with('.') {
                            set.insert(name.to_string());
                        }
                    }
                }
            }
        }
        for b in ["cd", "export", "source", "alias", "echo", "exit", "history", "type", "unset", "jobs", "fg", "bg"] {
            set.insert(b.to_string());
        }
        set
    })
}

/// Préfixes à ignorer pour trouver le vrai programme d'une ligne.
const WRAPPERS: [&str; 6] = ["sudo", "time", "nohup", "exec", "env", "doas"];

/// Programme d'une ligne de commande et sa position (en octets) dans la ligne.
fn program_of(line: &str) -> Option<(usize, &str)> {
    let mut offset = 0;
    for tok in line.split_whitespace() {
        let pos = line[offset..].find(tok)? + offset;
        offset = pos + tok.len();
        if WRAPPERS.contains(&tok) || tok.contains('=') {
            continue;
        }
        return Some((pos, tok));
    }
    None
}

/// Correction d'une faute de frappe sur le nom du programme. `used` = commandes de
/// l'historique (pour départager). Retourne la ligne corrigée.
pub fn suggest_typo(line: &str, used: &dyn Fn(&str) -> u32) -> Option<String> {
    let (pos, prog) = program_of(line)?;
    if prog.contains('/') || prog.len() < 2 {
        return None;
    }
    let cmds = path_commands();
    if cmds.contains(prog) {
        return None;
    }
    let max = if prog.chars().count() <= 3 { 1 } else { 2 };
    let best = cmds
        .iter()
        .filter(|c| c.len().abs_diff(prog.len()) <= max)
        .filter_map(|c| {
            let d = distance(prog, c);
            (d > 0 && d <= max).then_some((d, std::cmp::Reverse(used(c)), c.len(), c))
        })
        .min()?;
    Some(format!("{}{}{}", &line[..pos], best.3, &line[pos + prog.len()..]))
}

/// Premier nombre après « : » ou « port » dans le texte (numéro de port).
fn find_port(text: &str) -> Option<u16> {
    let bytes = text.as_bytes();
    for (i, w) in text.match_indices([':', ' ']) {
        let _ = w;
        let rest = &bytes[i + 1..];
        let digits: String = rest.iter().take_while(|b| b.is_ascii_digit()).map(|b| *b as char).collect();
        if (2..=5).contains(&digits.len()) {
            if let Ok(p) = digits.parse::<u16>() {
                if p >= 20 {
                    return Some(p);
                }
            }
        }
    }
    None
}

fn quoted_after<'a>(text: &'a str, marker: &str) -> Option<&'a str> {
    let i = text.find(marker)? + marker.len();
    let rest = text[i..].trim_start();
    let q = rest.chars().next()?;
    if q == '\'' || q == '"' {
        rest[1..].split(q).next()
    } else {
        rest.split_whitespace().next()
    }
}

/// Paquet pip pour un module Python dont le nom diffère.
fn pip_name(module: &str) -> &str {
    match module {
        "cv2" => "opencv-python",
        "yaml" => "pyyaml",
        "PIL" => "pillow",
        "sklearn" => "scikit-learn",
        "bs4" => "beautifulsoup4",
        "dotenv" => "python-dotenv",
        "dateutil" => "python-dateutil",
        "jwt" => "pyjwt",
        "MySQLdb" => "mysqlclient",
        "magic" => "python-magic",
        m => m,
    }
}

/// Astuces pour une commande en échec (`output` = fin de sa sortie).
pub fn tips_for(command: &str, output: &str, exit: i32, used: &dyn Fn(&str) -> u32) -> Vec<Tip> {
    let mut tips = Vec::new();
    let low = output.to_lowercase();
    let cmd = command.trim();
    let has_sudo = cmd.starts_with("sudo ");

    if exit == 127 || low.contains("command not found") || low.contains("commande introuvable") {
        if let Some(fixed) = suggest_typo(cmd, used) {
            tips.push(Tip::new("Faute de frappe ?", format!("Vouliez-vous dire « {fixed} » ?"), Some(fixed)));
        } else if let Some((_, prog)) = program_of(cmd) {
            let install = if cfg!(target_os = "linux") {
                Command::new("/usr/lib/command-not-found")
                    .arg(prog)
                    .output()
                    .ok()
                    .map(|o| String::from_utf8_lossy(&o.stderr).to_string() + &String::from_utf8_lossy(&o.stdout))
                    .and_then(|t| t.lines().map(str::trim).find(|l| l.starts_with("sudo apt install")).map(String::from))
            } else {
                None
            };
            tips.push(Tip::new(
                "Programme introuvable",
                format!("« {prog} » n'est pas installé ou n'est pas dans le PATH."),
                install,
            ));
        }
    }
    if low.contains("permission denied") || low.contains("permission non accordée") || low.contains("operation not permitted") {
        if low.contains("docker daemon socket") || low.contains("docker.sock") {
            tips.push(Tip::new(
                "Accès à Docker refusé",
                "Ajoutez votre compte au groupe docker, puis fermez et rouvrez la session.",
                Some("sudo usermod -aG docker $USER".into()),
            ));
        } else if let Some(script) = program_of(cmd).map(|(_, p)| p).filter(|p| p.starts_with("./")) {
            tips.push(Tip::new(
                "Script non exécutable",
                format!("Rendez « {script} » exécutable."),
                Some(format!("chmod +x {script} && {cmd}")),
            ));
        } else if !has_sudo && cfg!(unix) {
            tips.push(Tip::new("Droits insuffisants", "Relancer avec les droits administrateur ?", Some(format!("sudo {cmd}"))));
        }
    }
    if low.contains("address already in use") || low.contains("eaddrinuse") || low.contains("adresse déjà utilisée") {
        let port = find_port(output);
        let fix = port.map(|p| if cfg!(target_os = "linux") { format!("ss -ltnp | grep :{p}") } else { format!("lsof -i :{p}") });
        tips.push(Tip::new(
            "Port déjà utilisé",
            match port {
                Some(p) => format!("Un autre programme écoute déjà sur le port {p} : voyez lequel, ou changez de port."),
                None => "Un autre programme utilise déjà ce port.".into(),
            },
            fix,
        ));
    }
    if low.contains("no space left on device") || low.contains("aucun espace disponible") {
        tips.push(Tip::new("Disque plein", "Voyez l'espace libre et les plus gros dossiers.", Some("df -h && du -sh ~/* 2>/dev/null | sort -h | tail".into())));
    }
    if let Some(m) = quoted_after(output, "No module named") {
        let pkg = pip_name(m.split('.').next().unwrap_or(m));
        tips.push(Tip::new("Module Python manquant", format!("Installez « {pkg} » (dans votre environnement virtuel si vous en utilisez un)."), Some(format!("pip install {pkg}"))));
    }
    if let Some(m) = quoted_after(output, "Cannot find module") {
        if !m.starts_with('.') && !m.starts_with('/') {
            tips.push(Tip::new("Module Node manquant", format!("Installez « {m} »."), Some(format!("npm install {m}"))));
        }
    }
    if low.contains("missing script:") {
        tips.push(Tip::new("Script npm inconnu", "Voici les scripts disponibles dans package.json.", Some("npm run".into())));
    }
    if low.contains("not a git repository") {
        tips.push(Tip::new("Pas un dépôt git", "Ce dossier n'est pas suivi par git : créez un dépôt ou placez-vous dans le bon dossier.", Some("git init".into())));
    }
    if let Some(line) = output.lines().map(str::trim).find(|l| l.starts_with("git push --set-upstream") || l.starts_with("git push -u ")) {
        tips.push(Tip::new("Branche sans amont", "Publiez la branche en la reliant au dépôt distant.", Some(line.to_string())));
    }
    if low.contains("could not resolve host") || low.contains("temporary failure in name resolution") || low.contains("name or service not known") {
        tips.push(Tip::new("Pas de connexion / DNS", "Le nom du serveur ne peut pas être résolu : vérifiez la connexion réseau.", Some("ping -c 3 1.1.1.1".into())));
    }
    if low.contains("unable to locate package") {
        tips.push(Tip::new("Paquet introuvable", "La liste des paquets est peut-être ancienne : mettez-la à jour puis réessayez.", Some(format!("sudo apt update && {cmd}"))));
    }
    if low.contains("could not get lock") && low.contains("dpkg") {
        tips.push(Tip::new("apt est déjà occupé", "Une autre installation est en cours (Gestionnaire de mises à jour ?). Attendez qu'elle se termine.", Some("ps aux | grep -E 'apt|dpkg' | grep -v grep".into())));
    }
    if low.contains("connection refused") {
        tips.push(Tip::new("Connexion refusée", "Le service visé ne tourne pas (ou pas sur ce port).", None));
    }
    if low.contains("could not find `cargo.toml`") {
        tips.push(Tip::new("Pas un projet Rust", "Placez-vous dans le dossier du projet (celui qui contient Cargo.toml).", None));
    }
    if low.contains("externally-managed-environment") {
        tips.push(Tip::new(
            "pip protégé par le système",
            "Utilisez un environnement virtuel pour installer des paquets Python.",
            Some("python3 -m venv .venv && . .venv/bin/activate".into()),
        ));
    }
    tips.dedup_by(|a, b| a.title == b.title);
    tips.truncate(3);
    tips
}

/// Une partie expliquée d'une commande.
#[derive(Clone, Debug)]
pub struct Part {
    pub token: String,
    pub text: String,
}

/// Explication hors ligne : programme (whatis), options (page man), sous-commandes
/// connues, et exemples tldr si l'outil est installé.
#[derive(Clone, Debug, Default)]
pub struct Explanation {
    pub command: String,
    pub parts: Vec<Part>,
    pub examples: String,
}

fn run(prog: &str, args: &[&str]) -> Option<String> {
    let out = Command::new(prog).args(args).env("MANWIDTH", "120").env("MANPAGER", "cat").env("PAGER", "cat").output().ok()?;
    if !out.status.success() && out.stdout.is_empty() {
        return None;
    }
    // Retire le gras / souligné des pages man (« X\bX », « _\bX ») et les couleurs.
    let raw = String::from_utf8_lossy(&out.stdout);
    let mut clean = String::with_capacity(raw.len());
    let mut chars = raw.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\u{8}' {
            clean.pop();
        } else if c == '\x1b' {
            for n in chars.by_ref() {
                if n.is_ascii_alphabetic() {
                    break;
                }
            }
        } else {
            clean.push(c);
        }
    }
    Some(clean)
}

/// Nettoie une description de page man : mots coupés recollés, espaces simples,
/// première phrase seulement.
fn tidy(text: &str) -> String {
    let joined = text.replace("‐ ", "").replace("- \n", "");
    let one: String = joined.split_whitespace().collect::<Vec<_>>().join(" ");
    let first = match one.find(". ") {
        Some(i) if i >= 15 => &one[..=i],
        _ => one.as_str(),
    };
    first.chars().take(200).collect()
}

/// Description d'une option dans le texte d'une page man.
fn option_doc(man: &str, flag: &str) -> Option<String> {
    option_doc_raw(man, flag).map(|t| tidy(&t))
}

fn option_doc_raw(man: &str, flag: &str) -> Option<String> {
    let lines: Vec<&str> = man.lines().collect();
    for (i, l) in lines.iter().enumerate() {
        let t = l.trim_start();
        // Seulement la colonne des options (avant le texte de description).
        let spec = t.split("  ").next().unwrap_or(t);
        let found = spec.split([',', ' ', '=', '[']).any(|w| w == flag);
        if !found || !t.starts_with('-') {
            continue;
        }
        // Texte sur la même ligne après la spécification, sinon lignes suivantes.
        let same: String = t.split("  ").skip(1).map(str::trim).filter(|s| !s.is_empty()).collect::<Vec<_>>().join(" ");
        if !same.is_empty() {
            return Some(same);
        }
        let mut desc = Vec::new();
        for next in lines.iter().skip(i + 1).take(4) {
            let n = next.trim();
            if n.is_empty() || n.starts_with('-') {
                break;
            }
            desc.push(n);
        }
        if !desc.is_empty() {
            let text = desc.join(" ");
            return Some(text.chars().take(220).collect());
        }
    }
    None
}

/// Explique une ligne de commande (lent : à appeler hors de l'interface).
pub fn explain(line: &str) -> Explanation {
    let mut ex = Explanation { command: line.trim().to_string(), ..Default::default() };
    let Some((_, prog)) = program_of(line) else { return ex };
    let name = prog.rsplit('/').next().unwrap_or(prog);
    // Section 1 (commandes) de préférence : « git (1) » plutôt que « Git (3pm) ».
    let summary = run("whatis", &[name])
        .and_then(|w| {
            let lines: Vec<&str> = w.lines().collect();
            let pick = lines.iter().find(|l| l.contains("(1)")).or(lines.first()).copied()?;
            pick.split_once(" - ").map(|(_, d)| d.trim().to_string())
        })
        .filter(|s| !s.is_empty());
    ex.parts.push(Part {
        token: name.to_string(),
        text: summary.unwrap_or_else(|| "programme (pas de description locale ; installez les pages man ou tldr)".into()),
    });
    let words: Vec<&str> = line.split_whitespace().skip_while(|w| *w != prog).skip(1).collect();
    // Sous-commande (git status, docker run…) : ses options sont dans « man git-status ».
    let sub = words.iter().find(|w| !w.starts_with('-')).copied();
    let man = if cfg!(windows) {
        None
    } else {
        sub.and_then(|s| run("man", &[&format!("{name}-{s}")]).filter(|m| m.len() > 200))
            .or_else(|| run("man", &[name]))
    };
    let mut seen_sub = false;
    for w in words.iter().take(12) {
        if w.starts_with("--") {
            let flag = w.split('=').next().unwrap_or(w);
            let text = man.as_deref().and_then(|m| option_doc(m, flag)).unwrap_or_else(|| "option".into());
            ex.parts.push(Part { token: (*w).to_string(), text });
        } else if w.starts_with('-') && w.len() > 1 && !w[1..].starts_with(|c: char| c.is_ascii_digit()) {
            // -la → -l, -a
            for c in w[1..].chars().take(6) {
                let flag = format!("-{c}");
                let text = man.as_deref().and_then(|m| option_doc(m, &flag)).unwrap_or_else(|| "option".into());
                ex.parts.push(Part { token: flag, text });
            }
        } else if !seen_sub && !w.contains('/') && !w.contains('.') {
            seen_sub = true;
            if let Some(d) = crate::completions::describe(name, w) {
                ex.parts.push(Part { token: (*w).to_string(), text: d.to_string() });
            }
        }
    }
    if crate::platform::which("tldr").is_some() {
        if let Some(t) = run("tldr", &[name]) {
            ex.examples = t.lines().filter(|l| !l.trim().is_empty()).take(18).collect::<Vec<_>>().join("\n");
        }
    }
    ex
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distances() {
        assert_eq!(distance("gti", "git"), 1);
        assert_eq!(distance("sl", "ls"), 1);
        assert_eq!(distance("dokcer", "docker"), 1);
        assert_eq!(distance("pyhton", "python"), 1);
        assert_eq!(distance("abc", "xyz"), 3);
    }

    #[test]
    fn typo_keeps_rest_of_line() {
        // « ls » existe partout : « sl -la » → « ls -la ».
        if path_commands().contains("ls") && !path_commands().contains("sl") {
            let fixed = suggest_typo("sudo sl -la /tmp", &|_| 0).unwrap();
            assert!(fixed.starts_with("sudo ") && fixed.ends_with(" -la /tmp"), "{fixed}");
        }
        assert_eq!(suggest_typo("ls -la", &|_| 0), None);
    }

    #[test]
    fn tips() {
        let t = tips_for("python3 -m http.server 8000", "OSError: [Errno 98] Address already in use", 1, &|_| 0);
        assert_eq!(t[0].title, "Port déjà utilisé");
        let t = tips_for("python app.py", "ModuleNotFoundError: No module named 'yaml'", 1, &|_| 0);
        assert_eq!(t[0].fix.as_deref(), Some("pip install pyyaml"));
        let out = "fatal: The current branch dev has no upstream branch.\n    git push --set-upstream origin dev\n";
        let t = tips_for("git push", out, 128, &|_| 0);
        assert_eq!(t[0].fix.as_deref(), Some("git push --set-upstream origin dev"));
        let t = tips_for("./deploy.sh", "bash: ./deploy.sh: Permission denied", 126, &|_| 0);
        assert_eq!(t[0].fix.as_deref(), Some("chmod +x ./deploy.sh && ./deploy.sh"));
        assert!(tips_for("ls", "", 0, &|_| 0).is_empty());
    }

    #[test]
    fn man_option_parsing() {
        let man = "OPTIONS\n       -a, --all\n              do not ignore entries starting with .\n\n       -l     use a long listing format\n";
        assert_eq!(option_doc(man, "-a").as_deref(), Some("do not ignore entries starting with ."));
        assert_eq!(option_doc(man, "--all").as_deref(), Some("do not ignore entries starting with ."));
        assert_eq!(option_doc(man, "-l").as_deref(), Some("use a long listing format"));
        assert_eq!(find_port("Error: listen EADDRINUSE: address already in use :::3000"), Some(3000));
    }
}
