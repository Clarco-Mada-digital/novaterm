# NovaTerm

Terminal moderne pour Linux, écrit en Rust. Onglets, splits d'écran, thèmes,
transparence, image de fond, curseur animé (déplacement fluide, fondu, pulsation,
rotation verticale), notifications animées et fenêtre entièrement personnalisable
(barre de titre maison, boutons, coins arrondis).

## Prérequis

- **Rust** (via [rustup](https://rustup.rs)) :
  ```bash
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
  ```
- **Dépendances système** (Linux Mint / Ubuntu / Debian) :
  ```bash
  sudo apt install build-essential cmake pkg-config \
    libx11-dev libxcursor-dev libxrandr-dev libxi-dev \
    libgl1-mesa-dev libxkbcommon-dev libwayland-dev
  ```
  Déjà installées sur la machine de développement.

## Lancer en dev

```bash
cd novaterm
cargo run
```

Le premier build compile toutes les dépendances (quelques minutes) ; les
suivants ne recompilent que NovaTerm (quelques secondes). `cargo run` build en
mode **debug** puis lance le terminal. Les logs s'affichent dans le terminal
courant.

Pour lancer sans rebuild à chaque fois :

```bash
cargo build          # build debug seul
./target/debug/novaterm
```

## Builder (release optimisée)

```bash
cargo build --release
```

- **Résultat du build** : le binaire est dans `target/release/novaterm`.
  ```bash
  ls -lh target/release/novaterm     # voir la taille (~12 Mo)
  ./target/release/novaterm          # le lancer
  ```
- **Garder les logs de build** :
  ```bash
  cargo build --release 2>&1 | tee build.log
  ```
- Le profil release est déjà optimisé dans `Cargo.toml` :
  `opt-level = 3`, `lto = "fat"`, `codegen-units = 1`, `panic = "abort"`, `strip = true`.

### Encore plus optimisé pour CE processeur

```bash
RUSTFLAGS="-C target-cpu=native" cargo build --release
```

Le binaire utilise alors les instructions spécifiques de ton CPU (non portable
vers une autre machine).

## Installer (binaire + icône + menu)

```bash
./install.sh
```

Le script compile en release puis installe :
- le binaire dans `~/.local/bin/novaterm` ;
- l'icône (SVG + PNG 16 → 512 px) dans `~/.local/share/icons/hicolor/` ;
- l'entrée de menu `~/.local/share/applications/novaterm.desktop`
  (NovaTerm apparaît dans le menu Cinnamon → Système).

L'icône est dessinée par le programme lui-même (`src/icon.rs`) ; pour régénérer
les PNG : `novaterm --export-icon assets/icon`.

## Tests et qualité

```bash
cargo test       # tests unitaires (clavier, souris, raccourcis, splits, liens)
cargo clippy     # lint — doit rester sans warning
cargo fmt        # formatage (optionnel)
```

## Configuration

Fichier : `~/.config/novaterm/config.toml` (créé automatiquement).
Tout est modifiable en direct via **Ctrl+,** (Paramètres, organisés en pages :
Apparence, Police, Fenêtre, Curseur, Terminal, Souris, Raccourcis) ; les
changements sont sauvegardés automatiquement.

- **Police** : toutes les polices à chasse fixe du système (Nerd Fonts, Fira
  Code…), taille, interligne, espacement des lettres, marges, aperçu.
- **Fenêtre** : taille au démarrage (ou mémorisée à la fermeture), démarrer
  agrandie / en plein écran, toujours au premier plan, barres d'onglets / de
  statut, confirmation avant de fermer si un programme tourne encore.
- **Souris** : focus qui suit la souris, copie à la sélection, clic du milieu
  pour coller, action du clic droit, vitesse de la molette, Ctrl+molette pour
  zoomer, Ctrl+clic pour ouvrir les liens, souris transmise à htop/vim/mc
  (Maj pour sélectionner quand même).
- **Terminal** : shell, dossier de départ, historique, Ctrl+C « intelligent »
  (copie s'il y a une sélection), Ctrl+V qui colle, confirmation du collage
  multi-lignes, cloche, garder le panneau ouvert à la fin du shell.
- **Raccourcis** : chaque action peut recevoir plusieurs raccourcis ; cliquer
  sur « + » puis appuyer sur la combinaison. Les conflits sont résolus
  automatiquement.

## Raccourcis (par défaut)

**Ctrl+Maj+P** ouvre la **palette de commandes** : tapez quelques lettres
(« div », « police », « thème »…) pour lancer n'importe quelle action.
**F1** affiche tous les raccourcis (cliquer sur une ligne l'exécute).

| Raccourci | Action |
|---|---|
| Ctrl+Maj+P | Palette de commandes |
| Ctrl+, | Paramètres |
| F1 | Aide des raccourcis |
| Ctrl+Maj+T / Ctrl+Alt+W | Nouvel onglet / fermer l'onglet |
| Ctrl+Tab, Ctrl+PageSuiv | Onglet suivant |
| Ctrl+Maj+Tab, Ctrl+PagePréc | Onglet précédent |
| Alt+1 … Alt+9, Alt+0 | Aller à l'onglet N / dernier onglet (marche en AZERTY) |
| Ctrl+Maj+PagePréc / PageSuiv | Déplacer l'onglet |
| Ctrl+Maj+R, F2 | Renommer l'onglet (ou double-clic dessus) |
| Ctrl+Maj+E / Ctrl+Maj+O | Diviser à droite / en bas |
| Ctrl+Maj+W | Fermer le panneau |
| Alt+Flèches, Ctrl+Maj+Flèches | Aller au panneau voisin |
| Ctrl+Maj+J / Ctrl+Maj+H | Panneau suivant / précédent |
| Ctrl+Maj+Alt+Flèches | Redimensionner le panneau |
| Ctrl+Alt+E | Égaliser les panneaux |
| Ctrl+Maj+X | Zoomer le panneau (plein onglet) |
| Ctrl+Maj+B | Déplacer le panneau dans un nouvel onglet |
| Ctrl+Maj+C, Ctrl+Inser | Copier |
| Ctrl+Maj+V, Maj+Inser | Coller |
| Ctrl+Maj+S | Coller la sélection (PRIMARY) |
| Ctrl+Maj+A | Tout sélectionner |
| Ctrl+Maj+F, puis Entrée / Maj+Entrée / F3 | Rechercher (regex) |
| Ctrl+Maj+K | Effacer l'écran et l'historique |
| Ctrl+Alt+R | Réinitialiser le terminal |
| Maj+PagePréc / PageSuiv | Défiler d'une page |
| Ctrl+Maj+Y / Ctrl+Maj+D | Défiler d'une ligne |
| Ctrl+Maj+Début / Fin | Début / fin de l'historique |
| Ctrl+Maj++, Ctrl++ / Ctrl+Maj+-, Ctrl+- / Ctrl+0 | Police plus grande / plus petite / par défaut |
| Ctrl+Maj+Alt++ / - | Opacité |
| Ctrl+Maj+G | Thème suivant |
| Ctrl+Maj+N | Nouvelle fenêtre |
| Ctrl+Maj+M / F11 | Agrandir / plein écran |
| Ctrl+Maj+F5 | Recharger la configuration |
| Ctrl+Maj+Q | Quitter (avec confirmation si un programme tourne) |

**Ctrl+C** est toujours transmis au programme (interrompt un serveur, etc.).

Souris : double-clic = mot, triple-clic = ligne, Alt+glisser = sélection
rectangulaire, clic du milieu = coller, Ctrl+clic = ouvrir un lien,
double-clic sur un séparateur = 50 / 50, clic du milieu sur un onglet = fermer.

## Suggestions de commandes ✨

Pendant que vous tapez, NovaTerm propose la suite de la commande d'après votre
historique (texte grisé en italique après le curseur), comme fish :

| Touche | Action |
|---|---|
| → ou Fin | Accepter la suggestion |
| Ctrl+→ | Accepter un mot |
| Ctrl+↓ / Ctrl+↑ | Choisir dans la liste (si activée) |
| Ctrl+Alt+H | Rechercher dans tout l'historique (Entrée : insérer, Maj+Entrée : exécuter) |

Paramètres → **Suggestions** : activer/désactiver, liste déroulante, touche
d'acceptation (→, → ou Fin, Tab), tri (récent / fréquent), nombre de
caractères avant de suggérer, opacité, sources (zsh, bash), commandes à ne
jamais suggérer.

Fonctionnement : NovaTerm lance zsh avec une petite **intégration shell**
(`~/.cache/novaterm/shell-integration/zsh/`) qui charge votre `~/.zshrc`
normalement puis signale la ligne en cours d'édition. Votre configuration
n'est pas modifiée. Si `zsh-autosuggestions` est déjà installé, NovaTerm le
détecte et s'efface. L'historique reste en mémoire, rien n'est envoyé.

## Curseur et animations

Paramètres → **Curseur** :

- **Forme** : Bloc, Barre ou Souligné. S'applique **immédiatement** à tous les
  panneaux ouverts (pas besoin d'en recréer).
- **Laisser les applications changer la forme** : si coché, vim & co. peuvent
  imposer leur propre curseur ; sinon la forme choisie reste forcée.
- **Animer le curseur** + **Style** :
  - *Clignotement* : apparition/disparition classique.
  - *Fondu* : variation douce d'opacité.
  - *Pulsation* : léger battement vertical.
  - *Rotation verticale* : le curseur se replie sur lui-même comme une carte qui
    tourne (effet « flip »).
- **Déplacement fluide (traînée)** : le curseur glisse vers sa nouvelle position
  avec une traînée estompée (effet « cursor trail / smear », comme Kitty/Neovide).

Paramètres → **Comportement** → **Notifications animées** : des « toasts »
(snackbars) apparaissent en bas au centre (glissement vers le haut + fondu) lors
des actions utiles : copie, collage, split, nouvel onglet, changement de thème,
zoom, effacement…

## Personnalisation de la fenêtre

Paramètres → **Fenêtre** :

- **Décorations personnalisées** : barre de titre dessinée par NovaTerm (logo
  `>_`, titre, boutons réduire / agrandir / fermer) au lieu de celle du système.
  Décoche pour revenir aux décorations du système.
- **Titre** : texte affiché dans la barre (vide = titre du panneau actif).
- **Arrondi des coins** : rayon des coins de la fenêtre (0 = carré).

Souris : glisser la barre de titre pour déplacer, double-clic pour maximiser,
glisser les bords pour redimensionner.

Le reste (thèmes, transparence, image de fond, police, curseur) se règle dans
les autres sections des Paramètres.

## Structure du projet

```
src/
  main.rs      lancement, options de fenêtre
  app.rs       onglets, splits, interception clavier, souris, palette, rendu
  keys.rs      actions et raccourcis configurables
  history.rs   historique zsh/bash, suggestions (récent / fréquent)
  shell.rs     intégration shell zsh + filtre des séquences OSC privées
  fonts.rs     polices système (fontconfig) + police intégrée
  icon.rs      icône dessinée par programme (+ export PNG/SVG)
  terminal.rs  PTY + émulation VT (alacritty_terminal)
  render.rs    rendu du texte (cache de glyphes et de shapes)
  layout.rs    arbre de splits
  input.rs     encodage clavier xterm
  config.rs    configuration TOML
  theme.rs     thèmes de couleurs
  settings.rs  paramètres (pages), éditeur de raccourcis, aide, sélecteur d'image
assets/fonts/  JetBrains Mono (embarquée)
assets/icon/   icône SVG + PNG
assets/novaterm.desktop  entrée de menu
```