//! Static tray menu, live panel label, and dashboard-window lifecycle.
//!
//! Linux tray labels are best-effort presentation: Tauri publishes `XAyatanaLabel`
//! unconditionally, while desktops which do not render it naturally remain icon-only.

use std::sync::Arc;

use tauri::{
    image::Image,
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Manager, WebviewUrl, WebviewWindowBuilder,
};
use wockit::{
    formatting::{
        menu_bar_presentation, DataFeedState, MenuBarPresentation, Phase, RealmAvailability,
    },
    models::MenuBarDisplayMode,
};

use crate::{
    app_state::AppState,
    poller::{
        DataFeedState as PollFeedState, PollerSnapshot, RealmAvailability as PollRealmAvailability,
    },
    settings::DisplayMode,
    strings,
};

pub const TRAY_ID: &str = "main";
pub const DASHBOARD_LABEL: &str = "dashboard";
pub const OPEN_DASHBOARD_ID: &str = "open-dashboard";
pub const REFRESH_ID: &str = "refresh";
pub const QUIT_ID: &str = "quit";

const DASHBOARD_BACKGROUND: tauri::window::Color = tauri::window::Color(13, 13, 38, 255);

/// A stable maximum-width sample for each mode, published as `XAyatanaLabelGuide` on Linux.
pub fn label_guide(mode: MenuBarDisplayMode) -> &'static str {
    match mode {
        MenuBarDisplayMode::Players => "🟠 0000",
        MenuBarDisplayMode::PlayersAndChange => "🟠 0000 · WOC +100.0%",
        MenuBarDisplayMode::Token => "$0.0000000 +100.0%",
        MenuBarDisplayMode::Full => "$0.0000000 (+100.0%) 🟠 0000",
        MenuBarDisplayMode::IconOnly => "",
    }
}

pub fn presentation(snapshot: &PollerSnapshot, mode: DisplayMode) -> MenuBarPresentation {
    let status = snapshot.status.value.as_ref();
    menu_bar_presentation(
        display_mode(mode),
        status.map(|value| value.players_online),
        realm_availability(snapshot.realm_availability),
        phase(snapshot.status.state),
        status.is_some_and(|value| value.ok),
        snapshot
            .quote
            .value
            .as_ref()
            .map(|quote| quote.price.as_str()),
        snapshot.quote.value.as_ref().map(|quote| quote.change24h),
        feed_state(snapshot.quote.state),
    )
}

pub fn update(app: &AppHandle, snapshot: &PollerSnapshot, mode: DisplayMode) {
    let value = presentation(snapshot, mode);
    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        // Never gate title publication on desktop detection. Empty is intentional for
        // iconOnly and still clears a previously published text label.
        if let Err(error) = tray.set_title(Some(value.label.clone())) {
            eprintln!("failed to update tray title: {error}");
        }
        publish_label_guide(&tray, value.label, label_guide(display_mode(mode)));
        // This is the only semantic-name-like surface in the pinned public API. On GTK,
        // tray-icon currently treats tooltips as unsupported, so essential information
        // remains available in the dashboard rather than relying on this call.
        if let Err(error) = tray.set_tooltip(Some(value.accessibility_label)) {
            eprintln!("failed to update tray accessibility text: {error}");
        }
    }
}

pub fn build(
    app: &AppHandle,
    initial_snapshot: &PollerSnapshot,
    mode: DisplayMode,
) -> tauri::Result<()> {
    let open = MenuItem::with_id(
        app,
        OPEN_DASHBOARD_ID,
        strings::OPEN_DASHBOARD,
        true,
        None::<&str>,
    )?;
    let refresh_item = MenuItem::with_id(app, REFRESH_ID, strings::REFRESH, true, None::<&str>)?;
    let quit = MenuItem::with_id(app, QUIT_ID, strings::QUIT, true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open, &refresh_item, &quit])?;
    let icon = symbolic_tray_icon()?;
    let initial = presentation(initial_snapshot, mode);

    TrayIconBuilder::with_id(TRAY_ID)
        .icon(icon)
        .title(initial.label)
        .tooltip(initial.accessibility_label)
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            OPEN_DASHBOARD_ID => open_dashboard(app),
            REFRESH_ID => refresh(app),
            QUIT_ID => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if matches!(
                event,
                TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                }
            ) {
                open_dashboard(tray.app_handle());
            }
        })
        .build(app)?;
    update(app, initial_snapshot, mode);
    Ok(())
}

pub fn set_visible(app: &AppHandle, visible: bool) -> tauri::Result<()> {
    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        tray.set_visible(visible)?;
    }
    Ok(())
}

fn symbolic_tray_icon() -> tauri::Result<Image<'static>> {
    Image::from_bytes(include_bytes!("../icons/tray-symbolic.png")).map(Image::to_owned)
}

#[cfg(target_os = "linux")]
fn publish_label_guide(tray: &tauri::tray::TrayIcon, label: String, guide: &'static str) {
    if let Err(error) = tray.with_inner_tray_icon(move |inner| {
        // SAFETY: tray-icon guarantees this pointer is the live Linux AppIndicator owned by
        // the tray. Tauri runs the closure synchronously on the GTK main thread, the pointer
        // is only borrowed immutably, and it does not escape the closure.
        unsafe { (&*inner.app_indicator()).set_label_shared(&label, guide) };
    }) {
        eprintln!("failed to update tray label guide: {error}");
    }
}

#[cfg(not(target_os = "linux"))]
fn publish_label_guide(_tray: &tauri::tray::TrayIcon, _label: String, _guide: &'static str) {}

pub fn open_dashboard(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(DASHBOARD_LABEL) {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
        mark_dashboard_visible(app);
        return;
    }
    let result = WebviewWindowBuilder::new(app, DASHBOARD_LABEL, WebviewUrl::default())
        .title(strings::PRODUCT_NAME)
        .inner_size(440.0, 660.0)
        .resizable(false)
        .maximizable(false)
        .minimizable(false)
        .theme(Some(tauri::Theme::Dark))
        .background_color(DASHBOARD_BACKGROUND)
        .build();
    match result {
        Ok(_) => mark_dashboard_visible(app),
        Err(error) => eprintln!("failed to create dashboard window: {error}"),
    }
}

pub fn mark_dashboard_hidden(app: &AppHandle) {
    let state = app.state::<Arc<AppState>>().inner().clone();
    tauri::async_runtime::spawn(async move {
        state.poller.set_dashboard_visible(false).await;
    });
}

fn mark_dashboard_visible(app: &AppHandle) {
    let state = app.state::<Arc<AppState>>().inner().clone();
    tauri::async_runtime::spawn(async move {
        tokio::join!(
            state.poller.set_dashboard_visible(true),
            state.poller.refresh_visible_content(),
        );
    });
}

fn refresh(app: &AppHandle) {
    let state = app.state::<Arc<AppState>>().inner().clone();
    tauri::async_runtime::spawn(async move {
        tokio::join!(state.poller.refresh_status(), state.poller.refresh_quote());
    });
}

fn display_mode(mode: DisplayMode) -> MenuBarDisplayMode {
    match mode {
        DisplayMode::Players => MenuBarDisplayMode::Players,
        DisplayMode::PlayersAndChange => MenuBarDisplayMode::PlayersAndChange,
        DisplayMode::Token => MenuBarDisplayMode::Token,
        DisplayMode::Full => MenuBarDisplayMode::Full,
        DisplayMode::IconOnly => MenuBarDisplayMode::IconOnly,
    }
}

fn realm_availability(value: PollRealmAvailability) -> RealmAvailability {
    match value {
        PollRealmAvailability::Loading => RealmAvailability::Loading,
        PollRealmAvailability::Healthy => RealmAvailability::Healthy,
        PollRealmAvailability::ServerReportedDown => RealmAvailability::ServerReportedDown,
        PollRealmAvailability::Unreachable(_) => RealmAvailability::Unreachable,
    }
}

fn phase(value: PollFeedState) -> Phase {
    match value {
        PollFeedState::Idle | PollFeedState::Loading => Phase::Loading,
        PollFeedState::Live => Phase::Ok,
        PollFeedState::Cached | PollFeedState::Unavailable => Phase::Error,
    }
}

fn feed_state(value: PollFeedState) -> DataFeedState {
    match value {
        PollFeedState::Idle => DataFeedState::Idle,
        PollFeedState::Loading => DataFeedState::Loading,
        PollFeedState::Live => DataFeedState::Live,
        PollFeedState::Cached => DataFeedState::Cached,
        PollFeedState::Unavailable => DataFeedState::Unavailable,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::poller::{DataFeedState as PollFeedState, Feed};
    use std::collections::HashMap;
    use wockit::models::{CryptoQuote, StatusResponse};

    fn live_snapshot() -> PollerSnapshot {
        PollerSnapshot {
            status: Feed {
                value: Some(StatusResponse::new(true, None, 7, None)),
                state: PollFeedState::Live,
                ..Feed::default()
            },
            realm_availability: PollRealmAvailability::Healthy,
            quote: Feed {
                value: Some(CryptoQuote {
                    price: "0.0005594".into(),
                    change24h: 47.59,
                    market: HashMap::new(),
                    liquidity_usd: None,
                    fdv_usd: None,
                    market_cap_usd: None,
                    pair_url: None,
                }),
                state: PollFeedState::Live,
                ..Feed::default()
            },
            ..PollerSnapshot::default()
        }
    }

    #[test]
    fn every_mode_uses_the_domain_formatter() {
        let snapshot = live_snapshot();
        let cases = [
            (DisplayMode::Players, "🟢 7"),
            (DisplayMode::PlayersAndChange, "🟢 7 · WOC +47.6%"),
            (DisplayMode::Token, "$0.0005594 +47.6%"),
            (DisplayMode::Full, "$0.0005594 (+47.6%) 🟢 7"),
            (DisplayMode::IconOnly, ""),
        ];
        for (mode, expected) in cases {
            assert_eq!(presentation(&snapshot, mode).label, expected);
        }
    }

    #[test]
    fn cached_quote_never_appears_live_in_any_price_mode() {
        let mut snapshot = live_snapshot();
        snapshot.quote.state = PollFeedState::Cached;
        for mode in [
            DisplayMode::PlayersAndChange,
            DisplayMode::Token,
            DisplayMode::Full,
        ] {
            assert_eq!(presentation(&snapshot, mode).label, "🟢 7");
        }
    }

    #[test]
    fn guides_cover_each_visible_mode_and_icon_only_reserves_nothing() {
        assert_eq!(label_guide(MenuBarDisplayMode::Players), "🟠 0000");
        assert_eq!(
            label_guide(MenuBarDisplayMode::PlayersAndChange),
            "🟠 0000 · WOC +100.0%"
        );
        assert_eq!(label_guide(MenuBarDisplayMode::Token), "$0.0000000 +100.0%");
        assert_eq!(
            label_guide(MenuBarDisplayMode::Full),
            "$0.0000000 (+100.0%) 🟠 0000"
        );
        assert_eq!(label_guide(MenuBarDisplayMode::IconOnly), "");
    }

    #[test]
    fn symbolic_tray_asset_is_a_panel_sized_rgba_icon() {
        let icon = symbolic_tray_icon().expect("embedded symbolic tray icon should decode");
        assert_eq!((icon.width(), icon.height()), (32, 32));
        assert!(icon
            .rgba()
            .as_chunks::<4>()
            .0
            .iter()
            .any(|pixel| pixel[3] > 0));
        assert!(icon
            .rgba()
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|pixel| pixel[3] > 0)
            .all(|pixel| pixel[..3] == [255, 255, 255]));
    }
}
