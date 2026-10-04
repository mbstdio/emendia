//! English is the source language. French translations live in this catalog.
//! The process-wide preference also covers messages produced by worker threads.
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum UiLanguage {
    #[default]
    System,
    English,
    French,
}

impl UiLanguage {
    pub const ALL: [Self; 3] = [Self::System, Self::English, Self::French];

    pub fn label(self) -> &'static str {
        match self {
            Self::System => t("System"),
            Self::English => "English",
            Self::French => "Français",
        }
    }
}

static FRENCH: AtomicBool = AtomicBool::new(false);

pub fn apply(language: UiLanguage) {
    let french = match language {
        UiLanguage::French => true,
        UiLanguage::English => false,
        UiLanguage::System => system_is_french(),
    };
    FRENCH.store(french, Ordering::Relaxed);
}

fn system_is_french() -> bool {
    #[cfg(target_os = "windows")]
    {
        // The primary language occupies the low ten bits of a Windows LANGID.
        unsafe { windows::Win32::Globalization::GetUserDefaultUILanguage() & 0x03ff == 0x0c }
    }
    #[cfg(target_os = "linux")]
    {
        ["LC_ALL", "LC_MESSAGES", "LANG"]
            .into_iter()
            .find_map(|name| std::env::var(name).ok().filter(|value| !value.is_empty()))
            .is_some_and(|locale| locale.split(['_', '-', '.', '@']).next() == Some("fr"))
    }
}

pub fn t(english: &'static str) -> &'static str {
    translate(english, FRENCH.load(Ordering::Relaxed))
}

macro_rules! catalog {
    ($($english:literal => $french:literal,)*) => {
        const MESSAGES: &[(&str, &str)] = &[$(($english, $french)),*];

        pub fn translate(english: &'static str, french: bool) -> &'static str {
            if french {
                match english { $($english => $french,)* _ => english }
            } else {
                english
            }
        }

        /// Re-localize already generated status messages, including contextual suffixes.
        pub fn localize_message(value: &str) -> String {
            localize_message_for(value, FRENCH.load(Ordering::Relaxed))
        }

        fn localize_message_for(value: &str, french: bool) -> String {
            let matching = MESSAGES.iter().filter_map(|(en, fr)| {
                [*en, *fr].into_iter().filter(|prefix| value.starts_with(prefix))
                    .max_by_key(|prefix| prefix.len()).map(|prefix| (*en, prefix))
            }).max_by_key(|(_, prefix)| prefix.len());
            if let Some((english, prefix)) = matching {
                let suffix = &value[prefix.len()..];
                if let Some(context) = suffix.strip_prefix(": ").filter(|_| english != "Connection successful") {
                    format!("{}: {}", translate(english, french), localize_message_for(context, french))
                } else {
                    format!("{}{}", translate(english, french), suffix)
                }
            } else {
                value.to_owned()
            }
        }
    };
}

catalog! {
        "Enter your OpenAI API key, or choose another provider." => "Saisissez votre clé API OpenAI ou choisissez un autre fournisseur.",
        "Emendia — Welcome" => "Emendia — Bienvenue",
        "Welcome to Emendia" => "Bienvenue dans Emendia",
        "Translate, proofread and refine your text without leaving your application." => "Traduisez, corrigez et améliorez vos textes sans quitter votre application.",
        "Connect your AI provider" => "Connectez votre fournisseur IA",
        "Choose a cloud or local model. Testing the connection is recommended but optional." => "Choisissez un modèle cloud ou local. Le test de connexion est recommandé mais facultatif.",
        "Make Emendia yours" => "Personnalisez Emendia",
        "Choose your default translation languages and proofreading styles." => "Choisissez vos langues de traduction et vos styles de correction par défaut.",
        "Choose your shortcuts" => "Choisissez vos raccourcis",
        "Keep the defaults or record your own shortcuts. Preview lets you review; quick actions replace text directly." => "Gardez les raccourcis proposés ou choisissez les vôtres. L’aperçu permet de vérifier le résultat ; les actions rapides remplacent directement le texte.",
        "Ready to start" => "Prêt à démarrer",
        "Review your configuration, then save it to activate Emendia." => "Vérifiez votre configuration, puis enregistrez-la pour activer Emendia.",
        "You're all set!" => "Tout est prêt !",
        "Open an editor, select text, press your translation shortcut and release its keys. Review the suggestion, then choose Replace or Copy." => "Ouvrez un éditeur, sélectionnez du texte, appuyez sur votre raccourci de traduction, puis relâchez les touches. Vérifiez la proposition et choisissez Remplacer ou Copier.",
        "Interface language does not affect translation languages." => "La langue de l’interface ne change pas les langues de traduction.",
        "For local providers, start the server and load the model before testing. OpenAI requires API credits, separate from a ChatGPT subscription." => "Pour les fournisseurs locaux, démarrez le serveur et chargez le modèle avant le test. OpenAI nécessite des crédits API, distincts d’un abonnement ChatGPT.",
        "Step" => "Étape",
        "of" => "sur",
        "Back" => "Retour",
        "Next" => "Suivant",
        "Get started" => "Commencer",
        "Finish setup" => "Terminer la configuration",
        "Let's go" => "C’est parti",
        "System" => "Système",
        "Light" => "Clair",
        "Dark" => "Sombre",
        "Settings" => "Paramètres",
        "SETTINGS" => "PARAMÈTRES",
        "Emendia — Settings" => "Emendia — Paramètres",
        "Emendia — translation and proofreading" => "Emendia — traduction et correction",
        "Shortcuts enabled" => "Raccourcis actifs",
        "Quit" => "Quitter",
        "General" => "Général",
        "AI Provider" => "Provider IA",
        "Translation" => "Traduction",
        "Proofreading" => "Correction",
        "Shortcuts" => "Raccourcis",
        "Close" => "Fermer",
        "Save" => "Enregistrer",
        "Cancel" => "Annuler",
        "Copy" => "Copier",
        "Replace" => "Remplacer",
        "Retry" => "Réessayer",
        "New suggestion" => "Nouvelle proposition",
        "Show original" => "Voir l’original",
        "Hide original" => "Masquer l’original",
        "Source: " => "Source : ",
        "Target: " => "Cible : ",
        "Automatic" => "Automatique",
        "French" => "Français",
        "English" => "Anglais",
        "German" => "Allemand",
        "Spanish" => "Espagnol",
        "Italian" => "Italien",
        "Portuguese" => "Portugais",
        "Dutch" => "Néerlandais",
        "Japanese" => "Japonais",
        "Chinese" => "Chinois",
        "Korean" => "Coréen",
        "Arabic" => "Arabe",
        "Ukrainian" => "Ukrainien",
        "Faithful correction" => "Correction fidèle",
        "More fluent" => "Plus fluide",
        "Professional" => "Professionnel",
        "Casual" => "Décontracté",
        "Concise" => "Concis",
        "Faithful" => "Fidèle",
        "Fluent" => "Fluide",
        "Translating…" => "Traduction en cours…",
        "Proofreading…" => "Correction en cours…",
        "Capturing text… Release the shortcut keys." => "Capture du texte… Relâche les touches du raccourci.",
        "Generating a new suggestion…" => "Nouvelle proposition en cours…",
        "Ready — you can edit the result before replacing." => "Prêt — tu peux modifier le résultat avant de remplacer.",
        "Processing cancelled." => "Traitement annulé.",
        "The processing service failed." => "Le service de traitement a échoué.",
        "Replacing in the original window…" => "Remplacement dans la fenêtre d’origine…",
        "Replacement failed. Use Copy." => "Le remplacement a échoué. Utilise Copier.",
        "Result copied to the clipboard." => "Résultat copié dans le presse-papiers.",
        "Unable to copy." => "Copie impossible.",
        "Replacing in the document…" => "Remplacement dans le document…",
        "Done — text replaced." => "Terminé — remplacement effectué.",
        "Replacement unavailable" => "Remplacement impossible",
        "Capture" => "Capture",
        "Status window" => "Fenêtre d’état",
        "Opening preview" => "Ouverture de l’aperçu",
        "Customize the application's appearance and startup." => "Personnalise l’apparence et le démarrage de l’application.",
        "Configure the model used to translate and proofread your text." => "Configure le modèle utilisé pour traduire et corriger tes textes.",
        "Choose translation languages and shortcuts." => "Choisis les langues et les raccourcis de traduction.",
        "Adjust proofreading styles and shortcuts." => "Adapte le style de correction et ses raccourcis.",
        "Find all your global shortcuts in one place." => "Retrouve tous tes raccourcis globaux au même endroit.",
        "Appearance" => "Apparence",
        "The theme applies to every application window." => "Le thème s’applique à toutes les fenêtres de l’application.",
        "Theme" => "Thème",
        "Saved immediately. System follows the desktop theme." => "Le choix est enregistré immédiatement. Système suit le thème du bureau.",
        "Interface language" => "Langue de l’interface",
        "Saved immediately. System uses French for a French system locale, English otherwise." => "Enregistré immédiatement. Système utilise le français si la langue du système est le français, l’anglais sinon.",
        "Startup" => "Démarrage",
        "Find the application in the notification area." => "Retrouve l’application dans la zone de notification.",
        "Launch at sign-in" => "Lancer à l’ouverture de session",
        "Save to apply this option. The application starts in the tray when you sign in." => "Enregistre pour appliquer cette option. L’application démarre dans le tray à l’ouverture de ta session.",
        "Closing windows leaves the application in the tray. To exit: tray → Quit." => "Fermer les fenêtres laisse l’application dans le tray. Pour arrêter : tray → Quitter.",
        "Provider" => "Provider",
        "Custom" => "Personnalisé",
        "Base URL" => "URL de base",
        "Include /v1, without /chat/completions." => "Avec /v1, sans /chat/completions.",
        "Model" => "Modèle",
        "API key" => "Clé API",
        "Optional for a local server" => "Facultative pour un serveur local",
        "Stored in the system keyring. Optional for a local server." => "Enregistrée dans le trousseau du système. Facultative pour un serveur local.",
        "Testing…" => "Test en cours…",
        "Test connection" => "Tester la connexion",
        "Default languages" => "Langues par défaut",
        "You can change these languages in the preview." => "Ces langues restent modifiables dans l’aperçu.",
        "Source language" => "Langue source",
        "Target language" => "Langue cible",
        "With preview" => "Avec aperçu",
        "Review or edit the translation before replacing the text." => "Vérifie ou édite la traduction avant de remplacer le texte.",
        "Translates and replaces the selection in the background using the saved languages and provider." => "Traduit et remplace directement la sélection en arrière-plan, avec les langues et le provider enregistrés.",
        "Proofreading keeps the text's language. Faithful mode preserves tone and wording; other modes adjust the style without changing the meaning." => "La correction conserve la langue du texte. Le mode fidèle préserve le ton et les formulations ; les autres modes adaptent le style sans changer le sens.",
        "Review or edit the correction before replacing the text." => "Vérifie ou édite la correction avant de remplacer le texte.",
        "Default mode" => "Mode par défaut",
        "Proofreads and replaces the selection directly in the background." => "Corrige et remplace directement la sélection en arrière-plan.",
        "Preview or direct replacement with Quick Translate." => "Avec aperçu ou remplacement direct avec Quick Translate.",
        "Preview or direct replacement with Quick Check." => "Avec aperçu ou remplacement direct avec Quick Check.",
        "Shortcuts are shared with Translation and Proofreading. They must be distinct. Save to activate them." => "Les raccourcis sont partagés avec les rubriques Traduction et Correction. Ils doivent être distincts. Enregistre pour les activer.",
        "Press the shortcut…" => "Appuie sur le raccourci…",
        "Change shortcut" => "Changer le raccourci",
        "Press the desired combination (Escape to cancel)." => "Appuie sur la combinaison souhaitée (Échap pour annuler).",
        "Shortcut capture cancelled." => "Capture du raccourci annulée.",
        "Shortcut captured. Save to activate it." => "Raccourci capturé. Enregistre pour l’activer.",
        "Translate with preview" => "Traduction avec aperçu",
        "Proofread with preview" => "Correction avec aperçu",
        "Quick Translate" => "Traduction rapide",
        "Quick Check" => "Correction rapide",
        "Unassigned — choose a shortcut" => "Non assigné — choisir un raccourci",
        "Assign a shortcut to every action before saving." => "Réassigne un raccourci à chaque action avant d’enregistrer.",
        "Shortcut {shortcut} was already assigned to {actions}. It is now assigned to {target}. Reassign the actions without a shortcut before saving." => "Le raccourci {shortcut} était déjà assigné à {actions}. Il est maintenant assigné à {target}. Réassigne les actions sans raccourci avant d’enregistrer.",
        "Use a letter, digit or F1–F24 with Ctrl, Alt or Super/Win." => "Utilise une lettre, un chiffre ou F1–F24 avec Ctrl, Alt ou Super/Win.",
        "Theme applied and saved." => "Thème appliqué et enregistré.",
        "Interface language applied and saved." => "Langue de l’interface appliquée et enregistrée.",
        "Enter the exact model name available from this provider." => "Renseigne le nom exact du modèle disponible sur ce provider.",
        "Configure the provider, then test the connection and save." => "Configure le provider, puis teste la connexion et enregistre.",
        "Configuration changed; run the connection test again." => "Configuration modifiée ; relance le test de connexion.",
        "Choose the source language" => "Choisis la langue source",
        "Choose the target language" => "Choisis la langue cible",
        "Settings saved. Shortcuts are active; you can close this window." => "Paramètres enregistrés. Les raccourcis sont actifs ; tu peux fermer cette fenêtre.",
        "Saving settings…" => "Enregistrement des paramètres…",
        "A settings save is already in progress." => "Un enregistrement des paramètres est déjà en cours.",
        "The settings service failed" => "Le service de configuration a échoué",
        "Wait for the API key to load, or enter it manually if the keyring is unavailable." => "Attendez le chargement de la clé API, ou saisissez-la manuellement si le trousseau est indisponible.",
        "Testing connection…" => "Test de connexion en cours…",
        "Connection successful" => "Connexion réussie",
        "Test interrupted." => "Test interrompu.",
        "Invalid provider URL" => "URL du provider invalide",
        "The URL must start with http:// or https://." => "L’URL doit commencer par http:// ou https://.",
        "The URL must not contain credentials, query parameters or a fragment." => "L’URL ne doit contenir ni identifiants, ni paramètres, ni fragment.",
        "Enter the model name." => "Renseigne le nom du modèle.",
        "Choose a source language and an explicit target language." => "Choisis une langue source et une langue cible explicite.",
        "Unable to find the configuration directory" => "Impossible de trouver le dossier de configuration",
        "Unreadable configuration" => "Configuration illisible",
        "Unable to read the configuration" => "Impossible de lire la configuration",
        "Missing configuration directory" => "Dossier de configuration absent",
        "Unable to save the configuration" => "Impossible d’enregistrer la configuration",
        "Unable to access the system keyring" => "Accès au trousseau du système impossible",
        "Unable to read the API key" => "Impossible de lire la clé API",
        "Unable to save the API key" => "Impossible d’enregistrer la clé API",
        "Unable to restore the previous API key; check the saved credential." => "Impossible de restaurer la clé API précédente ; vérifie la clé enregistrée.",
        "No text to process." => "Aucun texte à traiter.",
        "Unable to connect to the provider (URL, network or timeout)" => "Connexion au provider impossible (URL, réseau ou délai d’attente)",
        "Incompatible response: expected chat/completions JSON" => "Réponse incompatible : JSON chat/completions attendu",
        "The provider response exceeds the 4 MiB limit." => "La réponse du provider dépasse la limite de 4 Mio.",
        "Unable to read the provider response" => "Impossible de lire la réponse du provider",
        "The provider returned no suggestions" => "Le provider n’a retourné aucune proposition",
        "The provider truncated the result. No replacement performed; reduce the selection or increase the provider's output limit." => "Le provider a tronqué le résultat. Aucun remplacement effectué ; réduisez la sélection ou augmentez la limite de sortie du provider.",
        "The provider filtered this result. No replacement performed." => "Le provider a filtré ce résultat. Aucun remplacement effectué.",
        "The provider did not complete the result. No replacement performed." => "Le provider n’a pas terminé le résultat. Aucun remplacement effectué.",
        "The model refused this request." => "Le modèle a refusé ce traitement.",
        "The provider returned no text" => "Le provider n’a retourné aucun texte",
        "The provider returned empty text." => "Le provider a retourné un texte vide.",
        "Invalid shortcut (example: Ctrl+Alt+KeyT)" => "Raccourci invalide (exemple : Ctrl+Alt+KeyT)",
        "The shortcut must include Ctrl, Alt or Super/Win." => "Le raccourci doit inclure Ctrl, Alt ou Super/Win.",
        "Preview and Quick Translate shortcuts must be different." => "Les raccourcis de l’aperçu et du Quick Translate doivent être différents.",
        "All four shortcuts must be different." => "Les quatre raccourcis doivent être différents.",
        "Shortcut already used by another application" => "Raccourci déjà utilisé par une autre application",
        "Unable to release shortcut" => "Impossible de libérer le raccourci",
        "Provider HTTP error" => "Erreur HTTP du provider",
        "Check the API key and permissions." => "Vérifie la clé API et les permissions.",
        "Check the base URL and model name." => "Vérifie l’URL de base et le nom du modèle.",
        "Quota exceeded or too many requests; try again later." => "Quota atteint ou trop de requêtes ; réessaie plus tard.",
        "Check the server's /chat/completions compatibility and logs." => "Vérifie la compatibilité /chat/completions du serveur et ses journaux.",
        "Unable to locate the executable" => "Impossible de trouver l’exécutable",
        "Unable to restore startup registration" => "Impossible de restaurer le démarrage automatique",
        "Unable to read Windows startup registration" => "Impossible de lire le démarrage automatique Windows",
        "Unable to update Windows startup registration" => "Impossible de modifier le démarrage automatique Windows",
        "No active window." => "Aucune fenêtre active.",
        "Select text in another application." => "Sélectionne du texte dans une autre application.",
        "Invalid status window." => "Fenêtre d’état invalide.",
        "Clipboard service interrupted" => "Service presse-papiers interrompu",
        "The document or active control changed; capture cancelled." => "Le document ou le contrôle actif a changé ; capture annulée.",
        "The selection contains no text." => "La sélection ne contient pas de texte.",
        "Selection too long (maximum 100,000 UTF-16 code units)." => "Sélection trop longue (100 000 caractères UTF-16 maximum).",
        "The result is empty." => "Le résultat est vide.",
        "The original window was closed. Use Copy." => "La fenêtre d’origine a été fermée. Utilise Copier.",
        "Unable to focus the original window. Use Copy." => "Impossible de retrouver le focus de la fenêtre d’origine. Utilise Copier.",
        "The original document or control changed. Use Copy." => "Le document ou le contrôle d’origine a changé. Utilise Copier.",
        "The original selection position changed. Use Copy." => "La position de la sélection d’origine a changé. Utilise Copier.",
        "The selection changed. No replacement performed; use Copy." => "La sélection a changé. Aucun remplacement effectué ; utilise Copier.",
        "The clipboard changed before pasting. Use Copy." => "Le presse-papiers a changé avant le collage. Utilise Copier.",
        "The active window changed; operation cancelled." => "La fenêtre active a changé ; opération annulée.",
        "The document or active control changed before pasting. Use Copy." => "Le document ou le contrôle actif a changé avant le collage. Utilise Copier.",
        "The accessible selection disappeared; use Copy" => "La sélection accessible a disparu ; utilise Copier",
        "The selection position or content changed. Use Copy." => "La position ou le contenu de la sélection a changé. Utilise Copier.",
        "Release the shortcut keys, then try again." => "Relâche les touches du raccourci, puis réessaie.",
        "Windows blocked keyboard simulation (elevated application or protected input)." => "Windows a bloqué la simulation clavier (application administrateur ou saisie protégée).",
        "No text copied. Check the selection and Ctrl+C support." => "Aucun texte copié. Vérifie la sélection et le support de Ctrl+C.",
        "The clipboard is busy in another application." => "Le presse-papiers est occupé par une autre application.",
        "The clipboard contains a private format that cannot be restored. Copy text first, then try again." => "Le presse-papiers contient un format privé non restaurable. Copie d’abord du texte, puis réessaie.",
        "Unable to preserve the clipboard" => "Impossible de préserver le presse-papiers",
        "Unable to restore the previous clipboard" => "Impossible de restaurer le presse-papiers précédent",
        "Unable to preserve a clipboard format; capture cancelled." => "Impossible de préserver un format du presse-papiers ; capture annulée.",
        "The clipboard was modified by another application." => "Le presse-papiers a été modifié par une autre application.",
        "Unable to verify the clipboard source; capture cancelled." => "Impossible de vérifier l’origine du presse-papiers ; capture annulée.",
        "X-Resource 1.2 is required to verify the clipboard source." => "X-Resource 1.2 est nécessaire pour vérifier l’origine du presse-papiers.",
        "The copied selection is not Unicode text" => "La sélection copiée n’est pas du texte Unicode",
        "Unable to read the clipboard." => "Impossible de lire le presse-papiers.",
        "The copied text is not valid Unicode" => "Le texte copié n’est pas un Unicode valide",
        "The text contains a null character or exceeds 100,000 UTF-16 code units." => "Le texte à copier contient un caractère nul ou dépasse 100 000 caractères UTF-16.",
        "The clipboard changed while writing." => "Le presse-papiers a changé pendant l’écriture.",
        "Unable to allocate clipboard memory." => "Allocation presse-papiers impossible.",
        "Unable to publish the result; the clipboard may be empty." => "Impossible de publier le résultat ; le presse-papiers peut être vide.",
        "The capture service failed" => "Le service de capture a échoué",
        "Unable to show the status window" => "Impossible d’afficher la fenêtre d’état",
        "The quick processing service failed" => "Le service de traitement rapide a échoué",
        "The replacement service failed" => "Le service de remplacement a échoué",
        "Incomplete cleanup of new shortcuts" => "Nettoyage des nouveaux raccourcis incomplet",
        "Unable to import the previous configuration" => "Impossible d’importer l’ancienne configuration",
}

/// Convert persisted legacy French names and localized labels to stable English names.
/// Unknown custom languages are preserved verbatim.
pub fn canonical_language(value: &str) -> &str {
    crate::settings::LANGUAGES
        .iter()
        .copied()
        .chain([crate::settings::AUTO])
        .find(|name| *name == value || translate(name, true) == value)
        .unwrap_or(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn existing_status_messages_follow_language_changes_without_translating_provider_output() {
        let english =
            "Replacement unavailable: The selection changed. No replacement performed; use Copy.";
        let french = localize_message_for(english, true);
        assert!(french.starts_with("Remplacement impossible: La sélection a changé."));
        assert_eq!(localize_message_for(&french, false), english);
        assert_eq!(
            localize_message_for("Translating…", true),
            "Traduction en cours…"
        );
        assert_eq!(
            localize_message_for("Connexion réussie: English", false),
            "Connection successful: English"
        );
        assert_eq!(
            localize_message_for("Connection successful: English", true),
            "Connexion réussie: English"
        );
    }

    #[test]
    fn language_labels_round_trip_without_changing_translation_targets() {
        for name in crate::settings::LANGUAGES
            .iter()
            .copied()
            .chain([crate::settings::AUTO])
        {
            assert_eq!(canonical_language(translate(name, true)), name);
            assert_eq!(canonical_language(translate(name, false)), name);
        }
        assert_eq!(canonical_language("Esperanto"), "Esperanto");
        assert_eq!(translate("Copy", false), "Copy");
        assert_eq!(translate("Copy", true), "Copier");
    }
}
