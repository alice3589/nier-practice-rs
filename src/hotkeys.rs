//! グローバルホットキー (GetAsyncKeyState のポーリング。CE の isKeyPressed と同じ方式)

use std::collections::HashMap;

use windows_sys::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;
use windows_sys::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};

/// 仮想キーコード
pub mod vk {
    pub const SHIFT: u16 = 0x10;
    pub const CONTROL: u16 = 0x11;
    pub const MENU: u16 = 0x12;
    pub const K0: u16 = 0x30;
    pub const K1: u16 = 0x31;
    pub const K2: u16 = 0x32;
    pub const K3: u16 = 0x33;
    pub const K4: u16 = 0x34;
    pub const K5: u16 = 0x35;
    pub const K6: u16 = 0x36;
    pub const K7: u16 = 0x37;
    pub const K8: u16 = 0x38;
    pub const K9: u16 = 0x39;
    pub const A: u16 = 0x41;
    pub const C: u16 = 0x43;
    pub const D: u16 = 0x44;
    pub const V: u16 = 0x56;
    pub const X: u16 = 0x58;
    pub const Z: u16 = 0x5A;
    pub const NUMPAD0: u16 = 0x60;
    pub const NUMPAD1: u16 = 0x61;
    pub const NUMPAD2: u16 = 0x62;
    pub const NUMPAD3: u16 = 0x63;
    pub const NUMPAD4: u16 = 0x64;
    pub const NUMPAD5: u16 = 0x65;
    pub const NUMPAD6: u16 = 0x66;
    pub const NUMPAD7: u16 = 0x67;
    pub const NUMPAD8: u16 = 0x68;
    pub const NUMPAD9: u16 = 0x69;
    pub const ADD: u16 = 0x6B;
    pub const SUBTRACT: u16 = 0x6D;
    pub const DECIMAL: u16 = 0x6E;
    pub const F1: u16 = 0x70;
    pub const F2: u16 = 0x71;
    pub const F3: u16 = 0x72;
    pub const F4: u16 = 0x73;
    pub const F5: u16 = 0x74;
    pub const F6: u16 = 0x75;
    pub const F7: u16 = 0x76;
    pub const OEM_PLUS: u16 = 0xBB;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Combo {
    pub key: u16,
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
}

impl Combo {
    pub const fn key(key: u16) -> Self {
        Self { key, ctrl: false, shift: false, alt: false }
    }
    pub const fn shift(key: u16) -> Self {
        Self { key, ctrl: false, shift: true, alt: false }
    }
    pub const fn ctrl_shift(key: u16) -> Self {
        Self { key, ctrl: true, shift: true, alt: false }
    }

    pub fn label(&self) -> String {
        let mut s = String::new();
        if self.ctrl {
            s.push_str("Ctrl+");
        }
        if self.shift {
            s.push_str("Shift+");
        }
        if self.alt {
            s.push_str("Alt+");
        }
        s.push_str(&key_name(self.key));
        s
    }
}

pub fn key_name(key: u16) -> String {
    match key {
        0x30..=0x39 | 0x41..=0x5A => (key as u8 as char).to_string(),
        0x60..=0x69 => format!("Num{}", key - 0x60),
        vk::ADD => "Num+".into(),
        vk::SUBTRACT => "Num-".into(),
        vk::DECIMAL => "Num.".into(),
        0x70..=0x87 => format!("F{}", key - 0x6F),
        vk::OEM_PLUS => "=".into(),
        _ => format!("VK{key:#04X}"),
    }
}

fn is_down(key: u16) -> bool {
    unsafe { (GetAsyncKeyState(key as i32) as u16 & 0x8000) != 0 }
}

/// 押された瞬間 (立ち上がり) だけを検出する
#[derive(Default)]
pub struct Poller {
    prev: HashMap<Combo, bool>,
}

impl Poller {
    /// 今回のポーリングで新たに押されたコンボを返す。
    /// 修飾キーは完全一致 (Shift+C と C を区別する)。
    pub fn poll(&mut self, combos: &[Combo]) -> Vec<Combo> {
        let ctrl = is_down(vk::CONTROL);
        let shift = is_down(vk::SHIFT);
        let alt = is_down(vk::MENU);
        let mut fired = Vec::new();
        for &c in combos {
            let down = is_down(c.key) && c.ctrl == ctrl && c.shift == shift && c.alt == alt;
            let was = self.prev.insert(c, down).unwrap_or(false);
            if down && !was {
                fired.push(c);
            }
        }
        fired
    }
}

/// 前面ウィンドウのプロセス ID
pub fn foreground_pid() -> Option<u32> {
    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.is_null() {
            return None;
        }
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, &mut pid);
        (pid != 0).then_some(pid)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels() {
        assert_eq!(Combo::shift(vk::C).label(), "Shift+C");
        assert_eq!(Combo::ctrl_shift(vk::D).label(), "Ctrl+Shift+D");
        assert_eq!(Combo::key(vk::NUMPAD0).label(), "Num0");
        assert_eq!(Combo::key(vk::F7).label(), "F7");
    }
}
