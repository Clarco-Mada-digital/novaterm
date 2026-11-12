//! Assistant IA optionnel (désactivé par défaut).
//!
//! Principes : rien n'est envoyé sans action de l'utilisateur, les secrets sont
//! masqués avant l'envoi, le contexte capturé (sortie, diff) est montré avant
//! d'être envoyé, et aucune commande n'est jamais exécutée automatiquement :
//! elles sont seulement proposées (insérer / copier).
//!
//! Le réseau passe par `curl` (comme les mises à jour) : la clé et le corps de la
//! requête sont transmis par l'entrée standard, jamais dans la ligne de commande.

use std::io::{BufRead, BufReader, Read, Write};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::i18n::t;

/// Service utilisé pour l'assistant.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Provider {
    /// Modèles locaux (https://ollama.com) : rien ne quitte la machine.
    Ollama,
    OpenAi,
    Anthropic,
    Gemini,
    Mistral,
    Groq,
    OpenRouter,
    /// Tout serveur compatible OpenAI (LM Studio, llama.cpp, vLLM, LocalAI…).
    Custom,
}

pub const PROVIDERS: [Provider; 8] = [
    Provider::Ollama,
    Provider::OpenAi,
    Provider::Anthropic,
    Provider::Gemini,
    Provider::Mistral,
    Provider::Groq,
    Provider::OpenRouter,
    Provider::Custom,
];

impl Provider {
    pub fn label(self) -> &'static str {
        match self {
            Provider::Ollama => "Ollama (local)",
            Provider::OpenAi => "OpenAI",
            Provider::Anthropic => "Anthropic (Claude)",
            Provider::Gemini => "Google Gemini",
            Provider::Mistral => "Mistral AI",
            Provider::Groq => "Groq",
            Provider::OpenRouter => "OpenRouter",
            Provider::Custom => t("Autre serveur compatible OpenAI"),
        }
    }

    /// Identifiant stable (fichier des clés).
    pub fn id(self) -> &'static str {
        match self {
            Provider::Ollama => "ollama",
            Provider::OpenAi => "openai",
            Provider::Anthropic => "anthropic",
            Provider::Gemini => "gemini",
            Provider::Mistral => "mistral",
            Provider::Groq => "groq",
            Provider::OpenRouter => "openrouter",
            Provider::Custom => "custom",
        }
    }

    pub fn default_base(self) -> &'static str {
        match self {
            Provider::Ollama => "http://localhost:11434/v1",
            Provider::OpenAi => "https://api.openai.com/v1",
            Provider::Anthropic => "https://api.anthropic.com/v1",
            Provider::Gemini => "https://generativelanguage.googleapis.com/v1beta/openai",
            Provider::Mistral => "https://api.mistral.ai/v1",
            Provider::Groq => "https://api.groq.com/openai/v1",
            Provider::OpenRouter => "https://openrouter.ai/api/v1",
            Provider::Custom => "http://localhost:1234/v1",
        }
    }

    /// Modèle proposé par défaut (modifiable ; « Lister les modèles » donne les autres).
    pub fn default_model(self) -> &'static str {
        match self {
            Provider::Ollama => "llama3.2",
            Provider::OpenAi => "gpt-4o-mini",
            Provider::Anthropic => "claude-haiku-5-5",
            Provider::Gemini => "gemini-2.5-flash",
            Provider::Mistral => "mistral-small-latest",
            Provider::Groq => "llama-3.3-70b-versatile",
            Provider::OpenRouter => "openrouter/auto",
            Provider::Custom => "",
        }
    }

    /// Variable d'environnement lue si aucune clé n'est enregistrée.
    pub fn env_var(self) -> &'static str {
        match self {
            Provider::Ollama => "OLLAMA_API_KEY",
            Provider::OpenAi => "OPENAI_API_KEY",
            Provider::Anthropic => "ANTHROPIC_API_KEY",
            Provider::Gemini => "GEMINI_API_KEY",
            Provider::Mistral => "MISTRAL_API_KEY",
            Provider::Groq => "GROQ_API_KEY",
            Provider::OpenRouter => "OPENROUTER_API_KEY",
            Provider::Custom => "OPENAI_API_KEY",
        }
    }

    pub fn needs_key(self) -> bool {
        !matches!(self, Provider::Ollama | Provider::Custom)
    }

    /// Page où obtenir une clé.
    pub fn key_url(self) -> Option<&'static str> {
        match self {
            Provider::OpenAi => Some("https://platform.openai.com/api-keys"),
            Provider::Anthropic => Some("https://console.anthropic.com/settings/keys"),
            Provider::Gemini => Some("https://aistudio.google.com/apikey"),
            Provider::Mistral => Some("https://console.mistral.ai/api-keys"),
            Provider::Groq => Some("https://console.groq.com/keys"),
            Provider::OpenRouter => Some("https://openrouter.ai/keys"),
            Provider::Ollama => Some("https://ollama.com/download"),
            Provider::Custom => None,
        }
    }
}

/// L'adresse désigne-t-elle cette machine (rien ne part sur Internet) ?
pub fn is_local(base: &str) -> bool {
    let rest = base.split_once("://").map(|(_, r)| r).unwrap_or(base);
    let host = rest.split(['/', '?']).next().unwrap_or("");
    let host = host.rsplit_once('@').map(|(_, h)| h).unwrap_or(host);
    let host = if host.starts_with('[') { host.split(']').next().unwrap_or("").trim_start_matches('[') } else { host.split(':').next().unwrap_or("") };
    matches!(host, "localhost" | "127.0.0.1" | "::1" | "0.0.0.0") || host.starts_with("127.")
}

// ---------------------------------------------------------------- clés

/// Fichier des clés (séparé de config.toml, lisible par vous seul).
pub fn keys_path() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("novaterm").join("ai-keys.toml"))
}

fn read_keys() -> toml::Table {
    keys_path().and_then(|p| std::fs::read_to_string(p).ok()).and_then(|s| s.parse::<toml::Table>().ok()).unwrap_or_default()
}

/// Où la clé a été trouvée.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum KeySource {
    File,
    Env(&'static str),
    Missing,
}

pub fn key_for(p: Provider) -> (Option<String>, KeySource) {
    if let Some(k) = read_keys().get(p.id()).and_then(|v| v.as_str()).filter(|k| !k.trim().is_empty()) {
        return (Some(k.trim().to_string()), KeySource::File);
    }
    match std::env::var(p.env_var()) {
        Ok(k) if !k.trim().is_empty() => (Some(k.trim().to_string()), KeySource::Env(p.env_var())),
        _ => (None, KeySource::Missing),
    }
}

/// Enregistre (ou efface si vide) la clé d'un service. Fichier en mode 600 sous Unix.
pub fn save_key(p: Provider, key: &str) -> Result<(), String> {
    let path = keys_path().ok_or("dossier de configuration introuvable")?;
    let mut table = read_keys();
    let key = key.trim();
    if key.is_empty() {
        table.remove(p.id());
    } else {
        table.insert(p.id().to_string(), toml::Value::String(key.to_string()));
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let text = format!("# Clés d'API de l'assistant NovaTerm. Ne partagez pas ce fichier.\n{}", toml::to_string(&table).unwrap_or_default());
    let tmp = path.with_extension("toml.tmp");
    {
        let mut opts = std::fs::OpenOptions::new();
        opts.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            opts.mode(0o600);
        }
        let mut f = opts.open(&tmp).map_err(|e| e.to_string())?;
        f.write_all(text.as_bytes()).map_err(|e| e.to_string())?;
    }
    std::fs::rename(&tmp, &path).map_err(|e| e.to_string())
}

// ---------------------------------------------------------------- requêtes

/// Service, adresse, modèle et clé résolus.
#[derive(Clone, Debug)]
pub struct Endpoint {
    pub provider: Provider,
    pub base: String,
    pub model: String,
    pub key: Option<String>,
}

impl Endpoint {
    pub fn new(provider: Provider, base: &str, model: &str) -> Self {
        let base = if base.trim().is_empty() { provider.default_base() } else { base.trim() };
        let model = if model.trim().is_empty() { provider.default_model() } else { model.trim() };
        Self { provider, base: base.trim_end_matches('/').to_string(), model: model.to_string(), key: key_for(provider).0 }
    }

    pub fn is_local(&self) -> bool {
        is_local(&self.base)
    }

    fn anthropic(&self) -> bool {
        self.provider == Provider::Anthropic
    }

    fn headers(&self) -> Vec<String> {
        let mut h = vec!["Content-Type: application/json".to_string(), "User-Agent: NovaTerm".to_string()];
        if let Some(k) = &self.key {
            if self.anthropic() {
                h.push(format!("x-api-key: {k}"));
            } else {
                h.push(format!("Authorization: Bearer {k}"));
            }
        }
        if self.anthropic() {
            h.push("anthropic-version: 2023-06-01".into());
        }
        if self.provider == Provider::OpenRouter {
            h.push("X-Title: NovaTerm".into());
        }
        h
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    User,
    Assistant,
}

#[derive(Clone, Debug)]
pub struct Message {
    pub role: Role,
    pub content: String,
}

fn request_body(ep: &Endpoint, system: &str, msgs: &[Message]) -> Value {
    let role = |r: Role| if r == Role::User { "user" } else { "assistant" };
    if ep.anthropic() {
        let messages: Vec<Value> = msgs.iter().map(|m| json!({"role": role(m.role), "content": m.content})).collect();
        json!({"model": ep.model, "max_tokens": 2048, "stream": true, "system": system, "messages": messages})
    } else {
        let mut messages = vec![json!({"role": "system", "content": system})];
        messages.extend(msgs.iter().map(|m| json!({"role": role(m.role), "content": m.content})));
        json!({"model": ep.model, "stream": true, "messages": messages})
    }
}

/// Chaîne entre guillemets pour un fichier de configuration curl (`-K`).
fn curl_quote(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Configuration curl transmise par l'entrée standard (la clé n'apparaît pas dans `ps`).
fn curl_config(ep: &Endpoint, url: &str, body: Option<&Value>) -> String {
    let mut cfg = format!("url = {}\n", curl_quote(url));
    for h in ep.headers() {
        cfg.push_str(&format!("header = {}\n", curl_quote(&h)));
    }
    if let Some(b) = body {
        cfg.push_str(&format!("data-binary = {}\n", curl_quote(&b.to_string())));
    }
    cfg
}

const STATUS_MARK: &str = "__NOVATERM_HTTP__";

fn spawn_curl(config: &str, stream: bool, max_time: u32) -> Result<Child, String> {
    let mut cmd = Command::new(if cfg!(windows) { "curl.exe" } else { "curl" });
    cmd.args(["-sS", "--max-time", &max_time.to_string(), "--connect-timeout", "10", "-w", &format!("\n{STATUS_MARK}%{{http_code}}\n"), "-K", "-"]);
    if stream {
        cmd.arg("-N");
    }
    cmd.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    let mut child = cmd.spawn().map_err(|_| t("curl est introuvable : installez-le pour utiliser l'assistant").to_string())?;
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(config.as_bytes());
    }
    Ok(child)
}

/// Message d'erreur lisible à partir d'une réponse HTTP en échec.
fn http_error(ep: &Endpoint, status: u32, body: &str, stderr: &str) -> String {
    if status == 0 {
        let detail = stderr.trim().trim_start_matches("curl: ").to_string();
        if ep.provider == Provider::Ollama {
            return format!("{} ({detail})", t("Ollama ne répond pas : lancez « ollama serve » (ou l'application Ollama)"));
        }
        return format!("{} ({detail})", t("Connexion impossible"));
    }
    let v: Option<Value> = serde_json::from_str(body.trim()).ok();
    let msg = v
        .as_ref()
        .and_then(|v| {
            let e = v.get("error").unwrap_or(v);
            e.get("message").and_then(|m| m.as_str()).map(String::from).or_else(|| e.as_str().map(String::from))
        })
        .or_else(|| v.as_ref().and_then(|v| v.as_array()).and_then(|a| a.first()).and_then(|e| e.pointer("/error/message")).and_then(|m| m.as_str()).map(String::from))
        .unwrap_or_else(|| body.trim().chars().take(300).collect());
    let hint = match status {
        401 | 403 => t(" — clé d'API refusée (Paramètres → Assistant IA)"),
        404 => t(" — modèle ou adresse introuvable"),
        429 => t(" — trop de requêtes ou quota épuisé"),
        _ => "",
    };
    format!("HTTP {status}{hint} : {msg}")
}

/// Morceau de texte d'un événement SSE (OpenAI ou Anthropic), ou erreur.
fn sse_text(anthropic: bool, data: &str) -> Result<Option<String>, String> {
    let Ok(v) = serde_json::from_str::<Value>(data) else { return Ok(None) };
    if let Some(e) = v.get("error") {
        let msg = e.get("message").and_then(|m| m.as_str()).map(String::from).unwrap_or_else(|| e.to_string());
        return Err(msg);
    }
    let text = if anthropic {
        (v.get("type").and_then(|t| t.as_str()) == Some("content_block_delta")).then(|| v.pointer("/delta/text")).flatten()
    } else {
        v.pointer("/choices/0/delta/content")
    };
    Ok(text.and_then(|t| t.as_str()).filter(|t| !t.is_empty()).map(String::from))
}

pub enum Event {
    Chunk(String),
    Done,
    Error(String),
}

/// Requête en cours (annulable).
pub struct Job {
    pub rx: Receiver<Event>,
    child: Arc<Mutex<Option<Child>>>,
    cancelled: Arc<AtomicBool>,
}

impl Job {
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Relaxed);
        if let Some(c) = self.child.lock().unwrap_or_else(|e| e.into_inner()).as_mut() {
            let _ = c.kill();
        }
    }
}

impl Drop for Job {
    fn drop(&mut self) {
        self.cancel();
    }
}

/// Lance la requête en arrière-plan ; la réponse arrive morceau par morceau.
pub fn start(ep: Endpoint, system: String, msgs: Vec<Message>, repaint: impl Fn() + Send + 'static) -> Job {
    let (tx, rx) = mpsc::channel();
    let child_slot: Arc<Mutex<Option<Child>>> = Arc::new(Mutex::new(None));
    let cancelled = Arc::new(AtomicBool::new(false));
    let job = Job { rx, child: child_slot.clone(), cancelled: cancelled.clone() };
    std::thread::spawn(move || {
        let send = |tx: &Sender<Event>, e: Event| {
            let _ = tx.send(e);
            repaint();
        };
        if ep.provider.needs_key() && ep.key.is_none() {
            send(&tx, Event::Error(crate::i18n::tf("Aucune clé d'API pour {} : ajoutez-la dans Paramètres → Assistant IA (ou variable {}).", &[&ep.provider.label(), &ep.provider.env_var()])));
            return;
        }
        let url = format!("{}/{}", ep.base, if ep.anthropic() { "messages" } else { "chat/completions" });
        let body = request_body(&ep, &system, &msgs);
        let mut child = match spawn_curl(&curl_config(&ep, &url, Some(&body)), true, 300) {
            Ok(c) => c,
            Err(e) => return send(&tx, Event::Error(e)),
        };
        let stdout = child.stdout.take();
        let mut stderr = child.stderr.take();
        *child_slot.lock().unwrap_or_else(|e| e.into_inner()) = Some(child);
        let mut raw = String::new();
        let mut status: u32 = 0;
        let mut got_text = false;
        if let Some(out) = stdout {
            for line in BufReader::new(out).lines() {
                if cancelled.load(Ordering::Relaxed) {
                    break;
                }
                let Ok(line) = line else { break };
                if let Some(code) = line.strip_prefix(STATUS_MARK) {
                    status = code.trim().parse().unwrap_or(0);
                    continue;
                }
                let l = line.trim_end_matches('\r');
                if let Some(data) = l.strip_prefix("data:") {
                    let data = data.trim();
                    if data == "[DONE]" {
                        continue;
                    }
                    match sse_text(ep.anthropic(), data) {
                        Ok(Some(text)) => {
                            got_text = true;
                            send(&tx, Event::Chunk(text));
                        }
                        Ok(None) => {}
                        Err(e) => {
                            send(&tx, Event::Error(e));
                            return;
                        }
                    }
                } else if !l.starts_with("event:") && !l.starts_with(':') && raw.len() < 8000 {
                    raw.push_str(l);
                    raw.push('\n');
                }
            }
        }
        let child = child_slot.lock().unwrap_or_else(|e| e.into_inner()).take();
        if let Some(mut c) = child {
            let _ = c.wait();
        }
        if cancelled.load(Ordering::Relaxed) {
            return;
        }
        let mut err = String::new();
        if let Some(e) = stderr.as_mut() {
            let _ = e.read_to_string(&mut err);
        }
        if (200..300).contains(&status) {
            if !got_text && !raw.trim().is_empty() {
                // Réponse non « stream » (certains serveurs ignorent stream=true).
                if let Ok(v) = serde_json::from_str::<Value>(raw.trim()) {
                    let text = v.pointer("/choices/0/message/content").or_else(|| v.pointer("/content/0/text")).and_then(|t| t.as_str());
                    if let Some(text) = text {
                        send(&tx, Event::Chunk(text.to_string()));
                    }
                }
            }
            send(&tx, Event::Done);
        } else {
            send(&tx, Event::Error(http_error(&ep, status, &raw, &err)));
        }
    });
    job
}

/// Liste des modèles proposés par le service (bloquant : à appeler hors interface).
pub fn list_models(ep: &Endpoint) -> Result<Vec<String>, String> {
    let child = spawn_curl(&curl_config(ep, &format!("{}/models", ep.base), None), false, 20)?;
    let out = child.wait_with_output().map_err(|e| e.to_string())?;
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    let (body, code) = text.rsplit_once(STATUS_MARK).unwrap_or((&text, "0"));
    let status: u32 = code.trim().parse().unwrap_or(0);
    if !(200..300).contains(&status) {
        return Err(http_error(ep, status, body, &String::from_utf8_lossy(&out.stderr)));
    }
    let v: Value = serde_json::from_str(body.trim()).map_err(|e| e.to_string())?;
    let list = v.get("data").or_else(|| v.get("models")).and_then(|d| d.as_array()).cloned().unwrap_or_default();
    let mut names: Vec<String> = list
        .iter()
        .filter_map(|m| m.get("id").or_else(|| m.get("name")).and_then(|i| i.as_str()))
        .map(|s| s.trim_start_matches("models/").to_string())
        .collect();
    names.sort();
    names.dedup();
    Ok(names)
}

// ---------------------------------------------------------------- secrets

const SECRET_NAMES: &[&str] = &[
    "password",
    "passwd",
    "passphrase",
    "secret",
    "token",
    "apikey",
    "api_key",
    "api-key",
    "access_key",
    "accesskey",
    "private_key",
    "authorization",
    "credential",
];

const SECRET_PREFIXES: &[&str] = &[
    "sk-", "sk_live_", "sk_test_", "rk_live_", "pk_live_", "ghp_", "gho_", "ghu_", "ghs_", "ghr_", "github_pat_", "glpat-", "xoxb-", "xoxp-", "xoxa-", "xapp-", "hf_", "npm_", "pypi-", "AKIA", "ASIA", "AIza",
    "ya29.", "shpat_", "dop_v1_",
];

const MASK: &str = "[masqué]";

fn sensitive_name(name: &str) -> bool {
    let n = name.trim_matches(|c: char| c == '"' || c == '\'' || c == '-' || c.is_whitespace()).to_ascii_lowercase();
    !n.is_empty() && SECRET_NAMES.iter().any(|s| n.contains(s))
}

fn token_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | '+' | '/')
}

/// Jeton isolé qui ressemble à une clé connue (préfixe) ou à un JWT.
fn looks_like_secret(word: &str) -> bool {
    let w = word.trim_end_matches(['.', ',']);
    if w.len() >= 16 && SECRET_PREFIXES.iter().any(|p| w.starts_with(p)) && w.chars().filter(|c| c.is_ascii_digit()).count() >= 2 {
        return true;
    }
    // JWT : trois parties base64url, la première commence par « eyJ ».
    w.starts_with("eyJ") && w.len() > 30 && w.matches('.').count() == 2
}

/// Masque les secrets probables d'un texte. Retourne le texte et le nombre de masques.
pub fn mask_secrets(text: &str) -> (String, usize) {
    let mut count = 0;
    let mut out = String::with_capacity(text.len());
    let mut in_key = false;
    for line in text.split_inclusive('\n') {
        // Clés privées PEM : tout le bloc.
        if line.contains("-----BEGIN") && line.contains("PRIVATE KEY") {
            in_key = true;
            count += 1;
            out.push_str(t("[clé privée masquée]"));
            out.push('\n');
            if line.contains("-----END") {
                in_key = false;
            }
            continue;
        }
        if in_key {
            if line.contains("-----END") {
                in_key = false;
            }
            continue;
        }
        out.push_str(&mask_line(line, &mut count));
    }
    (out, count)
}

fn mask_line(line: &str, count: &mut usize) -> String {
    let chars: Vec<char> = line.chars().collect();
    let mut out = String::with_capacity(line.len());
    let mut i = 0;
    // Valeur après « nom=… », « nom: … », « --nom … » ou « user:pass@ ».
    let mut mask_next_word = false;
    while i < chars.len() {
        let c = chars[i];
        if token_char(c) || c == '"' || c == '\'' {
            // Mot entier.
            let start = i;
            while i < chars.len() && token_char(chars[i]) {
                i += 1;
            }
            // URL entière : identifiants et paramètres sensibles masqués.
            if i > start && chars[i..].starts_with(&[':', '/', '/']) {
                while i < chars.len() && !chars[i].is_whitespace() && !matches!(chars[i], '"' | '\'' | '<' | '>') {
                    i += 1;
                }
                let url: String = chars[start..i].iter().collect();
                out.push_str(&mask_url(&url, count));
                continue;
            }
            if i == start {
                // Guillemet seul.
                out.push(c);
                i += 1;
                continue;
            }
            let word: String = chars[start..i].iter().collect();
            if mask_next_word {
                mask_next_word = false;
                if matches!(word.to_ascii_lowercase().as_str(), "bearer" | "basic" | "token") {
                    out.push_str(&word);
                    mask_next_word = true;
                } else {
                    *count += 1;
                    out.push_str(MASK);
                }
                continue;
            }
            if looks_like_secret(&word) {
                *count += 1;
                out.push_str(MASK);
                continue;
            }
            // Option « --password valeur ».
            if word.starts_with("--") && !word.contains('=') && sensitive_name(&word) && chars.get(i) == Some(&' ') {
                out.push_str(&word);
                mask_next_word = true;
                continue;
            }
            // nom=valeur / nom: valeur / "nom": "valeur"
            let mut j = i;
            if chars.get(j) == Some(&'"') || chars.get(j) == Some(&'\'') {
                j += 1;
            }
            if let Some(&sep) = chars.get(j).filter(|c| **c == '=' || **c == ':') {
                let name = word.trim_start_matches(['"', '\'']);
                if sensitive_name(name) && !(sep == ':' && chars.get(j + 1) == Some(&':')) {
                    out.extend(&chars[start..=j]);
                    i = j + 1;
                    // Espaces, puis valeur (entre guillemets ou jusqu'au prochain blanc).
                    while i < chars.len() && chars[i] == ' ' {
                        out.push(' ');
                        i += 1;
                    }
                    if let Some(&q) = chars.get(i).filter(|c| **c == '"' || **c == '\'') {
                        let end = chars[i + 1..].iter().position(|c| *c == q).map(|p| i + 1 + p);
                        if let Some(end) = end.filter(|e| *e > i + 1) {
                            *count += 1;
                            out.push(q);
                            out.push_str(MASK);
                            out.push(q);
                            i = end + 1;
                        }
                        continue;
                    }
                    let vstart = i;
                    while i < chars.len() && !chars[i].is_whitespace() && chars[i] != ',' && chars[i] != ';' {
                        i += 1;
                    }
                    let value: String = chars[vstart..i].iter().collect();
                    if value.is_empty() {
                        continue;
                    }
                    if matches!(value.to_ascii_lowercase().as_str(), "bearer" | "basic" | "token") {
                        out.push_str(&value);
                        mask_next_word = true;
                    } else {
                        *count += 1;
                        out.push_str(MASK);
                    }
                    continue;
                }
            }
            out.push_str(&word);
            continue;
        }
        out.push(c);
        i += 1;
    }
    out
}

/// scheme://user:pass@hôte/…?token=… → mot de passe et paramètres sensibles masqués.
fn mask_url(url: &str, count: &mut usize) -> String {
    let Some(p) = url.find("://") else { return url.to_string() };
    let (scheme, rest) = url.split_at(p + 3);
    let host_end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let (authority, tail) = rest.split_at(host_end);
    let mut out = scheme.to_string();
    match authority.rsplit_once('@') {
        Some((userinfo, host)) if userinfo.contains(':') => {
            *count += 1;
            out.push_str(&format!("{}:{MASK}@{host}", userinfo.split(':').next().unwrap_or("")));
        }
        _ => out.push_str(authority),
    }
    match tail.split_once('?') {
        Some((path, query)) => {
            out.push_str(path);
            out.push('?');
            let parts: Vec<String> = query
                .split('&')
                .map(|kv| match kv.split_once('=') {
                    Some((k, v)) if sensitive_name(k) && !v.is_empty() => {
                        *count += 1;
                        format!("{k}={MASK}")
                    }
                    _ => kv.to_string(),
                })
                .collect();
            out.push_str(&parts.join("&"));
        }
        None => out.push_str(tail),
    }
    out
}

// ---------------------------------------------------------------- invites

/// Ce que l'utilisateur demande.
pub enum Task {
    /// Question libre ou demande de commande en langage naturel.
    Ask { question: String },
    /// Commande en échec : pourquoi, et comment corriger.
    ExplainError { command: String, output: String, exit: i32 },
    /// Résumer la sortie d'une commande.
    Summarize { command: String, output: String },
    /// Expliquer une commande.
    ExplainCommand { command: String },
    /// Message de commit à partir des modifications indexées.
    CommitMessage { diff: String },
}

impl Task {
    pub fn title(&self) -> &'static str {
        match self {
            Task::Ask { .. } => t("Question"),
            Task::ExplainError { .. } => t("Expliquer l'erreur"),
            Task::Summarize { .. } => t("Résumer la sortie"),
            Task::ExplainCommand { .. } => t("Expliquer la commande"),
            Task::CommitMessage { .. } => t("Message de commit"),
        }
    }

    /// Libellé court affiché dans l'historique de la conversation.
    pub fn label(&self) -> String {
        let one = |s: &str| s.lines().next().unwrap_or("").chars().take(70).collect::<String>();
        match self {
            Task::Ask { question } => question.clone(),
            Task::ExplainError { command, .. } | Task::Summarize { command, .. } if !command.is_empty() => format!("{} : {}", self.title(), one(command)),
            Task::ExplainCommand { command } => format!("{} : {}", self.title(), one(command)),
            _ => self.title().to_string(),
        }
    }

    /// Le message contient-il du contenu capturé (à montrer avant envoi) ?
    pub fn has_captured(&self) -> bool {
        !matches!(self, Task::Ask { .. })
    }
}

/// Environnement de la commande (pour des réponses adaptées).
pub struct Context {
    pub shell: String,
    pub cwd: Option<String>,
}

/// Garde les dernières lignes d'une sortie, raccourcies.
pub fn tail(text: &str, max_lines: usize, max_chars: usize) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let skip = lines.len().saturating_sub(max_lines.max(1));
    let mut out = String::new();
    if skip > 0 {
        out.push_str(&format!("[… {skip} lignes plus haut omises …]\n"));
    }
    for l in &lines[skip..] {
        if l.chars().count() > 400 {
            out.extend(l.chars().take(400));
            out.push_str(" […]\n");
        } else {
            out.push_str(l);
            out.push('\n');
        }
    }
    if out.len() > max_chars {
        let cut = out.len() - max_chars;
        let cut = (cut..out.len()).find(|i| out.is_char_boundary(*i)).unwrap_or(out.len());
        out = format!("[…]\n{}", &out[cut..]);
    }
    out
}

pub fn system_prompt(ctx: &Context, english: bool) -> String {
    let os = match std::env::consts::OS {
        "linux" => "Linux",
        "macos" => "macOS",
        "windows" => "Windows",
        o => o,
    };
    let lang = if english { "English" } else { "French" };
    let mut s = format!(
        "You are the assistant built into NovaTerm, a terminal emulator. The user runs {os} with the {} shell.",
        if ctx.shell.is_empty() { "default" } else { &ctx.shell }
    );
    if let Some(cwd) = &ctx.cwd {
        s.push_str(&format!(" Current directory: {cwd}."));
    }
    s.push_str(&format!(
        " Always answer in {lang}. Be concise and practical. Put every shell command in its own fenced code block (```sh), one command per block, \
ready to paste. You cannot run anything yourself: never claim you did. Warn clearly before any destructive command (rm, dd, mkfs, chmod -R, force push…). \
Text shown as [masqué] was hidden for privacy: never ask for it."
    ));
    s
}

pub fn user_message(task: &Task) -> String {
    match task {
        Task::Ask { question } => question.clone(),
        Task::ExplainError { command, output, exit } => format!(
            "This command failed (exit code {exit}).\n\nCommand:\n```\n{command}\n```\n\nOutput:\n```\n{output}```\n\n\
Explain briefly why it failed, then give the corrected command or the steps to fix it."
        ),
        Task::Summarize { command, output } => format!(
            "Summarize the output of this command: what matters, any warnings or errors, and what to do next if relevant.\n\nCommand:\n```\n{command}\n```\n\nOutput:\n```\n{output}```"
        ),
        Task::ExplainCommand { command } => format!(
            "Explain what this command does, part by part (options, pipes, redirections), and point out any risk.\n\n```\n{command}\n```"
        ),
        Task::CommitMessage { diff } => format!(
            "Write a git commit message for these staged changes: a short summary line (max 72 characters, imperative mood), a blank line, \
then a few bullet points if useful. Answer with the message only, inside a single ```text code block, then the git commit command to use it.\n\n```diff\n{diff}```"
        ),
    }
}

// ---------------------------------------------------------------- réponse

/// Partie d'une réponse : texte, ou bloc de code (commande à insérer).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Segment {
    Text(String),
    Code { lang: String, code: String },
}

/// Découpe une réponse (éventuellement incomplète) en texte et blocs de code.
pub fn segments(text: &str) -> Vec<Segment> {
    let mut out = Vec::new();
    let mut buf = String::new();
    let mut code: Option<(String, String)> = None;
    for line in text.split_inclusive('\n') {
        let trimmed = line.trim();
        match &mut code {
            None if trimmed.starts_with("```") => {
                if !buf.trim().is_empty() {
                    out.push(Segment::Text(buf.trim().to_string()));
                }
                buf.clear();
                code = Some((trimmed.trim_start_matches('`').trim().to_string(), String::new()));
            }
            None => buf.push_str(line),
            Some(_) if trimmed == "```" => {
                if let Some((lang, c)) = code.take() {
                    out.push(Segment::Code { lang, code: c.trim_end_matches('\n').to_string() });
                }
            }
            Some((_, c)) => c.push_str(line),
        }
    }
    if let Some((lang, c)) = code {
        out.push(Segment::Code { lang, code: c.trim_end_matches('\n').to_string() });
    } else if !buf.trim().is_empty() {
        out.push(Segment::Text(buf.trim().to_string()));
    }
    out
}

/// Un bloc de code est-il une commande à insérer (et pas un fichier, un diff…) ?
pub fn is_command(lang: &str) -> bool {
    matches!(lang.to_ascii_lowercase().as_str(), "" | "sh" | "bash" | "zsh" | "shell" | "console" | "fish" | "powershell" | "ps1" | "pwsh" | "cmd" | "bat")
}

/// Modifications indexées du dépôt (pour un message de commit).
pub fn staged_diff(cwd: &std::path::Path, max_chars: usize) -> Result<String, String> {
    let run = |args: &[&str]| -> Result<String, String> {
        let mut cmd = Command::new("git");
        cmd.args(args).current_dir(cwd);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x0800_0000);
        }
        let out = cmd.output().map_err(|_| t("git est introuvable").to_string())?;
        if !out.status.success() {
            return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
        }
        Ok(String::from_utf8_lossy(&out.stdout).to_string())
    };
    let stat = run(&["diff", "--staged", "--stat"])?;
    if stat.trim().is_empty() {
        return Err(t("Rien n'est indexé : faites d'abord « git add … »").to_string());
    }
    let diff = run(&["diff", "--staged", "--no-color", "--unified=2"])?;
    let mut text = format!("{stat}\n{diff}");
    if text.len() > max_chars {
        let mut cut = max_chars;
        while !text.is_char_boundary(cut) {
            cut -= 1;
        }
        text.truncate(cut);
        text.push_str("\n[… diff tronqué …]\n");
    }
    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn masks_common_secrets() {
        let (m, n) = mask_secrets("export GITHUB_TOKEN=ghp_abcdefghijklmnop1234567890\n");
        assert_eq!(m, "export GITHUB_TOKEN=[masqué]\n");
        assert_eq!(n, 1);
        let (m, _) = mask_secrets("curl -H 'Authorization: Bearer abc.def.ghi' https://x\n");
        assert!(m.contains("Bearer [masqué]") && !m.contains("abc.def"), "{m}");
        let (m, _) = mask_secrets("{\"password\": \"hunter2\", \"user\": \"bob\"}");
        assert_eq!(m, "{\"password\": \"[masqué]\", \"user\": \"bob\"}");
        let (m, _) = mask_secrets("key sk-proj-AbCdEf1234567890XyZ used\n");
        assert_eq!(m, "key [masqué] used\n");
        let (m, _) = mask_secrets("git clone https://bob:s3cret@github.com/a/b\n");
        assert_eq!(m, "git clone https://bob:[masqué]@github.com/a/b\n");
        let (m, _) = mask_secrets("GET https://api.x.io/v1?q=1&access_token=abc123 200\n");
        assert_eq!(m, "GET https://api.x.io/v1?q=1&access_token=[masqué] 200\n");
        let (m, _) = mask_secrets("mysql --password hunter2 -u root\n");
        assert_eq!(m, "mysql --password [masqué] -u root\n");
        let (m, n) = mask_secrets("a\n-----BEGIN OPENSSH PRIVATE KEY-----\nAAAA\nBBBB\n-----END OPENSSH PRIVATE KEY-----\nb\n");
        assert_eq!(m, "a\n[clé privée masquée]\nb\n");
        assert_eq!(n, 1);
        let (m, _) = mask_secrets("AWS_SECRET_ACCESS_KEY=wJalrXUtnFEMI/K7MDENG\n");
        assert_eq!(m, "AWS_SECRET_ACCESS_KEY=[masqué]\n");
    }

    #[test]
    fn keeps_ordinary_text() {
        for s in ["ls -la /tmp\n", "error: cannot find module 'express'\n", "std::io::Error at src/main.rs:12:5\n", "https://example.com/path?a=1\n", "mkdir -p a/b\n"] {
            assert_eq!(mask_secrets(s), (s.to_string(), 0), "{s}");
        }
    }

    #[test]
    fn locality() {
        assert!(is_local("http://localhost:11434/v1"));
        assert!(is_local("http://127.0.0.1:8080"));
        assert!(is_local("http://[::1]:1234/v1"));
        assert!(!is_local("https://api.openai.com/v1"));
        assert!(!is_local("https://localhost.evil.com/v1"));
    }

    #[test]
    fn parses_segments() {
        let s = segments("Try this:\n```sh\nls -la\n```\nThen:\n```bash\necho hi\n");
        assert_eq!(
            s,
            vec![
                Segment::Text("Try this:".into()),
                Segment::Code { lang: "sh".into(), code: "ls -la".into() },
                Segment::Text("Then:".into()),
                Segment::Code { lang: "bash".into(), code: "echo hi".into() },
            ]
        );
    }

    #[test]
    fn sse_parsing() {
        assert_eq!(sse_text(false, r#"{"choices":[{"delta":{"content":"Hi"}}]}"#), Ok(Some("Hi".into())));
        assert_eq!(sse_text(true, r#"{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Yo"}}"#), Ok(Some("Yo".into())));
        assert_eq!(sse_text(true, r#"{"type":"message_start"}"#), Ok(None));
        assert!(sse_text(false, r#"{"error":{"message":"bad key"}}"#).is_err());
    }

    #[test]
    fn curl_config_escapes() {
        assert_eq!(curl_quote("a\"b\\c\nd"), "\"a\\\"b\\\\c\\nd\"");
    }

    #[test]
    fn staged_diff_reads_index() {
        let dir = std::env::temp_dir().join(format!("novaterm-ai-git-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let git = |args: &[&str]| Command::new("git").args(args).current_dir(&dir).output().unwrap();
        if !git(&["init", "-q"]).status.success() {
            return; // git absent : rien à tester
        }
        assert!(staged_diff(&dir, 10_000).is_err(), "rien d'indexé");
        std::fs::write(dir.join("a.txt"), "bonjour\n").unwrap();
        git(&["add", "a.txt"]);
        let d = staged_diff(&dir, 10_000).unwrap();
        assert!(d.contains("a.txt") && d.contains("+bonjour"), "{d}");
        assert!(staged_diff(&dir, 40).unwrap().contains("tronqué"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn tail_keeps_end() {
        let text: String = (1..=10).map(|i| format!("l{i}\n")).collect();
        let out = tail(&text, 3, 10_000);
        assert!(out.ends_with("l8\nl9\nl10\n") && out.contains("7 lignes"), "{out}");
    }
}

#[cfg(test)]
mod live {
    use super::*;

    /// `NOVATERM_AI_MODEL=llama3.2 cargo test live_ollama -- --ignored --nocapture` (Ollama lancé).
    #[test]
    #[ignore]
    fn live_ollama() {
        let model = std::env::var("NOVATERM_AI_MODEL").unwrap_or_else(|_| "llama3.2".into());
        let ep = Endpoint::new(Provider::Ollama, "", &model);
        println!("modèles : {:?}", list_models(&ep));
        let ctx = Context { shell: "zsh".into(), cwd: Some("/tmp".into()) };
        let job = start(ep, system_prompt(&ctx, false), vec![Message { role: Role::User, content: "Comment lister les fichiers cachés ?".into() }], || {});
        let mut text = String::new();
        loop {
            match job.rx.recv_timeout(std::time::Duration::from_secs(120)).expect("délai dépassé") {
                Event::Chunk(c) => text.push_str(&c),
                Event::Done => break,
                Event::Error(e) => panic!("{e}"),
            }
        }
        println!("{text}\n{:?}", segments(&text));
        assert!(!text.is_empty());
    }
}
