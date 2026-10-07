//! 設定ファイル (exe と同じフォルダの config.json)

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::game::WarpPoint;
use crate::saves::BaseSaveInfo;
use crate::warps::WarpSet;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// 自分のセーブから取り込んだ Steam ID・設定・キー設定
    pub base_save_info: Option<BaseSaveInfo>,
    /// カスタムワープ 2 か所 (CE 版と違い、ツールを閉じても残る)
    pub custom_warps: [Option<WarpPoint>; 2],
    pub custom_warps_enabled: bool,
    pub warp_set: WarpSet,
    pub ws_toggle_enabled: bool,
    pub pod_laser_lock: bool,
    /// ゲームが前面のときだけホットキーを受け付ける
    pub hotkeys_game_only: bool,
    pub always_on_top: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            base_save_info: None,
            custom_warps: [None, None],
            custom_warps_enabled: false,
            warp_set: WarpSet::None,
            ws_toggle_enabled: false,
            pod_laser_lock: false,
            hotkeys_game_only: true,
            always_on_top: false,
        }
    }
}

impl Config {
    pub fn path(data_dir: &Path) -> PathBuf {
        data_dir.join("config.json")
    }

    pub fn load(data_dir: &Path) -> Self {
        fs::read_to_string(Self::path(data_dir))
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, data_dir: &Path) -> anyhow::Result<()> {
        fs::write(Self::path(data_dir), serde_json::to_string_pretty(self)?)?;
        Ok(())
    }
}

/// データ置き場 (YASM と同じく exe と同じフォルダ。持ち運びできるように)
pub fn data_dir() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf))
        .unwrap_or_else(|| PathBuf::from("."))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_fields_use_defaults() {
        let c: Config = serde_json::from_str(r#"{"always_on_top": true}"#).unwrap();
        assert!(c.always_on_top);
        assert!(c.hotkeys_game_only);
        assert_eq!(c.warp_set, WarpSet::None);
    }

    #[test]
    fn roundtrip() {
        let c = Config { warp_set: WarpSet::AEnding, ..Config::default() };
        let s = serde_json::to_string(&c).unwrap();
        let back: Config = serde_json::from_str(&s).unwrap();
        assert_eq!(back.warp_set, WarpSet::AEnding);
    }
}
