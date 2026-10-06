use std::{
    env,
    ffi::{CString, c_char, c_int, c_void},
    io::{Read, Write},
    os::unix::net::UnixStream,
    path::PathBuf,
    time::Duration,
};

use serde::Deserialize;

#[link(name = "ayatana-appindicator3")]
unsafe extern "C" {
    fn app_indicator_new(
        id: *const c_char,
        icon_name: *const c_char,
        category: c_int,
    ) -> *mut c_void;
    fn app_indicator_set_status(indicator: *mut c_void, status: c_int);
    fn app_indicator_set_menu(indicator: *mut c_void, menu: *mut c_void);
}

#[link(name = "gtk-3")]
unsafe extern "C" {
    fn gtk_init(argc: *mut c_int, argv: *mut *mut *mut c_char);
    fn gtk_menu_new() -> *mut c_void;
    fn gtk_menu_item_new_with_label(label: *const c_char) -> *mut c_void;
    fn gtk_menu_shell_append(menu_shell: *mut c_void, child: *mut c_void);
    fn gtk_widget_show(widget: *mut c_void);
    fn gtk_widget_hide(widget: *mut c_void);
    fn gtk_widget_show_all(widget: *mut c_void);
    fn gtk_menu_item_set_label(menu_item: *mut c_void, label: *const c_char);
    fn gtk_main();
    fn gtk_main_quit();
}

#[link(name = "gobject-2.0")]
unsafe extern "C" {
    fn g_signal_connect_data(
        instance: *mut c_void,
        signal: *const c_char,
        callback: *mut c_void,
        data: *mut c_void,
        destroy: *mut c_void,
        flags: c_int,
    ) -> u64;
}

#[link(name = "glib-2.0")]
unsafe extern "C" {
    fn g_timeout_add_seconds(
        interval: u32,
        callback: Option<unsafe extern "C" fn(*mut c_void) -> c_int>,
        data: *mut c_void,
    ) -> u32;
}

#[derive(Debug, Deserialize)]
struct Snapshot {
    devices: Vec<Device>,
}

#[derive(Debug, Deserialize)]
struct Device {
    name: Option<String>,
    connected: bool,
    single_battery: bool,
    left: Component,
    right: Component,
    case: Component,
}

#[derive(Debug, Deserialize)]
struct Component {
    level: Option<u8>,
    charging: Option<bool>,
}

struct AppState {
    status_rows: Vec<*mut c_void>,
}

fn cstring(value: &str) -> CString {
    CString::new(value.replace('\0', "")).expect("sanitized C string")
}

fn socket_path() -> PathBuf {
    let runtime_dir = env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(env::temp_dir);
    runtime_dir.join(format!("nurapods-{}.sock", unsafe { libc::getuid() }))
}

fn component(label: &str, part: &Component) -> String {
    match (part.level, part.charging) {
        (Some(level), Some(true)) => format!("{label} {level}% ⚡"),
        (Some(level), _) => format!("{label} {level}%"),
        (None, _) => format!("{label} —"),
    }
}

fn fetch_status() -> Result<Vec<String>, String> {
    let mut stream = UnixStream::connect(socket_path()).map_err(|e| e.to_string())?;
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .map_err(|e| e.to_string())?;
    stream.write_all(b"json").map_err(|e| e.to_string())?;
    stream
        .shutdown(std::net::Shutdown::Write)
        .map_err(|e| e.to_string())?;
    let mut response = String::new();
    stream
        .read_to_string(&mut response)
        .map_err(|e| e.to_string())?;
    let snapshot: Snapshot = serde_json::from_str(&response).map_err(|e| e.to_string())?;
    if snapshot.devices.is_empty() {
        return Ok(vec!["AirPods non rilevati".into()]);
    }
    Ok(snapshot
        .devices
        .iter()
        .map(|device| {
            let name = device.name.as_deref().unwrap_or("AirPods");
            let state = if device.connected {
                "connessi"
            } else {
                "rilevati"
            };
            if device.single_battery {
                format!("{name} ({state}) · {}", component("Cuffie", &device.left))
            } else {
                format!(
                    "{name} ({state}) · {} · {} · {}",
                    component("Sx", &device.left),
                    component("Dx", &device.right),
                    component("Custodia", &device.case)
                )
            }
        })
        .collect())
}

unsafe fn refresh(state: &mut AppState) {
    let lines = fetch_status().unwrap_or_else(|_| {
        vec![
            "Demone NuraPods non avviato".into(),
            "Controlla: systemctl --user status nurapods-daemon".into(),
        ]
    });
    for (index, row) in state.status_rows.iter().enumerate() {
        if let Some(line) = lines.get(index) {
            let label = cstring(line);
            unsafe {
                gtk_menu_item_set_label(*row, label.as_ptr());
                gtk_widget_show(*row);
            }
        } else {
            unsafe {
                gtk_widget_hide(*row);
            }
        }
    }
}

unsafe extern "C" fn on_refresh(_item: *mut c_void, data: *mut c_void) {
    unsafe {
        refresh(&mut *(data as *mut AppState));
    }
}

unsafe extern "C" fn on_quit(_item: *mut c_void, _data: *mut c_void) {
    unsafe {
        gtk_main_quit();
    }
}

unsafe extern "C" fn on_timer(data: *mut c_void) -> c_int {
    unsafe {
        refresh(&mut *(data as *mut AppState));
    }
    1
}

unsafe fn add_item(menu: *mut c_void, label: &str) -> *mut c_void {
    let label = cstring(label);
    let item = unsafe { gtk_menu_item_new_with_label(label.as_ptr()) };
    unsafe {
        gtk_menu_shell_append(menu, item);
    }
    item
}

unsafe fn connect(
    item: *mut c_void,
    signal: &str,
    callback: unsafe extern "C" fn(*mut c_void, *mut c_void),
    data: *mut c_void,
) {
    let signal = cstring(signal);
    let callback = callback as *const () as *mut c_void;
    unsafe {
        g_signal_connect_data(
            item,
            signal.as_ptr(),
            callback,
            data,
            std::ptr::null_mut(),
            0,
        );
    }
}

fn main() {
    unsafe {
        gtk_init(std::ptr::null_mut(), std::ptr::null_mut());
        let id = cstring("NuraPods");
        let icon = cstring("audio-headphones");
        let indicator = app_indicator_new(id.as_ptr(), icon.as_ptr(), 0);
        app_indicator_set_status(indicator, 0);
        let menu = gtk_menu_new();
        let mut state = Box::new(AppState {
            status_rows: Vec::new(),
        });
        let state_ptr = (&mut *state) as *mut AppState as *mut c_void;
        for _ in 0..5 {
            state.status_rows.push(add_item(menu, "Caricamento stato…"));
        }
        let refresh_item = add_item(menu, "Aggiorna ora");
        let quit_item = add_item(menu, "Esci");
        connect(refresh_item, "activate", on_refresh, state_ptr);
        connect(quit_item, "activate", on_quit, state_ptr);
        app_indicator_set_menu(indicator, menu);
        gtk_widget_show_all(menu);
        refresh(&mut state);
        g_timeout_add_seconds(3, Some(on_timer), state_ptr);
        gtk_main();
    }
}
