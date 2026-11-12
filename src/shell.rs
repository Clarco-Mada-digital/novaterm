//! Intégration shell : NovaTerm apprend ce qui se passe au prompt (ligne en cours,
//! début/fin de commande, code de retour, dossier) sans modifier la configuration
//! de l'utilisateur.
//!
//! - zsh : `ZDOTDIR` pointe vers un dossier NovaTerm dont le `.zshenv` restaure le
//!   `ZDOTDIR` d'origine, charge le `.zshenv` de l'utilisateur puis installe des hooks.
//!   Le `.zshrc` habituel est ensuite chargé normalement.
//! - bash : lancé avec `--rcfile` vers un fichier qui charge `~/.bashrc` puis les hooks.
//!
//! - PowerShell / pwsh : le script d'intégration est passé en `-EncodedCommand`
//!   (rien sur le disque, aucune stratégie d'exécution à contourner). Il **enveloppe**
//!   la fonction `prompt` existante (oh-my-posh, starship… continuent de fonctionner)
//!   et pose un gestionnaire PSReadLine sur Entrée pour connaître la commande lancée.
//! - cmd : la variable `PROMPT` est préfixée / suffixée par les séquences (début de
//!   prompt, dossier, fin de prompt). Pas de code de retour : pas de blocs.
//!
//! Messages (OSC 6973) :
//! - `L;<curseur>;<ligne>` ligne en cours d'édition (zsh) ; `E` fin d'édition ;
//! - `A` début du prompt ; `B` fin du prompt (début de la zone de saisie) ;
//! - `H;<commande>` commande lancée ; `D;<code>` commande terminée ;
//! - `P;<dossier>` dossier courant ; `X` suggestions déjà fournies par le shell.
//!
//! Les shells qui n'envoient pas `L` (PowerShell, cmd, fish, nu…) obtiennent quand
//! même les suggestions : la ligne est relue dans la grille entre `B` et le curseur
//! (voir `terminal.rs`). L'OSC 133 standard (starship, oh-my-posh, Git Bash récents)
//! est accepté en plus de l'OSC 6973.

use std::path::PathBuf;

const ZSHENV: &str = r#"# NovaTerm — intégration shell (généré automatiquement, ne pas modifier)
if [[ -n "${NOVATERM_ORIG_ZDOTDIR+x}" ]]; then
  ZDOTDIR="$NOVATERM_ORIG_ZDOTDIR"
else
  unset ZDOTDIR
fi
unset NOVATERM_ORIG_ZDOTDIR
_novaterm_dir="{DIR}"
[[ -f "${ZDOTDIR:-$HOME}/.zshenv" ]] && source "${ZDOTDIR:-$HOME}/.zshenv"
[[ -o interactive && -f "$_novaterm_dir/novaterm.zsh" ]] && source "$_novaterm_dir/novaterm.zsh"
unset _novaterm_dir
"#;

const ZSH_INTEGRATION: &str = r#"# NovaTerm — hooks zsh (généré automatiquement)
[[ -n "$_novaterm_loaded" ]] && return
_novaterm_loaded=1

_novaterm_report() {
  if [[ "$BUFFER" == *$'\n'* ]]; then
    print -rn -- $'\e]6973;E\a'
  else
    print -rn -- $'\e]6973;L;'"$CURSOR;${BUFFER//[[:cntrl:]]/ }"$'\a'
  fi
}
_novaterm_finish() { print -rn -- $'\e]6973;E\a' }
_novaterm_preexec() { print -rn -- $'\e]6973;H;'"${1//[[:cntrl:]]/ }"$'\a' }
_novaterm_precmd() {
  local s=$?
  print -rn -- $'\e]6973;D;'"$s"$'\a\e]6973;A\a\e]6973;P;'"${PWD//[[:cntrl:]]/}"$'\a'
}

# Installé au premier prompt, après le .zshrc (plugins, thème…).
_novaterm_setup() {
  precmd_functions=(${precmd_functions:#_novaterm_setup})
  autoload -Uz add-zle-hook-widget 2>/dev/null || return
  add-zle-hook-widget line-pre-redraw _novaterm_report
  add-zle-hook-widget line-finish _novaterm_finish
  (( ${+functions[_zsh_autosuggest_start]} )) && print -rn -- $'\e]6973;X\a'
}
autoload -Uz add-zsh-hook
add-zsh-hook precmd _novaterm_precmd
add-zsh-hook precmd _novaterm_setup
add-zsh-hook preexec _novaterm_preexec

(( $+commands[imgcat] )) || imgcat() { "${NOVATERM_BIN:-novaterm}" --imgcat "$@" }
"#;

const BASH_INTEGRATION: &str = r#"# NovaTerm — hooks bash (généré automatiquement, ne pas modifier)
[ -f "$HOME/.bashrc" ] && source "$HOME/.bashrc"
if [ -z "$__novaterm_loaded" ]; then
  __novaterm_loaded=1
  __novaterm_inprompt=0
  __novaterm_preexec() {
    [ -n "$COMP_LINE" ] && return
    [ "$__novaterm_inprompt" = 1 ] || return
    __novaterm_inprompt=0
    local c
    c=$(HISTTIMEFORMAT= builtin history 1 | sed 's/^ *[0-9]* *//')
    printf '\e]6973;H;%s\a' "${c//[[:cntrl:]]/ }"
  }
  __novaterm_precmd() {
    printf '\e]6973;D;%s\a\e]6973;A\a\e]6973;P;%s\a' "$__novaterm_status" "$PWD"
    __novaterm_inprompt=1
  }
  trap '__novaterm_preexec' DEBUG
  PROMPT_COMMAND="__novaterm_status=\$?;${PROMPT_COMMAND:+$PROMPT_COMMAND;}__novaterm_precmd"
  command -v imgcat >/dev/null 2>&1 || imgcat() { "${NOVATERM_BIN:-novaterm}" --imgcat "$@"; }
fi
"#;

/// Intégration PowerShell (Windows PowerShell 5.1 et PowerShell 7+).
///
/// Tout est protégé : si PSReadLine est absent ou qu'une API manque, le shell
/// démarre normalement et l'on perd seulement la fonction concernée.
const PS_INTEGRATION: &str = r#"# NovaTerm - integration PowerShell (genere automatiquement, ne pas modifier)
if ($global:__novatermLoaded) { return }
$global:__novatermLoaded = $true
$global:__novatermEsc = [char]27

# Le prompt de l'utilisateur (oh-my-posh, starship, defaut...) est conserve et appele.
$global:__novatermPrompt = $function:prompt

function global:prompt {
  $ok = $?
  $native = $global:LASTEXITCODE
  $code = if ($ok) { 0 } elseif ($native) { $native } else { 1 }
  $e = $global:__novatermEsc
  $cwd = ''
  try { $cwd = $ExecutionContext.SessionState.Path.CurrentLocation.Path } catch { }
  $head = "$e]6973;D;$code$e\$e]6973;A$e\"
  if ($cwd -and -not $cwd.Contains('::')) { $head += "$e]6973;P;$cwd$e\" }
  $body = ''
  try {
    if ($global:__novatermPrompt) { $body = (& $global:__novatermPrompt) -join '' }
  } catch { }
  if (-not $body) { $body = "PS $($executionContext.SessionState.Path.CurrentLocation)$('>' * ($nestedPromptLevel + 1)) " }
  # $LASTEXITCODE doit rester tel quel pour les scripts de l'utilisateur.
  $global:LASTEXITCODE = $native
  "$head$body$e]6973;B$e\"
}

# Commande lancee : PSReadLine donne la ligne juste avant de la valider.
try {
  if (Get-Command Set-PSReadLineKeyHandler -ErrorAction Stop) {
    Set-PSReadLineKeyHandler -Chord Enter -BriefDescription NovaTermAccept -ScriptBlock {
      $line = $null; $cursor = $null
      try {
        [Microsoft.PowerShell.PSConsoleReadLine]::GetBufferState([ref]$line, [ref]$cursor)
        if ($line) {
          $one = ($line -replace '[\r\n\t]', ' ')
          [Console]::Write("$([char]27)]6973;H;$one$([char]27)\")
        }
      } catch { }
      [Microsoft.PowerShell.PSConsoleReadLine]::AcceptLine()
    }
  }
} catch { }

# Suggestions deja fournies par PSReadLine : NovaTerm s'efface.
try {
  if ((Get-PSReadLineOption).PredictionSource -ne 'None') {
    [Console]::Write("$($global:__novatermEsc)]6973;X$($global:__novatermEsc)\")
  }
} catch { }

if ($env:NOVATERM_CLI -and -not (Get-Command imgcat -ErrorAction Ignore)) {
  function global:imgcat { & $env:NOVATERM_CLI --imgcat @args }
}
"#;

/// Script PowerShell encodé pour `-EncodedCommand` (UTF-16LE puis base64).
///
/// C'est la seule façon fiable d'installer l'intégration : un fichier `.ps1`
/// serait refusé par la stratégie d'exécution par défaut de Windows PowerShell
/// (« Restricted »), et il faudrait alors la contourner avec
/// `-ExecutionPolicy Bypass` — ce qui autoriserait au passage tous les autres
/// scripts de la session. Ici, rien n'est écrit sur le disque et la stratégie de
/// l'utilisateur reste intacte.
fn encoded_command(script: &str) -> String {
    use base64::Engine;
    let utf16: Vec<u8> = script.encode_utf16().flat_map(u16::to_le_bytes).collect();
    base64::engine::general_purpose::STANDARD.encode(utf16)
}

/// Séquences ajoutées au `PROMPT` de cmd : début de prompt, dossier, fin de prompt.
/// `$e` = échappement, `$p` = dossier courant. cmd ne permet pas de connaître le
/// code de retour depuis le prompt : pas de blocs de commandes, mais le dossier et
/// les suggestions fonctionnent.
fn cmd_prompt() -> String {
    let original = std::env::var("PROMPT").unwrap_or_default();
    let original = if original.trim().is_empty() { "$P$G".to_string() } else { original };
    format!("$e]6973;A$e\\$e]6973;P;$P$e\\{original}$e]6973;B$e\\")
}

fn integration_dir(shell: &str) -> Option<PathBuf> {
    let dir = dirs::cache_dir()?.join("novaterm").join("shell-integration").join(shell);
    std::fs::create_dir_all(&dir).ok()?;
    let files: Vec<(&str, String)> = match shell {
        "zsh" => vec![
            (".zshenv", ZSHENV.replace("{DIR}", &dir.display().to_string())),
            ("novaterm.zsh", ZSH_INTEGRATION.to_string()),
        ],
        _ => vec![("novaterm.bash", BASH_INTEGRATION.to_string())],
    };
    for (name, content) in files {
        let path = dir.join(name);
        if std::fs::read_to_string(&path).ok().as_deref() != Some(content.as_str()) {
            std::fs::write(&path, content).ok()?;
        }
    }
    Some(dir)
}

/// Modifications à apporter au lancement du shell.
///
/// Les arguments de l'utilisateur (réglage « Terminal → shell ») sont conservés :
/// la ligne finale est `pre` + arguments de l'utilisateur + `post`. C'est ce qui
/// permet à « Git Bash » (`bash.exe --login -i`) de garder son `--login`.
#[derive(Default)]
pub struct Launch {
    pub env: Vec<(String, String)>,
    /// Arguments placés avant ceux de l'utilisateur (bash : `--rcfile …`).
    pub pre: Vec<String>,
    /// Arguments placés après (PowerShell : `-Command ". 'novaterm.ps1'"`).
    pub post: Vec<String>,
}

impl Launch {
    /// Y a-t-il des arguments à ajouter ?
    pub fn has_args(&self) -> bool {
        !self.pre.is_empty() || !self.post.is_empty()
    }

    /// Ligne d'arguments complète, autour de ceux de l'utilisateur.
    pub fn args_around(&self, user: &[String]) -> Vec<String> {
        let mut v = self.pre.clone();
        v.extend(user.iter().cloned());
        v.extend(self.post.iter().cloned());
        v
    }
}

/// Nom du shell en minuscules, sans dossier ni « .exe » (`C:\…\pwsh.exe` → `pwsh`).
pub fn shell_name(shell_path: &str) -> String {
    let file = shell_path.trim().trim_matches('"').rsplit(['/', '\\']).next().unwrap_or("").to_ascii_lowercase();
    file.strip_suffix(".exe").unwrap_or(&file).to_string()
}

/// Touches qui vident la ligne en cours avant d'y insérer une commande.
/// Ctrl+E (fin de ligne) puis Ctrl+U (effacer) : compris par readline, zle, fish,
/// reedline… mais PowerShell et cmd les affichaient tels quels (« ^E^U ») : pour eux,
/// Échap vide la ligne (PSReadLine et cmd). Pour un shell inconnu, on n'envoie rien :
/// mieux vaut ajouter à la ligne que la casser.
pub fn line_clear_keys(shell_path: &str) -> &'static [u8] {
    match shell_name(shell_path).as_str() {
        "bash" | "zsh" | "fish" | "sh" | "dash" | "ksh" | "mksh" | "yash" | "nu" | "elvish" | "xonsh" | "wsl" | "ubuntu" | "debian" => b"\x05\x15",
        "pwsh" | "powershell" | "cmd" => b"\x1b",
        _ => b"",
    }
}

/// Prépare le lancement du shell. `integration` à faux : on se limite au strict
/// nécessaire (bannière PowerShell, chemin des binaires), sans aucun hook.
pub fn launch_for(shell_path: &str, integration: bool) -> Launch {
    let mut launch = Launch::default();
    if let Some(exe) = crate::appimage::stable_exe() {
        launch.env.push(("NOVATERM_BIN".into(), exe.display().to_string()));
        // Binaire console, seul capable d'écrire dans le terminal sous Windows.
        if let Some(cli) = crate::platform::cli_exe(&exe) {
            launch.env.push(("NOVATERM_CLI".into(), cli.display().to_string()));
        }
    }
    if !integration {
        // Sans cela, la bannière de copyright s'affiche à chaque onglet.
        if matches!(shell_name(shell_path).as_str(), "pwsh" | "powershell") {
            launch.pre = vec!["-NoLogo".into()];
        }
        return launch;
    }
    match shell_name(shell_path).as_str() {
        "zsh" => {
            if let Some(dir) = integration_dir("zsh") {
                launch.env.push(("ZDOTDIR".into(), dir.display().to_string()));
                if let Ok(orig) = std::env::var("ZDOTDIR") {
                    launch.env.push(("NOVATERM_ORIG_ZDOTDIR".into(), orig));
                }
            }
        }
        "bash" => {
            if let Some(dir) = integration_dir("bash") {
                launch.pre = vec!["--rcfile".into(), dir.join("novaterm.bash").display().to_string(), "-i".into()];
            }
        }
        "pwsh" | "powershell" => {
            launch.pre = vec!["-NoLogo".into(), "-NoExit".into()];
            // `-EncodedCommand` consomme tout ce qui suit : il reste en dernier.
            launch.post = vec!["-EncodedCommand".into(), encoded_command(PS_INTEGRATION)];
        }
        "cmd" => launch.env.push(("PROMPT".into(), cmd_prompt())),
        _ => {}
    }
    launch
}

/// Message envoyé par le shell.
#[derive(Debug, PartialEq, Eq)]
pub enum ShellMsg {
    Line { cursor: usize, buffer: String },
    EndLine,
    PromptStart,
    /// Fin du prompt : la saisie de l'utilisateur commence ici.
    PromptEnd,
    Executed(String),
    /// Commande lancée, texte inconnu (OSC 133;C) : relu dans la grille.
    ExecutedUnknown,
    Done(i32),
    Cwd(String),
    Conflict,
}

pub fn parse(payload: &str) -> Option<ShellMsg> {
    let (kind, rest) = payload.split_once(';').unwrap_or((payload, ""));
    match kind {
        "L" => {
            let (cursor, buffer) = rest.split_once(';')?;
            Some(ShellMsg::Line { cursor: cursor.parse().ok()?, buffer: buffer.to_string() })
        }
        "E" => Some(ShellMsg::EndLine),
        "A" => Some(ShellMsg::PromptStart),
        "B" => Some(ShellMsg::PromptEnd),
        "H" => Some(ShellMsg::Executed(rest.to_string())),
        "D" => Some(ShellMsg::Done(rest.trim().parse().unwrap_or(0))),
        "P" => Some(ShellMsg::Cwd(rest.to_string())),
        "X" => Some(ShellMsg::Conflict),
        _ => None,
    }
}

/// OSC 133 : intégration shell standard (starship, oh-my-posh, Git Bash récents,
/// Windows Terminal…). Donne les blocs et les suggestions sans notre propre script.
///
/// `A` début du prompt, `B` fin du prompt, `C` commande lancée (texte non transmis),
/// `D[;code]` commande terminée. Les paramètres après la lettre sont ignorés.
pub fn parse_osc133(payload: &str) -> Option<ShellMsg> {
    let (kind, rest) = payload.split_once(';').unwrap_or((payload, ""));
    match kind {
        "A" => Some(ShellMsg::PromptStart),
        "B" => Some(ShellMsg::PromptEnd),
        "C" => Some(ShellMsg::ExecutedUnknown),
        "D" => {
            // « D », « D;1 », « D;1;aid=… »
            let code = rest.split(';').next().unwrap_or("").trim();
            Some(ShellMsg::Done(code.parse().unwrap_or(0)))
        }
        _ => None,
    }
}

/// Dossier d'une séquence OSC 7 (`file://hôte/chemin`, encodé en %XX).
pub fn parse_osc7(payload: &str) -> Option<String> {
    let rest = payload.strip_prefix("file://")?;
    let path = &rest[rest.find('/')?..];
    let bytes = path.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(b) = u8::from_str_radix(std::str::from_utf8(&bytes[i + 1..i + 3]).ok()?, 16) {
                out.push(b);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8(out).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clear_keys_per_shell() {
        assert_eq!(line_clear_keys("/bin/zsh"), b"\x05\x15");
        assert_eq!(line_clear_keys("/usr/bin/bash"), b"\x05\x15");
        assert_eq!(line_clear_keys("C:\\Windows\\System32\\wsl.exe"), b"\x05\x15");
        // Échap vide la ligne dans PSReadLine et dans cmd.
        assert_eq!(line_clear_keys("pwsh.exe"), b"\x1b");
        assert_eq!(line_clear_keys("C:\\Program Files\\PowerShell\\7\\pwsh.exe"), b"\x1b");
        assert_eq!(line_clear_keys("powershell.exe"), b"\x1b");
        assert_eq!(line_clear_keys("cmd.exe"), b"\x1b");
        assert!(line_clear_keys("/usr/bin/inconnu").is_empty());
    }

    #[test]
    fn names() {
        assert_eq!(shell_name("C:\\Program Files\\Git\\bin\\bash.exe"), "bash");
        assert_eq!(shell_name("\"C:\\Windows\\System32\\cmd.exe\""), "cmd");
        assert_eq!(shell_name("/bin/zsh"), "zsh");
    }

    #[test]
    fn powershell_launch() {
        let l = launch_for("C:\\Program Files\\PowerShell\\7\\pwsh.exe", true);
        // Les arguments de l'utilisateur restent avant « -EncodedCommand ».
        let args = l.args_around(&["-NoProfile".to_string()]);
        assert_eq!(args[0], "-NoLogo");
        assert_eq!(args[2], "-NoProfile");
        assert_eq!(args[3], "-EncodedCommand");
        // Le script est bien du UTF-16LE encodé en base64, et non un chemin de fichier.
        use base64::Engine;
        let raw = base64::engine::general_purpose::STANDARD.decode(&args[4]).unwrap();
        let decoded: String = String::from_utf16(&raw.chunks(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect::<Vec<u16>>()).unwrap();
        assert!(decoded.contains("function global:prompt"), "{decoded}");
        assert!(decoded.contains("6973;B"), "la fin de prompt doit être signalée");
        // Git Bash : « --login -i » n'est plus perdu.
        let l = launch_for("C:\\Program Files\\Git\\bin\\bash.exe", true);
        let args = l.args_around(&["--login".to_string(), "-i".to_string()]);
        assert_eq!(args[0], "--rcfile");
        assert_eq!(args[3..], ["--login", "-i"]);
        // cmd : prompt enrichi, aucun argument (le shell reste interactif).
        let l = launch_for("cmd.exe", true);
        assert!(!l.has_args());
        let prompt = l.env.iter().find(|(k, _)| k == "PROMPT").map(|(_, v)| v.clone()).unwrap_or_default();
        assert!(prompt.starts_with("$e]6973;A$e\\") && prompt.ends_with("$e]6973;B$e\\"), "{prompt}");
        assert!(prompt.contains("$e]6973;P;$P$e\\"), "{prompt}");
    }

    #[test]
    fn osc133_messages() {
        assert_eq!(parse_osc133("A"), Some(ShellMsg::PromptStart));
        assert_eq!(parse_osc133("B"), Some(ShellMsg::PromptEnd));
        assert_eq!(parse_osc133("C"), Some(ShellMsg::ExecutedUnknown));
        assert_eq!(parse_osc133("D;3"), Some(ShellMsg::Done(3)));
        assert_eq!(parse_osc133("D;1;aid=7"), Some(ShellMsg::Done(1)));
        assert_eq!(parse_osc133("D"), Some(ShellMsg::Done(0)));
        assert_eq!(parse_osc133("A;cl=m"), Some(ShellMsg::PromptStart));
        assert_eq!(parse_osc133("Z"), None);
        assert_eq!(parse("B"), Some(ShellMsg::PromptEnd));
    }

    #[test]
    fn parse_messages() {
        assert_eq!(parse("L;4;git ;x"), Some(ShellMsg::Line { cursor: 4, buffer: "git ;x".into() }));
        assert_eq!(parse("E"), Some(ShellMsg::EndLine));
        assert_eq!(parse("H;make"), Some(ShellMsg::Executed("make".into())));
        assert_eq!(parse("D;127"), Some(ShellMsg::Done(127)));
        assert_eq!(parse("P;/tmp/a b"), Some(ShellMsg::Cwd("/tmp/a b".into())));
        assert_eq!(parse("Z"), None);
    }

    #[test]
    fn osc7() {
        assert_eq!(parse_osc7("file://pc/home/a%20b").as_deref(), Some("/home/a b"));
        assert_eq!(parse_osc7("http://x"), None);
    }
}
