//! Tension curves: the default target arch, and the observed-tension
//! estimate for each bar.

use crate::config::TensionConfig;
use crate::rules::{Context, RuleScore};
use crate::theory::PitchClass;

/// Default arch: low at the start, peak around 70 % of the way, low at
/// the end. Values in 0.15..=0.9.
pub fn default_curve(bars: u32) -> Vec<f32> {
    if bars == 0 {
        return vec![];
    }
    (0..bars)
        .map(|b| {
            let x = if bars == 1 { 0.5 } else { b as f32 / (bars - 1) as f32 };
            let peak = 0.7;
            let d = if x <= peak { x / peak } else { (1.0 - x) / (1.0 - peak) };
            0.15 + 0.75 * d
        })
        .collect()
}

/// Per-bar observed tension in 0..1, from:
/// - tension contributions of breakable rules broken in that bar,
/// - dissonance of notes against the chord, weighted by beat strength,
/// - register height within the melody's range,
/// - leap size, note density and syncopation,
/// - scale-degree instability (1 < 5 < 3 < others).
pub fn observed(ctx: &Context, rules: &[RuleScore], breakable_tension: &[(u32, f32)], cfg: &TensionConfig) -> Vec<f32> {
    let bars = ctx.bars_present().max(1);
    let spb = ctx.meter.steps_per_bar() as f32;
    let notes = ctx.notes();
    let lo = notes.iter().map(|n| n.pitch).min().unwrap_or(60) as f32;
    let hi = notes.iter().map(|n| n.pitch).max().unwrap_or(72) as f32;
    let span = (hi - lo).max(5.0);

    let mut out = vec![0.0f32; bars as usize];
    let _ = rules;
    // Broken breakable rules.
    for &(bar, t) in breakable_tension {
        if (bar as usize) < out.len() {
            out[bar as usize] += cfg.broken_rules * t;
        }
    }
    for bar in 0..bars {
        let ev: Vec<_> = notes.iter().filter(|n| ctx.bar_of(n.start) == bar).collect();
        if ev.is_empty() {
            continue;
        }
        let mut dissonance = 0.0;
        let mut register = 0.0;
        let mut instability = 0.0;
        let mut sync = 0.0;
        let mut wsum = 0.0;
        for n in &ev {
            let w = ctx.strength(n.start);
            wsum += w;
            let in_chord = ctx.chord_at(n.start).map(|c| c.contains_midi(n.pitch)).unwrap_or(true);
            if !in_chord {
                dissonance += w;
            }
            register += w * (n.pitch as f32 - lo) / span;
            let deg = ctx.key.degree(PitchClass::of_midi(n.pitch));
            instability += w * match deg {
                Some(1) => 0.0,
                Some(5) => 0.25,
                Some(3) => 0.5,
                _ => 1.0,
            };
            if crate::rules::syncopation::is_syncopated(ctx, n.start, n.dur) {
                sync += 1.0;
            }
        }
        let wsum = wsum.max(1e-3);
        // Leaps into notes of this bar.
        let mut leap = 0.0;
        for n in &ev {
            if let Some(prev) = notes.iter().rev().find(|p| p.start < n.start) {
                let d = (n.pitch as f32 - prev.pitch as f32).abs();
                leap = f32::max(leap, (d / 12.0).min(1.0));
            }
        }
        let density = (ev.len() as f32 / (spb / 2.0)).min(1.0); // 8ths fill = 1
        let v = &mut out[bar as usize];
        *v += cfg.dissonance * dissonance / wsum
            + cfg.register * register / wsum
            + cfg.instability * instability / wsum
            + cfg.leap * leap
            + cfg.density * density
            + cfg.syncopation * (sync / ev.len() as f32);
    }
    let norm = cfg.scale.max(1e-3);
    out.iter().map(|v| (v / norm).clamp(0.0, 1.0)).collect()
}

/// `-lambda * sum (observed - target)^2` over bars present.
pub fn match_term(observed: &[f32], target: &[f32], lambda: f32) -> f32 {
    let mut s = 0.0;
    for (i, o) in observed.iter().enumerate() {
        let t = target.get(i).copied().unwrap_or(0.5);
        s += (o - t).powi(2);
    }
    -lambda * s
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::test_util::*;

    #[test]
    fn arch_peaks_late() {
        let c = default_curve(8);
        assert_eq!(c.len(), 8);
        let peak = c.iter().enumerate().max_by(|a, b| a.1.partial_cmp(b.1).unwrap()).unwrap().0;
        assert!(peak == 5 || peak == 4);
        assert!(c[0] < c[peak] && c[7] < c[peak]);
        assert!(c.iter().all(|v| (0.0..=1.0).contains(v)));
    }

    #[test]
    fn dissonant_high_busy_bar_is_tenser() {
        let f = fixture(
            "C C",
            "C",
            2,
            &[(C4, 8), (E4, 8), (A4, 2), (B4, 2), (D5, 2), (F5, 2), (E5, 4), (B4, 4)],
        );
        let ctx = f.ctx();
        let o = observed(&ctx, &[], &[], &f.cfg.tension);
        assert!(o[1] > o[0] + 0.2, "{o:?}");
    }

    #[test]
    fn match_term_prefers_close_curves() {
        let t = [0.2, 0.8];
        assert!(match_term(&[0.2, 0.8], &t, 1.0) > match_term(&[0.8, 0.2], &t, 1.0));
    }
}
