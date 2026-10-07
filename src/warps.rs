//! NAPT 1.2.CT の RedDots / A Ending ワープ座標

// 座標は CT の値をそのまま写している
#![allow(clippy::excessive_precision)]

use serde::{Deserialize, Serialize};

use crate::hotkeys::vk;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WarpSet {
    None,
    RedDots,
    AEnding,
}

impl WarpSet {
    pub fn label(self) -> &'static str {
        match self {
            WarpSet::None => "なし",
            WarpSet::RedDots => "Red Dots",
            WarpSet::AEnding => "A Ending",
        }
    }

    pub fn warps(self) -> &'static [Warp] {
        match self {
            WarpSet::None => &[],
            WarpSet::RedDots => RED_DOTS,
            WarpSet::AEnding => A_ENDING,
        }
    }
}

pub struct Warp {
    pub name: &'static str,
    pub key: u16,
    pub pos: [f32; 3],
}

const fn w(name: &'static str, key: u16, x: f32, y: f32, z: f32) -> Warp {
    Warp { name, key, pos: [x, y, z] }
}

pub const RED_DOTS: &[Warp] = &[
    w("BBJ", vk::K1, -614.1346436, 470.4415894, -9.26349926),
    w("Shit Clip", vk::K2, 8.293560028, 299.333313, 0.1906298399),
    w("FRC / Escalator Clip", vk::K3, 139.9146576, -156.8749695, 1.281329155),
    w("Pit Jump", vk::K4, 52.14932632, -424.0651245, -66.78956604),
    w("007 Plunge", vk::K5, -36.58647156, -540.5149536, -45.65612793),
    w("Under the Pipe", vk::K6, -424.688324, -677.5863037, -17.48782539),
    w("Oiled Rock", vk::K7, -880.3734131, -114.8448944, -10.07034111),
    w("Reverse Barricade", vk::K8, -476.908844, -197.1511536, -0.01514256001),
    w("Stubby OOB", vk::K9, -309.9678345, -76.14994049, 11.27253914),
    w("Railing Clip", vk::K0, 171.9150085, -168.8201141, 13.88848019),
    w("Tree Clip", vk::NUMPAD1, 505.6756287, -402.0822144, -26.82228851),
    w("Mushroom Clip", vk::NUMPAD2, 500.1080017, -391.8441162, -18.03734589),
    w("Castle Skip", vk::NUMPAD3, 939.4246826, -364.5061951, -31.8599987),
    w("Castle Plunge", vk::NUMPAD4, 883.7270508, -308.8825073, 27.51751328),
    w("Gate Clip", vk::NUMPAD5, 611.9493408, -138.026474, -40.17249298),
    w("Park Plunge", vk::NUMPAD6, 511.1660156, 285.0306396, 16.44798851),
    w("Park Box Clip", vk::NUMPAD7, 616.5217285, 351.0013428, 13.32746887),
    w("Park to Res Camp", vk::NUMPAD8, 377.2723083, 359.7017212, -0.1899940968),
    w("Building Climb", vk::NUMPAD9, 389.8188477, 258.3286133, 2.701360703),
    w("Flooded Plunge", vk::F1, 341.3547668, 647.4865112, 13.39050388),
];

// README とスクリプトで Numpad8/9 の割り当てが逆になっているが、実際の動作 (スクリプト側) に合わせる
pub const A_ENDING: &[Warp] = &[
    w("Crane Skip", vk::K1, -647.9671631, 442.992218, -0.00618994236),
    w("Bridge Skip", vk::K2, -557.1624146, 339.7683105, 37.385746),
    w("Lava OOB", vk::K3, -476.9242554, 311.7558594, 35.74295807),
    w("Engels Swag", vk::K4, -464.3654785, 548.3460693, 12.89531708),
    w("Wschsengels", vk::K5, -440.8577271, 969.8424072, 32.56860352),
    w("City Ruins Plunge", vk::K6, -91.36110687, 657.447998, 60.32508087),
    w("Desert Barricade", vk::K7, -449.0111084, -144.8113098, 3.658255577),
    w("Housing Slide", vk::K8, -423.7272949, -656.8052979, -2.72300601),
    w("Desert Housing", vk::K9, -135.3109283, -674.4573364, -72.1519165),
    w("Platformless", vk::K0, 68.24066162, -566.5601807, -84.72198486),
    w("Early Don", vk::NUMPAD1, 61.01496887, -526.0592041, -103.2772827),
    w("Box Clip / Boxless", vk::NUMPAD2, -193.0847626, -430.5201111, -21.63484955),
    w("Let's Play", vk::NUMPAD3, 604.3236084, 302.3002625, 13.32748032),
    w("Obama Clip", vk::NUMPAD4, 464.971283, -45.19567108, 15.51146984),
    w("Dan's Hole", vk::NUMPAD5, -41.53388596, 708.9450073, 52.99628067),
    w("Skype Glitch Plunge", vk::NUMPAD6, 159.3785706, 301.7878113, -33.97693253),
    w("EDS Voidout", vk::NUMPAD7, 431.8598328, 294.9081116, -0.1834000349),
    w("Rainbow Road", vk::NUMPAD8, 651.1810913, -270.4514465, -66.18862152),
    w("Castle Skip", vk::NUMPAD9, 939.4246826, -364.5061951, -31.8599987),
    w("Shore Fight", vk::F1, 360.0169678, 687.0420532, 9.520748138),
    w("CCC", vk::F2, 284.6212769, 454.8380432, -37.72412491),
    w("Mansplaining", vk::F3, 250.9987793, 630.0366821, -112.0854797),
    w("Eat Ass", vk::F4, -581.1045532, 439.3626099, 0.9106545448),
    w("Factory V2", vk::F5, -525.9864502, 449.934906, 0.7824599743),
    w("Wschseng's Elevator", vk::F6, -617.2134399, 214.4011993, -133.1156006),
    w("Shabu", vk::F7, -456.4382629, 221.5089569, 59.98922729),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_match_table() {
        assert_eq!(RED_DOTS.len(), 20);
        assert_eq!(A_ENDING.len(), 26);
    }

    #[test]
    fn keys_unique_within_set() {
        for set in [RED_DOTS, A_ENDING] {
            for (i, a) in set.iter().enumerate() {
                for b in &set[i + 1..] {
                    assert_ne!(a.key, b.key, "{} と {}", a.name, b.name);
                }
            }
        }
    }
}
