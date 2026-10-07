//! NAPT 1.2.CT の Auto Assembler スクリプトを「元バイト → 書き換えバイト」の
//! インプレースパッチに置き換えたもの。
//!
//! CE 版はコードケーブ (alloc + jmp) を使っていたが、どれも即値や分岐命令を
//! 1 か所変えるだけで同じ動作になるため、ここでは同じ長さのバイト列を直接書き換える。

use serde::{Deserialize, Serialize};

/// パッチの書き込み先の探し方
#[derive(Debug, Clone, Copy)]
pub enum Locator {
    /// aobscanmodule と同じ。パターン先頭からのオフセット位置に書く
    Aob { pattern: &'static str, offset: usize },
    /// "NieRAutomata.exe"+RVA の固定アドレス
    Rva(usize),
}

/// 1 か所分の書き換え
#[derive(Debug, Clone, Copy)]
pub struct Site {
    pub locator: Locator,
    pub original: &'static [u8],
    pub patched: &'static [u8],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PatchId {
    NoClip,
    InfiniteDoubleJump,
    InfiniteDash,
    NoPodCooldown,
    FastDrownWarp,
    DisableDebugMenu,
    SystemMenuEverywhere,
    KeepSlowWalk,
    NoSlowWalkPascal,
    NoSlowWalk9s2b,
    NoSlowWalkNpc,
    DisablePascalAi,
}

pub struct PatchDef {
    pub id: PatchId,
    pub name: &'static str,
    pub description: &'static str,
    pub sites: &'static [Site],
}

const NOP5: &[u8] = &[0x90; 5];

pub const PATCHES: &[PatchDef] = &[
    PatchDef {
        id: PatchId::NoClip,
        name: "ノークリップ",
        description: "壁をすり抜ける (mov r13d,1 → mov r13d,0)",
        sites: &[Site {
            locator: Locator::Aob { pattern: "41 BD 01 00 00 00 75", offset: 0 },
            original: &[0x41, 0xBD, 0x01, 0x00, 0x00, 0x00],
            patched: &[0x41, 0xBD, 0x00, 0x00, 0x00, 0x00],
        }],
    },
    PatchDef {
        id: PatchId::InfiniteDoubleJump,
        name: "無限二段ジャンプ",
        description: "空中ジャンプ回数の上限 2 → 127",
        sites: &[Site {
            locator: Locator::Aob { pattern: "83 BB A8 14 00 00 02", offset: 0 },
            original: &[0x83, 0xBB, 0xA8, 0x14, 0x00, 0x00, 0x02],
            patched: &[0x83, 0xBB, 0xA8, 0x14, 0x00, 0x00, 0x7F],
        }],
    },
    PatchDef {
        id: PatchId::InfiniteDash,
        name: "無限空中ダッシュ",
        description: "空中ダッシュ済みフラグのチェックを飛ばす (jne → jmp)",
        sites: &[Site {
            locator: Locator::Aob { pattern: "75 1E 39 83 98 0A 01 00", offset: 0 },
            original: &[0x75, 0x1E],
            patched: &[0xEB, 0x1E],
        }],
    },
    PatchDef {
        id: PatchId::NoPodCooldown,
        name: "ポッドクールダウンなし",
        description: "ポッドプログラムのクールダウン書き込みを NOP 化",
        sites: &[Site {
            locator: Locator::Rva(0x4FAFFD),
            original: &[0xF3, 0x0F, 0x11, 0x84, 0xC3, 0x34, 0x6A, 0x01, 0x00],
            patched: &[0x90; 9],
        }],
    },
    PatchDef {
        id: PatchId::FastDrownWarp,
        name: "高速溺れワープ",
        description: "溺れアニメの代わりに受け身 (高速起き上がり) を使う",
        sites: &[Site {
            locator: Locator::Rva(0x503A12),
            original: &[0xE8, 0x49, 0x02, 0xFB, 0xFF],
            patched: &[0xEB, 0x2F, 0x90, 0x90, 0x90],
        }],
    },
    PatchDef {
        id: PatchId::DisableDebugMenu,
        name: "デバッグメニュー無効 (by Jasper)",
        description: "練習セーブに入っているデバッグメニューを出さない",
        sites: &[Site {
            locator: Locator::Aob { pattern: "E8 E5 D0 B0 FF", offset: 0 },
            original: &[0xE8, 0xE5, 0xD0, 0xB0, 0xFF],
            patched: NOP5,
        }],
    },
    PatchDef {
        id: PatchId::SystemMenuEverywhere,
        name: "どこでもシステムメニュー (ft. Jasper)",
        description: "ムービー以外ならどこでもシステムメニューを開ける",
        sites: &[
            Site {
                locator: Locator::Rva(0x839342),
                original: &[0x74, 0x14],
                patched: &[0x90, 0x90],
            },
            Site {
                locator: Locator::Rva(0x83924E),
                original: &[0x0F, 0x82, 0x04, 0x01, 0x00, 0x00],
                patched: &[0x90; 6],
            },
            Site {
                locator: Locator::Rva(0x83923E),
                original: &[0x0F, 0x82, 0x14, 0x01, 0x00, 0x00],
                patched: &[0x90; 6],
            },
            Site {
                locator: Locator::Rva(0x839234),
                original: &[0x0F, 0x85, 0x1E, 0x01, 0x00, 0x00],
                patched: &[0x90; 6],
            },
        ],
    },
    PatchDef {
        id: PatchId::KeepSlowWalk,
        name: "NPC 会話後もスロー歩き維持",
        description: "会話終了時にスロー歩きを解除する処理を飛ばす (je → jmp)",
        sites: &[Site {
            locator: Locator::Aob { pattern: "74 53 0F 1F 80 00 00 00 00", offset: 0 },
            original: &[0x74, 0x53],
            patched: &[0xEB, 0x53],
        }],
    },
    PatchDef {
        id: PatchId::NoSlowWalkPascal,
        name: "スロー歩き無効: パスカル",
        description: "パートナーのパスカルと会話中のスロー歩きを無効化",
        sites: &[Site {
            locator: Locator::Rva(0x33F5EA),
            original: &[0x48, 0x8B, 0xC6],
            patched: &[0x90, 0x90, 0x90],
        }],
    },
    PatchDef {
        id: PatchId::NoSlowWalk9s2b,
        name: "スロー歩き無効: 9S/2B",
        description: "2B⇔9S の会話中のスロー歩きを無効化",
        sites: &[Site {
            locator: Locator::Rva(0x355807),
            original: &[0xE8, 0x14, 0x1B, 0x00, 0x00],
            patched: NOP5,
        }],
    },
    PatchDef {
        id: PatchId::NoSlowWalkNpc,
        name: "スロー歩き無効: NPC",
        description: "NPC と会話中のスロー歩きを無効化",
        sites: &[Site {
            locator: Locator::Rva(0x319E9A),
            original: &[0x48, 0x8B, 0xDA],
            patched: &[0x90, 0x90, 0x90],
        }],
    },
    PatchDef {
        id: PatchId::DisablePascalAi,
        name: "パスカルの AI 無効",
        description: "パスカルの AI の一部を止める (検証用)",
        sites: &[Site {
            locator: Locator::Rva(0x7125AF),
            original: &[0xE8, 0x7C, 0x13, 0xD0, 0xFF],
            patched: NOP5,
        }],
    },
];

pub fn def(id: PatchId) -> &'static PatchDef {
    PATCHES.iter().find(|p| p.id == id).expect("未定義のパッチ")
}

/// AOB パターンの `offset` 位置にバイト列を重ねたパターンを作る
/// (既にパッチ済みのゲームでも場所を見つけられるようにするため)
pub fn overlay_pattern(pattern: &str, offset: usize, bytes: &[u8]) -> Vec<Option<u8>> {
    let mut pat = crate::memory::parse_pattern(pattern);
    for (i, b) in bytes.iter().enumerate() {
        pat[offset + i] = Some(*b);
    }
    pat
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::parse_pattern;

    #[test]
    fn sites_are_consistent() {
        for p in PATCHES {
            assert!(!p.sites.is_empty(), "{:?}", p.id);
            for s in p.sites {
                assert_eq!(s.original.len(), s.patched.len(), "{:?}", p.id);
                assert_ne!(s.original, s.patched, "{:?}", p.id);
                if let Locator::Aob { pattern, offset } = s.locator {
                    // パターンの該当部分が元バイトと一致していること
                    let pat = parse_pattern(pattern);
                    assert!(offset + s.original.len() <= pat.len(), "{:?}", p.id);
                    for (i, b) in s.original.iter().enumerate() {
                        assert_eq!(pat[offset + i], Some(*b), "{:?}", p.id);
                    }
                }
            }
        }
    }

    #[test]
    fn ids_are_unique() {
        for (i, a) in PATCHES.iter().enumerate() {
            for b in &PATCHES[i + 1..] {
                assert_ne!(a.id, b.id);
            }
        }
    }

    #[test]
    fn overlay_replaces_bytes() {
        let pat = overlay_pattern("41 BD 01 00 00 00 75", 0, &[0x41, 0xBD, 0x00]);
        assert_eq!(pat[2], Some(0x00));
        assert_eq!(pat[6], Some(0x75));
    }
}
