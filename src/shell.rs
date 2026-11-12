//! Intégration shell : NovaTerm apprend ce qui se passe au prompt (ligne en cours,
//! début/fin de commande, code de retour, dossier) sans modifier la configuration
//! de l'utilisateur.
//!
//! - zsh : `ZDOTDIR` pointe vers un dossier NovaTerm dont le `.zshenv` restaure le
//!   `ZDOTDIR` d'origine, charge le `.zshenv` de l'utilisateur puis installe des hooks.
//!   Le `.zshrc` habituel est ensuite chargé normalement.
//! - bash : lancé avec `--rcfile` vers un fichier qui charge `~/.bashrc` puis les hooks.
//!
//! Messages (OSC 6973) :
//! - `L;<curseur>;<ligne>` ligne en cours d'édition (zsh) ; `E` fin d'édition ;
//! - `A` début du prompt ; `H;<commande>` commande lancée ; `D;<code>` commande terminée ;
//! - `P;<dossier>` dossier courant ; `X` zsh-autosuggestions déjà actif.

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
#[derive(Default)]
pub struct Launch {
    pub env: Vec<(String, String)>,
    /// Arguments à ajouter (bash : `--rcfile …`). Si non vide, le shell est lancé
    /// explicitement avec ces arguments.
    pub args: Vec<String>,
}

/// Prépare l'intégration pour ce shell (vide si non pris en charge).
/// Touches qui vident la ligne en cours avant d'y insérer une commande.
/// Ctrl+E (fin de ligne) puis Ctrl+U (effacer) : compris par readline, zle, fish,
/// reedline… mais PowerShell et cmd les affichent tels quels (« ^E^U »). Pour un
/// shell inconnu, on n'envoie rien : mieux vaut ajouter à la ligne que la casser.
pub fn line_clear_keys(shell_path: &str) -> &'static [u8] {
    // Nom du programme, quel que soit le séparateur (/ ou \\), sans « .exe ».
    let file = shell_path.trim().rsplit(['/', '\\']).next().unwrap_or("").to_ascii_lowercase();
    let name = file.strip_suffix(".exe").unwrap_or(&file);
    match name {
        "bash" | "zsh" | "fish" | "sh" | "dash" | "ksh" | "mksh" | "yash" | "nu" | "elvish" | "xonsh" | "wsl" | "ubuntu" | "debian" => b"\x05\x15",
        _ => b"",
    }
}

pub fn launch_for(shell_path: &str) -> Launch {
    let name = std::path::Path::new(shell_path)
        .file_stem()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_string();
    let mut launch = Launch::default();
    if let Some(exe) = crate::appimage::stable_exe() {
        launch.env.push(("NOVATERM_BIN".into(), exe.display().to_string()));
    }
    match name.as_str() {
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
                launch.args = vec!["--rcfile".into(), dir.join("novaterm.bash").display().to_string(), "-i".into()];
            }
        }
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
    Executed(String),
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
        "H" => Some(ShellMsg::Executed(rest.to_string())),
        "D" => Some(ShellMsg::Done(rest.trim().parse().unwrap_or(0))),
        "P" => Some(ShellMsg::Cwd(rest.to_string())),
        "X" => Some(ShellMsg::Conflict),
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
        assert!(line_clear_keys("pwsh.exe").is_empty());
        assert!(line_clear_keys("powershell.exe").is_empty());
        assert!(line_clear_keys("cmd.exe").is_empty());
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
