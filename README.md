# Translation Tool

Application Windows de traduction contextuelle, écrite en **Rust avec GPUI**.

Sélectionner un texte → **Ctrl+Alt+T** → aperçu près de la sélection → éditer ou demander une nouvelle proposition → **Remplacer**.

## Fonctionnalités

- Hotkey globale configurable, avec capture des touches et détection des conflits.
- **Quick Translate** : second raccourci configurable (**Ctrl+Alt+Q** par défaut) pour traduire et remplacer directement la sélection en arrière-plan, sans validation.
- Fenêtre compacte et déplaçable, placée près de la sélection via Windows UI Automation ; repli près du curseur lorsque les coordonnées ne sont pas accessibles.
- Positionnement dans la zone utile du moniteur, avec prise en compte du DPI.
- Langue source automatique ou explicite et langue cible explicite, configurables dans les paramètres et dans l’aperçu.
- Traduction éditable, nouvelle proposition du LLM, copie, remplacement et annulation avec Échap.
- Annulation des requêtes à la fermeture ; une réponse obsolète ne remplace pas une traduction plus récente.
- Providers OpenAI, LM Studio, Ollama ou serveur personnalisé compatible `POST /chat/completions`.
- Configuration JSON locale et clés API séparées par endpoint dans le gestionnaire d’identifiants Windows.
- Icône tray : paramètres, activation/désactivation de la hotkey, quitter. Fermer toutes les fenêtres n’arrête pas le processus.
- Option pour lancer l’application dans le tray à l’ouverture de session Windows, désactivée par défaut.

## Prérequis Windows

### 1. Rust

Installer [Rustup](https://rustup.rs/) si nécessaire. Avec Rust déjà installé :

```powershell
rustup update stable
rustup component add rustfmt clippy
rustc --version
cargo --version
```

Le projet utilise le canal **stable** et demande **Rust 1.99 ou plus récent** pour cet ensemble de dépendances. Le fichier `Cargo.lock` fixe leurs versions. `rust-toolchain.toml` sélectionne stable ; si ce canal ou les composants demandés manquent, Rustup peut proposer/télécharger les outils lors d’une commande Cargo. Installe-les toi-même avant de compiler.

### 2. Outils Microsoft

Installer [Visual Studio Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/), avec la charge **Développement Desktop en C++** et :

- MSVC x64/x86 ;
- Windows SDK récent ;
- CMake pour Windows.

Les bibliothèques Spectre peuvent être requises selon la version des dépendances et des Build Tools. Les ajouter via Visual Studio Installer si le linker les réclame. Aucun PostgreSQL, Node.js, navigateur embarqué ou serveur web n’est nécessaire pour cette application.

Ouvrir **Developer PowerShell for VS** si Cargo ne trouve pas le linker Microsoft. Attention à un éventuel `link.exe` provenant de Git/MSYS dans le `PATH` : ce n’est pas le linker MSVC.

Si CMake est installé par Build Tools mais absent du `PATH`, ajouter son dossier `bin` via les variables d’environnement Windows. Exemple pour Build Tools 2026 :

```text
C:\Program Files (x86)\Microsoft Visual Studio\18\BuildTools\Common7\IDE\CommonExtensions\Microsoft\CMake\CMake\bin
```

Les pilotes graphiques doivent permettre le rendu GPU de GPUI. En cas d’échec graphique, mettre à jour les pilotes de la carte.

### 3. Compiler et lancer

Depuis le dossier du projet :

```powershell
cargo run --locked
```

Cargo télécharge les crates puis compile le projet. Le premier build est sensiblement plus long que les suivants : GPUI et ses composants ont de nombreuses dépendances.

Le premier lancement ouvre les paramètres. Les suivants restent dans le tray. Cliquer sur l’icône ou choisir **Paramètres** dans son menu pour configurer l’application.

Pour une build optimisée, sans console :

```powershell
cargo build --release --locked
```

Exécutable : **`target/release/translation-tool.exe`**. Cette première version fournit un exécutable, pas encore un installateur.

Pour le lancement automatique, cocher **Lancer au démarrage de Windows** dans les paramètres puis **Enregistrer**. L’application démarre dans le tray à l’ouverture de la session de l’utilisateur courant, sans droits administrateur. Décocher puis enregistrer retire le lancement automatique. L’option utilise l’entrée `TranslationTool` dans `HKEY_CURRENT_USER\Software\Microsoft\Windows\CurrentVersion\Run`, avec le chemin de l’exécutable courant. Après déplacement de l’exécutable, enregistrer à nouveau depuis son nouvel emplacement. Pour éviter une console au démarrage, activer l’option depuis la build release.

## Configurer le provider

| Provider | URL de base | Modèle |
|---|---|---|
| OpenAI | `https://api.openai.com/v1` | Par défaut `gpt-4.1-mini` ; utiliser un modèle accessible sur ton compte |
| LM Studio | `http://localhost:1234/v1` | Identifiant du modèle chargé dans LM Studio |
| Ollama | `http://localhost:11434/v1` | Nom exact du modèle installé, par exemple `llama3.2` |
| Personnalisé | URL de l’API compatible, avec son préfixe | Identifiant attendu par ce serveur |

L’URL doit être une **URL de base**, sans `/chat/completions` à la fin. Pour un serveur local, activer son service HTTP et charger le modèle avant de tester. La clé est facultative si le serveur n’en demande pas.

Le bouton **Tester la connexion** réalise une vraie petite traduction avec les valeurs du formulaire, sans devoir les enregistrer. Avec OpenAI, cet appel est facturé selon le modèle et ton compte API. Un abonnement ChatGPT ne remplace pas une clé et un accès à l’API.

**Enregistrer** persiste les paramètres et active les deux raccourcis. Ils doivent être différents. Les changements de langues dans l’aperçu concernent seulement la traduction en cours. Si un nouveau raccourci est déjà réservé, les anciens restent actifs. Le menu **Raccourcis actifs** du tray active/désactive les deux ensemble.

Les préréglages proposent une URL et un modèle d’exemple ; les champs restent éditables. Une modification d’URL recharge la clé associée à cet endpoint, afin de ne pas envoyer la clé d’un autre provider. Effacer la clé puis enregistrer supprime l’identifiant de cet endpoint.

Configuration : `%APPDATA%\TranslationTool\TranslationTool\config\settings.json` (chemin Windows calculé par `directories`). Aucun texte traduit ni clé API n’est enregistré dans ce JSON.

## Utiliser l’aperçu

1. Sélectionner du texte dans un éditeur, un champ de navigateur, etc.
2. Appuyer sur la hotkey, puis **relâcher ses touches**.
3. L’aperçu apparaît dès la capture terminée et affiche l’état de traduction.
4. Modifier les langues si nécessaire : la requête précédente est annulée et une nouvelle traduction démarre.
5. Modifier directement la traduction ou cliquer sur **Nouvelle proposition** pour une alternative basée sur le texte original et la proposition actuelle.
6. Cliquer sur **Remplacer** pour revenir à la sélection d’origine, ou **Copier** pour coller manuellement.

**Annuler**, Échap et la fermeture de la fenêtre interrompent la traduction sans remplacer le texte. Un second déclenchement pendant une session active remet son aperçu au premier plan. La fermeture est bloquée pendant le bref remplacement en cours.

### Quick Translate

1. Enregistrer le provider, le modèle, les langues source/cible et le raccourci **Quick Translate** dans les paramètres.
2. Sélectionner du texte puis appuyer sur **Ctrl+Alt+Q** (ou le raccourci configuré) et relâcher ses touches.
3. Rester dans le document avec la même sélection : la traduction remplace automatiquement le texte, sans aperçu ni clic de validation.

Les paramètres enregistrés sont figés au déclenchement. Pendant une traduction rapide, les nouveaux déclenchements sont ignorés. Si un aperçu est déjà ouvert, l’un ou l’autre raccourci le remet au premier plan. Les anciennes configurations reçoivent automatiquement le raccourci par défaut ; un conflit est signalé dans les paramètres.

Une erreur de traduction ouvre les paramètres avec le problème. Si le remplacement échoue (sélection modifiée, changement de fenêtre, etc.), l’aperçu s’ouvre avec la traduction déjà obtenue et l’erreur, pour permettre **Copier** ou un remplacement manuel sans nouvelle requête. Quick Translate ne reprend pas le focus d’une autre application pour coller. Après un remplacement réussi, la traduction reste dans le presse-papiers.

### Intégration avec les autres applications

La capture et le remplacement utilisent les conventions `Ctrl+C` et `Ctrl+V`. Avant de remplacer, l’application vérifie la fenêtre, le processus, le titre du document, le contrôle actif et le texte sélectionné. Lorsque UI Automation l’expose, elle vérifie aussi l’identifiant du contrôle et la position de la sélection dans le document.

La capture sauvegarde le presse-papiers, notamment texte Unicode, HTML, RTF et bitmap, puis le restaure uniquement s’il n’a pas été modifié entretemps. Si un format privé ne peut pas être sauvegardé, UI Automation est utilisée pour lire directement la sélection quand c’est possible ; sinon la capture s’arrête sans modifier le presse-papiers.

Après **Copier** ou **Remplacer**, la traduction **reste dans le presse-papiers**. Il n’existe pas d’accusé de réception universel du collage : restaurer arbitrairement l’ancien contenu après quelques millisecondes pourrait faire coller ce contenu à un éditeur lent.

Le remplacement reste best-effort : certaines applications ne conservent pas leur sélection après un changement de focus, n’exposent pas leurs contrôles accessibles, ou utilisent des raccourcis différents. Les zones en lecture seule ne peuvent pas être remplacées. Une application normale ne peut pas injecter des touches dans une application lancée en administrateur. Dans ces cas, utiliser **Copier**. Le texte remplacé est du **texte brut** ; la mise en forme riche du document n’est pas conservée.

## Stack expliquée

- **Rust** : langage compilé. Le résultat est un exécutable natif Windows.
- **Cargo** : gestion des dépendances (les *crates*), compilation et tests.
- **GPUI Kit 0.7** : moteur GPUI et composants prêts à l’emploi. Les vues s’écrivent en Rust, sans HTML ni WebView. Kit fixe un snapshot GPUI compatible avec ses composants.
- **Reqwest + Tokio** : client HTTP et runtime asynchrone. Les appels au LLM se déroulent hors du thread de rendu ; les opérations Windows bloquantes utilisent le pool de workers et sont sérialisées pour le presse-papiers.
- **Serde** : conversion des structures Rust en JSON pour la configuration et l’API.
- **`global-hotkey` / `tray-icon`** : raccourci global et zone de notification, sur la boucle d’événements Win32 de GPUI.
- **`windows`** : accès typé aux APIs Win32, à UI Automation et au presse-papiers. Le code `unsafe` est limité à cette intégration système.
- **`keyring`** : stockage des clés dans le gestionnaire d’identifiants de Windows.
- **`tracing`** : diagnostics de développement, sans journaliser les sélections ou les clés.

### Organisation

```text
src/
  main.rs                  Démarrage GPUI, runtime et durée de vie
  app.rs                   Coordination des fenêtres, tray et captures
  settings.rs              Configuration JSON et identifiants Windows
  translation.rs           Client OpenAI-compatible et prompts
  ui/
    settings.rs            Formulaire et capture de hotkey
    preview.rs             Aperçu éditable et tâches annulables
    mod.rs                 Construction des champs et sélecteurs
  platform/
    windows.rs             Capture, identité, placement et remplacement
    hotkey.rs              Réservation transactionnelle de la hotkey
    startup.rs             Lancement à l’ouverture de session Windows
    tray.rs                Icône et menu
```

## Vérification

```powershell
cargo fmt --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked
cargo run --locked -- --smoke-test
cargo run --locked -- --smoke-test-quick
```

Les tests HTTP utilisent un vrai serveur simulé sur localhost ; aucune clé ni API externe n’est nécessaire. Le smoke test ouvre les paramètres, un aperçu avec texte fictif et le tray pendant trois secondes. Il vérifie que la réponse d’un provider simulé sur localhost est visible, puis quitte, sans enregistrer de paramètres ni contacter un provider externe. L’aperçu de diagnostic n’a aucune destination de remplacement.

Le smoke test Quick Translate vérifie le parcours en arrière-plan puis l’aperçu de récupération : la destination fictive refuse le collage et la traduction obtenue reste visible avec l’erreur, sans second appel au provider.

Un test Windows supplémentaire lance son propre éditeur natif dans un processus séparé et vérifie capture, collage et restauration du presse-papiers (texte et bitmap). Il nécessite une session graphique interactive, prend temporairement le focus et sauvegarde/restaure le presse-papiers ; ne pas utiliser d’autre application pendant son exécution :

```powershell
cargo test --locked --lib clipboard_and_native_edit_round_trip -- --ignored --nocapture --test-threads=1
```

Tests manuels de bout en bout recommandés :

- Bloc-notes : capturer une sélection, éditer la traduction et remplacer ; annuler sans changement.
- Quick Translate : vérifier le remplacement direct avec les langues enregistrées ; changer la sélection ou la fenêtre pendant la requête et vérifier l’aperçu de récupération ; déclencher plusieurs fois pour vérifier qu’une seule traduction est effectuée.
- Raccourcis : configurer deux combinaisons distinctes, les échanger, tester un conflit et activer/désactiver les deux depuis le tray.
- Démarrage Windows : cocher l’option et enregistrer, rouvrir les paramètres puis vérifier le lancement dans le tray après une nouvelle ouverture de session ; décocher et enregistrer pour vérifier sa suppression.
- Champ éditable du navigateur et éditeur de code : vérifier le retour de focus.
- Changer la sélection ou le document pendant l’aperçu : le remplacement doit être refusé si le changement est détecté.
- Changer rapidement de langue puis fermer l’aperçu : aucune ancienne réponse ne doit rouvrir la fenêtre.
- Provider arrêté, modèle inconnu, clé invalide : erreur affichée, interface toujours utilisable.
- Moniteur secondaire, bord de l’écran et DPI 125/150 % : vérifier le placement.
- Fermer les fenêtres, puis réutiliser la hotkey ; quitter réellement via le tray.

## Références

- [GPUI](https://www.gpui.rs/)
- [GPUI Kit](https://github.com/longbridge/gpui-kit)
- [Build Windows de Zed](https://github.com/zed-industries/zed/blob/main/docs/src/development/windows.md)
- [SendInput et limites Windows](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-sendinput)
- [OleDuplicateData](https://learn.microsoft.com/en-us/windows/win32/api/ole2/nf-ole2-oleduplicatedata)
