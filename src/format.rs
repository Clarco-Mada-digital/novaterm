//! Mise en forme de la sortie d'une commande, pour la « vue » d'un bloc.
//!
//! Principe : **la sortie brute n'est jamais modifiée**. Un terminal doit rendre
//! exactement ce que le programme a écrit — « copier la sortie » doit donner le
//! texte d'origine, sinon les copier-coller vers un fichier ou un ticket seraient
//! faussés. Ce module ne fait que produire une *seconde* représentation, affichée
//! à la demande dans une fenêtre, à côté de la sortie d'origine.
//!
//! Pour l'instant : JSON (objet, tableau, ou un objet par ligne comme en produisent
//! beaucoup de journaux). Le reste est affiché tel quel.

/// Nature d'un fragment de texte, pour le colorer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    /// Nom de propriété (`"clé":`), propriété CSS, clé YAML, attribut HTML.
    Key,
    /// Chaîne de caractères.
    Text,
    Number,
    /// `true`, `false`, `null`, `None`…
    Literal,
    /// Mot-clé du langage, balise HTML.
    Keyword,
    Comment,
    /// Accolades, crochets, virgules, deux-points.
    Punct,
    /// Texte ordinaire (noms de variables, de fonctions…).
    Plain,
    // --- markdown ---
    /// Titre (`# Titre`).
    Heading,
    /// Mis en gras (`**texte**`).
    Bold,
    /// Mis en italique (`*texte*`).
    Italic,
    /// Code entre accents graves.
    Code,
    /// Texte d'un lien.
    Link,
}

/// Fragment coloré d'une ligne.
#[derive(Clone, Debug, PartialEq)]
pub struct Span {
    pub text: String,
    pub role: Role,
}

/// Retire les séquences ANSI : un `jq` déjà coloré reste analysable.
fn strip_ansi(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\x1b' {
            out.push(c);
            continue;
        }
        // CSI … lettre, ou OSC … BEL : on saute jusqu'au terminateur.
        match chars.peek() {
            Some('[') => {
                for n in chars.by_ref() {
                    if n.is_ascii_alphabetic() {
                        break;
                    }
                }
            }
            Some(']') => {
                for n in chars.by_ref() {
                    if n == '\x07' {
                        break;
                    }
                }
            }
            _ => {
                chars.next();
            }
        }
    }
    out
}

/// Taille au-delà de laquelle on n'essaie même pas d'analyser (sortie d'un `cat`
/// sur un gros fichier : l'analyse coûterait plus que le service rendu).
pub const MAX_FORMAT_BYTES: usize = 8 * 1024 * 1024;

/// Est-ce un objet ou un tableau JSON valide ? (`serde_json` sert d'arbitre ;
/// une valeur isolée — nombre, chaîne — n'a rien à mettre en forme.)
fn is_json_document(text: &str) -> bool {
    matches!(serde_json::from_str::<serde_json::Value>(text), Ok(v) if v.is_object() || v.is_array())
}

/// Ré-indente du JSON **sans rien interpréter** : seuls des espaces et des retours
/// à la ligne sont ajoutés.
///
/// Passer par `serde_json::Value` aurait été plus court, mais cela trie les clés par
/// ordre alphabétique, arrondit les très grands nombres et perd les clés en double.
/// Pour une *vue*, le texte doit rester celui du programme, à la présentation près.
fn reindent_json(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len() * 2);
    let mut depth = 0usize;
    let mut i = 0;
    // Prochain caractère utile (hors espaces), pour laisser « {} » compact.
    let next_meaningful = |from: usize| chars[from..].iter().find(|c| !c.is_whitespace()).copied();
    let newline = |out: &mut String, depth: usize| {
        out.push('\n');
        out.push_str(&"  ".repeat(depth));
    };
    while i < chars.len() {
        let c = chars[i];
        match c {
            '"' => {
                // Chaîne : recopiée telle quelle, échappements compris.
                let start = i;
                i += 1;
                while i < chars.len() {
                    match chars[i] {
                        '\\' => i += 2,
                        '"' => {
                            i += 1;
                            break;
                        }
                        _ => i += 1,
                    }
                }
                out.extend(&chars[start..i.min(chars.len())]);
            }
            '{' | '[' => {
                out.push(c);
                i += 1;
                let close = if c == '{' { '}' } else { ']' };
                if next_meaningful(i) == Some(close) {
                    // Conteneur vide : « {} » reste sur une ligne.
                    out.push(close);
                    i = chars[i..].iter().position(|x| *x == close).map(|p| i + p + 1).unwrap_or(chars.len());
                } else {
                    depth += 1;
                    newline(&mut out, depth);
                }
            }
            '}' | ']' => {
                depth = depth.saturating_sub(1);
                newline(&mut out, depth);
                out.push(c);
                i += 1;
            }
            ',' => {
                out.push(',');
                i += 1;
                newline(&mut out, depth);
            }
            ':' => {
                out.push_str(": ");
                i += 1;
            }
            c if c.is_whitespace() => i += 1,
            _ => {
                out.push(c);
                i += 1;
            }
        }
    }
    out
}

/// Met en forme une sortie JSON. `None` si ce n'en est pas, ou si c'est trop gros.
///
/// Gère aussi le « JSON par lignes » (un objet complet par ligne), courant dans les
/// journaux de `docker`, `kubectl` ou des serveurs web.
pub fn format_json(raw: &str) -> Option<String> {
    if raw.len() > MAX_FORMAT_BYTES {
        return None;
    }
    let clean = strip_ansi(raw);
    let trimmed = clean.trim();
    if trimmed.is_empty() {
        return None;
    }
    // Document unique.
    if is_json_document(trimmed) {
        return Some(reindent_json(trimmed));
    }
    // Un document par ligne : il en faut au moins deux pour que ce soit crédible.
    let lines: Vec<&str> = trimmed.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
    if lines.len() < 2 {
        return None;
    }
    let mut out = String::new();
    for line in &lines {
        if !is_json_document(line) {
            return None;
        }
        out.push_str(&reindent_json(line));
        out.push('\n');
    }
    Some(out)
}

/// Découpe une ligne de JSON mis en forme en fragments colorés.
/// Point d'entrée direct du moteur, pour les tests.
#[cfg(test)]
pub fn highlight_json_line(line: &str) -> Vec<Span> {
    let mut carry = Carry::None;
    highlight_line(line, &syntax(Lang::Json), &mut carry)
}

// ------------------------------------------------------- coloration des langages

/// Langage reconnu pour la coloration.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lang {
    Json,
    Python,
    JavaScript,
    Rust,
    Shell,
    Css,
    Html,
    Yaml,
    Toml,
    Markdown,
}

/// Règles d'un langage. Volontairement simples : on vise une lecture agréable,
/// pas une analyse exacte — un terminal n'est pas un éditeur de code.
struct Syntax {
    keywords: &'static [&'static str],
    literals: &'static [&'static str],
    line_comment: &'static [&'static str],
    block_comment: Option<(&'static str, &'static str)>,
    quotes: &'static [char],
    /// Python : `"""…"""` sur plusieurs lignes.
    triple_quotes: bool,
    /// Un identifiant suivi de « : » est une clé (CSS, YAML).
    key_before_colon: bool,
}

const PYTHON_KW: &[&str] = &[
    "def", "class", "return", "if", "elif", "else", "for", "while", "import", "from", "as", "try", "except", "finally",
    "with", "lambda", "and", "or", "not", "in", "is", "pass", "raise", "yield", "global", "nonlocal", "assert", "del",
    "async", "await", "break", "continue",
];
const JS_KW: &[&str] = &[
    "function", "const", "let", "var", "return", "if", "else", "for", "while", "class", "new", "import", "export",
    "from", "async", "await", "try", "catch", "finally", "throw", "typeof", "instanceof", "this", "switch", "case",
    "break", "continue", "default", "extends", "interface", "type", "enum", "implements",
];
const RUST_KW: &[&str] = &[
    "fn", "let", "mut", "struct", "enum", "impl", "trait", "pub", "use", "mod", "match", "if", "else", "for", "while",
    "loop", "return", "self", "Self", "where", "const", "static", "async", "await", "move", "ref", "dyn", "crate",
    "type", "unsafe", "break", "continue", "in", "as",
];
const SHELL_KW: &[&str] = &[
    "if", "then", "else", "elif", "fi", "for", "while", "until", "do", "done", "case", "esac", "function", "return",
    "export", "local", "source", "echo", "exit", "set", "unset", "readonly", "shift",
];

fn syntax(lang: Lang) -> Syntax {
    let base = Syntax {
        keywords: &[],
        literals: &[],
        line_comment: &[],
        block_comment: None,
        quotes: &['"', '\''],
        triple_quotes: false,
        key_before_colon: false,
    };
    match lang {
        Lang::Json => Syntax { literals: &["true", "false", "null"], key_before_colon: true, quotes: &['"'], ..base },
        Lang::Python => Syntax {
            keywords: PYTHON_KW,
            literals: &["True", "False", "None", "self"],
            line_comment: &["#"],
            triple_quotes: true,
            ..base
        },
        Lang::JavaScript => Syntax {
            keywords: JS_KW,
            literals: &["true", "false", "null", "undefined"],
            line_comment: &["//"],
            block_comment: Some(("/*", "*/")),
            quotes: &['"', '\'', '`'],
            ..base
        },
        Lang::Rust => Syntax {
            keywords: RUST_KW,
            literals: &["true", "false", "None", "Some", "Ok", "Err"],
            line_comment: &["//"],
            block_comment: Some(("/*", "*/")),
            ..base
        },
        Lang::Shell => Syntax { keywords: SHELL_KW, line_comment: &["#"], ..base },
        Lang::Css => Syntax { block_comment: Some(("/*", "*/")), key_before_colon: true, ..base },
        // Le HTML est traité à part (balises et attributs), mais garde ses commentaires.
        Lang::Html => Syntax { block_comment: Some(("<!--", "-->")), ..base },
        Lang::Yaml => Syntax {
            literals: &["true", "false", "null", "yes", "no"],
            line_comment: &["#"],
            key_before_colon: true,
            ..base
        },
        Lang::Toml => Syntax { literals: &["true", "false"], line_comment: &["#"], key_before_colon: true, ..base },
        // Le markdown a son propre analyseur ; cette entrée n'est jamais utilisée.
        Lang::Markdown => base,
    }
}

/// Langage d'après la commande qui a produit la sortie.
///
/// Uniquement pour les commandes qui **affichent un fichier** : `cat script.py`
/// montre du Python, alors que `python script.py` montre ce que le programme écrit,
/// qui n'a aucune raison d'être du Python.
pub fn detect_lang(command: &str) -> Option<Lang> {
    const AFFICHEURS: &[&str] = &["cat", "bat", "batcat", "head", "tail", "less", "more", "view", "type", "get-content", "gc"];
    let mut mots = command.split_whitespace();
    let prog = mots.next()?.rsplit(['/', '\\']).next()?.trim_end_matches(".exe").to_ascii_lowercase();
    if !AFFICHEURS.contains(&prog.as_str()) {
        return None;
    }
    // Premier argument qui n'est pas une option et qui porte une extension connue.
    mots.filter(|m| !m.starts_with('-')).find_map(lang_from_extension)
}

/// Langage d'après l'extension d'un nom de fichier.
pub fn lang_from_extension(name: &str) -> Option<Lang> {
    let ext = name.rsplit('.').next()?.to_ascii_lowercase();
    Some(match ext.as_str() {
        "json" => Lang::Json,
        "py" | "pyw" => Lang::Python,
        "js" | "mjs" | "cjs" | "ts" | "tsx" | "jsx" => Lang::JavaScript,
        "rs" => Lang::Rust,
        "sh" | "bash" | "zsh" | "fish" => Lang::Shell,
        "css" | "scss" | "sass" => Lang::Css,
        "html" | "htm" | "xml" | "svg" | "vue" => Lang::Html,
        "yaml" | "yml" => Lang::Yaml,
        "toml" => Lang::Toml,
        "md" | "markdown" => Lang::Markdown,
        _ => return None,
    })
}

/// Nombre de lignes au-delà duquel on renonce à colorer : au-delà, le coût du
/// calcul et de la mémoire dépasse le service rendu.
pub const MAX_HIGHLIGHT_LINES: usize = 20_000;

/// État reporté d'une ligne sur la suivante (commentaire ou chaîne non terminés).
#[derive(Clone, Copy, PartialEq)]
enum Carry {
    None,
    Block,
    Triple(char),
}

/// Colore un texte entier, ligne par ligne.
///
/// Le calcul est fait **une seule fois**, à l'ouverture de la vue : le rendu ne
/// fait ensuite que piocher les lignes visibles.
pub fn highlight(text: &str, lang: Lang) -> Vec<Vec<Span>> {
    let lines: Vec<&str> = text.lines().collect();
    if lines.len() > MAX_HIGHLIGHT_LINES {
        return Vec::new();
    }
    if lang == Lang::Markdown {
        return highlight_markdown(&lines);
    }
    let syn = syntax(lang);
    let mut carry = Carry::None;
    lines
        .into_iter()
        .map(|line| {
            if lang == Lang::Html {
                highlight_html_line(line, &syn, &mut carry)
            } else {
                highlight_line(line, &syn, &mut carry)
            }
        })
        .collect()
}

/// Ajoute un fragment, en fusionnant avec le précédent s'il est de même nature.
fn push(spans: &mut Vec<Span>, text: String, role: Role) {
    if text.is_empty() {
        return;
    }
    match spans.last_mut() {
        Some(last) if last.role == role => last.text.push_str(&text),
        _ => spans.push(Span { text, role }),
    }
}

/// Lit une chaîne à partir de `i` (qui pointe sur le guillemet ouvrant).
/// Retourne la position après la chaîne, et si elle reste ouverte en fin de ligne.
fn read_string(chars: &[char], mut i: usize, quote: char) -> (usize, bool) {
    i += 1;
    while i < chars.len() {
        match chars[i] {
            '\\' => i += 2,
            c if c == quote => return (i + 1, false),
            _ => i += 1,
        }
    }
    (chars.len(), true)
}

fn highlight_line(line: &str, syn: &Syntax, carry: &mut Carry) -> Vec<Span> {
    let chars: Vec<char> = line.chars().collect();
    let mut spans = Vec::new();
    let mut i = 0;
    // Suite d'un commentaire ou d'une chaîne commencés plus haut.
    match *carry {
        Carry::Block => {
            let fin = syn.block_comment.map(|(_, f)| f).unwrap_or("*/");
            match line.find(fin) {
                Some(p) => {
                    let coupe = line[..p + fin.len()].chars().count();
                    push(&mut spans, chars[..coupe].iter().collect(), Role::Comment);
                    i = coupe;
                    *carry = Carry::None;
                }
                None => {
                    push(&mut spans, line.to_string(), Role::Comment);
                    return spans;
                }
            }
        }
        Carry::Triple(q) => {
            let fin: String = std::iter::repeat_n(q, 3).collect();
            match line.find(&fin) {
                Some(p) => {
                    let coupe = line[..p + 3].chars().count();
                    push(&mut spans, chars[..coupe].iter().collect(), Role::Text);
                    i = coupe;
                    *carry = Carry::None;
                }
                None => {
                    push(&mut spans, line.to_string(), Role::Text);
                    return spans;
                }
            }
        }
        Carry::None => {}
    }

    while i < chars.len() {
        let reste: String = chars[i..].iter().collect();
        // Commentaire de fin de ligne.
        if let Some(marque) = syn.line_comment.iter().find(|m| reste.starts_with(**m)) {
            let _ = marque;
            push(&mut spans, reste, Role::Comment);
            break;
        }
        // Commentaire de bloc.
        if let Some((debut, fin)) = syn.block_comment {
            if let Some(apres) = reste.strip_prefix(debut) {
                match apres.find(fin) {
                    Some(p) => {
                        let long = debut.len() + p + fin.len();
                        let coupe = reste[..long].chars().count();
                        push(&mut spans, chars[i..i + coupe].iter().collect(), Role::Comment);
                        i += coupe;
                    }
                    None => {
                        push(&mut spans, reste, Role::Comment);
                        *carry = Carry::Block;
                        break;
                    }
                }
                continue;
            }
        }
        let c = chars[i];
        // Chaîne, éventuellement sur trois guillemets.
        if syn.quotes.contains(&c) {
            if syn.triple_quotes && chars.get(i + 1) == Some(&c) && chars.get(i + 2) == Some(&c) {
                let fin: String = std::iter::repeat_n(c, 3).collect();
                match reste[3..].find(&fin) {
                    Some(p) => {
                        let coupe = reste[..3 + p + 3].chars().count();
                        push(&mut spans, chars[i..i + coupe].iter().collect(), Role::Text);
                        i += coupe;
                    }
                    None => {
                        push(&mut spans, reste, Role::Text);
                        *carry = Carry::Triple(c);
                        break;
                    }
                }
                continue;
            }
            let (fin, ouverte) = read_string(&chars, i, c);
            // Une chaîne suivie de « : » est un nom de propriété, pas une valeur.
            let suite = chars[fin..].iter().find(|c| !c.is_whitespace());
            let role = if syn.key_before_colon && suite == Some(&':') { Role::Key } else { Role::Text };
            push(&mut spans, chars[i..fin].iter().collect(), role);
            i = fin;
            if ouverte {
                break;
            }
            continue;
        }
        // Nombre, signe négatif compris (« -1.5e3 », « -2px »).
        if c.is_ascii_digit() || (c == '-' && chars.get(i + 1).is_some_and(char::is_ascii_digit)) {
            let debut = i;
            if c == '-' {
                i += 1;
            }
            while i < chars.len() && (chars[i].is_ascii_alphanumeric() || chars[i] == '.' || chars[i] == '_') {
                i += 1;
            }
            push(&mut spans, chars[debut..i].iter().collect(), Role::Number);
            continue;
        }
        // Mot : mot-clé, littéral, clé, ou texte ordinaire.
        if c.is_alphabetic() || c == '_' || c == '-' || c == '$' || c == '@' {
            let debut = i;
            while i < chars.len() && (chars[i].is_alphanumeric() || matches!(chars[i], '_' | '-' | '$' | '@')) {
                i += 1;
            }
            let mot: String = chars[debut..i].iter().collect();
            let suite = chars[i..].iter().find(|c| !c.is_whitespace());
            let role = if syn.keywords.contains(&mot.as_str()) {
                Role::Keyword
            } else if syn.literals.contains(&mot.as_str()) {
                Role::Literal
            } else if syn.key_before_colon && matches!(suite, Some(&':') | Some(&'=')) {
                // « clé: valeur » (YAML, CSS) et « clé = valeur » (TOML).
                Role::Key
            } else {
                Role::Plain
            };
            push(&mut spans, mot, role);
            continue;
        }
        push(&mut spans, c.to_string(), Role::Punct);
        i += 1;
    }
    spans
}

/// HTML et XML : balises, attributs, valeurs, commentaires.
fn highlight_html_line(line: &str, syn: &Syntax, carry: &mut Carry) -> Vec<Span> {
    let chars: Vec<char> = line.chars().collect();
    let mut spans = Vec::new();
    let mut i = 0;
    if *carry == Carry::Block {
        match line.find("-->") {
            Some(p) => {
                let coupe = line[..p + 3].chars().count();
                push(&mut spans, chars[..coupe].iter().collect(), Role::Comment);
                i = coupe;
                *carry = Carry::None;
            }
            None => {
                push(&mut spans, line.to_string(), Role::Comment);
                return spans;
            }
        }
    }
    while i < chars.len() {
        let reste: String = chars[i..].iter().collect();
        if reste.starts_with("<!--") {
            match reste.find("-->") {
                Some(p) => {
                    let coupe = reste[..p + 3].chars().count();
                    push(&mut spans, chars[i..i + coupe].iter().collect(), Role::Comment);
                    i += coupe;
                }
                None => {
                    push(&mut spans, reste, Role::Comment);
                    *carry = Carry::Block;
                    break;
                }
            }
            continue;
        }
        if chars[i] == '<' {
            push(&mut spans, "<".to_string(), Role::Punct);
            i += 1;
            // Nom de la balise (« / » d'une balise fermante compris).
            if chars.get(i) == Some(&'/') {
                push(&mut spans, "/".to_string(), Role::Punct);
                i += 1;
            }
            let debut = i;
            while i < chars.len() && (chars[i].is_alphanumeric() || matches!(chars[i], '_' | '-' | ':' | '!')) {
                i += 1;
            }
            push(&mut spans, chars[debut..i].iter().collect(), Role::Keyword);
            // Attributs jusqu'au « > ».
            while i < chars.len() && chars[i] != '>' {
                let c = chars[i];
                if syn.quotes.contains(&c) {
                    let (fin, _) = read_string(&chars, i, c);
                    push(&mut spans, chars[i..fin].iter().collect(), Role::Text);
                    i = fin;
                } else if c.is_alphabetic() || c == '_' {
                    let d = i;
                    while i < chars.len() && (chars[i].is_alphanumeric() || matches!(chars[i], '_' | '-' | ':')) {
                        i += 1;
                    }
                    push(&mut spans, chars[d..i].iter().collect(), Role::Key);
                } else {
                    push(&mut spans, c.to_string(), Role::Punct);
                    i += 1;
                }
            }
            // Le « > » fait partie de la balise, pas du texte qui suit.
            if i < chars.len() {
                push(&mut spans, ">".to_string(), Role::Punct);
                i += 1;
            }
            continue;
        }
        // Texte hors balise : d'un seul tenant jusqu'au prochain « < ».
        let debut = i;
        while i < chars.len() && chars[i] != '<' {
            i += 1;
        }
        push(&mut spans, chars[debut..i].iter().collect(), Role::Plain);
    }
    spans
}

// ------------------------------------------------------------------- markdown

/// Langage d'un bloc de code clôturé (« ```python »).
fn lang_from_name(nom: &str) -> Option<Lang> {
    Some(match nom.trim().to_ascii_lowercase().as_str() {
        "python" | "py" => Lang::Python,
        "javascript" | "js" | "typescript" | "ts" | "jsx" | "tsx" => Lang::JavaScript,
        "rust" | "rs" => Lang::Rust,
        "bash" | "sh" | "shell" | "zsh" | "console" => Lang::Shell,
        "json" => Lang::Json,
        "html" | "xml" | "svg" => Lang::Html,
        "css" | "scss" => Lang::Css,
        "yaml" | "yml" => Lang::Yaml,
        "toml" => Lang::Toml,
        _ => return None,
    })
}

/// Rend du markdown lisible.
///
/// Règle suivie : les **marques d'emphase** (`**`, `*`, accents graves, `#` de
/// titre) sont masquées puisqu'elles ne servent qu'à la mise en forme, mais toute
/// **information** est conservée — l'adresse d'un lien reste affichée. Et le texte
/// d'origine est toujours à un clic, dans l'autre onglet de la fenêtre.
fn highlight_markdown(lines: &[&str]) -> Vec<Vec<Span>> {
    let mut out = Vec::with_capacity(lines.len());
    // Bloc de code en cours : son langage éventuel, et l'état du coloriseur.
    let mut fence: Option<Option<Lang>> = None;
    let mut carry = Carry::None;
    for line in lines {
        let sans_marge = line.trim_start();
        if sans_marge.starts_with("```") || sans_marge.starts_with("~~~") {
            if fence.is_some() {
                fence = None;
            } else {
                fence = Some(lang_from_name(sans_marge.trim_start_matches(['`', '~'])));
                carry = Carry::None;
            }
            out.push(vec![Span { text: (*line).to_string(), role: Role::Punct }]);
            continue;
        }
        if let Some(lang) = fence {
            // Dans un bloc clôturé, on colore avec le langage annoncé s'il est connu.
            out.push(match lang {
                Some(l) => highlight_line(line, &syntax(l), &mut carry),
                None => vec![Span { text: (*line).to_string(), role: Role::Code }],
            });
            continue;
        }
        out.push(markdown_line(line));
    }
    aligner_tableaux(&mut out);
    out
}

/// Aligne les tableaux markdown en colonnes.
///
/// Comme l'indentation du JSON, cela n'ajoute que des espaces : le texte d'origine
/// reste accessible dans l'autre onglet. Sans cela, un tableau de README est une
/// bouillie de barres verticales.
fn aligner_tableaux(lignes: &mut [Vec<Span>]) {
    let largeur = |spans: &[Span]| spans.iter().map(|s| s.text.chars().count()).sum::<usize>();
    let est_tableau = |spans: &[Span]| {
        let t: String = spans.iter().map(|s| s.text.as_str()).collect();
        let t = t.trim();
        t.starts_with('|') && t.len() > 1
    };
    let mut debut = 0;
    while debut < lignes.len() {
        if !est_tableau(&lignes[debut]) {
            debut += 1;
            continue;
        }
        let mut fin = debut;
        while fin < lignes.len() && est_tableau(&lignes[fin]) {
            fin += 1;
        }
        // Largeur de chaque colonne sur l'ensemble du tableau.
        let mut largeurs: Vec<usize> = Vec::new();
        for ligne in &lignes[debut..fin] {
            for (i, cellule) in cellules(ligne).iter().enumerate() {
                let l = largeur(cellule);
                match largeurs.get_mut(i) {
                    Some(max) => *max = (*max).max(l),
                    None => largeurs.push(l),
                }
            }
        }
        for ligne in &mut lignes[debut..fin] {
            let cols = cellules(ligne);
            let separateur = cols.iter().all(|c| {
                let t: String = c.iter().map(|s| s.text.as_str()).collect();
                !t.trim().is_empty() && t.trim().chars().all(|c| c == '-' || c == ':' || c == ' ')
            });
            let mut neuf = Vec::new();
            for (i, cellule) in cols.iter().enumerate() {
                let cible = largeurs.get(i).copied().unwrap_or(0);
                neuf.push(Span { text: if i == 0 { "│ ".into() } else { " │ ".into() }, role: Role::Punct });
                if separateur {
                    neuf.push(Span { text: "─".repeat(cible), role: Role::Punct });
                } else {
                    neuf.extend(cellule.iter().cloned());
                    let reste = cible.saturating_sub(largeur(cellule));
                    if reste > 0 {
                        neuf.push(Span { text: " ".repeat(reste), role: Role::Punct });
                    }
                }
            }
            neuf.push(Span { text: " │".into(), role: Role::Punct });
            *ligne = neuf;
        }
        debut = fin;
    }
}

/// Découpe une ligne de tableau en cellules, sans couper dans un fragment de code.
fn cellules(ligne: &[Span]) -> Vec<Vec<Span>> {
    let mut out: Vec<Vec<Span>> = Vec::new();
    let mut courante: Vec<Span> = Vec::new();
    let mut commence = false;
    for span in ligne {
        // Les barres ne séparent que hors code (« `a|b` » reste une seule cellule).
        if span.role != Role::Code && span.text.contains('|') {
            let mut morceaux = span.text.split('|').peekable();
            while let Some(m) = morceaux.next() {
                if !m.is_empty() {
                    courante.push(Span { text: m.to_string(), role: span.role });
                }
                if morceaux.peek().is_some() {
                    if commence {
                        out.push(std::mem::take(&mut courante));
                    } else {
                        courante.clear();
                        commence = true;
                    }
                }
            }
        } else {
            courante.push(span.clone());
        }
    }
    // Ce qui suit la dernière barre est le reste de la ligne, pas une cellule.
    out.into_iter().map(|c| rogner(&c)).collect()
}

/// Retire les espaces en bordure d'une cellule.
fn rogner(cellule: &[Span]) -> Vec<Span> {
    let mut v: Vec<Span> = cellule.to_vec();
    if let Some(p) = v.first_mut() {
        p.text = p.text.trim_start().to_string();
    }
    if let Some(d) = v.last_mut() {
        d.text = d.text.trim_end().to_string();
    }
    v.retain(|s| !s.text.is_empty());
    v
}

/// Une ligne de markdown, hors bloc de code.
fn markdown_line(line: &str) -> Vec<Span> {
    let marge: String = line.chars().take_while(|c| c.is_whitespace()).collect();
    let corps = &line[marge.len()..];
    let mut spans = Vec::new();
    // Ligne de balisage HTML pur (fréquent dans les README : <div>, <table>, <img>).
    if corps.starts_with('<') {
        let mut carry = Carry::None;
        return highlight_html_line(line, &syntax(Lang::Html), &mut carry);
    }
    if !marge.is_empty() {
        spans.push(Span { text: marge, role: Role::Punct });
    }
    // Titre : les dièses ne servent qu'à la mise en forme.
    let dieses = corps.chars().take_while(|c| *c == '#').count();
    if (1..=6).contains(&dieses) && corps.chars().nth(dieses) == Some(' ') {
        spans.push(Span { text: corps[dieses + 1..].to_string(), role: Role::Heading });
        return spans;
    }
    // Filet horizontal.
    let net = corps.trim_end();
    if net.len() >= 3 && (net.chars().all(|c| c == '-') || net.chars().all(|c| c == '*') || net.chars().all(|c| c == '_')) {
        spans.push(Span { text: corps.to_string(), role: Role::Punct });
        return spans;
    }
    // Citation.
    if let Some(reste) = corps.strip_prefix("> ") {
        spans.push(Span { text: "▏ ".to_string(), role: Role::Keyword });
        spans.extend(markdown_inline(reste));
        return spans;
    }
    // Liste à puces ou numérotée.
    let puce = ["- ", "* ", "+ "].iter().find(|p| corps.starts_with(**p));
    if let Some(p) = puce {
        spans.push(Span { text: "• ".to_string(), role: Role::Keyword });
        spans.extend(markdown_inline(&corps[p.len()..]));
        return spans;
    }
    let chiffres = corps.chars().take_while(char::is_ascii_digit).count();
    if chiffres > 0 && corps[chiffres..].starts_with(". ") {
        spans.push(Span { text: corps[..chiffres + 2].to_string(), role: Role::Keyword });
        spans.extend(markdown_inline(&corps[chiffres + 2..]));
        return spans;
    }
    spans.extend(markdown_inline(corps));
    spans
}

/// Emphase commençant en `i` : position de la marque fermante, sa taille, et le rôle.
///
/// On suit les règles de CommonMark, sans quoi la vue abîmerait le texte : une
/// multiplication (« 2 * 3 * 4 ») passerait en italique, et surtout un nom comme
/// `ma_variable_ici` perdrait ses tirets bas.
fn emphase(chars: &[char], i: usize) -> Option<(usize, usize, Role)> {
    const MARQUES: [(&str, Role); 4] = [("**", Role::Bold), ("__", Role::Bold), ("*", Role::Italic), ("_", Role::Italic)];
    let bord = |c: Option<&char>| c.is_none_or(|c| c.is_whitespace() || c.is_ascii_punctuation());
    for (marque, role) in MARQUES {
        let taille = marque.len();
        let m: Vec<char> = marque.chars().collect();
        if i + taille > chars.len() || chars[i..i + taille] != m[..] {
            continue;
        }
        // Marque ouvrante : jamais suivie d'une espace.
        if chars.get(i + taille).is_none_or(|c| c.is_whitespace()) {
            continue;
        }
        // Le tiret bas ne vaut qu'en bordure de mot.
        if marque.starts_with('_') && !bord(i.checked_sub(1).and_then(|p| chars.get(p))) {
            continue;
        }
        let mut j = i + taille + 1;
        while j + taille <= chars.len() {
            if chars[j..j + taille] == m[..]
                && !chars[j - 1].is_whitespace()
                && (!marque.starts_with('_') || bord(chars.get(j + taille)))
            {
                return Some((j, taille, role));
            }
            j += 1;
        }
    }
    None
}

/// Emphase, code, images, liens et balises HTML à l'intérieur d'une ligne.
fn markdown_inline(text: &str) -> Vec<Span> {
    markdown_inline_depth(text, 0)
}

/// Profondeur maximale d'imbrication (« [**gras**](lien) » en demande deux).
/// Un badge — une image dans un lien — en demande deux également.
const MAX_INLINE_DEPTH: usize = 4;

fn markdown_inline_depth(text: &str, depth: usize) -> Vec<Span> {
    let chars: Vec<char> = text.chars().collect();
    let mut spans = Vec::new();
    let mut ordinaire = String::new();
    let mut i = 0;
    // Cherche la marque fermante à partir de `from` ; None si elle manque.
    let fermeture = |from: usize, marque: char| -> Option<usize> {
        (from..chars.len()).find(|j| chars[*j] == marque)
    };
    // Fermeture **équilibrée** : indispensable pour un badge « [![alt](img)](lien) »,
    // où le premier « ] » rencontré n'est pas celui du lien extérieur.
    let equilibre = |from: usize, ouvre: char, ferme: char| -> Option<usize> {
        let mut profondeur = 1usize;
        for (j, c) in chars.iter().enumerate().skip(from) {
            if *c == ouvre {
                profondeur += 1;
            } else if *c == ferme {
                profondeur -= 1;
                if profondeur == 0 {
                    return Some(j);
                }
            }
        }
        None
    };
    while i < chars.len() {
        let c = chars[i];
        // Code entre accents graves : son contenu n'est jamais réinterprété.
        if c == '`' {
            if let Some(fin) = fermeture(i + 1, '`') {
                push(&mut spans, std::mem::take(&mut ordinaire), Role::Plain);
                push(&mut spans, chars[i + 1..fin].iter().collect(), Role::Code);
                i = fin + 1;
                continue;
            }
        }
        // Balise HTML : colorée comme du HTML plutôt que laissée en texte brut.
        // Les README en contiennent beaucoup (<div>, <img>, <kbd>, <table>…).
        if c == '<' && chars.get(i + 1).is_some_and(|n| n.is_ascii_alphabetic() || *n == '/' || *n == '!') {
            if let Some(fin) = fermeture(i + 1, '>') {
                push(&mut spans, std::mem::take(&mut ordinaire), Role::Plain);
                let balise: String = chars[i..=fin].iter().collect();
                let mut carry = Carry::None;
                spans.extend(highlight_html_line(&balise, &syntax(Lang::Html), &mut carry));
                i = fin + 1;
                continue;
            }
        }
        // Gras ou italique. Le gras se teste en premier : « ** » contient « * ».
        if let Some((fin, taille, role)) = emphase(&chars, i) {
            push(&mut spans, std::mem::take(&mut ordinaire), Role::Plain);
            let interieur: String = chars[i + taille..fin].iter().collect();
            // Le contenu est réanalysé : « **un `code`** » garde son code.
            for mut span in sous_analyse(&interieur, depth) {
                // L'emphase l'emporte sur le texte ordinaire, sans écraser un rôle plus précis.
                if span.role == Role::Plain {
                    span.role = role;
                }
                push(&mut spans, span.text, span.role);
            }
            i = fin + taille;
            continue;
        }
        // Image « ![texte](adresse) », y compris dans un lien (badge).
        let image = c == '!' && chars.get(i + 1) == Some(&'[');
        if image || c == '[' {
            let ouvrante = if image { i + 1 } else { i };
            if let Some(fin_texte) = equilibre(ouvrante + 1, '[', ']') {
                if chars.get(fin_texte + 1) == Some(&'(') {
                    if let Some(fin_url) = equilibre(fin_texte + 2, '(', ')') {
                        push(&mut spans, std::mem::take(&mut ordinaire), Role::Plain);
                        let interieur: String = chars[ouvrante + 1..fin_texte].iter().collect();
                        let url: String = chars[fin_texte + 2..fin_url].iter().collect();
                        if image {
                            // L'image n'est pas affichée ici : on la signale clairement.
                            // Marqueur en texte simple : la police à chasse fixe de la
                            // vue n'a pas de glyphe d'emoji, qui s'afficherait en carré.
                            push(&mut spans, "[img] ".to_string(), Role::Keyword);
                        }
                        for mut span in sous_analyse(&interieur, depth) {
                            if span.role == Role::Plain {
                                span.role = Role::Link;
                            }
                            push(&mut spans, span.text, span.role);
                        }
                        push(&mut spans, format!(" ({url})"), Role::Punct);
                        i = fin_url + 1;
                        continue;
                    }
                }
            }
        }
        ordinaire.push(c);
        i += 1;
    }
    push(&mut spans, ordinaire, Role::Plain);
    spans
}

/// Analyse le contenu d'une emphase ou d'un lien, sans boucler indéfiniment.
fn sous_analyse(texte: &str, depth: usize) -> Vec<Span> {
    if depth + 1 >= MAX_INLINE_DEPTH {
        return vec![Span { text: texte.to_string(), role: Role::Plain }];
    }
    markdown_inline_depth(texte, depth + 1)
}

// ------------------------------------------------------------------ recherche

/// Occurrence d'une recherche : numéro de ligne et position en **caractères**
/// (et non en octets, pour découper les fragments sans se soucier de l'UTF-8).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Match {
    pub line: usize,
    pub start: usize,
}

/// Cherche `query` dans des lignes, sans distinction de casse.
///
/// Les occurrences qui se chevauchent sont ignorées : « aa » dans « aaa » ne
/// compte qu'une fois, comme dans un éditeur.
pub fn search(lines: &[String], query: &str) -> Vec<Match> {
    if query.is_empty() {
        return Vec::new();
    }
    let besoin: Vec<char> = query.to_lowercase().chars().collect();
    let mut out = Vec::new();
    for (num, ligne) in lines.iter().enumerate() {
        let foin: Vec<char> = ligne.to_lowercase().chars().collect();
        if foin.len() < besoin.len() {
            continue;
        }
        let mut i = 0;
        while i + besoin.len() <= foin.len() {
            if foin[i..i + besoin.len()] == besoin[..] {
                out.push(Match { line: num, start: i });
                i += besoin.len();
            } else {
                i += 1;
            }
        }
    }
    out
}

/// Découpe des fragments colorés pour isoler les occurrences : chaque morceau
/// est accompagné d'un drapeau « fait partie d'une occurrence ».
///
/// Appelé seulement pour les lignes **visibles** : le coût est négligeable.
pub fn split_on_matches(spans: &[Span], starts: &[usize], len: usize) -> Vec<(Span, bool)> {
    if starts.is_empty() || len == 0 {
        return spans.iter().map(|s| (s.clone(), false)).collect();
    }
    let mut out = Vec::new();
    let mut pos = 0usize; // position en caractères depuis le début de la ligne
    for span in spans {
        let chars: Vec<char> = span.text.chars().collect();
        let mut i = 0;
        while i < chars.len() {
            let absolu = pos + i;
            // Ce caractère appartient-il à une occurrence ?
            let dedans = starts.iter().any(|d| absolu >= *d && absolu < d + len);
            let mut j = i + 1;
            while j < chars.len() {
                let a = pos + j;
                if starts.iter().any(|d| a >= *d && a < d + len) != dedans {
                    break;
                }
                j += 1;
            }
            out.push((Span { text: chars[i..j].iter().collect(), role: span.role }, dedans));
            i = j;
        }
        pos += chars.len();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roles(line: &str) -> Vec<(Role, String)> {
        highlight_json_line(line).into_iter().map(|s| (s.role, s.text)).collect()
    }

    #[test]
    fn formats_objects_and_arrays() {
        let pretty = format_json(r#"{"a":1,"b":[true,null]}"#).unwrap();
        assert!(pretty.contains("\n"), "le JSON doit être indenté : {pretty}");
        assert!(pretty.starts_with('{') && pretty.trim_end().ends_with('}'));
        assert!(format_json("[1,2,3]").is_some());
        // Ni JSON, ni valeur isolée : rien à mettre en forme.
        assert!(format_json("bonjour").is_none());
        assert!(format_json("42").is_none());
        assert!(format_json("\"texte\"").is_none());
        assert!(format_json("").is_none());
        assert!(format_json("{cassé").is_none());
    }

    /// La mise en forme n'ajoute que des espaces : ordre des clés, précision des
    /// nombres et clés en double restent ceux du programme.
    #[test]
    fn formatting_preserves_the_original_content() {
        let pretty = format_json(r#"{"z":1,"a":2}"#).unwrap();
        let z = pretty.find("\"z\"").unwrap();
        let a = pretty.find("\"a\"").unwrap();
        assert!(z < a, "l'ordre des clés doit être conservé :\n{pretty}");

        // Un entier trop grand pour un flottant garderait ses décimales faussées
        // s'il était relu puis réécrit.
        let big = r#"{"id":123456789012345678901234567890}"#;
        assert!(format_json(big).unwrap().contains("123456789012345678901234567890"));

        // Les clés en double sont conservées (serde_json n'en garderait qu'une).
        let dup = format_json(r#"{"a":1,"a":2}"#).unwrap();
        assert_eq!(dup.matches("\"a\"").count(), 2, "{dup}");

        // En retirant les espaces ajoutés, on retrouve exactement le texte d'origine.
        let source = r#"{"a":[1,2],"b":{"c":"x, y"},"d":{},"e":[]}"#;
        let compact: String = format_json(source).unwrap().chars().filter(|c| !c.is_whitespace()).collect();
        let expected: String = source.chars().filter(|c| !c.is_whitespace()).collect();
        assert_eq!(compact, expected);
    }

    #[test]
    fn indentation_is_readable() {
        let pretty = format_json(r#"{"a":{"b":[1]},"c":{},"d":[]}"#).unwrap();
        assert_eq!(
            pretty,
            "{\n  \"a\": {\n    \"b\": [\n      1\n    ]\n  },\n  \"c\": {},\n  \"d\": []\n}"
        );
    }

    #[test]
    fn formats_one_object_per_line() {
        let logs = "{\"n\":1}\n{\"n\":2}\n";
        let pretty = format_json(logs).unwrap();
        assert_eq!(pretty.matches("\"n\"").count(), 2);
        // Une seule ligne non valide suffit à refuser l'ensemble.
        assert!(format_json("{\"n\":1}\npas du json\n").is_none());
    }

    #[test]
    fn ignores_ansi_colours() {
        // Sortie déjà colorée (par jq par exemple) : toujours analysable.
        let colored = "\x1b[1;39m{\x1b[0m\x1b[34;1m\"a\"\x1b[0m:\x1b[0;39m1\x1b[0m\x1b[1;39m}\x1b[0m";
        assert!(format_json(colored).is_some());
    }

    #[test]
    fn highlights_keys_values_and_punctuation() {
        assert_eq!(
            roles(r#"  "nom": "NovaTerm","#),
            vec![
                (Role::Punct, "  ".into()),
                (Role::Key, "\"nom\"".into()),
                (Role::Punct, ": ".into()),
                (Role::Text, "\"NovaTerm\"".into()),
                (Role::Punct, ",".into()),
            ]
        );
        assert_eq!(roles("  42,"), vec![(Role::Punct, "  ".into()), (Role::Number, "42".into()), (Role::Punct, ",".into())]);
        assert_eq!(roles("  -1.5e3"), vec![(Role::Punct, "  ".into()), (Role::Number, "-1.5e3".into())]);
        assert_eq!(roles("  true"), vec![(Role::Punct, "  ".into()), (Role::Literal, "true".into())]);
        assert_eq!(roles("  null"), vec![(Role::Punct, "  ".into()), (Role::Literal, "null".into())]);
    }

    #[test]
    fn strings_with_escapes_and_colons() {
        // Deux-points *dans* une chaîne : ce n'est pas un nom de propriété.
        assert_eq!(
            roles(r#""url": "https://exemple.fr""#),
            vec![(Role::Key, "\"url\"".into()), (Role::Punct, ": ".into()), (Role::Text, "\"https://exemple.fr\"".into())]
        );
        // Guillemet échappé : la chaîne ne s'arrête pas là.
        assert_eq!(roles(r#""a\"b""#), vec![(Role::Text, r#""a\"b""#.into())]);
    }

    /// Règle essentielle : seule une commande qui **affiche un fichier** permet de
    /// deviner le langage. `python script.py` produit la sortie du programme, qui
    /// n'a aucune raison d'être du Python.
    #[test]
    fn language_detected_only_for_display_commands() {
        assert_eq!(detect_lang("cat script.py"), Some(Lang::Python));
        assert_eq!(detect_lang("head -n 20 src/app.rs"), Some(Lang::Rust));
        assert_eq!(detect_lang("bat style.css"), Some(Lang::Css));
        assert_eq!(detect_lang("Get-Content index.html"), Some(Lang::Html));
        assert_eq!(detect_lang("/usr/bin/cat config.yaml"), Some(Lang::Yaml));
        // Le programme s'exécute : sa sortie n'est pas le fichier.
        assert_eq!(detect_lang("python script.py"), None);
        assert_eq!(detect_lang("node serveur.js"), None);
        assert_eq!(detect_lang("rustc main.rs"), None);
        assert_eq!(detect_lang("./script.sh"), None);
        // Rien à deviner.
        assert_eq!(detect_lang("cat"), None);
        assert_eq!(detect_lang("cat fichier.inconnu"), None);
        assert_eq!(detect_lang(""), None);
    }

    fn roles_of(text: &str, lang: Lang) -> Vec<(Role, String)> {
        highlight(text, lang).into_iter().flatten().map(|s| (s.role, s.text)).collect()
    }

    #[test]
    fn highlights_python() {
        let r = roles_of("def saluer(nom):  # bonjour\n    return f\"salut {nom}\"\n", Lang::Python);
        assert!(r.contains(&(Role::Keyword, "def".into())), "{r:?}");
        assert!(r.contains(&(Role::Plain, "saluer".into())), "{r:?}");
        assert!(r.contains(&(Role::Comment, "# bonjour".into())), "{r:?}");
        assert!(r.contains(&(Role::Keyword, "return".into())), "{r:?}");
        assert!(r.iter().any(|(role, t)| *role == Role::Text && t.contains("salut")), "{r:?}");
    }

    /// Une chaîne sur trois guillemets, ou un commentaire de bloc, continue sur les
    /// lignes suivantes : l'état est reporté d'une ligne à l'autre.
    #[test]
    fn multiline_strings_and_comments() {
        let lignes = highlight("x = \"\"\"début\nmilieu\nfin\"\"\"\napres = 1\n", Lang::Python);
        assert_eq!(lignes.len(), 4);
        assert_eq!(lignes[1], vec![Span { text: "milieu".into(), role: Role::Text }]);
        // Après la fermeture, la coloration normale reprend.
        assert!(lignes[3].iter().any(|s| s.role == Role::Number && s.text == "1"), "{:?}", lignes[3]);

        let lignes = highlight("/* un\ncommentaire */ const a = 1;\n", Lang::JavaScript);
        assert_eq!(lignes[0], vec![Span { text: "/* un".into(), role: Role::Comment }]);
        assert!(lignes[1].iter().any(|s| s.role == Role::Keyword && s.text == "const"), "{:?}", lignes[1]);
    }

    #[test]
    fn highlights_css_and_html() {
        let r = roles_of("body { color: #fff; margin: -2px; }", Lang::Css);
        assert!(r.contains(&(Role::Key, "color".into())), "{r:?}");
        assert!(r.contains(&(Role::Number, "-2px".into())), "{r:?}");

        let r = roles_of("<a href=\"/x\" class=\"lien\">texte</a>", Lang::Html);
        assert!(r.contains(&(Role::Keyword, "a".into())), "{r:?}");
        assert!(r.contains(&(Role::Key, "href".into())), "{r:?}");
        assert!(r.contains(&(Role::Text, "\"/x\"".into())), "{r:?}");
        assert!(r.contains(&(Role::Plain, "texte".into())), "{r:?}");
        assert!(r.contains(&(Role::Keyword, "a".into())), "{r:?}");
    }

    #[test]
    fn highlights_shell_and_yaml() {
        let r = roles_of("if [ -f x ]; then echo 'ok'; fi  # test", Lang::Shell);
        assert!(r.contains(&(Role::Keyword, "if".into())), "{r:?}");
        assert!(r.contains(&(Role::Comment, "# test".into())), "{r:?}");

        let r = roles_of("nom: NovaTerm\nactif: true\n# note", Lang::Yaml);
        assert!(r.contains(&(Role::Key, "nom".into())), "{r:?}");
        assert!(r.contains(&(Role::Literal, "true".into())), "{r:?}");
        assert!(r.contains(&(Role::Comment, "# note".into())), "{r:?}");
    }

    /// Garde-fou de performance : au-delà du plafond, on renonce à colorer plutôt
    /// que d'allouer des fragments pour des centaines de milliers de lignes.
    #[test]
    fn refuses_to_highlight_huge_files() {
        let petit = "x = 1\n".repeat(MAX_HIGHLIGHT_LINES - 1);
        assert!(!highlight(&petit, Lang::Python).is_empty());
        let gros = "x = 1\n".repeat(MAX_HIGHLIGHT_LINES + 1);
        assert!(highlight(&gros, Lang::Python).is_empty());
    }

    /// Mesure du coût de la coloration : elle doit rester imperceptible à
    /// l'ouverture de la vue, même sur un gros fichier.
    #[test]
    fn highlighting_is_fast_enough() {
        let source = "def fonction(valeur, actif=True):\n    # commentaire\n    return \"texte\" if actif else None\n".repeat(5000);
        let lignes = source.lines().count();
        let debut = std::time::Instant::now();
        let colore = highlight(&source, Lang::Python);
        let duree = debut.elapsed();
        assert_eq!(colore.len(), lignes);
        eprintln!("coloration de {lignes} lignes en {duree:?}");
        // Large, pour ne pas dépendre de la machine : on veut seulement détecter
        // un effondrement (comportement quadratique, par exemple).
        assert!(duree.as_millis() < 2000, "coloration trop lente : {duree:?}");
    }

    #[test]
    fn search_is_case_insensitive_and_non_overlapping() {
        let lignes: Vec<String> = ["Bonjour le monde", "bonjour", "rien ici", "aaa"].iter().map(|s| s.to_string()).collect();
        let r = search(&lignes, "bonjour");
        assert_eq!(r, vec![Match { line: 0, start: 0 }, Match { line: 1, start: 0 }]);
        // Occurrences qui se chevauchent : comptées une seule fois.
        assert_eq!(search(&lignes, "aa"), vec![Match { line: 3, start: 0 }]);
        assert!(search(&lignes, "").is_empty());
        assert!(search(&lignes, "introuvable").is_empty());
        // Plusieurs occurrences sur la même ligne.
        let deux = vec!["chat chien chat".to_string()];
        assert_eq!(search(&deux, "chat"), vec![Match { line: 0, start: 0 }, Match { line: 0, start: 11 }]);
    }

    /// La recherche compte en caractères : un accent ou un emoji ne décale rien.
    #[test]
    fn search_counts_characters_not_bytes() {
        let lignes = vec!["éé→cible".to_string()];
        assert_eq!(search(&lignes, "cible"), vec![Match { line: 0, start: 3 }]);
    }

    #[test]
    fn splitting_isolates_matches_without_losing_text() {
        let spans = vec![
            Span { text: "let ".into(), role: Role::Keyword },
            Span { text: "valeur".into(), role: Role::Plain },
        ];
        let morceaux = split_on_matches(&spans, &[4], 3);
        // Le texte reconstitué est identique à l'original.
        let rendu: String = morceaux.iter().map(|(s, _)| s.text.as_str()).collect();
        assert_eq!(rendu, "let valeur");
        // Seuls « val » est marqué, et la couleur d'origine est conservée.
        let marques: Vec<(&str, bool)> = morceaux.iter().map(|(s, m)| (s.text.as_str(), *m)).collect();
        assert_eq!(marques, vec![("let ", false), ("val", true), ("eur", false)]);
        assert_eq!(morceaux[1].0.role, Role::Plain);
        // Sans occurrence, les fragments passent tels quels.
        assert_eq!(split_on_matches(&spans, &[], 3).len(), 2);
    }

    fn md(text: &str) -> Vec<(Role, String)> {
        highlight(text, Lang::Markdown).into_iter().flatten().map(|s| (s.role, s.text)).collect()
    }

    #[test]
    fn markdown_titles_lists_and_quotes() {
        assert_eq!(md("## Mon titre"), vec![(Role::Heading, "Mon titre".into())]);
        // Un dièse sans espace n'est pas un titre.
        assert!(md("#pasuntitre").iter().all(|(r, _)| *r != Role::Heading));
        assert_eq!(md("- premier"), vec![(Role::Keyword, "• ".into()), (Role::Plain, "premier".into())]);
        assert_eq!(md("1. premier"), vec![(Role::Keyword, "1. ".into()), (Role::Plain, "premier".into())]);
        assert_eq!(md("> cité"), vec![(Role::Keyword, "▏ ".into()), (Role::Plain, "cité".into())]);
        assert_eq!(md("---"), vec![(Role::Punct, "---".into())]);
    }

    #[test]
    fn markdown_emphasis_code_and_links() {
        // Les marques d'emphase sont masquées : elles ne portent pas d'information.
        assert_eq!(md("un **gras** ici"), vec![(Role::Plain, "un ".into()), (Role::Bold, "gras".into()), (Role::Plain, " ici".into())]);
        assert_eq!(md("un *penché*"), vec![(Role::Plain, "un ".into()), (Role::Italic, "penché".into())]);
        assert_eq!(md("du `code`"), vec![(Role::Plain, "du ".into()), (Role::Code, "code".into())]);
        // L'adresse d'un lien est une information : elle reste visible.
        assert_eq!(
            md("voir [le site](https://exemple.fr) ici"),
            vec![
                (Role::Plain, "voir ".into()),
                (Role::Link, "le site".into()),
                (Role::Punct, " (https://exemple.fr)".into()),
                (Role::Plain, " ici".into()),
            ]
        );
        // Marque jamais fermée : le texte reste intact, rien n'est avalé.
        assert_eq!(md("deux * etoiles * ici").iter().map(|(_, t)| t.as_str()).collect::<String>(), "deux * etoiles * ici");
        assert_eq!(md("accent ` seul").iter().map(|(_, t)| t.as_str()).collect::<String>(), "accent ` seul");
    }

    /// Les cas d'un vrai README : badges, balises HTML, emphase dans un lien.
    #[test]
    fn markdown_handles_real_world_readme() {
        // Emphase à l'intérieur d'un lien : plus d'astérisques visibles.
        let r = md("[**Télécharger**](https://exemple.fr)");
        assert_eq!(r, vec![(Role::Bold, "Télécharger".into()), (Role::Punct, " (https://exemple.fr)".into())]);

        // Image : signalée comme telle, adresse conservée.
        let r = md("![Logo](assets/logo.png)");
        assert_eq!(r, vec![(Role::Keyword, "[img] ".into()), (Role::Link, "Logo".into()), (Role::Punct, " (assets/logo.png)".into())]);

        // Badge : une image dans un lien. Les deux adresses restent lisibles.
        let r = md("[![Version](https://img.shields.io/v.svg)](https://exemple.fr/releases)");
        let texte: String = r.iter().map(|(_, t)| t.as_str()).collect();
        assert!(texte.contains("[img] "), "{texte}");
        assert!(texte.contains("Version"), "{texte}");
        assert!(texte.contains("https://img.shields.io/v.svg"), "{texte}");
        assert!(texte.contains("https://exemple.fr/releases"), "{texte}");
        assert!(!texte.contains("!["), "les crochets d'image ne doivent plus apparaître : {texte}");

        // Balise HTML en début de ligne : colorée, pas laissée en texte brut.
        let r = md("<img src=\"a.png\" width=\"112\">");
        assert!(r.contains(&(Role::Keyword, "img".into())), "{r:?}");
        assert!(r.contains(&(Role::Key, "src".into())), "{r:?}");

        // Balise au milieu d'une phrase (<kbd> est partout dans les README).
        let r = md("Appuyez sur <kbd>Ctrl</kbd> pour copier");
        assert!(r.contains(&(Role::Keyword, "kbd".into())), "{r:?}");
        assert!(r.contains(&(Role::Plain, "Ctrl".into())), "{r:?}");

        // Code à l'intérieur d'un gras.
        let r = md("**un `bout` ici**");
        assert!(r.contains(&(Role::Code, "bout".into())), "{r:?}");
    }

    /// Un tableau est aligné en colonnes, comme le JSON est indenté.
    #[test]
    fn markdown_tables_are_aligned() {
        let lignes = highlight("| Fichier | Pour qui |\n|---|---|\n| `a.deb` | Ubuntu et Debian |\n", Lang::Markdown);
        let rendu: Vec<String> = lignes.iter().map(|l| l.iter().map(|s| s.text.as_str()).collect()).collect();
        // Toutes les lignes du tableau ont la même largeur une fois alignées.
        assert_eq!(rendu[0].chars().count(), rendu[2].chars().count(), "{rendu:#?}");
        assert_eq!(rendu[1].chars().count(), rendu[2].chars().count(), "{rendu:#?}");
        assert!(rendu[1].contains('─'), "la ligne de séparation doit être un filet : {rendu:#?}");
        assert!(rendu[0].starts_with('│') && rendu[0].ends_with('│'), "{rendu:#?}");
        // Le contenu des cellules est conservé.
        assert!(rendu[2].contains("a.deb") && rendu[2].contains("Ubuntu et Debian"), "{rendu:#?}");
    }

    /// Sans les règles de délimitation, la vue abîmerait du texte ordinaire.
    #[test]
    fn markdown_does_not_mangle_ordinary_text() {
        for texte in [
            "ma_variable_ici reste entière",
            "calcul : 2 * 3 * 4",
            "deux * etoiles * ici",
            "chemin snake_case_long",
            "* ", // une puce suivie de rien
        ] {
            let rendu: String = md(texte).iter().map(|(_, t)| t.as_str()).collect();
            let attendu = texte.strip_prefix("* ").map(|r| format!("• {r}")).unwrap_or_else(|| texte.to_string());
            assert_eq!(rendu, attendu, "texte abîmé : {texte:?}");
        }
    }

    /// Un bloc de code clôturé est coloré avec le langage annoncé.
    #[test]
    fn markdown_fenced_code_uses_the_language() {
        let lignes = highlight("texte\n```python\ndef f():\n    return 1\n```\nfin\n", Lang::Markdown);
        assert_eq!(lignes.len(), 6);
        assert!(lignes[2].iter().any(|s| s.role == Role::Keyword && s.text == "def"), "{:?}", lignes[2]);
        assert!(lignes[3].iter().any(|s| s.role == Role::Number && s.text == "1"), "{:?}", lignes[3]);
        // Après la clôture, on revient au markdown ordinaire.
        assert_eq!(lignes[5], vec![Span { text: "fin".into(), role: Role::Plain }]);
        // Langage inconnu : le bloc reste lisible, en style « code ».
        let lignes = highlight("```\nbrut\n```\n", Lang::Markdown);
        assert_eq!(lignes[1], vec![Span { text: "brut".into(), role: Role::Code }]);
    }

    #[test]
    fn markdown_is_detected_from_the_command() {
        assert_eq!(detect_lang("cat README.md"), Some(Lang::Markdown));
        assert_eq!(detect_lang("head CHANGELOG.markdown"), Some(Lang::Markdown));
    }

    /// Le coloriseur avance toujours : aucune entrée ne doit le faire boucler.
    #[test]
    fn never_loops_on_unusual_input() {
        for lang in [Lang::Json, Lang::Python, Lang::JavaScript, Lang::Rust, Lang::Shell, Lang::Css, Lang::Html, Lang::Yaml, Lang::Toml] {
            for texte in ["", "\"", "'", "/*", "<", "<a href=\"", "\"\"\"", "{[(", "é€𝄞", "\\", "---", "-", "#"] {
                let lignes = highlight(texte, lang);
                // Le texte colorié doit rendre exactement les mêmes caractères.
                let rendu: String = lignes.iter().flatten().map(|s| s.text.as_str()).collect();
                assert_eq!(rendu, texte.replace('\n', ""), "langage {lang:?}, entrée {texte:?}");
            }
        }
    }

    #[test]
    fn refuses_huge_outputs() {
        let big = format!("{{\"a\":\"{}\"}}", "x".repeat(MAX_FORMAT_BYTES));
        assert!(format_json(&big).is_none());
    }
}
