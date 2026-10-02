# Translation Tool

Application Windows de traduction et de correction contextuelles, écrite en **Rust avec GPUI**.

Sélectionner un texte → **Ctrl+F12** pour traduire ou **Ctrl+F11** pour corriger → aperçu près de la sélection → éditer ou demander une nouvelle proposition → **Remplacer**.

## Fonctionnalités

- Quatre hotkeys globales configurables, avec capture des touches et détection des conflits.
- **Quick Translate** (**Ctrl+Maj+F12**) et **Quick Check** (**Ctrl+Maj+F11**) : traitement et remplacement directs de la sélection, sans validation.
- Correction orthographique, grammaticale et de ponctuation dans la langue du texte, avec cinq modes : **Correction fidèle** (défaut), **Plus fluide**, **Professionnel**, **Décontracté**, **Concis**.
- Fenêtre d’état en bas à droite du moniteur du document, sans prise de focus : capture, traitement, remplacement, succès ou erreur. Pour les parcours avec validation, l’aperçu prend le relais après la capture.
- Fenêtre compacte et déplaçable, placée près de la sélection via Windows UI Automation ; repli près du curseur lorsque les coordonnées ne sont pas accessibles.
- Positionnement dans la zone utile du moniteur, avec prise en compte du DPI.
- Langue source automatique ou explicite et langue cible explicite, configurables dans les paramètres et dans l’aperçu.
- Traduction éditable, nouvelle proposition du LLM, copie, remplacement et annulation avec Échap.
- Annulation des requêtes à la fermeture ; une réponse obsolète ne remplace pas une traduction plus récente.
- Providers OpenAI, LM Studio, Ollama ou serveur personnalisé compatible `POST /chat/completions`.
- Configuration JSON locale et clés API séparées par endpoint dans le gestionnaire d’identifiants Windows.
- Icône tray : paramètres, activation/désactivation de la hotkey, quitter. Fermer toutes les fenêtres n’arrête pas le processus.
- Option pour lancer l’application dans le tray à l’ouverture de session Windows, désactivée par défaut.
- Paramètres organisés en cinq catégories avec navigation latérale : **Général**, **Provider IA**, **Traduction**, **Correction** et **Raccourcis**.
- Thèmes **Clair**, **Sombre** et **Système** (défaut), appliqués et enregistrés immédiatement pour toutes les fenêtres.

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

## Paramètres et apparence

La navigation latérale regroupe les réglages par catégorie. Les modifications du formulaire sont conservées lors des changements de rubrique. Les boutons **Fermer** et **Enregistrer** restent accessibles en bas de la page.

Dans **Général**, le choix **Clair / Sombre / Système** est appliqué et enregistré immédiatement, indépendamment des autres modifications du formulaire. **Système** suit le thème de Windows, y compris ses changements pendant l’exécution. Les anciennes configurations utilisent ce mode par défaut.

Les raccourcis sont éditables dans **Traduction** et **Correction**, ainsi que dans la rubrique centralisée **Raccourcis**. Ces emplacements partagent les mêmes valeurs ; cliquer sur **Enregistrer** active les changements. Le bouton **Tester la connexion** se trouve dans **Provider IA**.

## Configurer le provider

| Provider | URL de base | Modèle |
|---|---|---|
| OpenAI | `https://api.openai.com/v1` | Par défaut `gpt-4.1-mini` ; utiliser un modèle accessible sur ton compte |
| LM Studio | `http://localhost:1234/v1` | Identifiant du modèle chargé dans LM Studio |
| Ollama | `http://localhost:11434/v1` | Nom exact du modèle installé, par exemple `llama3.2` |
| Personnalisé | URL de l’API compatible, avec son préfixe | Identifiant attendu par ce serveur |

L’URL doit être une **URL de base**, sans `/chat/completions` à la fin. Pour un serveur local, activer son service HTTP et charger le modèle avant de tester. La clé est facultative si le serveur n’en demande pas.

Le bouton **Tester la connexion** réalise une vraie petite traduction avec les valeurs du formulaire, sans devoir les enregistrer. Avec OpenAI, cet appel est facturé selon le modèle et ton compte API. Un abonnement ChatGPT ne remplace pas une clé et un accès à l’API.

**Enregistrer** persiste les paramètres et active les quatre raccourcis. Ils doivent tous être différents. Les changements de langues ou de mode dans l’aperçu concernent seulement la session en cours. Si un nouveau raccourci est déjà réservé, les anciens restent actifs. Le menu **Raccourcis actifs** du tray active/désactive les quatre ensemble.

| Action | Raccourci par défaut |
|---|---|
| Traduction avec aperçu | **Ctrl+F12** |
| Quick Translate | **Ctrl+Maj+F12** |
| Correction avec aperçu | **Ctrl+F11** |
| Quick Check | **Ctrl+Maj+F11** |

Les configurations existantes conservent leurs raccourcis de traduction enregistrés. Les nouveaux champs absents reçoivent les valeurs par défaut ; un conflit avec un raccourci existant est signalé dans les paramètres.

Les préréglages proposent une URL et un modèle d’exemple ; les champs restent éditables. Une modification d’URL recharge la clé associée à cet endpoint, afin de ne pas envoyer la clé d’un autre provider. Effacer la clé puis enregistrer supprime l’identifiant de cet endpoint.

Configuration : `%APPDATA%\TranslationTool\TranslationTool\config\settings.json` (chemin Windows calculé par `directories`). Aucun texte traduit ni clé API n’est enregistré dans ce JSON.

## Utiliser l’aperçu

1. Sélectionner du texte dans un éditeur, un champ de navigateur, etc.
2. Appuyer sur la hotkey, puis **relâcher ses touches**.
3. La fenêtre d’état apparaît pendant la capture ; l’aperçu prend le relais dès la capture terminée et affiche l’état de traduction.
4. Modifier les langues si nécessaire : la requête précédente est annulée et une nouvelle traduction démarre.
5. Modifier directement la traduction ou cliquer sur **Nouvelle proposition** pour une alternative basée sur le texte original et la proposition actuelle.
6. Cliquer sur **Remplacer** pour revenir à la sélection d’origine, ou **Copier** pour coller manuellement.

**Annuler**, Échap et la fermeture de la fenêtre interrompent la traduction sans remplacer le texte. Un second déclenchement pendant une session active remet son aperçu au premier plan. La fermeture est bloquée pendant le bref remplacement en cours.

### Quick Translate

1. Enregistrer le provider, le modèle, les langues source/cible et le raccourci **Quick Translate** dans les paramètres.
2. Sélectionner du texte puis appuyer sur **Ctrl+Maj+F12** (ou le raccourci configuré) et relâcher ses touches.
3. Rester dans le document avec la même sélection : la traduction remplace automatiquement le texte, sans clic de validation. La fenêtre d’état indique le traitement et le remplacement sans prendre le focus.

Les paramètres enregistrés sont figés au déclenchement. Pendant la capture ou un traitement rapide, les nouveaux déclenchements sont ignorés. Si un aperçu est déjà ouvert, chacun des quatre raccourcis le remet au premier plan.

Après le remplacement, une confirmation apparaît pendant deux secondes puis disparaît. Une erreur de capture, de traitement ou de remplacement reste visible pendant cinq secondes dans la fenêtre d’état, avec **Paramètres** et **Fermer**, puis disparaît automatiquement. Le message revient à la ligne et la hauteur de la fenêtre s’adapte au contenu ; les messages longs restent accessibles par défilement. Si le remplacement échoue (sélection modifiée, changement de fenêtre, etc.), l’aperçu s’ouvre avec le résultat déjà obtenu et l’erreur, pour permettre **Copier** ou un remplacement manuel sans nouvelle requête. Cet aperçu reste disponible après la disparition de la fenêtre d’état. Les parcours rapides ne reprennent pas le focus d’une autre application pour coller. Après un remplacement réussi, le résultat reste dans le presse-papiers.

### Correction et Quick Check

**Ctrl+F11** ouvre un aperçu de correction. Un groupe de petits boutons **Fidèle / Fluide / Pro / Décontracté / Concis** remplace les sélecteurs de langues ; le mode actif est mis en évidence et les infobulles affichent les noms complets. Les paramètres utilisent les mêmes boutons pour les modes par défaut. La correction détecte la langue du texte et ne traduit pas. Changer de mode annule la requête précédente et repart du texte original. Le résultat peut être édité, copié ou remplacé ; **Nouvelle proposition** demande une nouvelle révision sans imposer de reformulations inutiles en mode fidèle. Échap ou **Annuler** ferme l’aperçu et annule la requête.

| Mode | Comportement |
|---|---|
| Correction fidèle | Corrige orthographe, grammaire et ponctuation en préservant le ton, le registre et les formulations autant que possible ; laisse les passages corrects inchangés |
| Plus fluide | Reformulation légère pour améliorer la lisibilité, en conservant le ton |
| Professionnel | Ton soigné adapté aux échanges de travail |
| Décontracté | Ton naturel et informel |
| Concis | Formulations plus courtes sans perte des informations essentielles |

Tous les modes demandent au modèle de préserver le sens, les paragraphes et la mise en forme, sans inventer d’informations. La qualité de correction dépend du modèle configuré.

Les paramètres définissent indépendamment le mode par défaut de l’aperçu et celui de **Quick Check** ; tous deux démarrent avec **Correction fidèle**. **Ctrl+Maj+F11** utilise le mode Quick Check enregistré et remplace directement la sélection, avec le même retour d’état et le même aperçu de récupération que Quick Translate.

### Intégration avec les autres applications

La capture et le remplacement utilisent les conventions `Ctrl+C` et `Ctrl+V`. Avant de remplacer, l’application vérifie la fenêtre, le processus, le titre du document, le contrôle actif et le texte sélectionné. Les différentes représentations des retours à la ligne (CR, LF et CRLF), notamment entre le moteur Word d’Outlook Classic et le presse-papiers, sont considérées comme équivalentes ; les autres caractères et le nombre de paragraphes restent vérifiés. Lorsque UI Automation l’expose, elle vérifie aussi l’identifiant du contrôle et la position de la sélection dans le document.

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
  translation.rs           Client OpenAI-compatible et prompts de traduction/correction
  ui/
    settings.rs            Formulaire et capture de hotkey
    preview.rs             Aperçu traduction/correction et tâches annulables
    status.rs              Fenêtre d’état non activante
    mod.rs                 Construction des champs et sélecteurs
  platform/
    windows.rs             Capture, identité, placement et remplacement
    hotkey.rs              Réservation transactionnelle des quatre hotkeys
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
cargo run --locked -- --smoke-test-correction
cargo run --locked -- --smoke-test-quick-check
```

Les tests HTTP utilisent un vrai serveur simulé sur localhost ; aucune clé ni API externe n’est nécessaire. Les smoke tests ouvrent les paramètres, un aperçu avec texte fictif et le tray pendant trois secondes, ou sept secondes pour les parcours rapides afin de vérifier la fermeture automatique de la fenêtre d’erreur. Ils vérifient que la réponse d’un provider simulé sur localhost est visible, puis quittent, sans enregistrer de paramètres ni contacter un provider externe. L’aperçu de diagnostic n’a aucune destination de remplacement.

Les quatre smoke tests vérifient aussi que la fenêtre d’état est visible, conserve le focus de la fenêtre d’origine et refuse l’activation à la souris. Les tests de correction vérifient l’aperçu avec une réponse fictive dans la langue source. Les parcours rapides vérifient le retour à la ligne, la hauteur compacte, les boutons visibles et la fermeture automatique de la fenêtre d’erreur, ainsi que l’aperçu de récupération : la destination fictive refuse le collage et le résultat obtenu reste visible sans second appel au provider.

Les smoke tests mesurent également le layout réel des aperçus : la ligne des sélecteurs doit rester compacte et l’éditeur doit récupérer l’espace vertical disponible.

Un test Windows supplémentaire lance son propre éditeur natif dans un processus séparé et vérifie capture, collage, maintien du focus avec une fenêtre d’état visible et restauration du presse-papiers (texte et bitmap). Il nécessite une session graphique interactive, prend temporairement le focus et sauvegarde/restaure le presse-papiers ; ne pas utiliser d’autre application pendant son exécution :

```powershell
cargo test --locked --lib clipboard_and_native_edit_round_trip -- --ignored --nocapture --test-threads=1
```

Tests manuels de bout en bout recommandés :

- Bloc-notes : capturer une sélection, éditer la traduction et remplacer ; annuler sans changement.
- Quick Translate : vérifier le remplacement direct avec les langues enregistrées ; changer la sélection ou la fenêtre pendant la requête et vérifier l’aperçu de récupération ; déclencher plusieurs fois pour vérifier qu’une seule traduction est effectuée.
- Correction : vérifier les cinq modes, le maintien de la langue, le changement rapide de mode, l’édition et l’annulation ; vérifier que Quick Check utilise son mode indépendant.
- État : vérifier capture, traitement, remplacement et confirmation brève ; cliquer sur la fenêtre pendant un parcours rapide et vérifier que le document conserve le focus ; tester une erreur persistante et sa fermeture.
- Raccourcis : configurer quatre combinaisons distinctes, les échanger, tester un conflit et activer/désactiver les quatre depuis le tray.
- Démarrage Windows : cocher l’option et enregistrer, rouvrir les paramètres puis vérifier le lancement dans le tray après une nouvelle ouverture de session ; décocher et enregistrer pour vérifier sa suppression.
- Paramètres : changer de catégorie avec des champs modifiés, vérifier leur conservation et la synchronisation des raccourcis entre les rubriques.
- Thème : choisir Clair puis Sombre avec un aperçu ouvert, vérifier toutes les fenêtres et la persistance après redémarrage ; choisir Système puis changer l’apparence Windows. Modifier un autre champ avant de changer le thème et fermer sans enregistrer : seul le thème doit être conservé.
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
