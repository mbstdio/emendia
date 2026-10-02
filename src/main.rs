#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use anyhow::Result;
use gpui_kit::*;
use translation_tool::{
    app::Controller,
    settings::{Settings, SettingsStore},
    translation::Translator,
};

struct AppController {
    _controller: Entity<Controller>,
}
impl Global for AppController {}

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "translation_tool=info".into()),
        )
        .init();
    if let Err(error) = run() {
        tracing::error!(%error, "Démarrage impossible");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let smoke_quick = std::env::args().any(|argument| argument == "--smoke-test-quick");
    let smoke_test = smoke_quick || std::env::args().any(|argument| argument == "--smoke-test");
    let store = SettingsStore::new()?;
    let (mut settings, first_run, error) = match store.load() {
        Ok(Some(settings)) => (settings, false, None),
        Ok(None) => (Settings::default(), true, None),
        Err(error) => (Settings::default(), true, Some(error.to_string())),
    };
    if smoke_test {
        settings = Settings {
            base_url: translation_tool::smoke::local_provider()?,
            model: "smoke-test".into(),
            ..Settings::default()
        };
    }
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()?;
    let translator = Translator::new()?;
    gpui_kit::application()
        .with_assets(gpui_kit::assets::Assets)
        .run(move |cx| {
            gpui_kit::init(cx);
            cx.set_quit_mode(QuitMode::Explicit);
            match Controller::new(settings, store, runtime, translator) {
                Ok(controller) => {
                    let controller = cx.new(|_| controller);
                controller.update(cx, |controller, cx| {
                    controller.start(first_run || smoke_test, error, cx);
                     if smoke_quick {
                         controller.open_smoke_quick(cx).expect("Ouverture du diagnostic Quick Translate");
                     } else if smoke_test {
                         controller.open_smoke_preview(cx).expect("Ouverture du diagnostic de traduction");
                     }
                });
                    // Keep the coordinator alive even while no windows are open.
                    cx.set_global(AppController {
                        _controller: controller,
                    });
                    if smoke_test {
                        cx.spawn(async move |cx| {
                            cx.background_executor()
                                .timer(std::time::Duration::from_secs(3))
                                .await;
                            cx.update(|cx| {
                            assert!(cx.windows().len() >= 2, "Paramètres et aperçu doivent être ouverts");
                             assert!(cx.global::<AppController>()._controller.read(cx).smoke_preview_complete(cx), "La traduction du provider simulé doit être visible dans l’aperçu");
                             if smoke_quick {
                                 assert!(cx.global::<AppController>()._controller.read(cx).smoke_quick_complete(cx), "Un collage impossible doit conserver la traduction et afficher l’erreur sans nouvelle requête");
                             }
                                tracing::info!(
                                "Smoke test GPUI : paramètres, aperçu traduit, tray et boucle d’événements actifs"
                                );
                                cx.quit();
                            });
                        })
                        .detach();
                    }
                }
                Err(error) => {
                    tracing::error!(%error, "Initialisation Windows impossible");
                    cx.quit();
                }
            }
        });
    Ok(())
}
