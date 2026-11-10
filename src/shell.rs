//! Intégration shell : NovaTerm apprend ce qui est tapé sur la ligne de commande
//! (pour les suggestions) sans modifier la configuration de l'utilisateur.
//!
//! zsh : on pointe `ZDOTDIR` vers un dossier NovaTerm dont le `.zshenv` restaure le
//! `ZDOTDIR` d'origine, charge le `.zshenv` de l'utilisateur puis installe des
//! « hooks » zle. Le `.zshrc` habituel est ensuite chargé normalement.
//!
//! Le shell envoie des séquences OSC privées (code 6973), interceptées par NovaTerm :
//! - `L;<curseur>;<ligne>` : contenu de la ligne en cours d'édition ;
//! - `E` : fin d'édition (commande validée ou annulée) ;
//! - `H;<commande>` : commande exécutée (ajoutée à l'historique) ;
//! - `X` : un autre plugin de suggestions est actif (zsh-autosuggestions).

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

const INTEGRATION: &str = r#"# NovaTerm — hooks zle (généré automatiquement)
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

# Installé au premier prompt, après le .zshrc (plugins, thème…).
_novaterm_setup() {
  precmd_functions=(${precmd_functions:#_novaterm_setup})
  autoload -Uz add-zle-hook-widget 2>/dev/null || return
  add-zle-hook-widget line-pre-redraw _novaterm_report
  add-zle-hook-widget line-finish _novaterm_finish
  (( ${+functions[_zsh_autosuggest_start]} )) && print -rn -- $'\e]6973;X\a'
}
autoload -Uz add-zsh-hook
add-zsh-hook precmd _novaterm_setup
add-zsh-hook preexec _novaterm_preexec
"#;

fn integration_dir() -> Option<PathBuf> {
    let dir = dirs::cache_dir()?.join("novaterm").join("shell-integration").join("zsh");
    std::fs::create_dir_all(&dir).ok()?;
    let zshenv = ZSHENV.replace("{DIR}", &dir.display().to_string());
    for (name, content) in [(".zshenv", zshenv.as_str()), ("novaterm.zsh", INTEGRATION)] {
        let path = dir.join(name);
        if std::fs::read_to_string(&path).ok().as_deref() != Some(content) {
            std::fs::write(&path, content).ok()?;
        }
    }
    Some(dir)
}

/// Variables d'environnement à ajouter au shell lancé (vide si non pris en charge).
pub fn env_for(shell_path: &str) -> Vec<(String, String)> {
    let name = std::path::Path::new(shell_path).file_name().and_then(|n| n.to_str()).unwrap_or("");
    if name != "zsh" {
        return Vec::new();
    }
    let Some(dir) = integration_dir() else { return Vec::new() };
    let mut env = vec![("ZDOTDIR".to_string(), dir.display().to_string())];
    if let Ok(orig) = std::env::var("ZDOTDIR") {
        env.push(("NOVATERM_ORIG_ZDOTDIR".to_string(), orig));
    }
    env
}

/// Message envoyé par le shell.
#[derive(Debug, PartialEq, Eq)]
pub enum ShellMsg {
    Line { cursor: usize, buffer: String },
    EndLine,
    Executed(String),
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
        "H" => Some(ShellMsg::Executed(rest.to_string())),
        "X" => Some(ShellMsg::Conflict),
        _ => None,
    }
}

/// Découpe le flux du PTY : extrait les séquences `ESC ] 6973 ; … (BEL | ESC \)`.
/// Les octets incomplets en fin de lecture sont conservés dans `carry`.
#[derive(Default)]
pub struct OscFilter {
    carry: Vec<u8>,
}

pub enum Chunk<'a> {
    Data(&'a [u8]),
    Msg(String),
}

const PREFIX: &[u8] = b"\x1b]6973;";

impl OscFilter {
    /// Appelle `f` pour chaque morceau, dans l'ordre du flux.
    pub fn feed(&mut self, input: &[u8], mut f: impl FnMut(Chunk<'_>)) {
        let mut data = std::mem::take(&mut self.carry);
        data.extend_from_slice(input);
        let mut pos = 0;
        while pos < data.len() {
            let rest = &data[pos..];
            let Some(start) = find(rest, PREFIX) else {
                // Garder une éventuelle amorce de séquence coupée en fin de lecture.
                let keep = (1..PREFIX.len().min(rest.len() + 1))
                    .rev()
                    .find(|&k| rest.ends_with(&PREFIX[..k]))
                    .unwrap_or(0);
                if rest.len() > keep {
                    f(Chunk::Data(&rest[..rest.len() - keep]));
                }
                self.carry = rest[rest.len() - keep..].to_vec();
                return;
            };
            if start > 0 {
                f(Chunk::Data(&rest[..start]));
            }
            let body = &rest[start + PREFIX.len()..];
            let end = body.iter().enumerate().find_map(|(i, &b)| match b {
                0x07 => Some((i, 1)),
                0x1b if body.get(i + 1) == Some(&b'\\') => Some((i, 2)),
                _ => None,
            });
            match end {
                Some((i, term_len)) => {
                    f(Chunk::Msg(String::from_utf8_lossy(&body[..i]).into_owned()));
                    pos += start + PREFIX.len() + i + term_len;
                }
                None if body.len() < 16 * 1024 => {
                    self.carry = rest[start..].to_vec();
                    return;
                }
                None => {
                    // Séquence anormalement longue : on la rend au terminal.
                    f(Chunk::Data(&rest[start..]));
                    return;
                }
            }
        }
    }
}

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(f: &mut OscFilter, input: &[u8]) -> (Vec<u8>, Vec<String>) {
        let mut data = Vec::new();
        let mut msgs = Vec::new();
        f.feed(input, |c| match c {
            Chunk::Data(d) => data.extend_from_slice(d),
            Chunk::Msg(m) => msgs.push(m),
        });
        (data, msgs)
    }

    #[test]
    fn extracts_and_splits() {
        let mut f = OscFilter::default();
        let (d, m) = run(&mut f, b"abc\x1b]6973;L;3;git\x07def\x1b]6973;E\x1b\\");
        assert_eq!(d, b"abcdef");
        assert_eq!(m, vec!["L;3;git", "E"]);
        // Séquence coupée entre deux lectures.
        let (d, m) = run(&mut f, b"xy\x1b]69");
        assert_eq!((d.as_slice(), m.len()), (&b"xy"[..], 0));
        let (d, m) = run(&mut f, b"73;H;ls -la\x07z");
        assert_eq!(d, b"z");
        assert_eq!(m, vec!["H;ls -la"]);
        // Les autres OSC passent intacts.
        let (d, _) = run(&mut f, b"\x1b]0;titre\x07");
        assert_eq!(d, b"\x1b]0;titre\x07");
    }

    #[test]
    fn parse_messages() {
        assert_eq!(parse("L;4;git ;x"), Some(ShellMsg::Line { cursor: 4, buffer: "git ;x".into() }));
        assert_eq!(parse("E"), Some(ShellMsg::EndLine));
        assert_eq!(parse("H;make"), Some(ShellMsg::Executed("make".into())));
        assert_eq!(parse("Z"), None);
    }
}
