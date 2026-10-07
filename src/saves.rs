//! YASM (Yet Another Save Manager) のセーブ管理部分の移植
//!
//! セーブ (SlotData_N.dat) には Steam ID・ゲーム設定・キー設定が含まれる。
//! 他人の練習セーブを読み込むときは、これらを自分のセーブの値で上書きする。

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

pub const SAVE_LEN: usize = 235_980;

const STEAM_ID: std::ops::Range<usize> = 4..8;
const SETTINGS: std::ops::Range<usize> = 235_340..235_340 + 264;
const CONTROLS: std::ops::Range<usize> = 235_604..235_604 + 172;

pub const DEFAULT_GROUP: &str = "|Default";

/// 自分のセーブから抜き出した、引き継ぐべき情報
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BaseSaveInfo {
    pub steam_id: Vec<u8>,
    pub settings: Vec<u8>,
    pub controls: Vec<u8>,
}

impl BaseSaveInfo {
    pub fn extract(save: &[u8]) -> Result<Self> {
        if save.len() != SAVE_LEN {
            bail!("セーブのサイズが不正です ({} バイト)", save.len());
        }
        Ok(Self {
            steam_id: save[STEAM_ID].to_vec(),
            settings: save[SETTINGS].to_vec(),
            controls: save[CONTROLS].to_vec(),
        })
    }

    pub fn inject(&self, save: &mut [u8]) -> Result<()> {
        if save.len() != SAVE_LEN {
            bail!("セーブのサイズが不正です ({} バイト)", save.len());
        }
        if self.steam_id.len() != STEAM_ID.len()
            || self.settings.len() != SETTINGS.len()
            || self.controls.len() != CONTROLS.len()
        {
            bail!("設定情報が壊れています。もう一度取り込んでください");
        }
        save[STEAM_ID].copy_from_slice(&self.steam_id);
        save[SETTINGS].copy_from_slice(&self.settings);
        save[CONTROLS].copy_from_slice(&self.controls);
        Ok(())
    }
}

/// ゲームのセーブフォルダ (Documents\My Games\NieR_Automata)
pub fn game_save_dir() -> Option<PathBuf> {
    Some(dirs::document_dir()?.join("My Games").join("NieR_Automata"))
}

pub fn slot_path(game_dir: &Path, slot: usize) -> PathBuf {
    game_dir.join(format!("SlotData_{slot}.dat"))
}

#[derive(Debug, Clone)]
pub struct CustomSave {
    pub group: String,
    pub name: String,
    pub path: PathBuf,
}

/// ファイル名から表示名を作る (YASM と同じく "_fog" を除去し拡張子を落とす)
fn display_name(file_name: &str) -> String {
    let s = file_name.replace("_fog", "");
    match s.rfind('.') {
        Some(i) => s[..i].to_string(),
        None => s,
    }
}

/// ファイル名・グループ名に使えない文字を除去する
pub fn sanitize(name: &str) -> String {
    let s: String = name
        .trim()
        .chars()
        .filter(|c| !matches!(c, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*'))
        .filter(|c| !c.is_control())
        .collect();
    let s = s.trim_matches(|c| c == '.' || c == ' ').to_string();
    s
}

/// 練習用セーブの保管庫 (Saves フォルダ。直下 = |Default、サブフォルダ = グループ)
pub struct SaveLibrary {
    pub root: PathBuf,
    pub saves: Vec<CustomSave>,
}

impl SaveLibrary {
    pub fn new(root: PathBuf) -> Self {
        Self { root, saves: Vec::new() }
    }

    pub fn reload(&mut self) -> Result<()> {
        fs::create_dir_all(&self.root)?;
        self.saves.clear();
        Self::load_group(&self.root, DEFAULT_GROUP, &mut self.saves)?;
        for entry in fs::read_dir(&self.root)? {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                let name = entry.file_name().to_string_lossy().into_owned();
                Self::load_group(&entry.path(), &name, &mut self.saves)?;
            }
        }
        Ok(())
    }

    fn load_group(dir: &Path, group: &str, out: &mut Vec<CustomSave>) -> Result<()> {
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let meta = entry.metadata()?;
            if meta.is_file() && meta.len() == SAVE_LEN as u64 {
                out.push(CustomSave {
                    group: group.to_string(),
                    name: display_name(&entry.file_name().to_string_lossy()),
                    path: entry.path(),
                });
            }
        }
        Ok(())
    }

    /// グループ名順・セーブ名順に並べたもの
    pub fn groups(&self) -> Vec<(String, Vec<&CustomSave>)> {
        let mut groups: Vec<(String, Vec<&CustomSave>)> = Vec::new();
        for s in &self.saves {
            match groups.iter_mut().find(|(g, _)| *g == s.group) {
                Some((_, v)) => v.push(s),
                None => groups.push((s.group.clone(), vec![s])),
            }
        }
        groups.sort_by(|a, b| a.0.cmp(&b.0));
        for (_, v) in &mut groups {
            v.sort_by(|a, b| a.name.cmp(&b.name));
        }
        groups
    }

    /// 練習セーブを自分の設定に差し替えてゲームのスロットへ書き込む。
    /// 上書き前のスロットは backup_dir に 1 世代だけ残す。
    pub fn load_into_slot(
        save: &CustomSave,
        slot_file: &Path,
        info: &BaseSaveInfo,
        backup_dir: Option<&Path>,
    ) -> Result<()> {
        let mut data = fs::read(&save.path)
            .with_context(|| format!("{} を読めません", save.path.display()))?;
        info.inject(&mut data)?;
        if let (Some(dir), true) = (backup_dir, slot_file.exists()) {
            fs::create_dir_all(dir)?;
            let name = slot_file.file_name().unwrap_or_default().to_string_lossy();
            fs::copy(slot_file, dir.join(format!("{name}.prev")))?;
        }
        if let Some(parent) = slot_file.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(slot_file, &data)
            .with_context(|| format!("{} に書き込めません", slot_file.display()))?;
        Ok(())
    }

    /// ゲームのスロットを練習セーブとして保存する
    pub fn create_from_slot(&mut self, slot_file: &Path, group: &str, name: &str) -> Result<PathBuf> {
        let name = sanitize(name);
        let group = sanitize(group);
        if name.is_empty() {
            bail!("セーブ名を入力してください");
        }
        let data = fs::read(slot_file)
            .with_context(|| format!("{} がありません", slot_file.display()))?;
        if data.len() != SAVE_LEN {
            bail!("スロットのサイズが不正です ({} バイト)", data.len());
        }
        let dir = if group.is_empty() { self.root.clone() } else { self.root.join(&group) };
        fs::create_dir_all(&dir)?;
        let dest = dir.join(format!("{name}.dat"));
        fs::write(&dest, &data)?;
        self.reload()?;
        Ok(dest)
    }

    /// zip 内の正しいサイズのセーブを、zip 名のグループとして取り込む。取り込んだ数を返す
    pub fn import_zip(&mut self, zip_path: &Path) -> Result<usize> {
        let group = sanitize(
            &zip_path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default(),
        );
        if group.is_empty() {
            bail!("zip の名前からグループ名を作れません");
        }
        let file = fs::File::open(zip_path)?;
        let mut archive = zip::ZipArchive::new(file).context("zip を開けません")?;
        let dir = self.root.join(&group);
        let mut count = 0;
        for i in 0..archive.len() {
            let mut entry = archive.by_index(i)?;
            if entry.is_dir() || entry.size() != SAVE_LEN as u64 {
                continue;
            }
            // フォルダ構造は無視してファイル名だけ使う (zip slip 対策も兼ねる)
            let raw = entry.name().replace('\\', "/");
            let file_name = sanitize(raw.rsplit('/').next().unwrap_or(""));
            if file_name.is_empty() {
                continue;
            }
            let mut data = Vec::with_capacity(SAVE_LEN);
            entry.read_to_end(&mut data)?;
            fs::create_dir_all(&dir)?;
            fs::write(dir.join(file_name), &data)?;
            count += 1;
        }
        self.reload()?;
        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn temp_dir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!(
            "nier-practice-test-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        fs::create_dir_all(&d).unwrap();
        d
    }

    fn fake_save(fill: u8) -> Vec<u8> {
        vec![fill; SAVE_LEN]
    }

    #[test]
    fn extract_and_inject_only_touch_ranges() {
        let mine = fake_save(0xAA);
        let info = BaseSaveInfo::extract(&mine).unwrap();
        let mut other = fake_save(0x11);
        info.inject(&mut other).unwrap();
        for (i, b) in other.iter().enumerate() {
            let inside = STEAM_ID.contains(&i) || SETTINGS.contains(&i) || CONTROLS.contains(&i);
            assert_eq!(*b, if inside { 0xAA } else { 0x11 }, "offset {i}");
        }
    }

    #[test]
    fn rejects_wrong_size() {
        assert!(BaseSaveInfo::extract(&[0u8; 100]).is_err());
    }

    #[test]
    fn display_name_strips_fog_and_ext() {
        assert_eq!(display_name("01. Prologue_fog.dat"), "01. Prologue");
        assert_eq!(display_name("noext"), "noext");
    }

    #[test]
    fn sanitize_blocks_traversal() {
        assert_eq!(sanitize("../../evil"), "evil");
        assert_eq!(sanitize("a/b\\c:d"), "abcd");
        assert_eq!(sanitize("  ok name  "), "ok name");
    }

    #[test]
    fn library_roundtrip() {
        let root = temp_dir("lib");
        let game = temp_dir("game");
        let lib_dir = root.join("Saves");
        let mut lib = SaveLibrary::new(lib_dir.clone());
        lib.reload().unwrap();
        assert!(lib.saves.is_empty());

        // 自分のスロット 0 と、他人の練習セーブ
        let slot0 = slot_path(&game, 0);
        fs::write(&slot0, fake_save(0xAA)).unwrap();
        fs::write(lib_dir.join("02. B.dat"), fake_save(0x22)).unwrap();
        fs::write(lib_dir.join("01. A_fog.dat"), fake_save(0x11)).unwrap();
        fs::write(lib_dir.join("junk.txt"), b"x").unwrap();

        lib.create_from_slot(&slot0, "My Group", "mine").unwrap();
        let groups = lib.groups();
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].0, "My Group");
        assert_eq!(groups[1].0, DEFAULT_GROUP);
        let names: Vec<_> = groups[1].1.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["01. A", "02. B"]);

        // 読み込み: 設定部分だけ自分のものになり、元スロットはバックアップされる
        let info = BaseSaveInfo::extract(&fs::read(&slot0).unwrap()).unwrap();
        let target = groups[1].1[0].clone();
        let backup = root.join("Backups");
        SaveLibrary::load_into_slot(&target, &slot0, &info, Some(&backup)).unwrap();
        let loaded = fs::read(&slot0).unwrap();
        assert_eq!(loaded[0], 0x11);
        assert_eq!(&loaded[STEAM_ID], &[0xAA; 4]);
        assert_eq!(fs::read(backup.join("SlotData_0.dat.prev")).unwrap(), fake_save(0xAA));

        fs::remove_dir_all(&root).ok();
        fs::remove_dir_all(&game).ok();
    }

    #[test]
    fn import_zip_filters_and_flattens() {
        let root = temp_dir("zip");
        let zip_path = root.join("Any% Saves.zip");
        {
            let f = fs::File::create(&zip_path).unwrap();
            let mut z = zip::ZipWriter::new(f);
            let opt = zip::write::SimpleFileOptions::default();
            z.start_file("sub/01. Start.dat", opt).unwrap();
            z.write_all(&fake_save(1)).unwrap();
            z.start_file("../../escape.dat", opt).unwrap();
            z.write_all(&fake_save(2)).unwrap();
            z.start_file("readme.txt", opt).unwrap();
            z.write_all(b"hello").unwrap();
            z.finish().unwrap();
        }
        let mut lib = SaveLibrary::new(root.join("Saves"));
        let n = lib.import_zip(&zip_path).unwrap();
        assert_eq!(n, 2);
        let g = root.join("Saves").join("Any% Saves");
        assert!(g.join("01. Start.dat").exists());
        assert!(g.join("escape.dat").exists());
        assert!(!root.join("escape.dat").exists());
        fs::remove_dir_all(&root).ok();
    }
}
