# Using Emendia

[Back to README](../README.md)

## Setup

On first launch, the setup assistant guides you through appearance, AI provider, translation and proofreading preferences, shortcuts, and startup. Enter your provider's base URL, exact model name and API key if required. **Test connection** is recommended but optional. Click **Finish setup** to save and activate your settings, then **Let's go** to leave Emendia running in the tray.

Closing the assistant before finishing quits Emendia and opens setup again on the next launch. After setup is saved, closing the confirmation screen leaves it running in the tray. Appearance and interface-language changes alone do not complete setup. Existing configurations, including imported legacy settings, retain their usual startup behavior.

Later launches stay in the tray. Click its icon or choose **Settings** to change your configuration. Setup and Settings open on the cursor's monitor, centered in its usable area with its DPI scaling. Setup is non-resizable; Settings is resizable.

## Providers

| Provider | Base URL | Model |
| --- | --- | --- |
| OpenAI | `https://api.openai.com/v1` | Default: `gpt-6-luna`; use a model available to your API account |
| LM Studio | `http://localhost:1234/v1` | The identifier of the model loaded in LM Studio |
| Ollama | `http://localhost:11434/v1` | The exact installed model name, such as `llama3.2` |
| Custom | Your compatible API's base URL, including its prefix | The identifier expected by your server |

Use a **base URL**, without `/chat/completions` at the end. For local providers, start the HTTP service and load the model before testing. An API key is optional if your server does not require one.

**Test connection** makes a small real translation request using the current form values, without saving them. OpenAI charges according to your model and API account; a ChatGPT subscription does not provide API access or credits.

Preset URLs and models remain editable. Changing the URL reloads the key associated with that endpoint. Clear the key and save to remove that endpoint's credential.

## Shortcuts

See the [default shortcuts](../README.md#default-shortcuts). Change them in **Translation**, **Proofreading** or the centralized **Shortcuts** page; these pages share the same values.

Save to activate changes. All four shortcuts must be distinct. If a new shortcut is already reserved, the previous shortcuts remain active. **Shortcuts enabled** in the tray menu toggles all four together.

After pressing a shortcut, release its keys. New shortcuts are ignored during capture and quick processing. When a preview is already open, any of the four shortcuts brings it to the foreground.

## Preview

The capture popup appears first, then the preview takes over. Change translation languages or proofreading style, edit the result directly, or request a **New suggestion**. Language and style changes apply only to that session.

- **Replace** returns to the original selection and attempts to paste the result.
- **Copy** puts the result on the clipboard for manual pasting.
- **Cancel**, Escape or closing the preview cancels pending processing without replacing text.

Changing language or style cancels the previous request; stale responses cannot overwrite newer results. The preview cannot be closed during the brief replacement operation.

## Quick actions

Quick Translate and Quick Check use your saved settings and replace the selection automatically. Stay in the original document with the same selection while processing. Status popups report capture, processing, replacement, success and errors without taking focus.

Success messages close after two seconds. Error popups offer **Settings** and **Close**, and close after five seconds; long messages wrap and can be scrolled.

If replacement fails, the existing result opens in a recovery preview so you can copy it or attempt manual replacement without another AI request. That preview remains after the error popup closes.

## Proofreading styles

| Style | Behavior |
| --- | --- |
| Faithful correction | Fix spelling, grammar and punctuation while preserving tone, register and wording; leave correct passages unchanged |
| More fluent | Light rewriting for smoother flow and readability, preserving the original tone |
| Professional | Polished wording suitable for workplace communication |
| Casual | Natural, informal wording |
| Concise | Shorter wording without losing essential information |

All styles ask the model to preserve meaning, paragraphs and formatting without inventing information. Proofreading detects the original language and does not translate. Results depend on the model. Preview and Quick Check have independent default styles; both start with Faithful correction.

## Appearance, language and startup

In **General**, select **Light / Dark / System** and **System / English / Français**. Both take effect immediately and are saved independently of other form edits. System theme follows Windows appearance changes; System language uses French when the Windows UI language is French, and English otherwise. Interface language does not affect translation targets or proofreading results.

Enable **Launch at Windows startup** and save to start Emendia in the tray at sign-in for the current user, without administrator rights. It is disabled by default. After moving the executable, save again from its new location. Use a release build to avoid a console window.

Closing every window leaves Emendia running in the tray. Choose **Quit** in the tray menu to exit.
