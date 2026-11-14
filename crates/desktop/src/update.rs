use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use tauri::{AppHandle, Manager};
use tauri_plugin_dialog::{DialogExt, MessageDialogBuilder, MessageDialogButtons};
use tauri_plugin_updater::{Update, UpdaterExt};

const FIRST_CHECK_AFTER: Duration = Duration::from_secs(20);
const CHECK_EVERY: Duration = Duration::from_secs(12 * 60 * 60);
const NOTES_LIMIT: usize = 1_500;
const CHECK_TIMEOUT: Duration = Duration::from_secs(30);

static BUSY: AtomicBool = AtomicBool::new(false);

pub fn watch(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(FIRST_CHECK_AFTER).await;
        loop {
            check(&app, false).await;
            tokio::time::sleep(CHECK_EVERY).await;
        }
    });
}

pub fn check_now(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move { check(&app, true).await });
}

async fn check(app: &AppHandle, asked: bool) {
    if BUSY.swap(true, Ordering::SeqCst) {
        return;
    }
    let found = match app.updater_builder().timeout(CHECK_TIMEOUT).build() {
        Ok(updater) => updater.check().await,
        Err(error) => Err(error),
    };
    match found {
        Ok(Some(update)) => offer(app, update).await,
        Ok(None) if asked => tell(app, "You have the latest version."),
        Err(error) if asked => tell(app, &format!("Could not check for updates: {error}")),
        _ => {}
    }
    BUSY.store(false, Ordering::SeqCst);
}

async fn offer(app: &AppHandle, update: Update) {
    let notes = plain_notes(update.body.as_deref().unwrap_or_default());
    let text = format!(
        "Discombobulator {} is available. You have {}.\n\n{notes}",
        update.version, update.current_version
    );
    let (answer, answered) = tokio::sync::oneshot::channel();
    dialog(app, text)
        .title("Update available")
        .buttons(MessageDialogButtons::OkCancelCustom(
            "Install and restart".into(),
            "Later".into(),
        ))
        .show(move |install| {
            let _ = answer.send(install);
        });
    if !answered.await.unwrap_or(false) {
        return;
    }
    match update.download_and_install(|_, _| {}, || {}).await {
        Ok(()) => app.restart(),
        Err(error) => tell(app, &format!("The update could not be installed: {error}")),
    }
}

fn dialog(app: &AppHandle, text: impl Into<String>) -> MessageDialogBuilder<tauri::Wry> {
    let builder = app.dialog().message(text);
    match app.get_webview_window(crate::MAIN_WINDOW) {
        Some(window) => builder.parent(&window),
        None => builder,
    }
}

fn tell(app: &AppHandle, text: &str) {
    dialog(app, text).title("Discombobulator").show(|_| {});
}

fn without_links(line: &str) -> String {
    let mut text = String::with_capacity(line.len());
    let mut rest = line;
    while let Some(open) = rest.find('[') {
        let Some(close) = rest[open..].find("](").map(|at| open + at) else {
            break;
        };
        let Some(end) = rest[close..].find(')').map(|at| close + at) else {
            break;
        };
        text.push_str(&rest[..open]);
        text.push_str(&rest[open + 1..close]);
        rest = &rest[end + 1..];
    }
    text.push_str(rest);
    text
}

fn is_reference(inner: &str) -> bool {
    let digits = |value: &str| !value.is_empty() && value.chars().all(|c| c.is_ascii_digit());
    inner.strip_prefix('#').is_some_and(digits)
        || (inner.len() >= 7 && inner.chars().all(|c| c.is_ascii_hexdigit()))
}

fn without_references(line: &str) -> &str {
    let mut line = line.trim_end();
    if let Some(at) = line.find(", closes ") {
        line = &line[..at];
    }
    while let Some(inner) = line.strip_suffix(')').and_then(|head| {
        let open = head.rfind(" (")?;
        is_reference(&head[open + 2..]).then_some(open)
    }) {
        line = line[..inner].trim_end();
    }
    line
}

pub fn plain_notes(body: &str) -> String {
    let mut lines: Vec<String> = Vec::new();
    for raw in body.lines() {
        let line = without_links(raw).replace("**", "");
        let line = line.trim();
        if line.starts_with("## ") {
            continue;
        }
        let line = if let Some(heading) = line.strip_prefix("### ") {
            format!("{heading}:")
        } else if let Some(item) = line.strip_prefix("* ") {
            format!("• {}", without_references(item))
        } else {
            line.to_string()
        };
        if line.is_empty() && lines.last().is_none_or(|last| last.is_empty()) {
            continue;
        }
        lines.push(line);
    }
    while lines.last().is_some_and(|last| last.is_empty()) {
        lines.pop();
    }
    let notes = lines.join("\n");
    match notes.char_indices().nth(NOTES_LIMIT) {
        Some((cut, _)) => format!("{}…", &notes[..cut]),
        None => notes,
    }
}

#[cfg(test)]
mod tests {
    use super::plain_notes;

    #[test]
    fn turns_release_notes_into_plain_text() {
        let body = "## [0.2.0](https://example.com/compare) (2026-11-14)\n\n\n### Features\n\n* move between scenes with Ctrl+arrows ([#62](https://example.com/62)) ([aab835e](https://example.com/c))\n* **core:** scan folders ([d3d8740](https://example.com/d)), closes [#3](https://example.com/3)\n\n\n### Bug Fixes\n\n* keep marks (#55)\n";
        assert_eq!(
            plain_notes(body),
            "Features:\n\n• move between scenes with Ctrl+arrows\n• core: scan folders\n\nBug Fixes:\n\n• keep marks"
        );
    }

    #[test]
    fn keeps_parentheses_that_are_not_references() {
        assert_eq!(
            plain_notes("* faster previews (about 2x)"),
            "• faster previews (about 2x)"
        );
    }

    #[test]
    fn shortens_long_notes() {
        let notes = plain_notes(&"* a long line of notes\n".repeat(200));
        assert!(notes.chars().count() <= 1_501);
        assert!(notes.ends_with('…'));
    }
}
