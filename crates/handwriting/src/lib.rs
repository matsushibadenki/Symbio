//! Reusable, GUI-independent stroke-to-LaTeX recognizer. No document or network access.
use image::{GrayImage, Luma};
use ort::{session::Session, value::Tensor};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};
pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;
/// Cooperative cancellation shared with the host; no GUI/runtime dependency.
#[derive(Clone, Default)]
pub struct CancellationToken(Arc<AtomicBool>);
#[derive(Debug)]
pub struct Cancelled;
impl std::fmt::Display for Cancelled {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("recognition cancelled")
    }
}
impl std::error::Error for Cancelled {}
impl CancellationToken {
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
    fn check(&self) -> Result<()> {
        if self.is_cancelled() {
            Err(Box::new(Cancelled))
        } else {
            Ok(())
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Ink {
    pub strokes: Vec<Vec<Point>>,
}
impl Ink {
    pub fn raster(&self) -> Result<GrayImage> {
        let points: Vec<_> = self.strokes.iter().flatten().collect();
        if self.strokes.len() > 256
            || points.len() < 2
            || points.len() > 16384
            || points.iter().any(|p| {
                !p.x.is_finite() || !p.y.is_finite() || p.x.abs() > 10000. || p.y.abs() > 10000.
            })
        {
            return Err("invalid ink limits".into());
        }
        let minx = points.iter().map(|p| p.x).fold(f32::INFINITY, f32::min);
        let miny = points.iter().map(|p| p.y).fold(f32::INFINITY, f32::min);
        let maxx = points.iter().map(|p| p.x).fold(f32::NEG_INFINITY, f32::max);
        let maxy = points.iter().map(|p| p.y).fold(f32::NEG_INFINITY, f32::max);
        let scale =
            (128. / ((maxy - miny).max(1.) + 32.)).min(1008. / ((maxx - minx).max(1.) + 32.));
        let width =
            ((((maxx - minx) * scale + 16.).ceil() as u32).div_ceil(64) * 64).clamp(128, 1024);
        let mut image = GrayImage::new(width, 256);
        for stroke in &self.strokes {
            for segment in stroke.windows(2) {
                let a = &segment[0];
                let b = &segment[1];
                let dx = (b.x - a.x) * scale;
                let dy = (b.y - a.y) * scale;
                let steps = dx.abs().max(dy.abs()).ceil() as usize;
                for i in 0..=steps {
                    let t = i as f32 / steps.max(1) as f32;
                    let x = ((a.x - minx) * scale + 16. * scale + t * dx).round() as i32;
                    let y = ((a.y - miny) * scale + 16. * scale + t * dy).round() as i32;
                    for oy in -2..=2 {
                        for ox in -2..=2 {
                            if ox * ox + oy * oy <= 4
                                && x + ox >= 0
                                && y + oy >= 0
                                && x + ox < width as i32
                                && y + oy < 256
                            {
                                image.put_pixel((x + ox) as u32, (y + oy) as u32, Luma([255]));
                            }
                        }
                    }
                }
            }
        }
        Ok(image)
    }
}
#[derive(Debug, Serialize)]
pub struct Candidate {
    pub latex: String,
    pub complete: bool,
    pub tokens: usize,
    /// Uncalibrated cumulative token log-probability, not a confidence percentage.
    pub log_score: f64,
}
#[derive(Debug, Serialize)]
pub struct Recognition {
    pub latex: String,
    pub complete: bool,
    pub tokens: usize,
    pub elapsed_ms: u128,
    pub candidates: Vec<Candidate>,
}
pub struct Recognizer {
    encoder: Session,
    decoder: Session,
    vocabulary: BTreeMap<String, String>,
}
impl Recognizer {
    pub fn load(models: &Path, runtime: &Path) -> Result<Self> {
        for name in ["encoder_int8.onnx", "decoder_int8.onnx", "vocab.json"] {
            let size = std::fs::metadata(models.join(name))?.len();
            if size == 0 || size > 32 * 1024 * 1024 || (name == "vocab.json" && size > 65536) {
                return Err("model file size".into());
            }
        }
        // ONNX Runtime is process-global; callers configure it before any session.
        ort::init_from(runtime.to_str().ok_or("runtime path")?)?
            .with_telemetry(false)
            .commit();
        let options = || Session::builder()?.with_intra_threads(2);
        let encoder = options()?.commit_from_file(models.join("encoder_int8.onnx"))?;
        let decoder = options()?.commit_from_file(models.join("decoder_int8.onnx"))?;
        let value: serde_json::Value =
            serde_json::from_slice(&std::fs::read(models.join("vocab.json"))?)?;
        let vocabulary: BTreeMap<String, String> =
            serde_json::from_value(value["idx2word"].clone())?;
        if vocabulary.len() != 113
            || value["special_tokens"]["sos"] != 1
            || value["special_tokens"]["eos"] != 2
        {
            return Err("incompatible model vocabulary".into());
        }
        Ok(Self {
            encoder,
            decoder,
            vocabulary,
        })
    }
    pub fn recognize(&mut self, ink: &Ink) -> Result<Recognition> {
        self.recognize_image(ink.raster()?)
    }
    /// Bounded width-three search; returns at most three distinct suggestions.
    pub fn recognize_candidates(&mut self, ink: &Ink) -> Result<Recognition> {
        self.recognize_candidates_with_cancel(ink, &CancellationToken::default())
    }
    /// Stops between native ONNX calls; an in-flight call is allowed to finish.
    pub fn recognize_candidates_with_cancel(
        &mut self,
        ink: &Ink,
        cancel: &CancellationToken,
    ) -> Result<Recognition> {
        cancel.check()?;
        self.decode_image(ink.raster()?, 3, cancel)
    }
    pub fn recognize_image(&mut self, image: GrayImage) -> Result<Recognition> {
        self.decode_image(image, 1, &CancellationToken::default())
    }
    fn decode_image(
        &mut self,
        image: GrayImage,
        width: usize,
        cancel: &CancellationToken,
    ) -> Result<Recognition> {
        cancel.check()?;
        let start = std::time::Instant::now();
        if !image.as_raw().iter().any(|v| *v > 0) {
            return Err("empty ink image".into());
        }
        let (w, h) = image.dimensions();
        if h != 256 || w == 0 || w > 1024 {
            return Err("normalized image shape".into());
        }
        let pixels: Vec<f32> = image.as_raw().iter().map(|v| *v as f32 / 255.).collect();
        cancel.check()?;
        let encoded=self.encoder.run(ort::inputs!{
            "pixel_values" => Tensor::from_array(([1,1,h as usize,w as usize], pixels))?,
            "pixel_mask" => Tensor::from_array(([1,h as usize,w as usize], (0..h).flat_map(|y| (0..w).map(move |_| y >= 128)).collect::<Vec<bool>>()))?
        })?;
        let (shape, data) = encoded["encoder_features"].try_extract_tensor::<f32>()?;
        let features = Tensor::from_array((shape.to_vec(), data.to_vec()))?;
        let (shape, data) = encoded["encoder_mask"].try_extract_tensor::<bool>()?;
        let mask = Tensor::from_array((shape.to_vec(), data.to_vec()))?;
        cancel.check()?;
        let beams = search(width, |ids| {
            cancel.check()?;
            let result = self.decoder.run(ort::inputs! {
                "encoder_features" => &features, "encoder_mask" => &mask,
                "input_ids" => Tensor::from_array(([1,ids.len()],ids.to_vec()))?
            })?;
            let (_, logits) = result["logits"].try_extract_tensor::<f32>()?;
            Ok(logits
                .get((ids.len() - 1) * 113..ids.len() * 113)
                .ok_or("decoder shape")?
                .to_vec())
        })?;
        cancel.check()?;
        let mut candidates = Vec::new();
        for beam in beams {
            let ids = &beam.ids[1..];
            let latex = ids
                .iter()
                .map(|i| {
                    self.vocabulary
                        .get(&i.to_string())
                        .cloned()
                        .unwrap_or_default()
                        .replace("\\\\", "\\")
                })
                .collect::<Vec<_>>()
                .join(" ");
            if !candidates.iter().any(|c: &Candidate| c.latex == latex) {
                candidates.push(Candidate {
                    latex,
                    complete: beam.complete,
                    tokens: ids.len(),
                    log_score: beam.score,
                });
            }
        }
        let first = candidates.first().ok_or("empty recognition")?;
        Ok(Recognition {
            latex: first.latex.clone(),
            complete: first.complete,
            tokens: first.tokens,
            elapsed_ms: start.elapsed().as_millis(),
            candidates,
        })
    }
}
#[derive(Clone, Debug)]
struct Beam {
    ids: Vec<i64>,
    score: f64,
    complete: bool,
    stopped: bool,
}
// At most 128 steps × 3 decoder calls. Completed hypotheses compete with live
// ones by cumulative log probability; scores include EOS and are not calibrated.
fn search(width: usize, mut logits: impl FnMut(&[i64]) -> Result<Vec<f32>>) -> Result<Vec<Beam>> {
    if !(1..=3).contains(&width) {
        return Err("beam width".into());
    }
    let mut beams = vec![Beam {
        ids: vec![1],
        score: 0.,
        complete: false,
        stopped: false,
    }];
    for _ in 0..128 {
        let mut pool = Vec::new();
        for beam in beams {
            if beam.stopped {
                pool.push(beam);
                continue;
            }
            let values = logits(&beam.ids)?;
            if values.len() != 113 || values.iter().any(|v| !v.is_finite()) {
                return Err("invalid decoder scores".into());
            }
            let max = values.iter().copied().fold(f32::NEG_INFINITY, f32::max) as f64;
            let normalizer = max
                + values
                    .iter()
                    .map(|v| (*v as f64 - max).exp())
                    .sum::<f64>()
                    .ln();
            let mut ranked: Vec<_> = (2..113).collect();
            ranked.sort_by(|a, b| values[*b].total_cmp(&values[*a]).then(a.cmp(b)));
            for token in ranked.into_iter().take(width) {
                let mut next = beam.clone();
                next.score += values[token] as f64 - normalizer;
                next.complete = token == 2;
                if !next.complete {
                    next.ids.push(token as i64);
                }
                next.stopped = next.complete
                    || (next.ids.len() > 4
                        && next.ids[next.ids.len() - 4..]
                            .iter()
                            .all(|v| *v == token as i64));
                pool.push(next);
            }
        }
        pool.sort_by(|a, b| b.score.total_cmp(&a.score).then(a.ids.cmp(&b.ids)));
        pool.truncate(width);
        let finished = pool.iter().all(|b| b.stopped);
        beams = pool;
        if finished {
            break;
        }
    }
    Ok(beams)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cloned_token_cancels_across_threads_and_stops_search() {
        let token = CancellationToken::default();
        let worker = token.clone();
        assert!(token.check().is_ok());
        std::thread::spawn(move || worker.cancel()).join().unwrap();
        assert!(token.check().unwrap_err().is::<Cancelled>());
        let token = CancellationToken::default();
        let mut calls = 0;
        let result = search(3, |_| {
            token.check()?;
            calls += 1;
            token.cancel();
            let mut scores = vec![-100.; 113];
            scores[3] = 10.;
            Ok(scores)
        });
        assert!(result.unwrap_err().is::<Cancelled>());
        assert_eq!(calls, 1);
    }
    #[test]
    fn beam_can_recover_a_better_sequence_than_greedy() {
        let decode = |ids: &[i64]| {
            let mut scores = vec![-100.; 113];
            if ids.len() == 1 {
                scores[3] = 2.;
                scores[4] = 1.9;
            } else if ids[1] == 3 {
                // An uncertain continuation of the locally preferred token.
                scores[2..].fill(0.);
            } else {
                scores[2] = 10.;
            }
            Ok(scores)
        };
        let greedy = search(1, decode).unwrap();
        let beam = search(3, decode).unwrap();
        assert_eq!(greedy[0].ids[1], 3);
        assert_eq!(beam[0].ids, vec![1, 4]);
        assert!(beam[0].complete);
        assert!(beam.windows(2).all(|b| b[0].score >= b[1].score));
        assert!(beam.iter().all(|b| b.score.is_finite() && b.score <= 0.));
    }
    #[test]
    fn search_rejects_bad_scores_and_enforces_bounds() {
        assert!(search(0, |_| Ok(vec![0.; 113])).is_err());
        assert!(search(4, |_| Ok(vec![0.; 113])).is_err());
        assert!(search(3, |_| Ok(vec![0.; 112])).is_err());
        assert!(search(3, |_| Ok(vec![f32::NAN; 113])).is_err());
        let mut calls = 0;
        let beams = search(3, |ids| {
            calls += 1;
            let mut scores = vec![-100.; 113];
            scores[3 + ids.len() % 2] = 10.;
            Ok(scores)
        })
        .unwrap();
        assert!(calls <= 384);
        assert!(beams.iter().all(|b| b.ids.len() <= 129));
        assert!(!beams[0].complete);
    }
    #[test]
    fn invalid_ink_is_rejected() {
        assert!(Ink { strokes: vec![] }.raster().is_err());
        assert!(
            Ink {
                strokes: vec![vec![Point { x: f32::NAN, y: 0. }, Point { x: 0., y: 1. }]]
            }
            .raster()
            .is_err()
        );
    }
    #[test]
    fn raster_is_bounded() {
        let ink = Ink {
            strokes: vec![vec![Point { x: 0., y: 0. }, Point { x: 100., y: 100. }]],
        };
        let image = ink.raster().unwrap();
        assert_eq!(image.height(), 256);
        assert!(image.width() <= 1024);
        assert!(image.as_raw().contains(&255));
    }
}
