//! UI スレッドとホットキースレッドで共有する状態と、練習機能の操作

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use crate::config::Config;
use crate::game::{Game, Position, WarpPoint, POD_LASER};
use crate::hotkeys::{foreground_pid, vk, Combo, Poller};
use crate::patches::{self, PatchId};
use crate::warps::WarpSet;

/// ワープ後にノークリップを戻すまでの時間 (CE 版の sleep(150) と同じ)
const NOCLIP_HOLD: Duration = Duration::from_millis(150);
const LOG_MAX: usize = 200;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Action {
    TogglePatch(PatchId),
    ToggleWs,
    ToggleCustomWarps,
    SaveCustom(usize),
    WarpCustom(usize),
    SelectSet(WarpSet),
    Warp(WarpSet, usize),
}

/// UI 表示用に定期的に読む値
#[derive(Default, Clone)]
pub struct Status {
    pub pos: Option<Position>,
    pub weapon_set: Option<u32>,
    pub health: Option<u32>,
    pub pods: [Option<u32>; 3],
}

pub struct Core {
    pub game: Option<Game>,
    pub cfg: Config,
    pub data_dir: PathBuf,
    pub logs: VecDeque<String>,
    pub status: Status,
    /// ワープのために一時的に ON にしたノークリップを戻す時刻
    noclip_off_at: Option<Instant>,
}

pub type Shared = Arc<Mutex<Core>>;

impl Core {
    pub fn new(data_dir: PathBuf) -> Self {
        let cfg = Config::load(&data_dir);
        Self {
            game: None,
            cfg,
            data_dir,
            logs: VecDeque::new(),
            status: Status::default(),
            noclip_off_at: None,
        }
    }

    pub fn log(&mut self, msg: impl Into<String>) {
        if self.logs.len() >= LOG_MAX {
            self.logs.pop_front();
        }
        self.logs.push_back(msg.into());
    }

    pub fn save_config(&mut self) {
        if let Err(e) = self.cfg.save(&self.data_dir) {
            self.log(format!("設定を保存できません: {e}"));
        }
    }

    pub fn is_patch_active(&self, id: PatchId) -> bool {
        self.game.as_ref().is_some_and(|g| g.is_active(id))
    }

    pub fn set_patch(&mut self, id: PatchId, on: bool) {
        let Some(game) = self.game.as_mut() else {
            self.log("ゲームに接続されていません");
            return;
        };
        if id == PatchId::NoClip {
            // 手動操作が優先。ワープ用の自動 OFF は取り消す
            self.noclip_off_at = None;
        }
        let name = patches::def(id).name;
        match game.set_patch(id, on) {
            Ok(()) => self.log(format!("{name}: {}", if on { "ON" } else { "OFF" })),
            Err(e) => self.log(format!("{e}")),
        }
    }

    pub fn toggle_patch(&mut self, id: PatchId) {
        let on = !self.is_patch_active(id);
        self.set_patch(id, on);
    }

    /// ノークリップを一瞬 ON にしてから座標を書き換える (壁や床に引っかからないように)
    fn teleport(&mut self, label: &str, apply: impl FnOnce(&Game) -> anyhow::Result<()>) {
        let Some(game) = self.game.as_mut() else {
            self.log("ゲームに接続されていません");
            return;
        };
        let temp = self.noclip_off_at.is_some();
        if !game.is_active(PatchId::NoClip) || temp {
            if let Err(e) = game.set_patch(PatchId::NoClip, true) {
                self.log(format!("{e}"));
                return;
            }
            self.noclip_off_at = Some(Instant::now() + NOCLIP_HOLD);
        }
        match apply(game) {
            Ok(()) => self.log(format!("ワープ: {label}")),
            Err(e) => self.log(format!("ワープ失敗: {e}")),
        }
    }

    pub fn warp_to(&mut self, set: WarpSet, index: usize) {
        let Some(w) = set.warps().get(index) else { return };
        let pos = Position { x: w.pos[0], y: w.pos[1], z: w.pos[2] };
        self.teleport(w.name, |g| g.set_position(pos));
    }

    pub fn save_custom(&mut self, slot: usize) {
        match self.game.as_ref().and_then(Game::warp_point) {
            Some(wp) => {
                self.cfg.custom_warps[slot] = Some(wp);
                self.save_config();
                self.log(format!(
                    "カスタム{} 保存: ({:.2}, {:.2}, {:.2})",
                    slot + 1,
                    wp.pos.x,
                    wp.pos.y,
                    wp.pos.z
                ));
            }
            None => self.log("座標を読めません (未接続かロード中)"),
        }
    }

    pub fn warp_custom(&mut self, slot: usize) {
        let Some(wp) = self.cfg.custom_warps[slot] else {
            self.log(format!("カスタム{} は未保存です", slot + 1));
            return;
        };
        let label = format!("カスタム{}", slot + 1);
        self.teleport(&label, |g: &Game| g.set_warp_point(&wp));
    }

    pub fn set_custom_point(&mut self, slot: usize, wp: Option<WarpPoint>) {
        self.cfg.custom_warps[slot] = wp;
        self.save_config();
    }

    pub fn toggle_ws(&mut self) {
        let Some(game) = self.game.as_ref() else { return };
        match game.weapon_set() {
            Some(v @ (0 | 1)) => {
                game.set_weapon_set(1 - v);
                self.log(format!("武器セット → {}", 2 - v));
            }
            Some(v) => self.log(format!("武器セットの値が想定外です: {v}")),
            None => self.log("武器セットを読めません"),
        }
    }

    pub fn set_pod_program(&mut self, pod: usize, v: u32) {
        if let Some(g) = self.game.as_ref() {
            g.set_pod_program(pod, v);
        }
    }

    /// 現在有効なホットキー
    pub fn bindings(&self) -> Vec<(Combo, Action)> {
        let mut b = vec![
            (Combo::key(vk::NUMPAD0), Action::TogglePatch(PatchId::NoClip)),
            (Combo::key(vk::DECIMAL), Action::TogglePatch(PatchId::InfiniteDoubleJump)),
            (Combo::key(vk::SUBTRACT), Action::TogglePatch(PatchId::InfiniteDash)),
            (Combo::key(vk::OEM_PLUS), Action::ToggleCustomWarps),
            (Combo::ctrl_shift(vk::D), Action::SelectSet(WarpSet::RedDots)),
            (Combo::ctrl_shift(vk::A), Action::SelectSet(WarpSet::AEnding)),
        ];
        if self.cfg.ws_toggle_enabled {
            b.push((Combo::key(vk::ADD), Action::ToggleWs));
        }
        if self.cfg.custom_warps_enabled {
            b.push((Combo::shift(vk::C), Action::SaveCustom(0)));
            b.push((Combo::key(vk::Z), Action::WarpCustom(0)));
            b.push((Combo::shift(vk::V), Action::SaveCustom(1)));
            b.push((Combo::key(vk::X), Action::WarpCustom(1)));
        }
        let set = self.cfg.warp_set;
        for (i, w) in set.warps().iter().enumerate() {
            b.push((Combo::key(w.key), Action::Warp(set, i)));
        }
        b
    }

    pub fn handle(&mut self, action: Action) {
        match action {
            Action::TogglePatch(id) => self.toggle_patch(id),
            Action::ToggleWs => self.toggle_ws(),
            Action::ToggleCustomWarps => {
                self.cfg.custom_warps_enabled = !self.cfg.custom_warps_enabled;
                let on = self.cfg.custom_warps_enabled;
                self.log(format!("カスタムワープ: {}", if on { "ON" } else { "OFF" }));
                self.save_config();
            }
            Action::SaveCustom(i) => self.save_custom(i),
            Action::WarpCustom(i) => self.warp_custom(i),
            Action::SelectSet(set) => {
                // 同じキーをもう一度押すと解除 (CE のスクリプト ON/OFF と同じ感覚)
                self.cfg.warp_set = if self.cfg.warp_set == set { WarpSet::None } else { set };
                let label = self.cfg.warp_set.label();
                self.log(format!("ワープセット: {label}"));
                self.save_config();
            }
            Action::Warp(set, i) => self.warp_to(set, i),
        }
    }

    fn update_connection(&mut self) {
        if let Some(g) = &self.game {
            if !g.proc.is_alive() {
                self.game = None;
                self.noclip_off_at = None;
                self.status = Status::default();
                self.log("ゲームが終了しました");
            }
            return;
        }
        if let Some(mut g) = Game::attach() {
            g.sync_active();
            let pid = g.proc.pid;
            let remaining: Vec<_> = g.active_patches().map(|id| patches::def(id).name).collect();
            self.game = Some(g);
            self.log(format!("ゲームに接続しました (PID {pid})"));
            if !remaining.is_empty() {
                self.log(format!("有効のまま残っていたパッチ: {}", remaining.join(", ")));
            }
        }
    }

    fn refresh_status(&mut self) {
        let Some(g) = self.game.as_ref() else { return };
        self.status = Status {
            pos: g.position(),
            weapon_set: g.weapon_set(),
            health: g.health(),
            pods: [g.pod_program(0), g.pod_program(1), g.pod_program(2)],
        };
    }

    fn tick(&mut self) {
        if let Some(t) = self.noclip_off_at {
            if Instant::now() >= t {
                self.noclip_off_at = None;
                if let Some(g) = self.game.as_mut() {
                    if let Err(e) = g.set_patch(PatchId::NoClip, false) {
                        self.log(format!("{e}"));
                    }
                }
            }
        }
        if self.cfg.pod_laser_lock {
            if let Some(g) = self.game.as_ref() {
                if g.pod_program(0).is_some_and(|v| v != POD_LASER) {
                    g.set_pod_program(0, POD_LASER);
                }
            }
        }
    }

    /// 終了時: パッチを全部戻す
    pub fn shutdown(&mut self) {
        if let Some(g) = self.game.as_mut() {
            g.restore_all();
        }
    }
}

/// ホットキー監視・接続管理・定期処理を行うスレッド
pub fn spawn_worker(shared: Shared, repaint: impl Fn() + Send + 'static) {
    thread::spawn(move || {
        let mut poller = Poller::default();
        let mut last_attach = Instant::now() - Duration::from_secs(10);
        let mut last_status = Instant::now();
        loop {
            thread::sleep(Duration::from_millis(10));
            let Ok(mut core) = shared.lock() else { return };

            if last_attach.elapsed() >= Duration::from_secs(1) {
                last_attach = Instant::now();
                core.update_connection();
            }

            let bindings = core.bindings();
            let combos: Vec<Combo> = bindings.iter().map(|(c, _)| *c).collect();
            let fired = poller.poll(&combos);
            let game_pid = core.game.as_ref().map(|g| g.proc.pid);
            let allowed = !core.cfg.hotkeys_game_only
                || (game_pid.is_some() && foreground_pid() == game_pid);
            if allowed {
                for c in fired {
                    if let Some((_, a)) = bindings.iter().find(|(b, _)| *b == c) {
                        core.handle(*a);
                    }
                }
            }

            core.tick();
            if last_status.elapsed() >= Duration::from_millis(50) {
                last_status = Instant::now();
                core.refresh_status();
                drop(core);
                repaint();
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn core() -> Core {
        Core::new(std::env::temp_dir().join("nier-practice-no-such-dir"))
    }

    #[test]
    fn bindings_have_no_duplicates() {
        let mut c = core();
        c.cfg.custom_warps_enabled = true;
        c.cfg.ws_toggle_enabled = true;
        for set in [WarpSet::RedDots, WarpSet::AEnding] {
            c.cfg.warp_set = set;
            let b = c.bindings();
            for (i, (x, _)) in b.iter().enumerate() {
                for (y, _) in &b[i + 1..] {
                    assert_ne!(x, y, "{}", x.label());
                }
            }
        }
    }

    #[test]
    fn select_set_toggles() {
        let mut c = core();
        c.handle(Action::SelectSet(WarpSet::RedDots));
        assert_eq!(c.cfg.warp_set, WarpSet::RedDots);
        c.handle(Action::SelectSet(WarpSet::AEnding));
        assert_eq!(c.cfg.warp_set, WarpSet::AEnding);
        c.handle(Action::SelectSet(WarpSet::AEnding));
        assert_eq!(c.cfg.warp_set, WarpSet::None);
    }
}
