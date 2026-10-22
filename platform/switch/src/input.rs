//! Horizon pad masks to the Switch face, shoulder and menu buttons.
//!
//! libnx's `HidNpadButton_*` values are represented by the same bit positions
//! exposed by `platform_buttons` in `runtime.c`.

pub const A: u64 = 1 << 0;
pub const B: u64 = 1 << 1;
pub const X: u64 = 1 << 2;
pub const Y: u64 = 1 << 3;
pub const STICK_LEFT: u64 = 1 << 4;
pub const STICK_RIGHT: u64 = 1 << 5;
pub const L: u64 = 1 << 6;
pub const R: u64 = 1 << 7;
pub const ZL: u64 = 1 << 8;
pub const ZR: u64 = 1 << 9;
pub const PLUS: u64 = 1 << 10;
pub const MINUS: u64 = 1 << 11;
pub const LEFT: u64 = 1 << 12;
pub const UP: u64 = 1 << 13;
pub const RIGHT: u64 = 1 << 14;
pub const DOWN: u64 = 1 << 15;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PadEdges {
    pub a: bool,
    pub b: bool,
    pub x: bool,
    pub y: bool,
    pub stick_left: bool,
    pub stick_right: bool,
    pub l: bool,
    pub r: bool,
    pub zl: bool,
    pub zr: bool,
    pub plus: bool,
    pub minus: bool,
    pub left: bool,
    pub up: bool,
    pub right: bool,
    pub down: bool,
}

impl PadEdges {
    pub const fn from_mask(mask: u64) -> Self {
        Self {
            a: mask & A != 0,
            b: mask & B != 0,
            x: mask & X != 0,
            y: mask & Y != 0,
            stick_left: mask & STICK_LEFT != 0,
            stick_right: mask & STICK_RIGHT != 0,
            l: mask & L != 0,
            r: mask & R != 0,
            zl: mask & ZL != 0,
            zr: mask & ZR != 0,
            plus: mask & PLUS != 0,
            minus: mask & MINUS != 0,
            left: mask & LEFT != 0,
            up: mask & UP != 0,
            right: mask & RIGHT != 0,
            down: mask & DOWN != 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn face_shoulder_and_menu_buttons_keep_distinct_edges() {
        let all = A | B | X | Y | STICK_LEFT | STICK_RIGHT | L | R | ZL | ZR | PLUS | MINUS;
        assert_eq!(
            PadEdges::from_mask(all),
            PadEdges {
                a: true,
                b: true,
                x: true,
                y: true,
                stick_left: true,
                stick_right: true,
                l: true,
                r: true,
                zl: true,
                zr: true,
                plus: true,
                minus: true,
                ..Default::default()
            }
        );
    }

    #[test]
    fn directional_buttons_do_not_set_face_buttons() {
        let pad = PadEdges::from_mask(LEFT | UP | RIGHT | DOWN);
        assert!(pad.left && pad.up && pad.right && pad.down);
        assert!(!pad.a && !pad.b && !pad.x && !pad.y && !pad.plus);
    }
}
