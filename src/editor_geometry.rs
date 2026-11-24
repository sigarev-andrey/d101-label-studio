//! Eight resize handles share exactly the same geometry as the selection overlay.
use niimbot_d101::document::Bounds;

pub const CANVAS_PAD_X: f64 = 64.0;
pub const CANVAS_PAD_Y: f64 = 48.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Handle(pub i8, pub i8);

pub const HANDLES: [Handle; 8] = [
    Handle(-1, -1),
    Handle(0, -1),
    Handle(1, -1),
    Handle(1, 0),
    Handle(1, 1),
    Handle(0, 1),
    Handle(-1, 1),
    Handle(-1, 0),
];

impl Handle {
    pub fn point(self, b: Bounds) -> (f32, f32) {
        (
            b.x + b.width * (self.0 + 1) as f32 / 2.0,
            b.y + b.height * (self.1 + 1) as f32 / 2.0,
        )
    }
    pub fn resize(self, b: Bounds, dx: f32, dy: f32, w: f32, h: f32, snap: bool) -> Bounds {
        let round = |n: f32| if snap { (n * 2.0).round() / 2.0 } else { n };
        let (mut left, mut top, mut right, mut bottom) = (b.x, b.y, b.x + b.width, b.y + b.height);
        match self.0 {
            -1 => left = round(left + dx).clamp(0.0, right - 0.5),
            1 => right = round(right + dx).clamp(left + 0.5, w),
            _ => {}
        }
        match self.1 {
            -1 => top = round(top + dy).clamp(0.0, bottom - 0.5),
            1 => bottom = round(bottom + dy).clamp(top + 0.5, h),
            _ => {}
        }
        Bounds {
            x: left,
            y: top,
            width: right - left,
            height: bottom - top,
        }
    }
}

pub fn fit_zoom(viewport: (f64, f64), paper: (u32, u32)) -> f64 {
    let available = (
        (viewport.0 - 2.0 * CANVAS_PAD_X - 20.0).max(1.0),
        (viewport.1 - 2.0 * CANVAS_PAD_Y - 20.0).max(1.0),
    );
    (100.0 * (available.0 / paper.0.max(1) as f64).min(available.1 / paper.1.max(1) as f64))
        .clamp(10.0, 400.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn all_handles_keep_the_opposite_edges_and_stay_on_paper() {
        let b = Bounds {
            x: 2.0,
            y: 1.0,
            width: 12.0,
            height: 8.0,
        };
        for handle in HANDLES {
            for (dx, dy) in [(-100.0, -100.0), (100.0, 100.0), (0.7, 1.3)] {
                let n = handle.resize(b, dx, dy, 40.0, 12.0, true);
                assert!(n.x >= 0.0 && n.y >= 0.0 && n.width >= 0.5 && n.height >= 0.5);
                assert!(n.x + n.width <= 40.0 && n.y + n.height <= 12.0);
                if handle.0 <= 0 {
                    assert_eq!(n.x + n.width, b.x + b.width);
                }
                if handle.0 >= 0 {
                    assert_eq!(n.x, b.x);
                }
                if handle.1 <= 0 {
                    assert_eq!(n.y + n.height, b.y + b.height);
                }
                if handle.1 >= 0 {
                    assert_eq!(n.y, b.y);
                }
            }
        }
    }
    #[test]
    fn fit_accounts_for_geometry_labels_including_long_cable_stock() {
        for paper in [(320, 96), (871, 100), (200, 200)] {
            let z = fit_zoom((650.0, 440.0), paper) / 100.0;
            assert!(paper.0 as f64 * z + 2.0 * CANVAS_PAD_X + 20.0 <= 650.01);
            assert!(paper.1 as f64 * z + 2.0 * CANVAS_PAD_Y + 20.0 <= 440.01);
        }
    }
}
