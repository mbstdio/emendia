#![cfg_attr(
    all(target_os = "windows", not(debug_assertions)),
    windows_subsystem = "windows"
)]

use anyhow::Result;
use emendia::{
    app::Controller,
    settings::{Operation, Settings, SettingsStore},
    translation::Translator,
};
use gpui_kit::*;

struct AppController {
    _controller: Entity<Controller>,
}
impl Global for AppController {}

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "emendia=info".into()),
        )
        .init();
    if let Err(error) = run() {
        tracing::error!(%error, "Unable to start Emendia");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    emendia::platform::initialize()?;
    let smoke_onboarding = std::env::args().any(|argument| argument == "--smoke-test-onboarding");
    let smoke_shortcuts = cfg!(target_os = "linux")
        && std::env::args().any(|argument| argument == "--smoke-test-shortcuts");
    let smoke_correction = std::env::args().any(|argument| {
        matches!(
            argument.as_str(),
            "--smoke-test-correction" | "--smoke-test-quick-check"
        )
    });
    let smoke_quick = std::env::args().any(|argument| {
        matches!(
            argument.as_str(),
            "--smoke-test-quick" | "--smoke-test-quick-check"
        )
    });
    let smoke_test = smoke_onboarding
        || smoke_shortcuts
        || smoke_correction
        || smoke_quick
        || std::env::args().any(|argument| argument == "--smoke-test");
    let show_settings = std::env::args().any(|argument| argument == "--settings");
    if smoke_test {
        // Graphical diagnostics must not depend on or access the user's keyring.
        keyring::set_default_credential_builder(keyring::mock::default_credential_builder());
    }
    let operation = if smoke_correction {
        Operation::Correction
    } else {
        Operation::Translation
    };
    let store = SettingsStore::new()?;
    let (mut settings, first_run, mut error) = match store.load() {
        Ok(Some(settings)) => {
            let first_run = !settings.onboarding_completed;
            (settings, first_run, None)
        }
        Ok(None) => (Settings::default(), true, None),
        Err(error) => (Settings::default(), true, Some(error.to_string())),
    };
    if smoke_test {
        settings = Settings {
            base_url: emendia::smoke::local_provider(operation)?,
            model: "smoke-test".into(),
            onboarding_completed: !smoke_onboarding,
            ..Settings::default()
        };
    }
    if smoke_onboarding {
        settings.theme = if std::env::args().any(|argument| argument == "--smoke-theme=dark") {
            emendia::settings::ThemePreference::Dark
        } else {
            emendia::settings::ThemePreference::Light
        };
    }
    if std::env::args().any(|argument| argument == "--ui-language=en") {
        settings.ui_language = emendia::i18n::UiLanguage::English;
    } else if std::env::args().any(|argument| argument == "--ui-language=fr") {
        settings.ui_language = emendia::i18n::UiLanguage::French;
    }
    emendia::i18n::apply(settings.ui_language);
    if !smoke_test && let Err(migration) = emendia::platform::startup::migrate_legacy() {
        error = Some(migration.to_string());
    }
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()?;
    let translator = Translator::new()?;
    gpui_kit::application()
        .with_assets(gpui_kit::assets::AllAssets)
        .run(move |cx| {
            gpui_kit::init(cx);
            cx.set_quit_mode(QuitMode::Explicit);
            match Controller::new(settings, store, runtime, translator, cx.window_appearance()) {
                Ok(controller) => {
                    let controller = cx.new(|_| controller);
                controller.update(cx, |controller, cx| {
                    controller.start(first_run || smoke_test || show_settings, error, cx);
                       #[cfg(target_os = "linux")]
                       if smoke_shortcuts {
                           controller.open_shortcut_smoke(cx);
                       }
                       if smoke_test && !smoke_onboarding && !smoke_shortcuts {
                          // Let the initial Settings window finish its foreground activation
                          // before measuring focus preservation by the status popup.
                          cx.spawn(async move |controller, cx| {
                              cx.background_executor().timer(std::time::Duration::from_millis(250)).await;
                              controller.update(cx, |controller, cx| controller.open_smoke(operation, smoke_quick, cx).expect("Open smoke test")).expect("Smoke test controller");
                          }).detach();
                     }
                });
                    // Keep the coordinator alive even while no windows are open.
                    cx.set_global(AppController {
                        _controller: controller,
                    });
                    if smoke_onboarding {
                        cx.spawn(async move |cx| {
                            for step in 0..6 {
                                cx.background_executor().timer(std::time::Duration::from_millis(500)).await;
                                cx.update(|cx| {
                                    let controller = cx.global::<AppController>()._controller.clone();
                                    controller.update(cx, |controller, cx| controller.smoke_onboarding_step(step, cx));
                                });
                            }
                            cx.background_executor().timer(std::time::Duration::from_millis(500)).await;
                            cx.update(|cx| {
                                tracing::info!("GPUI onboarding smoke test: sole setup window, all five steps and backward navigation verified without saving");
                                let controller = cx.global::<AppController>()._controller.clone();
                                controller.update(cx, |controller, cx| controller.smoke_close_onboarding(cx));
                            });
                            cx.background_executor().timer(std::time::Duration::from_secs(1)).await;
                            panic!("Closing incomplete onboarding must quit Emendia completely");
                        }).detach();
                    } else if smoke_test && !smoke_shortcuts {
                        cx.spawn(async move |cx| {
                            cx.background_executor()
                                .timer(std::time::Duration::from_secs(3))
                                .await;
                            cx.update(|cx| {
                            assert!(cx.windows().len() >= 2, "Settings and preview must be open");
                             assert!(cx.global::<AppController>()._controller.read(cx).smoke_preview_complete(cx), "The mock provider result must be visible in the preview");
                             if smoke_quick {
                                 assert!(cx.global::<AppController>()._controller.read(cx).smoke_quick_complete(cx), "Failed paste must preserve the result and show an error without another request");
                             }
                                tracing::info!(
                                "GPUI smoke test: settings, translated preview, tray and event loop active"
                                );
                                 if !smoke_quick {
                                     cx.quit();
                                 }
                             });
                             if smoke_quick {
                                 cx.background_executor().timer(std::time::Duration::from_secs(4)).await;
                                 cx.update(|cx| {
                                     assert!(cx.global::<AppController>()._controller.read(cx).smoke_status_closed(), "The error popup must close automatically");
                                     assert!(cx.global::<AppController>()._controller.read(cx).smoke_preview_complete(cx), "The result must remain available after the error popup closes");
                                     cx.quit();
                                 });
                             }
                        })
                        .detach();
                    }
                }
                Err(error) => {
                    tracing::error!(%error, "Unable to initialize desktop integration");
                    cx.quit();
                }
            }
        });
    Ok(())
}
