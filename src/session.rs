//! Sauvegarde / restauration de la session (onglets, splits, dossiers, contenu) et
//! profils de projet (dispositions nommées avec commandes de démarrage).

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Nœud de disposition : un panneau ou une division.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Node {
    Leaf {
        #[serde(default)]
        cwd: Option<String>,
        /// Commande lancée au démarrage du panneau (profils).
        #[serde(default)]
        command: Option<String>,
        /// Texte affiché avant le shell (restauration du contenu).
        #[serde(default)]
        content: Option<String>,
    },
    Split {
        horizontal: bool,
        ratio: f32,
        a: Box<Node>,
        b: Box<Node>,
    },
}

impl Node {
    #[cfg(test)]
    pub fn leaf(cwd: Option<String>) -> Self {
        Node::Leaf { cwd, command: None, content: None }
    }

    /// Panneaux dans l'ordre (pour l'édition des profils).
    pub fn leaves_mut(&mut self) -> Vec<&mut Node> {
        match self {
            Node::Leaf { .. } => vec![self],
            Node::Split { a, b, .. } => {
                let mut v = a.leaves_mut();
                v.extend(b.leaves_mut());
                v
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TabState {
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub focused: usize,
    pub root: Node,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Session {
    #[serde(default)]
    pub active: usize,
    #[serde(default)]
    pub tabs: Vec<TabState>,
}

/// Profil de projet : disposition nommée (avec commandes de démarrage).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Profile {
    pub name: String,
    #[serde(default)]
    pub tabs: Vec<TabState>,
}

pub fn path() -> Option<PathBuf> {
    dirs::data_dir().map(|d| d.join("novaterm").join("session.toml"))
}

pub fn save(session: &Session) {
    let Some(p) = path() else { return };
    if let Some(parent) = p.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    match toml::to_string(session) {
        Ok(s) => {
            // Écriture atomique : un plantage pendant l'écriture ne corrompt pas la session.
            let tmp = p.with_extension("tmp");
            if std::fs::write(&tmp, s).is_ok() {
                let _ = std::fs::rename(tmp, p);
            }
        }
        Err(e) => eprintln!("NovaTerm: session non sauvegardée: {e}"),
    }
}

pub fn load() -> Option<Session> {
    let text = std::fs::read_to_string(path()?).ok()?;
    toml::from_str(&text).map_err(|e| eprintln!("NovaTerm: session illisible: {e}")).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toml_roundtrip() {
        let s = Session {
            active: 1,
            tabs: vec![
                TabState { title: Some("Serveur".into()), focused: 0, root: Node::leaf(Some("/tmp".into())) },
                TabState {
                    title: None,
                    focused: 1,
                    root: Node::Split {
                        horizontal: true,
                        ratio: 0.4,
                        a: Box::new(Node::Leaf { cwd: None, command: Some("npm run dev".into()), content: Some("a\nb".into()) }),
                        b: Box::new(Node::leaf(None)),
                    },
                },
            ],
        };
        let text = toml::to_string(&s).unwrap();
        assert_eq!(toml::from_str::<Session>(&text).unwrap(), s);
        let p = Profile { name: "x".into(), tabs: s.tabs.clone() };
        assert_eq!(toml::from_str::<Profile>(&toml::to_string(&p).unwrap()).unwrap(), p);
    }
}
