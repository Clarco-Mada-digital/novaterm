//! Appels Windows directs (évite de lancer PowerShell pour des broutilles).
//!
//! NovaTerm est une application fenêtrée : elle n'a pas de console. Chaque
//! programme console lancé sans précaution ouvre alors une fenêtre noire. On
//! s'attache donc une console **masquée** au démarrage : les processus enfants
//! s'y rattachent au lieu d'en créer une visible.

use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::System::Console::{AllocConsole, GetConsoleWindow};
// MessageBeep est rangé dans Diagnostics::Debug par windows-sys (héritage de l'API Win32).
use windows_sys::Win32::System::Diagnostics::Debug::MessageBeep;
use windows_sys::Win32::UI::WindowsAndMessaging::{ShowWindow, MB_OK, SW_HIDE};

/// Console masquée pour les processus enfants. À appeler une seule fois, le plus
/// tôt possible dans `main` (avant tout lancement de shell).
pub fn hide_console() {
    unsafe {
        // Déjà une console (lancement depuis un terminal) : on n'y touche pas,
        // sinon on volerait la sortie standard du terminal appelant.
        if !GetConsoleWindow().is_null() {
            return;
        }
        if AllocConsole() == 0 {
            return;
        }
        let hwnd: HWND = GetConsoleWindow();
        if !hwnd.is_null() {
            ShowWindow(hwnd, SW_HIDE);
        }
    }
}

/// Petit son système (immédiat, là où lancer PowerShell prenait ~500 ms).
pub fn beep() {
    unsafe { MessageBeep(MB_OK) };
}

/// Ouvre un fichier, un dossier ou une URL avec l'application par défaut.
///
/// `ShellExecuteW` au lieu de `cmd /C start` : une cible contenant `&`, `%` ou
/// des guillemets n'est plus tronquée ni réinterprétée par cmd.
pub fn open(target: &str) -> bool {
    use windows_sys::Win32::UI::Shell::ShellExecuteW;
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
    let wide = |s: &str| s.encode_utf16().chain(std::iter::once(0)).collect::<Vec<u16>>();
    let verb = wide("open");
    let file = wide(target);
    let result = unsafe { ShellExecuteW(std::ptr::null_mut(), verb.as_ptr(), file.as_ptr(), std::ptr::null(), std::ptr::null(), SW_SHOWNORMAL) };
    // > 32 = succès (valeur de retour héritée de Win16).
    result as isize > 32
}
