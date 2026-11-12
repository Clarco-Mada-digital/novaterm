//! Complétion intelligente : sous-commandes et options (avec description) des outils
//! courants, et sources dynamiques (branches git, scripts npm, cibles make, hôtes SSH).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Completion {
    /// Ligne complète après acceptation.
    pub full: String,
    pub label: String,
    pub desc: String,
}

type Pairs = &'static [(&'static str, &'static str)];

const GIT: Pairs = &[
    ("status", "état du dépôt"),
    ("add", "ajouter à l'index"),
    ("commit", "enregistrer les changements"),
    ("push", "envoyer vers le dépôt distant"),
    ("pull", "récupérer et fusionner"),
    ("fetch", "récupérer sans fusionner"),
    ("checkout", "changer de branche / restaurer"),
    ("switch", "changer de branche"),
    ("branch", "lister / créer des branches"),
    ("merge", "fusionner une branche"),
    ("rebase", "rebaser la branche"),
    ("log", "historique des commits"),
    ("diff", "différences"),
    ("stash", "mettre de côté"),
    ("reset", "annuler des changements"),
    ("restore", "restaurer des fichiers"),
    ("clone", "cloner un dépôt"),
    ("init", "créer un dépôt"),
    ("remote", "dépôts distants"),
    ("tag", "étiquettes"),
    ("show", "afficher un objet"),
    ("cherry-pick", "appliquer un commit"),
    ("revert", "inverser un commit"),
    ("blame", "auteur de chaque ligne"),
    ("clean", "supprimer les fichiers non suivis"),
    ("rm", "supprimer des fichiers suivis"),
    ("mv", "déplacer / renommer"),
    ("config", "configuration"),
    ("worktree", "arbres de travail"),
    ("bisect", "recherche dichotomique d'un bug"),
];
const GIT_OPTS: Pairs = &[
    ("--help", "aide"),
    ("-m", "message de commit"),
    ("--amend", "modifier le dernier commit"),
    ("--all", "tout"),
    ("--oneline", "une ligne par commit"),
    ("--graph", "graphe des branches"),
    ("--force-with-lease", "forcer prudemment"),
    ("-u", "définir la branche amont"),
    ("--staged", "changements indexés"),
    ("-b", "créer une branche"),
    ("--hard", "réinitialisation complète"),
    ("--soft", "garder les changements indexés"),
];
const DOCKER: Pairs = &[
    ("ps", "conteneurs en cours"),
    ("images", "images locales"),
    ("run", "lancer un conteneur"),
    ("exec", "commande dans un conteneur"),
    ("build", "construire une image"),
    ("compose", "applications multi-conteneurs"),
    ("logs", "journaux d'un conteneur"),
    ("stop", "arrêter"),
    ("start", "démarrer"),
    ("restart", "redémarrer"),
    ("rm", "supprimer des conteneurs"),
    ("rmi", "supprimer des images"),
    ("pull", "télécharger une image"),
    ("push", "publier une image"),
    ("inspect", "détails bas niveau"),
    ("network", "réseaux"),
    ("volume", "volumes"),
    ("system", "gestion (prune, df…)"),
    ("login", "se connecter à un registre"),
    ("tag", "étiqueter une image"),
];
const COMPOSE: Pairs = &[
    ("up", "créer et démarrer"),
    ("down", "arrêter et supprimer"),
    ("ps", "lister les services"),
    ("logs", "journaux"),
    ("build", "construire les services"),
    ("exec", "commande dans un service"),
    ("restart", "redémarrer"),
    ("stop", "arrêter"),
    ("pull", "télécharger les images"),
    ("config", "afficher la configuration"),
];
const DOCKER_OPTS: Pairs = &[
    ("-d", "en arrière-plan"),
    ("-it", "interactif avec terminal"),
    ("--rm", "supprimer à l'arrêt"),
    ("-p", "publier un port"),
    ("-v", "monter un volume"),
    ("-e", "variable d'environnement"),
    ("--name", "nom du conteneur"),
    ("-f", "suivre / fichier"),
    ("--build", "reconstruire avant de lancer"),
    ("-a", "tous"),
];
const CARGO: Pairs = &[
    ("build", "compiler"),
    ("run", "compiler et lancer"),
    ("test", "lancer les tests"),
    ("check", "vérifier sans compiler"),
    ("clippy", "analyse (lint)"),
    ("fmt", "formater le code"),
    ("add", "ajouter une dépendance"),
    ("remove", "retirer une dépendance"),
    ("update", "mettre à jour Cargo.lock"),
    ("doc", "générer la documentation"),
    ("clean", "supprimer target/"),
    ("new", "nouveau projet"),
    ("init", "projet dans le dossier courant"),
    ("install", "installer un binaire"),
    ("bench", "lancer les benchmarks"),
    ("tree", "arbre des dépendances"),
    ("publish", "publier sur crates.io"),
];
const CARGO_OPTS: Pairs = &[
    ("--release", "mode optimisé"),
    ("--all-targets", "toutes les cibles"),
    ("--workspace", "tout l'espace de travail"),
    ("--features", "activer des fonctionnalités"),
    ("--target", "plateforme cible"),
    ("--bin", "binaire précis"),
    ("--example", "exemple précis"),
    ("--no-default-features", "sans les fonctionnalités par défaut"),
    ("-q", "silencieux"),
];
const NPM: Pairs = &[
    ("install", "installer les dépendances"),
    ("run", "lancer un script"),
    ("start", "script start"),
    ("test", "script test"),
    ("init", "nouveau package.json"),
    ("uninstall", "désinstaller"),
    ("update", "mettre à jour"),
    ("outdated", "paquets obsolètes"),
    ("audit", "audit de sécurité"),
    ("ci", "installation propre (CI)"),
    ("publish", "publier"),
    ("ls", "dépendances installées"),
    ("exec", "lancer une commande de paquet"),
];
const NPM_OPTS: Pairs = &[
    ("--save-dev", "dépendance de développement"),
    ("-D", "dépendance de développement"),
    ("-g", "installation globale"),
    ("--force", "forcer"),
];
const PIP: Pairs = &[
    ("install", "installer"),
    ("uninstall", "désinstaller"),
    ("list", "paquets installés"),
    ("freeze", "liste au format requirements"),
    ("show", "détails d'un paquet"),
    ("download", "télécharger"),
];
const PIP_OPTS: Pairs = &[
    ("-r", "depuis un fichier requirements"),
    ("-U", "mettre à jour"),
    ("--user", "pour l'utilisateur"),
    ("-e", "mode éditable"),
];
const SYSTEMCTL: Pairs = &[
    ("status", "état d'un service"),
    ("start", "démarrer"),
    ("stop", "arrêter"),
    ("restart", "redémarrer"),
    ("reload", "recharger la configuration"),
    ("enable", "activer au démarrage"),
    ("disable", "désactiver au démarrage"),
    ("list-units", "unités chargées"),
    ("daemon-reload", "recharger systemd"),
    ("is-active", "service actif ?"),
];
const APT: Pairs = &[
    ("update", "mettre à jour la liste des paquets"),
    ("upgrade", "mettre à jour les paquets"),
    ("install", "installer"),
    ("remove", "désinstaller"),
    ("purge", "désinstaller avec la configuration"),
    ("autoremove", "supprimer les paquets inutiles"),
    ("search", "rechercher"),
    ("show", "détails d'un paquet"),
    ("list", "lister (--installed, --upgradable)"),
    ("full-upgrade", "mise à niveau complète"),
];
const DJANGO: Pairs = &[
    ("runserver", "serveur de développement"),
    ("migrate", "appliquer les migrations"),
    ("makemigrations", "créer les migrations"),
    ("createsuperuser", "créer un administrateur"),
    ("shell", "shell Python avec Django"),
    ("test", "lancer les tests"),
    ("collectstatic", "rassembler les fichiers statiques"),
    ("showmigrations", "état des migrations"),
    ("startapp", "nouvelle application"),
    ("dbshell", "shell de la base de données"),
    ("check", "vérifier le projet"),
    ("loaddata", "charger des données"),
    ("dumpdata", "exporter des données"),
    ("flush", "vider la base"),
    ("sqlmigrate", "SQL d'une migration"),
    ("changepassword", "changer un mot de passe"),
];
const KUBECTL: Pairs = &[
    ("get", "lister des ressources"),
    ("describe", "détails d'une ressource"),
    ("apply", "appliquer une configuration"),
    ("delete", "supprimer"),
    ("logs", "journaux d'un pod"),
    ("exec", "commande dans un conteneur"),
    ("port-forward", "rediriger un port"),
    ("rollout", "gérer un déploiement"),
    ("config", "contextes et configuration"),
    ("scale", "changer le nombre de répliques"),
];

const WINGET: Pairs = &[
    ("install", "installer un paquet"),
    ("uninstall", "désinstaller"),
    ("upgrade", "mettre à jour (--all pour tout)"),
    ("search", "rechercher"),
    ("show", "détails d'un paquet"),
    ("list", "paquets installés"),
    ("source", "sources de paquets"),
    ("export", "exporter la liste"),
    ("import", "installer depuis une liste"),
    ("pin", "épingler une version"),
];
const SCOOP: Pairs = &[
    ("install", "installer"),
    ("uninstall", "désinstaller"),
    ("update", "mettre à jour (* pour tout)"),
    ("list", "paquets installés"),
    ("search", "rechercher"),
    ("info", "détails d'un paquet"),
    ("bucket", "dépôts de paquets"),
    ("status", "mises à jour disponibles"),
    ("cleanup", "supprimer les anciennes versions"),
];
const CHOCO: Pairs = &[
    ("install", "installer"),
    ("uninstall", "désinstaller"),
    ("upgrade", "mettre à jour"),
    ("list", "paquets installés"),
    ("search", "rechercher"),
    ("info", "détails d'un paquet"),
    ("outdated", "paquets obsolètes"),
];
const DOTNET: Pairs = &[
    ("build", "compiler"),
    ("run", "compiler et lancer"),
    ("test", "lancer les tests"),
    ("restore", "restaurer les dépendances"),
    ("publish", "publier"),
    ("clean", "nettoyer"),
    ("add", "ajouter une référence / un paquet"),
    ("new", "nouveau projet"),
    ("watch", "relancer à chaque modification"),
    ("tool", "outils globaux"),
    ("sln", "fichier de solution"),
    ("format", "formater le code"),
];
const DOTNET_OPTS: Pairs = &[
    ("-c", "configuration (Debug / Release)"),
    ("--configuration", "configuration (Debug / Release)"),
    ("-f", "framework cible"),
    ("--no-build", "sans recompiler"),
    ("--no-restore", "sans restaurer"),
    ("-v", "niveau de détail"),
];
/// Applets PowerShell courantes : utile dès les premières lettres.
const PS_CMDLETS: Pairs = &[
    ("Get-ChildItem", "lister un dossier (ls, dir)"),
    ("Get-Content", "afficher un fichier (cat, type)"),
    ("Get-Command", "où se trouve une commande"),
    ("Get-Help", "aide d'une commande"),
    ("Get-Process", "processus en cours"),
    ("Get-Service", "services Windows"),
    ("Set-Location", "changer de dossier (cd)"),
    ("New-Item", "créer un fichier / dossier"),
    ("Remove-Item", "supprimer (rm, del)"),
    ("Copy-Item", "copier (cp, copy)"),
    ("Move-Item", "déplacer (mv, move)"),
    ("Rename-Item", "renommer"),
    ("Select-String", "rechercher dans des fichiers (grep)"),
    ("Select-Object", "choisir des propriétés"),
    ("Where-Object", "filtrer"),
    ("ForEach-Object", "traiter chaque élément"),
    ("Sort-Object", "trier"),
    ("Measure-Object", "compter / mesurer"),
    ("Test-Path", "le chemin existe-t-il ?"),
    ("Invoke-WebRequest", "télécharger (curl, wget)"),
    ("Invoke-RestMethod", "appeler une API JSON"),
    ("Start-Process", "lancer un programme"),
    ("Stop-Process", "arrêter un processus"),
    ("Get-NetTCPConnection", "connexions et ports"),
    ("Set-ExecutionPolicy", "stratégie d'exécution des scripts"),
    ("Out-File", "écrire dans un fichier"),
    ("Write-Host", "afficher un message"),
    ("Clear-Host", "effacer l'écran (cls)"),
];

fn spec(cmd: &str, sub: Option<&str>) -> Option<(Pairs, Pairs)> {
    Some(match (cmd, sub) {
        ("git", None) => (GIT, GIT_OPTS),
        ("git", Some(_)) => (&[], GIT_OPTS),
        ("docker", None) => (DOCKER, DOCKER_OPTS),
        ("docker", Some("compose")) | ("docker-compose", None) => (COMPOSE, DOCKER_OPTS),
        ("docker", Some(_)) | ("docker-compose", Some(_)) => (&[], DOCKER_OPTS),
        ("cargo", None) => (CARGO, CARGO_OPTS),
        ("cargo", Some(_)) => (&[], CARGO_OPTS),
        ("npm" | "pnpm" | "yarn" | "bun", None) => (NPM, NPM_OPTS),
        ("npm" | "pnpm" | "yarn" | "bun", Some(_)) => (&[], NPM_OPTS),
        ("pip" | "pip3", None) => (PIP, PIP_OPTS),
        ("pip" | "pip3", Some(_)) => (&[], PIP_OPTS),
        ("systemctl", None) => (SYSTEMCTL, &[]),
        ("apt" | "apt-get", None) => (APT, &[("-y", "répondre oui")]),
        ("apt" | "apt-get", Some(_)) => (&[], &[("-y", "répondre oui"), ("--installed", "installés"), ("--upgradable", "à mettre à jour")]),
        ("kubectl", None) => (KUBECTL, &[("-n", "espace de noms"), ("-o", "format de sortie")]),
        ("winget", None) => (WINGET, &[("-e", "correspondance exacte"), ("--id", "par identifiant"), ("--all", "tout")]),
        ("winget", Some(_)) => (&[], &[("-e", "correspondance exacte"), ("--id", "par identifiant"), ("--silent", "sans interaction")]),
        ("scoop", None) => (SCOOP, &[]),
        ("choco", None) => (CHOCO, &[("-y", "répondre oui")]),
        ("choco", Some(_)) => (&[], &[("-y", "répondre oui")]),
        ("dotnet", None) => (DOTNET, DOTNET_OPTS),
        ("dotnet", Some(_)) => (&[], DOTNET_OPTS),
        _ => return None,
    })
}

type Candidates = Vec<(String, String)>;

/// Description d'une sous-commande connue (« git status » → « état du dépôt »).
pub fn describe(cmd: &str, sub: &str) -> Option<&'static str> {
    let (subs, opts) = spec(cmd, None)?;
    subs.iter().chain(opts.iter()).find(|(name, _)| *name == sub).map(|(_, d)| *d)
}

/// Nombre maximal d'entrées gardées en cache (sources × dossiers).
const MAX_CACHE: usize = 64;

/// Cache des sources dynamiques (évite de relancer git à chaque touche).
#[derive(Default)]
pub struct Completer {
    cache: HashMap<(String, PathBuf), (Instant, Candidates)>,
}

impl Completer {
    fn cached(&mut self, kind: &str, cwd: &Path, ttl: Duration, f: impl FnOnce(&Path) -> Vec<(String, String)>) -> Vec<(String, String)> {
        let key = (kind.to_string(), cwd.to_path_buf());
        if let Some((t, v)) = self.cache.get(&key) {
            if t.elapsed() < ttl {
                return v.clone();
            }
        }
        let v = f(cwd);
        // Plafond : une entrée par (source, dossier) visité, sinon le cache grossit
        // indéfiniment au fil d'une longue session. Les entrées périmées partent d'abord.
        if self.cache.len() >= MAX_CACHE {
            self.cache.retain(|_, (t, _)| t.elapsed() < ttl);
            if self.cache.len() >= MAX_CACHE {
                self.cache.clear();
            }
        }
        self.cache.insert(key, (Instant::now(), v.clone()));
        v
    }

    /// Propositions pour la ligne `buffer` (curseur en fin de ligne).
    pub fn complete(&mut self, buffer: &str, cwd: Option<&Path>, max: usize) -> Vec<Completion> {
        let ends_space = buffer.ends_with(' ');
        let mut tokens: Vec<&str> = buffer.split_whitespace().collect();
        if tokens.is_empty() {
            return Vec::new();
        }
        if tokens.len() == 1 && !ends_space {
            // Le premier mot est d'ordinaire laissé à l'historique — sauf une applet
            // PowerShell (« Get-Ch… »), qu'on ne peut pas devenir d'un historique vide.
            return cmdlet_completions(tokens[0], max);
        }
        let current = if ends_space { "" } else { tokens.pop().unwrap_or("") };
        let base = &buffer[..buffer.len() - current.len()];
        // « sudo apt … » → on complète « apt … ».
        while tokens.first().is_some_and(|t| matches!(*t, "sudo" | "time" | "nohup" | "exec")) {
            tokens.remove(0);
        }
        let Some(&cmd) = tokens.first() else { return Vec::new() };
        // Nom du programme, séparateur Unix ou Windows, sans « .exe ».
        let cmd = cmd.rsplit(['/', '\\']).next().unwrap_or(cmd);
        let cmd = cmd.strip_suffix(".exe").unwrap_or(cmd);
        let args: Vec<&str> = tokens[1..].iter().copied().filter(|t| !t.starts_with('-')).collect();
        let cwd = cwd.map(Path::to_path_buf).unwrap_or_else(|| PathBuf::from("."));

        let mut cands: Vec<(String, String)> = Vec::new();
        let dynamic = |s: &mut Self| -> Vec<(String, String)> {
            match (cmd, args.first().copied()) {
                ("git", Some("checkout" | "switch" | "merge" | "rebase" | "branch" | "diff" | "log" | "push" | "pull" | "cherry-pick"))
                    if !current.starts_with('-') =>
                {
                    s.cached("git-branches", &cwd, Duration::from_secs(10), git_branches)
                }
                ("npm" | "pnpm" | "bun", Some("run")) | ("yarn", _) if !current.starts_with('-') => {
                    s.cached("npm-scripts", &cwd, Duration::from_secs(10), npm_scripts)
                }
                ("make", _) => s.cached("make", &cwd, Duration::from_secs(10), make_targets),
                ("ssh" | "scp" | "sftp" | "mosh", _) if args.is_empty() => {
                    s.cached("ssh", Path::new("/"), Duration::from_secs(60), |_| ssh_hosts())
                }
                ("python" | "python3" | "py", Some(m)) if m.ends_with("manage.py") && args.len() == 1 => {
                    DJANGO.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect()
                }
                ("manage.py" | "./manage.py", _) if args.is_empty() => {
                    DJANGO.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect()
                }
                _ => Vec::new(),
            }
        };
        if cmd == "make" || cmd == "yarn" || cmd == "ssh" || cmd == "scp" || cmd.ends_with("manage.py") {
            cands = dynamic(self);
        }
        if cands.is_empty() {
            let sub = match (cmd, args.first().copied()) {
                ("docker", Some("compose")) if args.len() == 1 => Some("compose"),
                (_, Some(s)) => Some(s),
                _ => None,
            };
            let docker_compose = cmd == "docker" && sub == Some("compose") && args.len() == 1;
            if current.starts_with('-') {
                if let Some((_, opts)) = spec(cmd, sub.or(Some(""))) {
                    cands = opts.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect();
                }
            } else if args.is_empty() || docker_compose {
                if let Some((subs, _)) = spec(cmd, if docker_compose { Some("compose") } else { None }) {
                    cands = subs.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect();
                }
            } else {
                cands = dynamic(self);
            }
        }
        cands
            .into_iter()
            .filter(|(c, _)| c.starts_with(current) && c != current)
            .take(max)
            .map(|(c, d)| {
                let sep = if c.ends_with('/') || c.ends_with('=') { "" } else { " " };
                Completion { full: format!("{base}{c}{sep}"), label: c, desc: d }
            })
            .collect()
    }
}

/// Applets PowerShell qui prolongent le mot tapé (« get-ch » → « Get-ChildItem »).
/// Uniquement sous Windows : ailleurs ces noms n'existent pas.
fn cmdlet_completions(word: &str, max: usize) -> Vec<Completion> {
    if !cfg!(windows) || word.len() < 2 || max == 0 {
        return Vec::new();
    }
    let lower = word.to_ascii_lowercase();
    PS_CMDLETS
        .iter()
        .filter(|(name, _)| {
            let n = name.to_ascii_lowercase();
            n.starts_with(&lower) && n != lower
        })
        .take(max)
        .map(|(name, desc)| Completion { full: format!("{name} "), label: name.to_string(), desc: desc.to_string() })
        .collect()
}

fn run_in(cwd: &Path, prog: &str, args: &[&str]) -> Option<String> {
    let out = crate::platform::hidden_command(prog).args(args).current_dir(cwd).output().ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

fn git_branches(cwd: &Path) -> Vec<(String, String)> {
    let Some(out) = run_in(cwd, "git", &["branch", "-a", "--format=%(refname:short)"]) else { return Vec::new() };
    let mut v: Vec<(String, String)> = Vec::new();
    for b in out.lines().map(str::trim).filter(|b| !b.is_empty() && !b.ends_with("/HEAD")) {
        let (name, desc) = match b.split_once('/') {
            Some((remote, rest)) if b.starts_with("origin/") || remote == "upstream" => (rest.to_string(), format!("branche distante ({remote})")),
            _ => (b.to_string(), "branche".to_string()),
        };
        if !v.iter().any(|(n, _)| *n == name) {
            v.push((name, desc));
        }
    }
    v
}

/// Scripts de package.json (lecture minimale, sans dépendance JSON).
pub fn npm_scripts(cwd: &Path) -> Vec<(String, String)> {
    let Ok(text) = std::fs::read_to_string(cwd.join("package.json")) else { return Vec::new() };
    let Some(start) = text.find("\"scripts\"") else { return Vec::new() };
    let rest = &text[start..];
    let Some(open) = rest.find('{') else { return Vec::new() };
    let mut depth = 0;
    let mut end = rest.len();
    for (i, ch) in rest[open..].char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    end = open + i;
                    break;
                }
            }
            _ => {}
        }
    }
    let body = &rest[open + 1..end];
    let mut out = Vec::new();
    let mut parts = body.split('"');
    // Alternance : "clé" : "valeur" , ...
    let _ = parts.next();
    while let (Some(k), Some(_), Some(v)) = (parts.next(), parts.next(), parts.next()) {
        out.push((k.to_string(), v.chars().take(60).collect()));
        let _ = parts.next();
    }
    out
}

pub fn make_targets(cwd: &Path) -> Vec<(String, String)> {
    let text = ["Makefile", "makefile", "GNUmakefile"].iter().find_map(|f| std::fs::read_to_string(cwd.join(f)).ok());
    let Some(text) = text else { return Vec::new() };
    let mut out: Vec<(String, String)> = Vec::new();
    for line in text.lines() {
        if line.starts_with(['\t', ' ', '.', '#']) {
            continue;
        }
        if let Some((target, rest)) = line.split_once(':') {
            if rest.starts_with('=') || target.contains(['$', '%', '=']) {
                continue;
            }
            for t in target.split_whitespace() {
                if !out.iter().any(|(n, _)| n == t) {
                    out.push((t.to_string(), "cible make".into()));
                }
            }
        }
    }
    out
}

fn ssh_hosts() -> Vec<(String, String)> {
    let Some(home) = dirs::home_dir() else { return Vec::new() };
    let Ok(text) = std::fs::read_to_string(home.join(".ssh/config")) else { return Vec::new() };
    let mut out = Vec::new();
    let mut current: Option<String> = None;
    for line in text.lines().map(str::trim) {
        let lower = line.to_ascii_lowercase();
        if let Some(h) = lower.strip_prefix("host ").map(|_| &line[5..]) {
            for name in h.split_whitespace().filter(|n| !n.contains(['*', '?'])) {
                out.push((name.to_string(), "hôte SSH".to_string()));
            }
            current = h.split_whitespace().next().map(String::from);
        } else if let (Some(hostname), Some(cur)) = (lower.strip_prefix("hostname ").map(|_| line[9..].trim()), &current) {
            if let Some(e) = out.iter_mut().rev().find(|(n, _)| n == cur) {
                e.1 = format!("hôte SSH → {hostname}");
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subcommands_and_options() {
        let mut c = Completer::default();
        let r = c.complete("git st", None, 10);
        assert!(r.iter().any(|x| x.full == "git status " && x.desc == "état du dépôt"));
        assert!(r.iter().any(|x| x.full == "git stash "));
        assert!(c.complete("git", None, 10).is_empty());
        let r = c.complete("sudo apt ins", None, 10);
        assert_eq!(r[0].full, "sudo apt install ");
        let r = c.complete("cargo build --rel", None, 10);
        assert_eq!(r[0].full, "cargo build --release ");
        let r = c.complete("docker compose u", None, 10);
        assert_eq!(r[0].full, "docker compose up ");
        let r = c.complete("python manage.py mig", None, 10);
        assert_eq!(r[0].full, "python manage.py migrate ");
        // Chemin Windows : le programme est reconnu malgré les antislashs et « .exe ».
        let r = c.complete("C:\\tools\\git\\cmd\\git.exe st", None, 10);
        assert!(r.iter().any(|x| x.label == "status"), "{r:?}");
        let r = c.complete("winget inst", None, 10);
        assert_eq!(r[0].full, "winget install ");
        let r = c.complete("dotnet bu", None, 10);
        assert_eq!(r[0].full, "dotnet build ");
    }

    #[test]
    fn cache_is_bounded() {
        let mut c = Completer::default();
        for i in 0..MAX_CACHE * 2 {
            let dir = PathBuf::from(format!("/inexistant/{i}"));
            let _ = c.complete("make ", Some(&dir), 5);
        }
        assert!(c.cache.len() <= MAX_CACHE, "cache non borné : {}", c.cache.len());
    }

    #[test]
    fn powershell_cmdlets() {
        let mut c = Completer::default();
        let r = c.complete("get-ch", None, 5);
        if cfg!(windows) {
            assert_eq!(r[0].full, "Get-ChildItem ");
        } else {
            assert!(r.is_empty(), "les applets ne concernent que Windows");
        }
    }

    #[test]
    fn npm_and_make_files() {
        let dir = std::env::temp_dir().join(format!("novaterm-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("package.json"), r#"{"name":"x","scripts":{"dev":"vite","build":"vite build"},"deps":{}}"#).unwrap();
        std::fs::write(dir.join("Makefile"), "CC=gcc\nall: build\nbuild test: x\n\t$(CC) a.c\n.PHONY: all\n").unwrap();
        assert_eq!(npm_scripts(&dir), vec![("dev".to_string(), "vite".to_string()), ("build".to_string(), "vite build".to_string())]);
        let t: Vec<String> = make_targets(&dir).into_iter().map(|x| x.0).collect();
        assert_eq!(t, vec!["all", "build", "test"]);
        let mut c = Completer::default();
        let r = c.complete("npm run b", Some(&dir), 5);
        assert_eq!(r[0].full, "npm run build ");
        let _ = std::fs::remove_dir_all(dir);
    }
}
