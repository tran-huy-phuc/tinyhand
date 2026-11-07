//! Strings shown by the Rust side: the tray menu, its tooltip and window
//! titles. The pages' strings live in ui/i18n.js.

pub struct Text {
    pub turn_on: &'static str,
    pub turn_off: &'static str,
    pub clear: &'static str,
    pub settings: &'static str,
    pub quit: &'static str,
    pub tooltip_on: &'static str,
    pub settings_title: &'static str,
}

const EN: Text = Text {
    turn_on: "Turn pointer on\tCtrl+Alt+H",
    turn_off: "Turn pointer off\tCtrl+Alt+H",
    clear: "Clear drawing\tCtrl+Alt+X",
    settings: "Settings…",
    quit: "Quit TinyHand",
    tooltip_on: "TinyHand: on",
    settings_title: "TinyHand Settings",
};

const VI: Text = Text {
    turn_on: "Bật con trỏ\tCtrl+Alt+H",
    turn_off: "Tắt con trỏ\tCtrl+Alt+H",
    clear: "Xóa nét vẽ\tCtrl+Alt+X",
    settings: "Cài đặt…",
    quit: "Thoát TinyHand",
    tooltip_on: "TinyHand: đang bật",
    settings_title: "Cài đặt TinyHand",
};

/// A menu item's text. Windows shows the shortcut after a tab; macOS would
/// show the tab, and its menus are read differently anyway.
pub fn item(text: &'static str) -> &'static str {
    if cfg!(target_os = "macos") {
        text.split('\t').next().unwrap_or(text)
    } else {
        text
    }
}

/// `lang` is the saved choice; empty (not chosen yet) uses English.
pub fn text(lang: &str) -> &'static Text {
    match lang {
        "vi" => &VI,
        _ => &EN,
    }
}
