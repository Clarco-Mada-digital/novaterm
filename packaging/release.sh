#!/usr/bin/env bash
# Publie une nouvelle version de NovaTerm sur GitHub, uniquement quand vous le décidez.
#
#   ./packaging/release.sh patch            0.1.0 → 0.1.1   (corrections)
#   ./packaging/release.sh minor            0.1.0 → 0.2.0   (nouveautés)
#   ./packaging/release.sh major            0.1.0 → 1.0.0
#   ./packaging/release.sh 0.3.0-beta.1     version précise (« -beta » = pré-version)
#
# Options :
#   -m "texte"     notes de version (sinon : éditeur, ou liste des commits)
#   --no-tests     ne pas lancer cargo test avant
#   --dry-run      tout vérifier et afficher, sans rien modifier ni envoyer
#   -y             ne pas demander de confirmation
#
# Ce que fait le script :
#   1. vérifie le dépôt (branche, fichiers non enregistrés, tag déjà existant) ;
#   2. met à jour la version dans Cargo.toml / Cargo.lock et lance les tests ;
#   3. crée le commit « Version X » et le tag annoté vX (avec les notes) ;
#   4. après confirmation, envoie le tout sur GitHub.
# GitHub Actions construit alors Linux (x86_64, arm64), Windows et macOS (Intel,
# Apple Silicon) puis publie la Release avec tous les fichiers (~15-25 min).
set -euo pipefail
cd "$(dirname "$0")/.."

red() { printf '\033[1;31m%s\033[0m\n' "$*"; }
blue() { printf '\n\033[1;34m==> %s\033[0m\n' "$*"; }
die() { red "Erreur : $*"; exit 1; }

BUMP=""
NOTES=""
TESTS=1
DRY=0
YES=0
while [ $# -gt 0 ]; do
  case "$1" in
    -m|--notes) NOTES="${2:-}"; shift 2 ;;
    --no-tests) TESTS=0; shift ;;
    --dry-run) DRY=1; shift ;;
    -y|--yes) YES=1; shift ;;
    -h|--help) sed -n '2,25p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    -*) die "option inconnue : $1 (voir --help)" ;;
    *) BUMP="$1"; shift ;;
  esac
done
[ -n "$BUMP" ] || die "indiquez patch, minor, major ou un numéro de version (voir --help)"

# --- Vérifications -----------------------------------------------------------
git rev-parse --is-inside-work-tree >/dev/null 2>&1 || die "pas un dépôt git"
REMOTE_URL=$(git remote get-url origin 2>/dev/null) || die "aucun dépôt distant « origin » (git remote add origin …)"
case "$REMOTE_URL" in *github.com*) ;; *) die "« origin » n'est pas sur GitHub : $REMOTE_URL" ;; esac
BRANCH=$(git symbolic-ref --short HEAD 2>/dev/null) || die "HEAD détaché : placez-vous sur une branche"
if [ -n "$(git status --porcelain)" ]; then
  git status --short | head -20
  die "des modifications ne sont pas enregistrées. Faites d'abord un commit (git add -A && git commit -m \"…\")."
fi
[ -f .github/workflows/release.yml ] || die ".github/workflows/release.yml manquant"

CURRENT=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
base=${CURRENT%%-*}
IFS=. read -r MA MI PA <<<"$base"
case "$BUMP" in
  patch) NEW="$MA.$MI.$((PA + 1))" ;;
  minor) NEW="$MA.$((MI + 1)).0" ;;
  major) NEW="$((MA + 1)).0.0" ;;
  *) NEW="${BUMP#v}" ;;
esac
[[ "$NEW" =~ ^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.]+)?$ ]] || die "version invalide : $NEW (attendu : 1.2.3 ou 1.2.3-beta.1)"
TAG="v$NEW"
git fetch --tags --quiet origin || die "impossible de contacter GitHub"
git rev-parse -q --verify "refs/tags/$TAG" >/dev/null && die "le tag $TAG existe déjà"
if [ "$(git rev-list --count "HEAD..origin/$BRANCH" 2>/dev/null || echo 0)" != "0" ]; then
  die "origin/$BRANCH a des commits que vous n'avez pas : faites git pull d'abord"
fi

PRE=""
[[ "$NEW" == *-* || "$NEW" == 0.0.* ]] && PRE=" (pré-version bêta)"
blue "Version $CURRENT → $NEW$PRE  ·  branche $BRANCH  ·  $REMOTE_URL"

# --- Notes de version ---------------------------------------------------------
LAST_TAG=$(git describe --tags --abbrev=0 2>/dev/null || true)
if [ -z "$NOTES" ]; then
  RANGE=${LAST_TAG:+$LAST_TAG..}HEAD
  CHANGES=$(git log --no-merges --pretty='- %s' $RANGE | head -50)
  NOTES_FILE=$(mktemp)
  {
    echo "NovaTerm $NEW"
    echo
    echo "## Nouveautés"
    echo
    echo "${CHANGES:-- Améliorations diverses}"
  } >"$NOTES_FILE"
  if [ "$YES" = 0 ] && [ "$DRY" = 0 ] && [ -t 0 ]; then
    read -r -p "Modifier les notes de version dans un éditeur ? [o/N] " ans
    if [[ "$ans" =~ ^[oOyY]$ ]]; then "${EDITOR:-nano}" "$NOTES_FILE"; fi
  fi
  NOTES=$(cat "$NOTES_FILE")
  rm -f "$NOTES_FILE"
else
  NOTES=$(printf 'NovaTerm %s\n\n%s\n' "$NEW" "$NOTES")
fi
echo "$NOTES" | sed 's/^/  │ /'

if [ "$DRY" = 1 ]; then
  blue "Simulation (--dry-run) : rien n'a été modifié."
  exit 0
fi

# --- Version + tests ------------------------------------------------------------
blue "Mise à jour de la version"
sed -i.bak "0,/^version = \".*\"/s//version = \"$NEW\"/" Cargo.toml && rm -f Cargo.toml.bak
cargo update --workspace --offline >/dev/null 2>&1 || cargo check --quiet
if [ "$TESTS" = 1 ]; then
  blue "Tests"
  if ! cargo test --quiet; then
    git checkout -- Cargo.toml Cargo.lock
    die "les tests échouent : version annulée"
  fi
fi

blue "Commit et tag $TAG"
git add Cargo.toml Cargo.lock
git commit --quiet -m "Version $NEW"
git tag -a "$TAG" -m "$NOTES"

if [ "$YES" = 0 ]; then
  read -r -p "Envoyer $TAG sur GitHub et lancer la publication ? [o/N] " ans
  if ! [[ "$ans" =~ ^[oOyY]$ ]]; then
    echo "Rien n'a été envoyé. Pour annuler localement :"
    echo "  git tag -d $TAG && git reset --hard HEAD~1"
    echo "Pour publier plus tard : git push origin $BRANCH $TAG"
    exit 0
  fi
fi

blue "Envoi sur GitHub"
git push origin "$BRANCH"
git push origin "$TAG"

REPO=$(echo "$REMOTE_URL" | sed -E 's#(git@github.com:|https://github.com/)##; s#\.git$##')
echo
echo "La construction a commencé : https://github.com/$REPO/actions"
echo "La Release apparaîtra ici :   https://github.com/$REPO/releases/tag/$TAG"
if command -v gh >/dev/null && gh auth status >/dev/null 2>&1; then
  sleep 5
  RUN=$(gh run list --repo "$REPO" --workflow release.yml --branch "$TAG" --limit 1 --json databaseId -q '.[0].databaseId' 2>/dev/null || true)
  if [ -n "$RUN" ]; then
    blue "Suivi de la construction (Ctrl+C pour arrêter le suivi, la construction continue)"
    gh run watch "$RUN" --repo "$REPO" --exit-status && echo "✔ Release $TAG publiée : https://github.com/$REPO/releases/tag/$TAG"
  fi
fi
