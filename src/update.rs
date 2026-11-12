//! Mises à jour : vérifie les versions publiées sur GitHub, et met à jour l'AppImage
//! en un clic (téléchargement vérifié par SHA-256). Pour les autres installations,
//! NovaTerm indique la marche à suivre et ouvre la page de téléchargement.
//!
//! Seule la page publique des versions de GitHub est contactée, uniquement si
//! l'utilisateur l'a activé (Paramètres → Avancé).

use std::path::{Path, PathBuf};
use std::process::Command;

pub const REPO: &str = "Clarco-Mada-digital/novaterm";

/// Version comparable : (majeur, mineur, correctif, pré-version éventuelle).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Version {
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
    pub pre: Option<String>,
}

impl Version {
    pub fn parse(s: &str) -> Option<Self> {
        let s = s.trim().trim_start_matches('v');
        let (core, pre) = match s.split_once('-') {
            Some((c, p)) => (c, Some(p.to_string())),
            None => (s, None),
        };
        let mut it = core.split('.').map(|n| n.parse::<u64>());
        Some(Self { major: it.next()?.ok()?, minor: it.next()?.ok()?, patch: it.next().unwrap_or(Ok(0)).ok()?, pre })
    }

    pub fn current() -> Self {
        Self::parse(env!("CARGO_PKG_VERSION")).expect("version du paquet valide")
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        (self.major, self.minor, self.patch)
            .cmp(&(other.major, other.minor, other.patch))
            .then_with(|| match (&self.pre, &other.pre) {
                // 1.0.0-beta < 1.0.0
                (None, None) => std::cmp::Ordering::Equal,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (Some(_), None) => std::cmp::Ordering::Less,
                (Some(a), Some(b)) => a.cmp(b),
            })
    }
}

impl std::fmt::Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)?;
        if let Some(p) = &self.pre {
            write!(f, "-{p}")?;
        }
        Ok(())
    }
}

/// Une version disponible plus récente.
#[derive(Clone, Debug)]
pub struct UpdateInfo {
    pub version: Version,
    pub url: String,
    pub notes: String,
    pub prerelease: bool,
    /// AppImage correspondant à l'architecture actuelle (si publiée).
    pub appimage_url: Option<String>,
    pub sums_url: Option<String>,
}

fn curl(args: &[&str]) -> Result<Vec<u8>, String> {
    let mut cmd = Command::new(if cfg!(windows) { "curl.exe" } else { "curl" });
    cmd.args(["-fsSL", "--max-time", "30", "-H", "User-Agent: NovaTerm-updater"]).args(args);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    let out = cmd.output().map_err(|_| "curl est introuvable : impossible de vérifier les mises à jour".to_string())?;
    if out.status.success() {
        Ok(out.stdout)
    } else {
        Err(format!("connexion à GitHub impossible ({})", String::from_utf8_lossy(&out.stderr).trim()))
    }
}

/// Choisit, parmi les versions publiées (JSON de l'API GitHub), la plus récente
/// supérieure à `current`.
pub fn pick_update(json: &[u8], current: &Version, include_pre: bool) -> Result<Option<UpdateInfo>, String> {
    let list: serde_json::Value = serde_json::from_slice(json).map_err(|e| format!("réponse illisible : {e}"))?;
    let arch = std::env::consts::ARCH;
    let mut best: Option<UpdateInfo> = None;
    for r in list.as_array().into_iter().flatten() {
        if r["draft"].as_bool().unwrap_or(false) {
            continue;
        }
        let prerelease = r["prerelease"].as_bool().unwrap_or(false);
        if prerelease && !include_pre {
            continue;
        }
        let tag = r["tag_name"].as_str().unwrap_or_default();
        let Some(version) = Version::parse(tag) else { continue };
        if &version <= current || best.as_ref().is_some_and(|b| b.version >= version) {
            continue;
        }
        let assets = r["assets"].as_array().cloned().unwrap_or_default();
        let find = |pred: &dyn Fn(&str) -> bool| {
            assets.iter().find_map(|a| {
                let name = a["name"].as_str()?;
                pred(name).then(|| a["browser_download_url"].as_str().map(String::from)).flatten()
            })
        };
        best = Some(UpdateInfo {
            version,
            url: r["html_url"].as_str().unwrap_or_default().to_string(),
            notes: r["body"].as_str().unwrap_or_default().to_string(),
            prerelease,
            appimage_url: find(&|n: &str| n.ends_with(&format!("-{arch}.AppImage"))),
            sums_url: find(&|n: &str| n == "SHA256SUMS.txt"),
        });
    }
    Ok(best)
}

/// Interroge GitHub (bloquant : à appeler hors de l'interface).
pub fn check(include_pre: bool) -> Result<Option<UpdateInfo>, String> {
    let json = curl(&["-H", "Accept: application/vnd.github+json", &format!("https://api.github.com/repos/{REPO}/releases?per_page=15")])?;
    pick_update(&json, &Version::current(), include_pre)
}

fn sha256_file(path: &Path) -> Result<String, String> {
    use sha2::{Digest, Sha256};
    use std::io::Read;
    let mut f = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 1 << 16];
    loop {
        let n = f.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(h.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

/// Met à jour l'AppImage en place : téléchargement, vérification SHA-256, remplacement.
/// Retourne le chemin de la nouvelle AppImage (à relancer).
pub fn update_appimage(info: &UpdateInfo, appimage: &Path) -> Result<PathBuf, String> {
    let url = info.appimage_url.as_deref().ok_or("aucune AppImage publiée pour cette architecture")?;
    let sums_url = info.sums_url.as_deref().ok_or("sommes de contrôle absentes : mise à jour refusée")?;
    let file_name = url.rsplit('/').next().unwrap_or_default().to_string();
    let sums = String::from_utf8_lossy(&curl(&[sums_url])?).to_string();
    let expected = sums
        .lines()
        .find_map(|l| {
            let mut it = l.split_whitespace();
            let (h, n) = (it.next()?, it.next()?);
            (n.trim_start_matches('*') == file_name).then(|| h.to_ascii_lowercase())
        })
        .ok_or("fichier absent des sommes de contrôle : mise à jour refusée")?;
    let tmp = appimage.with_extension("download");
    let _ = std::fs::remove_file(&tmp);
    curl(&["-o", &tmp.display().to_string(), url])?;
    let got = sha256_file(&tmp)?;
    if got != expected {
        let _ = std::fs::remove_file(&tmp);
        return Err("le fichier téléchargé est corrompu (SHA-256 différent) : mise à jour annulée".into());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o755)).map_err(|e| e.to_string())?;
    }
    // Remplacement atomique : l'AppImage en cours continue de fonctionner jusqu'au redémarrage.
    std::fs::rename(&tmp, appimage).map_err(|e| format!("remplacement impossible : {e}"))?;
    Ok(appimage.to_path_buf())
}

/// Conseil de mise à jour selon le mode d'installation.
pub fn how_to_update(info: &UpdateInfo) -> String {
    if crate::appimage::current().is_some() {
        return "Mise à jour automatique de l'AppImage disponible.".into();
    }
    let exe = std::env::current_exe().map(|p| p.display().to_string()).unwrap_or_default();
    if cfg!(target_os = "linux") && exe.starts_with("/usr/") {
        format!("Paquet système : téléchargez le .deb ou .rpm de la version {} puis installez-le.", info.version)
    } else if cfg!(windows) {
        format!("Téléchargez l'installateur de la version {} : il remplacera l'ancienne.", info.version)
    } else if cfg!(target_os = "macos") {
        format!("Téléchargez le .dmg de la version {} et remplacez l'application.", info.version)
    } else {
        format!("Téléchargez la version {} depuis la page des versions.", info.version)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions() {
        let v = |s| Version::parse(s).unwrap();
        assert!(v("0.0.3") > v("0.0.2"));
        assert!(v("v0.1.0") > v("0.0.9"));
        assert!(v("1.0.0") > v("1.0.0-rc.1"));
        assert!(v("1.0.0-rc.2") > v("1.0.0-rc.1"));
        assert_eq!(v("v2.3.4").to_string(), "2.3.4");
        assert!(Version::parse("pas-une-version").is_none());
    }

    #[test]
    fn pick() {
        let arch = std::env::consts::ARCH;
        let json = format!(
            r#"[
            {{"tag_name":"v0.0.5","draft":true,"prerelease":true,"assets":[]}},
            {{"tag_name":"v0.0.4","draft":false,"prerelease":true,"html_url":"https://x/v0.0.4","body":"notes",
              "assets":[{{"name":"NovaTerm-0.0.4-{arch}.AppImage","browser_download_url":"https://x/a"}},
                        {{"name":"SHA256SUMS.txt","browser_download_url":"https://x/s"}}]}},
            {{"tag_name":"v0.0.3","draft":false,"prerelease":false,"assets":[]}},
            {{"tag_name":"v0.0.1","draft":false,"prerelease":true,"assets":[]}}]"#
        );
        let cur = Version::parse("0.0.2").unwrap();
        let u = pick_update(json.as_bytes(), &cur, true).unwrap().unwrap();
        assert_eq!(u.version.to_string(), "0.0.4");
        assert_eq!(u.appimage_url.as_deref(), Some("https://x/a"));
        let stable = pick_update(json.as_bytes(), &cur, false).unwrap().unwrap();
        assert_eq!(stable.version.to_string(), "0.0.3");
        assert!(pick_update(json.as_bytes(), &Version::parse("0.0.4").unwrap(), true).unwrap().is_none());
    }

    /// Test réel (réseau) : `cargo test live_appimage_update -- --ignored`.
    #[test]
    #[ignore]
    fn live_appimage_update() {
        let json = curl(&["-H", "Accept: application/vnd.github+json", &format!("https://api.github.com/repos/{REPO}/releases?per_page=15")]).unwrap();
        let old = Version::parse("0.0.1").unwrap();
        let info = pick_update(&json, &old, true).unwrap().expect("une version plus récente que 0.0.1");
        let dir = std::env::temp_dir().join(format!("novaterm-upd-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let app = dir.join("NovaTerm.AppImage");
        std::fs::write(&app, b"ancienne version").unwrap();
        let path = update_appimage(&info, &app).unwrap();
        let size = std::fs::metadata(&path).unwrap().len();
        assert!(size > 1_000_000, "AppImage téléchargée ({size} octets)");
        println!("mise à jour vers {} OK ({} Mo)", info.version, size / 1_048_576);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn sha() {
        let p = std::env::temp_dir().join(format!("novaterm-sha-{}", std::process::id()));
        std::fs::write(&p, b"abc").unwrap();
        assert_eq!(sha256_file(&p).unwrap(), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
        let _ = std::fs::remove_file(p);
    }
}
