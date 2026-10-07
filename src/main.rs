#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

//! NieR:Automata 練習ツール。
//! NieR-Automata-Practice-Table (aloyark) の練習機能と
//! YASM.NieRAutomata (Eveldee) のセーブ管理を 1 つの exe にまとめたもの。

mod config;
mod core;
mod game;
mod hotkeys;
mod memory;
mod patches;
mod saves;
mod warps;

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use eframe::egui;

use crate::core::{Core, Shared};
use crate::game::Position;
use crate::hotkeys::key_name;
use crate::patches::{PatchId, PATCHES};
use crate::saves::{game_save_dir, slot_path, BaseSaveInfo, SaveLibrary};
use crate::warps::WarpSet;

#[derive(PartialEq, Clone, Copy)]
enum Tab {
    Practice,
    Saves,
    Settings,
}

struct App {
    shared: Shared,
    tab: Tab,
    library: SaveLibrary,
    game_dir: Option<PathBuf>,
    new_save_name: String,
    new_save_group: String,
    pod_edit: [u32; 3],
}

fn setup_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    for path in [
        r"C:\Windows\Fonts\YuGothM.ttc",
        r"C:\Windows\Fonts\meiryo.ttc",
        r"C:\Windows\Fonts\msgothic.ttc",
    ] {
        if let Ok(bytes) = std::fs::read(path) {
            fonts.font_data.insert("jp".into(), Arc::new(egui::FontData::from_owned(bytes)));
            // 英数字と日本語のベースラインを揃えるため、本文は日本語フォントを先頭にする
            fonts.families.entry(egui::FontFamily::Proportional).or_default().insert(0, "jp".into());
            fonts.families.entry(egui::FontFamily::Monospace).or_default().push("jp".into());
            break;
        }
    }
    ctx.set_fonts(fonts);
}

/// パッチに対応するホットキー表示
fn patch_hotkey(id: PatchId) -> Option<&'static str> {
    match id {
        PatchId::NoClip => Some("Num0"),
        PatchId::InfiniteDoubleJump => Some("Num."),
        PatchId::InfiniteDash => Some("Num-"),
        _ => None,
    }
}

fn fmt_pos(p: &Position) -> String {
    format!("X {:9.3}   Y {:9.3}   Z {:9.3}", p.x, p.y, p.z)
}

impl App {
    fn new(cc: &eframe::CreationContext<'_>, shared: Shared) -> Self {
        setup_fonts(&cc.egui_ctx);
        let data_dir = shared.lock().unwrap().data_dir.clone();
        let mut library = SaveLibrary::new(data_dir.join("Saves"));
        if let Err(e) = library.reload() {
            shared.lock().unwrap().log(format!("Saves フォルダを読めません: {e}"));
        }
        let always_on_top = shared.lock().unwrap().cfg.always_on_top;
        if always_on_top {
            cc.egui_ctx.send_viewport_cmd(egui::ViewportCommand::WindowLevel(
                egui::WindowLevel::AlwaysOnTop,
            ));
        }
        Self {
            shared,
            tab: Tab::Practice,
            library,
            game_dir: game_save_dir(),
            new_save_name: String::new(),
            new_save_group: String::new(),
            pod_edit: [0; 3],
        }
    }

    fn header(&mut self, ui: &mut egui::Ui, core: &Core) {
        ui.horizontal(|ui| {
            match &core.game {
                Some(g) => ui.colored_label(
                    egui::Color32::from_rgb(90, 200, 120),
                    format!("● 接続中 (PID {})", g.proc.pid),
                ),
                None => ui.colored_label(egui::Color32::GRAY, "○ NieRAutomata.exe を待機中"),
            };
            ui.separator();
            for (t, label) in [(Tab::Practice, "練習"), (Tab::Saves, "セーブ"), (Tab::Settings, "設定")] {
                ui.selectable_value(&mut self.tab, t, label);
            }
        });
        let s = &core.status;
        ui.horizontal(|ui| {
            ui.monospace(s.pos.as_ref().map(fmt_pos).unwrap_or_else(|| "座標 ---".into()));
        });
        ui.horizontal(|ui| {
            let opt = |v: Option<u32>| v.map(|v| v.to_string()).unwrap_or_else(|| "-".into());
            ui.label(format!(
                "HP {}   武器セット {}   ポッド A/B/C {}/{}/{}",
                opt(s.health),
                s.weapon_set.map(|v| (v + 1).to_string()).unwrap_or_else(|| "-".into()),
                opt(s.pods[0]),
                opt(s.pods[1]),
                opt(s.pods[2]),
            ));
        });
    }

    fn practice_tab(&mut self, ui: &mut egui::Ui, core: &mut Core) {
        let connected = core.game.is_some();

        ui.heading("チート / 練習用パッチ");
        ui.add_enabled_ui(connected, |ui| {
            egui::Grid::new("patches").num_columns(2).striped(true).show(ui, |ui| {
                for p in PATCHES {
                    let mut on = core.is_patch_active(p.id);
                    if ui.checkbox(&mut on, p.name).on_hover_text(p.description).changed() {
                        core.set_patch(p.id, on);
                    }
                    ui.weak(patch_hotkey(p.id).unwrap_or(""));
                    ui.end_row();
                }
            });
        });

        ui.separator();
        ui.heading("武器セット / ポッド");
        ui.horizontal(|ui| {
            if ui.checkbox(&mut core.cfg.ws_toggle_enabled, "Num+ で武器セット切替").changed() {
                core.save_config();
            }
            if ui.add_enabled(connected, egui::Button::new("今すぐ切替")).clicked() {
                core.toggle_ws();
            }
        });
        if ui
            .checkbox(&mut core.cfg.pod_laser_lock, "ポッド A をレーザーに固定")
            .on_hover_text("プロローグのセーブを読み込むたびにレーザーが外れるのを防ぐ")
            .changed()
        {
            core.save_config();
        }
        ui.horizontal(|ui| {
            for (i, name) in ["A", "B", "C"].iter().enumerate() {
                ui.label(format!("ポッド{name}"));
                ui.add(egui::DragValue::new(&mut self.pod_edit[i]).range(0..=20));
                if ui.add_enabled(connected, egui::Button::new("設定")).clicked() {
                    core.set_pod_program(i, self.pod_edit[i]);
                }
            }
        });

        ui.separator();
        ui.heading("カスタムワープ");
        if ui
            .checkbox(&mut core.cfg.custom_warps_enabled, "ホットキー有効 (= で切替)")
            .changed()
        {
            core.save_config();
        }
        for slot in 0..2 {
            let (save_key, warp_key) = if slot == 0 { ("Shift+C", "Z") } else { ("Shift+V", "X") };
            ui.horizontal(|ui| {
                ui.label(format!("{}:", slot + 1));
                if ui
                    .add_enabled(connected, egui::Button::new(format!("保存 ({save_key})")))
                    .clicked()
                {
                    core.save_custom(slot);
                }
                if ui
                    .add_enabled(connected, egui::Button::new(format!("ワープ ({warp_key})")))
                    .clicked()
                {
                    core.warp_custom(slot);
                }
                match core.cfg.custom_warps[slot] {
                    Some(wp) => {
                        ui.monospace(fmt_pos(&wp.pos));
                        if ui.small_button("✕").on_hover_text("削除").clicked() {
                            core.set_custom_point(slot, None);
                        }
                    }
                    None => {
                        ui.weak("未保存");
                    }
                }
            });
        }

        ui.separator();
        ui.heading("ワープ");
        ui.horizontal(|ui| {
            let before = core.cfg.warp_set;
            ui.label("ホットキー:");
            ui.radio_value(&mut core.cfg.warp_set, WarpSet::None, "なし");
            ui.radio_value(&mut core.cfg.warp_set, WarpSet::RedDots, "Red Dots (Ctrl+Shift+D)");
            ui.radio_value(&mut core.cfg.warp_set, WarpSet::AEnding, "A Ending (Ctrl+Shift+A)");
            if core.cfg.warp_set != before {
                core.save_config();
            }
        });
        for set in [WarpSet::RedDots, WarpSet::AEnding] {
            egui::CollapsingHeader::new(set.label())
                .default_open(core.cfg.warp_set == set)
                .show(ui, |ui| {
                    ui.add_enabled_ui(connected, |ui| {
                        egui::Grid::new(set.label()).num_columns(3).show(ui, |ui| {
                            for (i, w) in set.warps().iter().enumerate() {
                                let text = format!("{}  [{}]", w.name, key_name(w.key));
                                if ui.button(text).clicked() {
                                    core.warp_to(set, i);
                                }
                                if i % 3 == 2 {
                                    ui.end_row();
                                }
                            }
                        });
                    });
                });
        }
    }

    fn saves_tab(&mut self, ui: &mut egui::Ui, core: &mut Core) {
        let Some(game_dir) = self.game_dir.clone() else {
            ui.colored_label(egui::Color32::RED, "ドキュメントフォルダが見つかりません");
            return;
        };

        ui.heading("自分の設定");
        match &core.cfg.base_save_info {
            Some(_) => ui.label("取り込み済み (Steam ID / ゲーム設定 / キー設定)"),
            None => ui.colored_label(
                egui::Color32::from_rgb(230, 180, 60),
                "未取り込み: 先に自分のセーブから設定を取り込んでください",
            ),
        };
        ui.horizontal(|ui| {
            ui.label("取り込み元:");
            for slot in 0..3 {
                let path = slot_path(&game_dir, slot);
                if ui.add_enabled(path.exists(), egui::Button::new(format!("スロット{slot}"))).clicked() {
                    match std::fs::read(&path).map_err(anyhow::Error::from).and_then(|d| BaseSaveInfo::extract(&d)) {
                        Ok(info) => {
                            core.cfg.base_save_info = Some(info);
                            core.save_config();
                            core.log(format!("スロット{slot} から設定を取り込みました"));
                        }
                        Err(e) => core.log(format!("取り込み失敗: {e}")),
                    }
                }
            }
        });

        ui.separator();
        ui.horizontal(|ui| {
            if ui.button("再読み込み").clicked() {
                if let Err(e) = self.library.reload() {
                    core.log(format!("{e}"));
                }
            }
            if ui.button("zip から取り込み").clicked() {
                if let Some(p) = rfd::FileDialog::new().add_filter("zip", &["zip"]).pick_file() {
                    match self.library.import_zip(&p) {
                        Ok(n) => core.log(format!("{n} 個のセーブを取り込みました")),
                        Err(e) => core.log(format!("zip 取り込み失敗: {e}")),
                    }
                }
            }
            if ui.button("Saves フォルダを開く").clicked() {
                let _ = std::process::Command::new("explorer").arg(&self.library.root).spawn();
            }
            if ui.button("ゲームのセーブフォルダを開く").clicked() {
                let _ = std::process::Command::new("explorer").arg(&game_dir).spawn();
            }
        });

        egui::CollapsingHeader::new("ゲームのスロットから練習セーブを作る").show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label("名前");
                ui.text_edit_singleline(&mut self.new_save_name);
            });
            ui.horizontal(|ui| {
                ui.label("グループ");
                ui.text_edit_singleline(&mut self.new_save_group);
            });
            ui.horizontal(|ui| {
                for slot in 0..3 {
                    let path = slot_path(&game_dir, slot);
                    if ui.add_enabled(path.exists(), egui::Button::new(format!("スロット{slot} から作成"))).clicked() {
                        match self.library.create_from_slot(&path, &self.new_save_group, &self.new_save_name) {
                            Ok(p) => core.log(format!("作成: {}", p.display())),
                            Err(e) => core.log(format!("作成失敗: {e}")),
                        }
                    }
                }
            });
        });

        ui.separator();
        ui.heading(format!("練習セーブ ({} 個)", self.library.saves.len()));
        if self.library.saves.is_empty() {
            ui.label("セーブがありません。zip から取り込むか、Saves フォルダに .dat を置いてください。");
            return;
        }
        let info = core.cfg.base_save_info.clone();
        if info.is_none() {
            ui.weak("※ 設定を取り込むまで読み込みはできません");
        }
        let backup_dir = core.data_dir.join("Backups");
        let mut result = None;
        {
            for (group, saves) in self.library.groups() {
                egui::CollapsingHeader::new(&group).default_open(true).show(ui, |ui| {
                    egui::Grid::new(&group).num_columns(4).striped(true).show(ui, |ui| {
                        for s in saves {
                            ui.label(&s.name);
                            for slot in 0..3 {
                                let btn = ui.add_enabled(info.is_some(), egui::Button::new(format!("→ スロット{slot}")));
                                if btn.clicked() {
                                    let r = SaveLibrary::load_into_slot(
                                        s,
                                        &slot_path(&game_dir, slot),
                                        info.as_ref().unwrap(),
                                        Some(&backup_dir),
                                    );
                                    result = Some(match r {
                                        Ok(()) => format!("「{}」をスロット{slot} に読み込みました", s.name),
                                        Err(e) => format!("読み込み失敗: {e}"),
                                    });
                                }
                            }
                            ui.end_row();
                        }
                    });
                });
            }
        }
        if let Some(msg) = result {
            core.log(msg);
        }
    }

    fn settings_tab(&mut self, ui: &mut egui::Ui, core: &mut Core) {
        if ui
            .checkbox(&mut core.cfg.hotkeys_game_only, "ゲームが前面のときだけホットキーを受け付ける")
            .changed()
        {
            core.save_config();
        }
        if ui.checkbox(&mut core.cfg.always_on_top, "常に最前面に表示").changed() {
            let level = if core.cfg.always_on_top {
                egui::WindowLevel::AlwaysOnTop
            } else {
                egui::WindowLevel::Normal
            };
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::WindowLevel(level));
            core.save_config();
        }
        ui.separator();
        ui.heading("現在のホットキー");
        let bindings = core.bindings();
        egui::Grid::new("hotkeys").striped(true).show(ui, |ui| {
            for (c, a) in bindings {
                ui.monospace(c.label());
                ui.label(format!("{a:?}"));
                ui.end_row();
            }
        });
        ui.separator();
        ui.label(format!("データフォルダ: {}", core.data_dir.display()));
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let shared = self.shared.clone();
        let mut core = shared.lock().unwrap();

        egui::TopBottomPanel::top("header").show(ctx, |ui| self.header(ui, &core));
        egui::TopBottomPanel::bottom("log")
            .resizable(true)
            .default_height(110.0)
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().stick_to_bottom(true).show(ui, |ui| {
                    for l in &core.logs {
                        ui.label(l);
                    }
                });
            });
        egui::CentralPanel::default().show(ctx, |ui| {
            egui::ScrollArea::vertical().id_salt("main").show(ui, |ui| match self.tab {
                Tab::Practice => self.practice_tab(ui, &mut core),
                Tab::Saves => self.saves_tab(ui, &mut core),
                Tab::Settings => self.settings_tab(ui, &mut core),
            });
        });
    }
}

fn main() -> eframe::Result<()> {
    let data_dir = config::data_dir();
    let shared: Shared = Arc::new(Mutex::new(Core::new(data_dir)));

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("NieR:Automata Practice Tool")
            .with_inner_size([620.0, 760.0]),
        ..Default::default()
    };
    let worker_shared = shared.clone();
    let ui_shared = shared.clone();
    let result = eframe::run_native(
        "nier-practice-rs",
        options,
        Box::new(move |cc| {
            let ctx = cc.egui_ctx.clone();
            core::spawn_worker(worker_shared, move || ctx.request_repaint());
            Ok(Box::new(App::new(cc, ui_shared)))
        }),
    );
    // ウィンドウを閉じたらゲームのコードを元に戻す
    if let Ok(mut core) = shared.lock() {
        core.shutdown();
    }
    result
}
