# Devenir le terminal par défaut de Windows 11

> État : **non implémenté**. Ce document décrit ce qu'il faut faire, dans quel
> ordre, et surtout comment ne pas rendre la machine inutilisable en chemin.

## Pourquoi ce n'est pas une affaire d'options

Sous Linux, « terminal par défaut » est un réglage qui lance `novaterm -e commande` :
c'est pour cela que `--working-directory` et `-e` suffisent (voir `main.rs`).

Windows 11 fonctionne autrement. Quand un programme console démarre, le système
lance `conhost.exe`, qui **transfère** (handoff) la session à l'application de
terminal enregistrée, via COM. Aucun argument de ligne de commande n'intervient :
une application qui n'implémente pas ce protocole ne peut pas être choisie dans
« Paramètres → Confidentialité et sécurité → Pour les développeurs → Terminal ».

C'est pourquoi l'entrée « Ouvrir dans le Terminal » de l'explorateur reste celle de
Windows Terminal, alors que notre entrée « Ouvrir NovaTerm ici » fonctionne.

## Le protocole

1. NovaTerm expose un **serveur COM hors processus** (`LocalServer32`) avec un CLSID
   qui lui est propre, implémentant `ITerminalHandoff` (version 2 ou 3 selon la
   version de Windows ; l'interface a changé, il faut gérer les deux).
2. La méthode `EstablishPtyHandoff` reçoit :
   - un handle de lecture (sortie du programme console),
   - un handle d'écriture (entrée du programme),
   - un handle de signal (redimensionnement, fermeture),
   - les informations du processus client (PID, ligne de commande, titre).
3. NovaTerm doit alors créer un onglet **à partir de ces handles**, sans lancer de
   shell lui-même.
4. Enregistrement, sous `HKCU\Console\%%Startup` :
   - `DelegationConsole` = CLSID du hôte de console (celui d'OpenConsole),
   - `DelegationTerminal` = CLSID de notre serveur.

## Travail préalable dans NovaTerm

`Pane::spawn` crée toujours un PTY et lance un shell (`terminal.rs`). Le transfert
exige un second chemin : **adopter** un PTY existant.

- Extraire de `Pane::spawn` tout ce qui suit la création du PTY (lecteur, écrivain,
  fil de lecture, émulateur) dans un `Pane::from_parts(reader, writer, resize)`.
- `spawn` devient : créer le PTY, lancer le shell, appeler `from_parts`.
- Le transfert appelle `from_parts` avec les handles reçus, le redimensionnement
  passant par le handle de signal au lieu de `MasterPty::resize`.

Cette étape est **sans risque et testable sous Linux** : c'est une refactorisation à
comportement constant. C'est par là qu'il faut commencer.

## Le danger, et comment l'éviter

Si le serveur COM ne démarre pas, ou si l'interface ne correspond pas à celle
attendue par la version de Windows, **plus aucun programme console ne se lance** :
ni cmd, ni PowerShell, ni l'invite d'un installateur. L'utilisateur se retrouve
coincé, et la réparation passe par l'éditeur de registre.

Règles à respecter :

1. N'écrire `DelegationConsole` / `DelegationTerminal` **qu'après** avoir vérifié,
   dans la même session, que le serveur COM répond (appel de test sur notre propre
   CLSID).
2. Fournir `novaterm-cli.exe --restore-default-terminal`, qui supprime les deux
   valeurs, et le documenter dans le README **avant** de publier la fonctionnalité.
3. Ne jamais poser l'enregistrement à l'installation : seulement sur une action
   explicite de l'utilisateur, avec un avertissement clair.
4. Tester sur une machine jetable (machine virtuelle avec instantané) avant toute
   publication, et sur Windows 10 et 11 : l'interface diffère.

## Ordre conseillé

1. Refactorisation `Pane::from_parts` (sans risque, vérifiable sous Linux).
2. Serveur COM minimal qui journalise les appels, **sans** enregistrement comme
   terminal par défaut — déclenché à la main pour valider le protocole.
3. Création d'un onglet à partir des handles reçus.
4. Enregistrement, avec la vérification préalable et la commande de restauration.
5. Prise en charge des deux versions de l'interface, puis publication.

Les étapes 2 à 5 demandent une machine Windows avec instantanés : elles ne peuvent
pas être écrites « à l'aveugle » depuis une autre plateforme.
