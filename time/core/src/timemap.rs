//! Output time -> source frame mapping (Speed / Frame Number modes) and the
//! source-timed segment overrides.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RemapMode {
    Speed,
    FrameNumber,
}

/// Values of "Src: Mark Segments", animated in source timing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Segment {
    CutA,
    CutB,
    CutC,
    NoMvNearest,
    NoMvBlend,
}

impl Segment {
    pub fn from_popup(v: i32) -> Self {
        match v {
            2 => Segment::CutB,
            3 => Segment::CutC,
            4 => Segment::NoMvNearest,
            5 => Segment::NoMvBlend,
            _ => Segment::CutA,
        }
    }
}

/// Source position in (fractional) source frames, at the input frame rate.
///
/// Speed mode integrates the speed curve: `speeds[k]` is Speed % sampled at
/// output frame k; `frac` is the position inside the current output frame.
/// Speed scales seconds, not frame counts, so differing frame rates convert
/// through seconds.
pub fn source_from_speed(speeds: &[f64], frac: f64, out_fps: f64, in_fps: f64) -> f64 {
    let Some((&cur, past)) = speeds.split_last() else {
        return 0.0;
    };
    let sum: f64 = past.iter().sum::<f64>() + cur * frac;
    sum / 100.0 / out_fps * in_fps
}

/// Frame Num is keyed in output frames; it is converted via seconds.
pub fn source_from_frame_num(value: f64, out_fps: f64, in_fps: f64) -> f64 {
    value / out_fps * in_fps
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pair {
    pub a: i64,
    pub b: i64,
    pub t: f32,
}

/// Splits a fractional source frame into the two neighbours and the fraction.
pub fn pair(src: f64) -> Pair {
    let a = src.floor();
    let t = (src - a) as f32;
    if t < 1e-4 {
        Pair { a: a as i64, b: a as i64, t: 0.0 }
    } else if t > 1.0 - 1e-4 {
        Pair { a: a as i64 + 1, b: a as i64 + 1, t: 0.0 }
    } else {
        Pair { a: a as i64, b: a as i64 + 1, t }
    }
}

/// How a pair must be rendered after segment marks are taken into account.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PairOverride {
    /// Use the regular motion-vector settings.
    None,
    /// Return the nearer source frame.
    Nearest,
    /// Cross-fade without vectors.
    Blend,
}

pub fn segment_override(seg_a: Segment, seg_b: Segment) -> PairOverride {
    match seg_a {
        Segment::NoMvNearest => PairOverride::Nearest,
        Segment::NoMvBlend => PairOverride::Blend,
        _ => {
            let cut = |s: Segment| matches!(s, Segment::CutA | Segment::CutB | Segment::CutC);
            if cut(seg_b) && seg_a != seg_b {
                PairOverride::Nearest
            } else {
                PairOverride::None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn speed_half_doubles_duration() {
        // 10 output frames at 50% from a 24p source into a 24p comp = 5 source frames.
        let s = vec![50.0; 11];
        assert!((source_from_speed(&s, 0.0, 24.0, 24.0) - 5.0).abs() < 1e-9);
    }

    #[test]
    fn speed_converts_through_seconds() {
        // 1 s of 24p at 50% into 30p: output frame 60 sits at source frame 24.
        let s = vec![50.0; 61];
        assert!((source_from_speed(&s, 0.0, 30.0, 24.0) - 24.0).abs() < 1e-9);
    }

    #[test]
    fn frame_num_rate_conversion() {
        assert!((source_from_frame_num(60.0, 30.0, 24.0) - 48.0).abs() < 1e-9);
    }

    #[test]
    fn cuts_block_interpolation() {
        assert_eq!(segment_override(Segment::CutA, Segment::CutB), PairOverride::Nearest);
        assert_eq!(segment_override(Segment::CutA, Segment::CutA), PairOverride::None);
        assert_eq!(segment_override(Segment::NoMvBlend, Segment::CutA), PairOverride::Blend);
    }
}
