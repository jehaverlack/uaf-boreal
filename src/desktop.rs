//! Native desktop integration for BOREAL.
//!
//! Windows and macOS use a native tray/menu-bar event loop. Linux uses the
//! freedesktop StatusNotifierItem protocol so the release binary does not
//! acquire GTK or AppIndicator runtime dependencies.

use std::{
    path::PathBuf,
    sync::{
        Arc, Mutex, OnceLock, Weak,
        atomic::{AtomicBool, Ordering},
    },
};

use crate::app::AppState;

const DEFAULT_WEB_URL: &str = "http://127.0.0.1:8765";

static APP_STATE: OnceLock<Mutex<Weak<AppState>>> = OnceLock::new();
static WEB_URL: OnceLock<Mutex<String>> = OnceLock::new();
static QUIT_REQUESTED: AtomicBool = AtomicBool::new(false);

pub fn set_web_url(url: String) {
    let value = WEB_URL.get_or_init(|| Mutex::new(DEFAULT_WEB_URL.to_string()));
    if let Ok(mut current) = value.lock() {
        *current = url;
    }
}

pub fn register_state(state: &Arc<AppState>) {
    let value = APP_STATE.get_or_init(|| Mutex::new(Weak::new()));
    if let Ok(mut current) = value.lock() {
        *current = Arc::downgrade(state);
    }
    if QUIT_REQUESTED.load(Ordering::Acquire) {
        state.request_shutdown();
    }
}

fn web_url() -> String {
    WEB_URL
        .get_or_init(|| Mutex::new(DEFAULT_WEB_URL.to_string()))
        .lock()
        .map(|value| value.clone())
        .unwrap_or_else(|_| DEFAULT_WEB_URL.to_string())
}

fn open_url(url: &str, label: &str) {
    if let Err(error) = webbrowser::open(url) {
        log::error!("Unable to open {label}: {error}");
    }
}

fn open_boreal() {
    open_url(&web_url(), "the BOREAL WebUI");
}

fn request_quit() {
    let state = APP_STATE
        .get()
        .and_then(|value| value.lock().ok())
        .and_then(|value| value.upgrade());
    if let Some(state) = &state {
        let active_jobs = state.active_job_descriptions();
        if !active_jobs.is_empty()
            && rfd::MessageDialog::new()
                .set_title("Quit BOREAL?")
                .set_description(format!(
                    "BOREAL is currently running {}. Quitting now will interrupt active work.",
                    active_jobs.join(" and ")
                ))
                .set_level(rfd::MessageLevel::Warning)
                .set_buttons(rfd::MessageButtons::YesNo)
                .show()
                != rfd::MessageDialogResult::Yes
        {
            return;
        }
    }
    QUIT_REQUESTED.store(true, Ordering::Release);
    if let Some(state) = state {
        log::info!("Quit requested from the BOREAL desktop menu");
        state.request_shutdown();
    }
}

/// A compact Bootstrap-style BOREAL mark rendered directly into the executable.
/// This avoids platform-specific icon files for the live tray icon.
fn boreal_icon_rgba(size: u32) -> Vec<u8> {
    let mut pixels = vec![0_u8; (size * size * 4) as usize];
    let set = |pixels: &mut [u8], x: u32, y: u32, color: [u8; 4]| {
        let offset = ((y * size + x) * 4) as usize;
        pixels[offset..offset + 4].copy_from_slice(&color);
    };
    for y in 0..size {
        for x in 0..size {
            let px = (x as f32 + 0.5) * 32.0 / size as f32;
            let py = (y as f32 + 0.5) * 32.0 / size as f32;
            let dx = (7.0 - px).max(0.0).max(px - 25.0);
            let dy = (7.0 - py).max(0.0).max(py - 25.0);
            if dx * dx + dy * dy <= 25.0 {
                set(&mut pixels, x, y, [0, 132, 193, 255]);
                let stem = (9.5..13.0).contains(&px) && (7.0..25.0).contains(&py);
                let upper_outer = ((px - 15.0) / 7.0).powi(2) + ((py - 11.5) / 5.0).powi(2) <= 1.0;
                let upper_inner = ((px - 14.5) / 3.0).powi(2) + ((py - 11.5) / 2.1).powi(2) <= 1.0;
                let lower_outer = ((px - 15.0) / 7.5).powi(2) + ((py - 20.0) / 5.5).powi(2) <= 1.0;
                let lower_inner = ((px - 14.5) / 3.2).powi(2) + ((py - 20.0) / 2.4).powi(2) <= 1.0;
                if stem
                    || (px >= 11.0 && upper_outer && !upper_inner)
                    || (px >= 11.0 && lower_outer && !lower_inner)
                {
                    set(&mut pixels, x, y, [255, 255, 255, 255]);
                }
            }
        }
    }
    pixels
}

#[cfg(all(unix, not(target_os = "macos")))]
const BOREAL_MENU_ICON_PNG: &[u8] = &[
    0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x10, 0x00, 0x00, 0x00, 0x10, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1f, 0xf3, 0xff,
    0x61, 0x00, 0x00, 0x00, 0x3d, 0x49, 0x44, 0x41, 0x54, 0x78, 0xda, 0x63, 0x60, 0xa0, 0x1a, 0x68,
    0x39, 0xf8, 0x9f, 0x24, 0x4c, 0x91, 0x66, 0x0c, 0x43, 0xd0, 0x24, 0xd0, 0x01, 0xc5, 0x06, 0xe0,
    0x34, 0x08, 0x9f, 0x01, 0xd8, 0x0c, 0x24, 0xcb, 0x00, 0xb2, 0x5d, 0x40, 0x54, 0x38, 0x0c, 0x8d,
    0x30, 0x20, 0xdb, 0x00, 0x8a, 0xbc, 0x40, 0x30, 0xf4, 0x89, 0x31, 0x80, 0xf6, 0xf9, 0x81, 0x5a,
    0x00, 0x00, 0x78, 0x58, 0xfc, 0xf8, 0x63, 0x1e, 0x6d, 0xf9, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45,
    0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
];

#[cfg(test)]
mod tests {
    use super::boreal_icon_rgba;

    #[test]
    fn generated_icon_has_expected_dimensions_and_visible_pixels() {
        let icon = boreal_icon_rgba(32);
        assert_eq!(icon.len(), 32 * 32 * 4);
        let (pixels, remainder) = icon.as_chunks::<4>();
        assert!(remainder.is_empty());
        assert!(pixels.iter().any(|pixel| pixel[3] != 0));
        assert!(pixels.iter().any(|pixel| *pixel == [255, 255, 255, 255]));
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
#[derive(Debug)]
pub struct BorealTray;

#[cfg(all(unix, not(target_os = "macos")))]
impl ksni::Tray for BorealTray {
    fn id(&self) -> String {
        "boreal".to_string()
    }

    fn title(&self) -> String {
        "BOREAL".to_string()
    }

    fn icon_pixmap(&self) -> Vec<ksni::Icon> {
        [16_u32, 32, 64]
            .into_iter()
            .map(|size| {
                let rgba = boreal_icon_rgba(size);
                let mut argb = Vec::with_capacity(rgba.len());
                let (pixels, remainder) = rgba.as_chunks::<4>();
                debug_assert!(remainder.is_empty());
                for pixel in pixels {
                    argb.extend_from_slice(&[pixel[3], pixel[0], pixel[1], pixel[2]]);
                }
                ksni::Icon {
                    width: size as i32,
                    height: size as i32,
                    data: argb,
                }
            })
            .collect()
    }

    fn activate(&mut self, _x: i32, _y: i32) {
        open_boreal();
    }

    fn menu(&self) -> Vec<ksni::MenuItem<Self>> {
        use ksni::menu::{MenuItem, StandardItem};
        vec![
            StandardItem {
                label: "Open BOREAL".to_string(),
                icon_data: BOREAL_MENU_ICON_PNG.to_vec(),
                activate: Box::new(|_| open_boreal()),
                ..Default::default()
            }
            .into(),
            MenuItem::Separator,
            StandardItem {
                label: "Quit BOREAL".to_string(),
                icon_name: "application-exit".to_string(),
                activate: Box::new(|_| request_quit()),
                ..Default::default()
            }
            .into(),
        ]
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
pub async fn start_linux_tray() -> Option<ksni::Handle<BorealTray>> {
    use ksni::TrayMethods;
    match BorealTray.spawn().await {
        Ok(handle) => {
            log::info!("BOREAL desktop tray started");
            Some(handle)
        }
        Err(error) => {
            log::warn!("BOREAL desktop tray is unavailable: {error}");
            None
        }
    }
}

pub fn pick_folder(title: &str) -> Option<PathBuf> {
    #[cfg(not(target_os = "macos"))]
    {
        rfd::FileDialog::new().set_title(title).pick_folder()
    }

    #[cfg(target_os = "macos")]
    {
        native::pick_folder(title)
    }
}

#[cfg(any(target_os = "windows", target_os = "macos"))]
pub use native::run_native;

#[cfg(any(target_os = "windows", target_os = "macos"))]
mod native {
    use super::*;
    #[cfg(target_os = "macos")]
    use std::sync::mpsc;
    use tao::{
        event::{Event, StartCause},
        event_loop::{ControlFlow, EventLoopBuilder, EventLoopProxy},
    };
    use tray_icon::{
        Icon, TrayIconBuilder, TrayIconEvent,
        menu::{
            Icon as MenuIcon, IconMenuItem, Menu, MenuEvent, MenuId, MenuItem, PredefinedMenuItem,
        },
    };

    enum DesktopEvent {
        Menu(MenuEvent),
        Tray(TrayIconEvent),
        #[cfg(target_os = "macos")]
        PickFolder(String, mpsc::Sender<Option<PathBuf>>),
        BackendStopped,
    }

    static EVENT_PROXY: OnceLock<EventLoopProxy<DesktopEvent>> = OnceLock::new();

    struct MenuIds {
        open: MenuId,
        quit: MenuId,
    }

    fn create_tray() -> Result<(tray_icon::TrayIcon, MenuIds), String> {
        let menu = Menu::new();
        let open_icon =
            MenuIcon::from_rgba(boreal_icon_rgba(16), 16, 16).map_err(|error| error.to_string())?;
        let open = IconMenuItem::new("Open BOREAL", true, Some(open_icon), None);
        let quit = MenuItem::new("Quit BOREAL", true, None);
        menu.append_items(&[&open, &PredefinedMenuItem::separator(), &quit])
            .map_err(|error| error.to_string())?;
        let ids = MenuIds {
            open: open.id().clone(),
            quit: quit.id().clone(),
        };
        let icon =
            Icon::from_rgba(boreal_icon_rgba(32), 32, 32).map_err(|error| error.to_string())?;
        let tray = TrayIconBuilder::new()
            .with_tooltip("BOREAL")
            .with_menu(Box::new(menu))
            .with_icon(icon)
            .build()
            .map_err(|error| error.to_string())?;
        Ok((tray, ids))
    }

    fn handle_menu(event: MenuEvent, ids: &MenuIds) {
        if event.id == ids.open {
            open_boreal();
        } else if event.id == ids.quit {
            request_quit();
        }
    }

    pub fn run_native<F>(backend: F) -> !
    where
        F: FnOnce() + Send + 'static,
    {
        let event_loop = EventLoopBuilder::<DesktopEvent>::with_user_event().build();
        let proxy = event_loop.create_proxy();
        let _ = EVENT_PROXY.set(proxy.clone());

        MenuEvent::set_event_handler(Some({
            let proxy = proxy.clone();
            move |event| {
                let _ = proxy.send_event(DesktopEvent::Menu(event));
            }
        }));
        TrayIconEvent::set_event_handler(Some({
            let proxy = proxy.clone();
            move |event| {
                let _ = proxy.send_event(DesktopEvent::Tray(event));
            }
        }));

        std::thread::spawn(move || {
            backend();
            let _ = proxy.send_event(DesktopEvent::BackendStopped);
        });

        let mut tray = None;
        let mut ids = None;
        event_loop.run(move |event, _, control_flow| {
            *control_flow = ControlFlow::Wait;
            match event {
                Event::NewEvents(StartCause::Init) => match create_tray() {
                    Ok((created_tray, created_ids)) => {
                        tray = Some(created_tray);
                        ids = Some(created_ids);
                        log::info!("BOREAL desktop tray started");
                    }
                    Err(error) => log::error!("Unable to start the BOREAL desktop tray: {error}"),
                },
                Event::UserEvent(DesktopEvent::Menu(event)) => {
                    if let Some(ids) = &ids {
                        handle_menu(event, ids);
                    }
                }
                Event::UserEvent(DesktopEvent::Tray(event)) => {
                    if matches!(
                        event,
                        TrayIconEvent::Click {
                            button: tray_icon::MouseButton::Left,
                            button_state: tray_icon::MouseButtonState::Up,
                            ..
                        }
                    ) {
                        open_boreal();
                    }
                }
                #[cfg(target_os = "macos")]
                Event::UserEvent(DesktopEvent::PickFolder(title, sender)) => {
                    let selected = rfd::FileDialog::new().set_title(title).pick_folder();
                    let _ = sender.send(selected);
                }
                Event::UserEvent(DesktopEvent::BackendStopped) => {
                    tray.take();
                    *control_flow = ControlFlow::Exit;
                }
                _ => {}
            }
        })
    }

    #[cfg(target_os = "macos")]
    pub fn pick_folder(title: &str) -> Option<PathBuf> {
        let proxy = EVENT_PROXY.get()?;
        let (sender, receiver) = mpsc::channel();
        proxy
            .send_event(DesktopEvent::PickFolder(title.to_string(), sender))
            .ok()?;
        receiver.recv().ok().flatten()
    }
}
