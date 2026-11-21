//! Parameter previews use a repeatable probe so the editor also works with transport stopped.
use crate::theme as t;
use eframe::egui::{self, Align2, Color32, Rect, Sense, Stroke, Ui, pos2, vec2};
use keyframexl::fx::{FxKind, FxState};
use std::f32::consts::TAU;

struct Display {
    title: &'static str,
    axis: &'static str,
    color: Color32,
    samples: usize,
    rate: f32,
}

fn display(kind: FxKind, tr: &crate::i18n::Strings) -> Option<Display> {
    use FxKind::*;
    let (title, axis, rgb, samples, rate) = match kind {
        AutoPan => (
            tr.fxr_autopan,
            "L / R     ·     2 s",
            [95, 210, 203],
            16000,
            8000.,
        ),
        Tremolo => (
            tr.fxr_tremolo,
            "GAIN     ·     1 s",
            [162, 215, 121],
            8000,
            8000.,
        ),
        RingMod => (
            tr.fxr_ringmod,
            "GAIN     ·     20 ms",
            [198, 150, 237],
            960,
            48000.,
        ),
        Phaser => (
            tr.fxr_phaser,
            "100 Hz – 10 kHz     ·     2 s",
            [246, 169, 103],
            256,
            48000.,
        ),
        Bitcrusher => (
            tr.fxr_sh,
            "1 kHz SINE     ·     3 ms",
            [245, 170, 85],
            144,
            48000.,
        ),
        Wavefolder => (
            tr.fxr_fold,
            "INPUT     −1 … +1",
            [247, 134, 142],
            256,
            48000.,
        ),
        TiltEq => (
            tr.fxr_shelf,
            "20 Hz – 20 kHz     ·     ±12 dB",
            [126, 186, 246],
            256,
            48000.,
        ),
        NoiseGate => (
            tr.fxr_gate,
            "GAIN     ·     500 ms",
            [123, 216, 182],
            4000,
            8000.,
        ),
        Transient => (
            tr.fxr_transient,
            "GAIN     ·     1 s",
            [244, 177, 100],
            8000,
            8000.,
        ),
        _ => return None,
    };
    Some(Display {
        title,
        axis,
        color: Color32::from_rgb(rgb[0], rgb[1], rgb[2]),
        samples,
        rate,
    })
}

fn trace(kind: FxKind, params: &[f32], view: &Display) -> Vec<[f32; 2]> {
    let n = view.samples;
    if kind == FxKind::TiltEq {
        return (0..n)
            .map(|i| {
                let hz = 20. * 1000f32.powf(i as f32 / (n - 1) as f32);
                let db = |band, gain| {
                    super::fx_views::band_db(band, hz, params[1], 0.707, gain, view.rate)
                };
                [(db(0, -params[0]) + db(6, params[0])) / 12.; 2]
            })
            .collect();
    }
    if kind == FxKind::Phaser {
        // The two cancellation frequencies of four cascaded first-order allpasses.
        return (0..n)
            .map(|i| {
                let phase = TAU * 2. * params[0] * i as f32 / (n - 1) as f32;
                let center = 760. * (2.2 * params[1] * 0.01 * phase.sin()).exp2();
                let warp = (std::f32::consts::PI * center / view.rate).tan();
                [0.41421357, 2.4142137].map(|k| {
                    let hz = (warp * k).atan() * view.rate / std::f32::consts::PI;
                    (hz / 100.).log10() - 1.
                })
            })
            .collect();
    }

    let probe = |i: usize| {
        let x = i as f32 / (n - 1) as f32;
        match kind {
            FxKind::Bitcrusher => (TAU * 3. * x).sin(),
            FxKind::Wavefolder => 2. * x - 1.,
            FxKind::NoiseGate => {
                if x < 0.12 {
                    0.00001
                } else {
                    0.9 * (-14. * (x - 0.12)).exp()
                }
            }
            FxKind::Transient => {
                if x < 0.01 {
                    1.0
                } else {
                    0.35 * (-8.0 * (x - 0.01)).exp()
                }
            }
            _ => 1.,
        }
    };
    let mut signal: Vec<_> = (0..n).flat_map(|i| [probe(i); 2]).collect();
    FxState::new(kind, view.rate).process(params, &mut signal, view.rate, 120.);
    let sample = |i: usize, ch| {
        let y = signal[2 * i + ch];
        match kind {
            FxKind::NoiseGate => 2. * y / probe(i) - 1.,
            FxKind::AutoPan => std::f32::consts::SQRT_2 * y - 1.,
            FxKind::Tremolo => 2. * y - 1.,
            _ => y,
        }
    };
    let count = n.min(512);
    (0..count)
        .map(|j| {
            let i = j * (n - 1) / (count - 1);
            [sample(i, 0), sample(i, 1)]
        })
        .collect()
}

pub fn panel(ui: &mut Ui, kind: FxKind, params: &[f32]) {
    let tr = crate::i18n::ctx_tr(ui.ctx());
    let Some(view) = display(kind, tr) else {
        return;
    };
    let id = ui.id().with(("fx-response", kind.label()));
    let points = ui.data_mut(|data| {
        let cached = data.get_temp_mut_or_insert_with(id, || {
            (
                Vec::<f32>::new(),
                std::sync::Arc::new(Vec::<[f32; 2]>::new()),
            )
        });
        if cached.0 != params {
            cached.0 = params.to_vec();
            cached.1 = std::sync::Arc::new(trace(kind, params, &view));
        }
        cached.1.clone()
    });
    let (face, _) = ui.allocate_exact_size(vec2(ui.available_width(), 142.), Sense::hover());
    let paint = ui.painter();
    paint.rect_filled(face, 5., Color32::from_rgb(22, 27, 29));
    let inset = face.shrink(12.);
    paint.text(
        inset.left_top(),
        Align2::LEFT_TOP,
        view.title,
        t::sans(14.),
        t::text(),
    );
    paint.text(
        inset.right_top(),
        Align2::RIGHT_TOP,
        view.axis,
        t::mono(10.),
        t::muted(),
    );
    let plot = Rect::from_min_max(inset.min + vec2(0., 25.), inset.max);
    for i in 0..=8 {
        let x = egui::lerp(plot.x_range(), i as f32 / 8.);
        paint.vline(
            x,
            plot.y_range(),
            Stroke::new(1.0_f32, Color32::from_gray(47)),
        );
    }
    for i in 0..=4 {
        let y = egui::lerp(plot.y_range(), i as f32 / 4.);
        paint.hline(
            plot.x_range(),
            y,
            Stroke::new(1.0_f32, Color32::from_gray(if i == 2 { 74 } else { 47 })),
        );
    }
    let channels = if matches!(kind, FxKind::AutoPan | FxKind::Phaser) {
        2
    } else {
        1
    };
    for ch in (0..channels).rev() {
        let line = points
            .iter()
            .enumerate()
            .map(|(i, v)| {
                pos2(
                    egui::lerp(plot.x_range(), i as f32 / (points.len() - 1) as f32),
                    plot.center().y - 0.47 * plot.height() * v[ch].clamp(-1., 1.),
                )
            })
            .collect();
        let color = if ch == 0 {
            view.color
        } else {
            Color32::from_rgb(203, 181, 236)
        };
        paint.add(egui::Shape::line(line, Stroke::new(2.0_f32, color)));
    }
}
