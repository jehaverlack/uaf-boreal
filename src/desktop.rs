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

const GOOGLE_DRIVE_URL: &str = "https://drive.google.com/drive/quota";
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

fn open_google_drive() {
    open_url(GOOGLE_DRIVE_URL, "Google Drive");
}

fn open_rclone() {
    open_url(&format!("{}/rclone-gui", web_url()), "the Rclone WebGUI");
}

fn request_quit() {
    QUIT_REQUESTED.store(true, Ordering::Release);
    let state = APP_STATE
        .get()
        .and_then(|value| value.lock().ok())
        .and_then(|value| value.upgrade());
    if let Some(state) = state {
        log::info!("Quit requested from the BOREAL desktop menu");
        state.request_shutdown();
    }
}

/// A compact BOREAL folder/link mark rendered directly into the executable.
/// This avoids platform-specific icon files for the live tray icon.
fn boreal_icon_rgba(size: u32) -> Vec<u8> {
    let mut pixels = vec![0_u8; (size * size * 4) as usize];
    let set = |pixels: &mut [u8], x: u32, y: u32, color: [u8; 4]| {
        let offset = ((y * size + x) * 4) as usize;
        pixels[offset..offset + 4].copy_from_slice(&color);
    };
    let scale = size as f32 / 32.0;
    let inside = |x: u32, y: u32, left: f32, top: f32, right: f32, bottom: f32| {
        (x as f32) >= left * scale
            && (x as f32) < right * scale
            && (y as f32) >= top * scale
            && (y as f32) < bottom * scale
    };

    for y in 0..size {
        for x in 0..size {
            if inside(x, y, 3.0, 8.0, 29.0, 26.0) || inside(x, y, 5.0, 5.0, 15.0, 11.0) {
                set(&mut pixels, x, y, [13, 110, 253, 255]);
            }
            if inside(x, y, 5.0, 11.0, 27.0, 24.0) {
                set(&mut pixels, x, y, [25, 135, 250, 255]);
            }
        }
    }

    // White linked-arrow mark inside the folder.
    for step in 0..12_u32 {
        let x = ((9.0 + step as f32) * scale) as u32;
        let y = ((21.0 - step as f32 * 0.65) * scale) as u32;
        let thickness = scale.max(1.0) as u32;
        for offset in 0..thickness {
            if x < size && y + offset < size {
                set(&mut pixels, x, y + offset, [255, 255, 255, 255]);
            }
        }
    }
    for step in 0..6_u32 {
        let x = ((18.0 + step as f32) * scale) as u32;
        let upper = ((13.0 + step as f32) * scale) as u32;
        let lower = ((13.0 + (5 - step) as f32) * scale) as u32;
        if x < size && upper < size {
            set(&mut pixels, x, upper, [255, 255, 255, 255]);
        }
        if x < size && lower < size {
            set(&mut pixels, x, lower, [255, 255, 255, 255]);
        }
    }
    pixels
}

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
                icon_name: "web-browser".to_string(),
                activate: Box::new(|_| open_boreal()),
                ..Default::default()
            }
            .into(),
            StandardItem {
                label: "Open Google Drive".to_string(),
                icon_name: "folder-remote".to_string(),
                activate: Box::new(|_| open_google_drive()),
                ..Default::default()
            }
            .into(),
            StandardItem {
                label: "Open Rclone".to_string(),
                icon_name: "folder-sync".to_string(),
                activate: Box::new(|_| open_rclone()),
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
        menu::{Menu, MenuEvent, MenuId, MenuItem, PredefinedMenuItem},
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
        drive: MenuId,
        rclone: MenuId,
        quit: MenuId,
    }

    fn create_tray() -> Result<(tray_icon::TrayIcon, MenuIds), String> {
        let menu = Menu::new();
        let open = MenuItem::new("Open BOREAL", true, None);
        let drive = MenuItem::new("Open Google Drive", true, None);
        let rclone = MenuItem::new("Open Rclone", true, None);
        let quit = MenuItem::new("Quit BOREAL", true, None);
        menu.append_items(&[
            &open,
            &drive,
            &rclone,
            &PredefinedMenuItem::separator(),
            &quit,
        ])
        .map_err(|error| error.to_string())?;
        let ids = MenuIds {
            open: open.id().clone(),
            drive: drive.id().clone(),
            rclone: rclone.id().clone(),
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
        } else if event.id == ids.drive {
            open_google_drive();
        } else if event.id == ids.rclone {
            open_rclone();
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
