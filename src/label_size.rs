//! D101 stock sizes (width across the roll × length along the feed).
//! Catalog references and printable margins: docs/D101-MEDIA.md.
use crate::bitmap::LabelSpec;
use crate::local_format;
use serde::{Deserialize, Serialize};

macro_rules! sizes {
    ($($name:ident => ($width:expr, $length:expr)),+ $(,)?) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
        pub enum D101LabelSize { $($name),+ }
        impl D101LabelSize {
            pub const ALL: &'static [Self] = &[$(Self::$name),+];
            /// Tenths of a millimetre; matches the printer's PaperInfo units.
            pub const fn tenths(self) -> (u16, u16) {
                match self { $(Self::$name => ($width, $length)),+ }
            }
        }
    };
}
sizes! {
    W12L22 => (120, 220), W12L30 => (120, 300), W12L40 => (120, 400),
    W12L75 => (120, 750), W12_5L109 => (125, 1090), W13L35 => (130, 350),
    W14L28 => (140, 280), W14L30 => (140, 300), W14L40 => (140, 400),
    W15L30 => (150, 300), W15L50 => (150, 500),
    W20L20 => (200, 200), W20L30 => (200, 300), W20L40 => (200, 400),
    W20L45 => (200, 450), W20L75 => (200, 750),
    W25L25 => (250, 250), W25L30 => (250, 300), W25L40 => (250, 400),
    W25L50 => (250, 500), W25L60 => (250, 600),
}
impl D101LabelSize {
    pub fn from_tenths(width: u16, length: u16) -> Option<Self> {
        Self::ALL
            .iter()
            .copied()
            .find(|s| s.tenths() == (width, length))
    }
    pub fn from_spec(spec: &LabelSpec) -> Option<Self> {
        // No nearest-size fallback: an unsupported custom size remains unsupported.
        Self::ALL.iter().copied().find(|s| {
            let (w, l) = s.tenths();
            (spec.width_mm - w as f32 / 10.0).abs() < 0.001
                && (spec.length_mm - l as f32 / 10.0).abs() < 0.001
        })
    }
    pub fn apply(self, spec: &mut LabelSpec) {
        if Self::from_spec(spec) != Some(self) {
            spec.outline = Default::default();
        }
        let (w, l) = self.tenths();
        spec.width_mm = w as f32 / 10.0;
        spec.length_mm = l as f32 / 10.0;
    }
    pub fn title(self) -> String {
        let (w, l) = self.tenths();
        local_format!("{} × {} мм", "{} × {} mm", w as f32 / 10.0, l as f32 / 10.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn catalog_round_trips_without_accepting_nearby_or_rotated_sizes() {
        for &size in D101LabelSize::ALL {
            let mut spec = LabelSpec::default();
            size.apply(&mut spec);
            assert_eq!(D101LabelSize::from_spec(&spec), Some(size));
            assert!(spec.dimensions().is_ok());
            let (w, l) = size.tenths();
            assert_eq!(D101LabelSize::from_tenths(w, l), Some(size));
            spec.length_mm += 0.1;
            assert_eq!(D101LabelSize::from_spec(&spec), None);
        }
        assert_eq!(D101LabelSize::from_tenths(400, 140), None);
        assert_eq!(D101LabelSize::from_tenths(240, 400), None);
    }
}
