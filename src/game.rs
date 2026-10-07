//! NieR:Automata のメモリ操作 (座標・WS・ポッド・パッチ)

use std::collections::{HashMap, HashSet};

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};

use crate::memory::{find_pattern, Process};
use crate::patches::{self, overlay_pattern, Locator, PatchId, Site};

pub const EXE_NAME: &str = "NieRAutomata.exe";

// NAPT 1.2.CT のポインタ ([[base+rva]+o1]+o2 ...)
const PTR_X: (usize, &[usize]) = (0x1020BE0, &[0x160, 0x50]);
const PTR_Y: (usize, &[usize]) = (0x10CB458, &[0x60, 0x568, 0x58]);
const PTR_Z: (usize, &[usize]) = (0x1250840, &[0x30, 0x240, 0x54]);
const PTR_ANGLE: (usize, &[usize]) = (0x1020BE0, &[0x160, 0x94]);
const PTR_WS: (usize, &[usize]) = (0x10CB438, &[0x60, 0x578, 0x68, 0x7A8]);
const PTR_HEALTH: (usize, &[usize]) = (0x10CB438, &[0x60, 0x578, 0x98, 0x458]);
// 静的アドレス
const CAM_PITCH: usize = 0x1020C60;
const CAM_YAW: usize = 0x1020C64;
const POD_PROGRAM: [usize; 3] = [0x1494688, 0x149468C, 0x1494690];

/// レーザーのポッドプログラム番号
pub const POD_LASER: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Position {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

/// カスタムワープで保存する地点 (向きとカメラも含む)
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct WarpPoint {
    pub pos: Position,
    pub angle: f32,
    pub pitch: f32,
    pub yaw: f32,
}

pub struct Game {
    pub proc: Process,
    /// AOB スキャン済みの書き込み先 (パッチ後は元パターンで見つからなくなるのでキャッシュする)
    site_addrs: HashMap<(PatchId, usize), usize>,
    active: HashSet<PatchId>,
}

impl Game {
    pub fn attach() -> Option<Self> {
        let proc = Process::attach(EXE_NAME)?;
        Some(Self { proc, site_addrs: HashMap::new(), active: HashSet::new() })
    }

    fn ptr(&self, p: (usize, &[usize])) -> Option<usize> {
        self.proc.resolve(p.0, p.1)
    }

    fn read_f32(&self, p: (usize, &[usize])) -> Option<f32> {
        self.proc.read(self.ptr(p)?)
    }

    fn write_f32(&self, p: (usize, &[usize]), v: f32) -> bool {
        self.ptr(p).is_some_and(|a| self.proc.write(a, v))
    }

    pub fn position(&self) -> Option<Position> {
        Some(Position {
            x: self.read_f32(PTR_X)?,
            y: self.read_f32(PTR_Y)?,
            z: self.read_f32(PTR_Z)?,
        })
    }

    pub fn set_position(&self, p: Position) -> Result<()> {
        // ポインタ先が無い (ロード中・メニュー等) と途中まで書いてしまうので先に全部解決する
        let (Some(ax), Some(ay), Some(az)) = (self.ptr(PTR_X), self.ptr(PTR_Y), self.ptr(PTR_Z))
        else {
            bail!("プレイヤー座標が見つかりません (ロード中?)");
        };
        self.proc.write(ax, p.x);
        self.proc.write(ay, p.y);
        self.proc.write(az, p.z);
        Ok(())
    }

    pub fn warp_point(&self) -> Option<WarpPoint> {
        Some(WarpPoint {
            pos: self.position()?,
            angle: self.read_f32(PTR_ANGLE)?,
            pitch: self.proc.read(self.proc.base + CAM_PITCH)?,
            yaw: self.proc.read(self.proc.base + CAM_YAW)?,
        })
    }

    pub fn set_warp_point(&self, w: &WarpPoint) -> Result<()> {
        self.set_position(w.pos)?;
        self.write_f32(PTR_ANGLE, w.angle);
        self.proc.write(self.proc.base + CAM_PITCH, w.pitch);
        self.proc.write(self.proc.base + CAM_YAW, w.yaw);
        Ok(())
    }

    /// 武器セット (0 = セット1, 1 = セット2)
    pub fn weapon_set(&self) -> Option<u32> {
        self.proc.read(self.ptr(PTR_WS)?)
    }

    pub fn set_weapon_set(&self, v: u32) -> bool {
        self.ptr(PTR_WS).is_some_and(|a| self.proc.write(a, v))
    }

    pub fn health(&self) -> Option<u32> {
        self.proc.read(self.ptr(PTR_HEALTH)?)
    }

    pub fn pod_program(&self, pod: usize) -> Option<u32> {
        self.proc.read(self.proc.base + POD_PROGRAM[pod])
    }

    pub fn set_pod_program(&self, pod: usize, v: u32) -> bool {
        self.proc.write(self.proc.base + POD_PROGRAM[pod], v)
    }

    pub fn is_active(&self, id: PatchId) -> bool {
        self.active.contains(&id)
    }

    pub fn active_patches(&self) -> impl Iterator<Item = PatchId> + '_ {
        self.active.iter().copied()
    }

    /// 未解決の AOB サイトをまとめてスキャンする
    fn resolve_aob_sites(&mut self) {
        let pending: Vec<(PatchId, usize, &'static Site)> = patches::PATCHES
            .iter()
            .flat_map(|p| p.sites.iter().enumerate().map(move |(i, s)| (p.id, i, s)))
            .filter(|(id, i, s)| {
                matches!(s.locator, Locator::Aob { .. }) && !self.site_addrs.contains_key(&(*id, *i))
            })
            .collect();
        if pending.is_empty() {
            return;
        }
        let image = self.proc.dump_module();
        for (id, i, site) in pending {
            let Locator::Aob { pattern, offset } = site.locator else { continue };
            // 元のバイト列で探し、無ければパッチ済みのバイト列でも探す
            let hit = [site.original, site.patched].iter().find_map(|bytes| {
                find_pattern(&image, &overlay_pattern(pattern, offset, bytes)).first().copied()
            });
            if let Some(off) = hit {
                self.site_addrs.insert((id, i), self.proc.base + off + offset);
            }
        }
    }

    fn site_addr(&mut self, id: PatchId, index: usize, site: &Site) -> Result<usize> {
        match site.locator {
            Locator::Rva(rva) => Ok(self.proc.base + rva),
            Locator::Aob { pattern, .. } => {
                if !self.site_addrs.contains_key(&(id, index)) {
                    self.resolve_aob_sites();
                }
                self.site_addrs
                    .get(&(id, index))
                    .copied()
                    .ok_or_else(|| anyhow::anyhow!("AOB が見つかりません: {pattern}"))
            }
        }
    }

    /// パッチの ON/OFF。書き込み前に現在のバイト列を確認し、
    /// 想定外 (ゲームのバージョン違いなど) なら何も書かずにエラーにする。
    pub fn set_patch(&mut self, id: PatchId, on: bool) -> Result<()> {
        let def = patches::def(id);
        let mut addrs = Vec::with_capacity(def.sites.len());
        for (i, site) in def.sites.iter().enumerate() {
            let addr = self.site_addr(id, i, site)?;
            let Some(cur) = self.proc.read_bytes(addr, site.original.len()) else {
                bail!("{}: メモリを読めません ({addr:#X})", def.name);
            };
            if cur != site.original && cur != site.patched {
                bail!(
                    "{}: 想定外のバイト列です (+{:#X})。ゲームのバージョンが違う可能性があります",
                    def.name,
                    addr - self.proc.base
                );
            }
            addrs.push(addr);
        }
        for (site, addr) in def.sites.iter().zip(addrs) {
            let bytes = if on { site.patched } else { site.original };
            if !self.proc.write_code(addr, bytes) {
                bail!("{}: 書き込みに失敗しました", def.name);
            }
        }
        if on {
            self.active.insert(id);
        } else {
            self.active.remove(&id);
        }
        Ok(())
    }

    /// ゲーム側の実際のバイト列から ON/OFF 状態を読み取る
    /// (前回このツールが落ちてパッチが残っている場合に備える)
    pub fn sync_active(&mut self) {
        self.resolve_aob_sites();
        for def in patches::PATCHES {
            let all_patched = def.sites.iter().enumerate().all(|(i, site)| {
                let addr = match site.locator {
                    Locator::Rva(rva) => Some(self.proc.base + rva),
                    Locator::Aob { .. } => self.site_addrs.get(&(def.id, i)).copied(),
                };
                addr.and_then(|a| self.proc.read_bytes(a, site.patched.len()))
                    .is_some_and(|cur| cur == site.patched)
            });
            if all_patched {
                self.active.insert(def.id);
            }
        }
    }

    /// 全パッチを元に戻す (終了時)
    pub fn restore_all(&mut self) {
        let ids: Vec<_> = self.active.iter().copied().collect();
        for id in ids {
            let _ = self.set_patch(id, false);
        }
    }
}
