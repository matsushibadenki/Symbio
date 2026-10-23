//! Small semantic label metadata shares exact world anchors / camera with GPU ruler lines.
use super::surface_gpu::Vertex;
use serde::Serialize;
#[derive(Clone)]
pub struct Label {
    pub axis: &'static str,
    pub text: String,
    pub position: [f32; 3],
    pub title: bool,
}
pub struct Axes {
    pub lines: Vec<Vertex>,
    pub labels: Vec<Label>,
}
#[derive(Clone, Serialize)]
pub struct ScreenLabel {
    pub axis: &'static str,
    pub text: String,
    pub position: [f32; 2],
    pub title: bool,
}
fn mapped(value: f64, range: [f64; 2], extent: f32) -> f32 {
    ((value - range[0]) / (range[1] - range[0])) as f32 * extent - extent / 2.
}
fn text(value: f64, range: [f64; 2]) -> String {
    if value == 0. {
        return "0".into();
    }
    let span = range[1] - range[0];
    if span == 0. {
        return value.to_string();
    }
    let resolution = span / 4.;
    if value.abs() >= 1e6 || value.abs() < 1e-3 {
        let digits =
            (value.abs().log10().floor() - resolution.log10().floor() + 1.).clamp(0., 16.) as usize;
        let formatted = format!("{value:.digits$e}");
        let (mantissa, exponent) = formatted.split_once('e').unwrap();
        format!(
            "{}e{}",
            if mantissa.contains('.') {
                mantissa.trim_end_matches('0').trim_end_matches('.')
            } else {
                mantissa
            },
            exponent
        )
    } else {
        let digits = (-resolution.log10().floor() + 1.).clamp(0., 24.) as usize;
        let formatted = format!("{value:.digits$}");
        if formatted.contains('.') {
            formatted.trim_end_matches('0').trim_end_matches('.').into()
        } else {
            formatted
        }
    }
}
fn ticks([min, max]: [f64; 2]) -> Vec<f64> {
    if max == min {
        return vec![min];
    }
    let rough = (max - min) / 4.;
    let power = 10f64.powf(rough.log10().floor());
    let fraction = rough / power;
    let step = power
        * if fraction <= 1. {
            1.
        } else if fraction <= 2. {
            2.
        } else if fraction <= 2.5 {
            2.5
        } else if fraction <= 5. {
            5.
        } else {
            10.
        };
    let first = (min / step).ceil();
    let mut values = vec![];
    for i in 0..8 {
        let v = (first + i as f64) * step;
        if v > max + step * 1e-9 {
            break;
        }
        if v >= min - step * 1e-9 {
            values.push(if v.abs() < step * 1e-9 { 0. } else { v });
        }
    }
    if values.len() < 2 {
        vec![min, max]
    } else {
        values
    }
}
impl Axes {
    pub fn build(x: [f64; 2], y: [f64; 2], z: [f64; 2], yaw: f32) -> Self {
        let mut out = Self {
            lines: vec![],
            labels: vec![],
        };
        let sx = if yaw.sin() >= 0. { 1. } else { -1. };
        let sy = if yaw.cos() >= 0. { 1. } else { -1. };
        let floor = -1.35;
        let edge_x = 2. * sx;
        let edge_y = 2. * sy;
        let colors = [[0.65, 0.22, 0.20], [0.13, 0.47, 0.34], [0.23, 0.34, 0.65]];
        let mut line = |a, b, axis: usize| {
            out.lines.extend([
                Vertex {
                    position: a,
                    color: colors[axis],
                },
                Vertex {
                    position: b,
                    color: colors[axis],
                },
            ])
        };
        line([-2., floor, edge_y], [2.25, floor, edge_y], 0);
        line([edge_x, floor, -2.], [edge_x, floor, 2.25], 1);
        let flat = z[0] == z[1] || z[1] - z[0] < 1e-10;
        line([edge_x, floor, edge_y], [edge_x, 1.5, edge_y], 2);
        for v in ticks(x) {
            let p = mapped(v, x, 4.);
            line([p, floor, edge_y], [p, floor, edge_y + sy * 0.1], 0);
            out.labels.push(Label {
                axis: "x",
                text: text(v, x),
                position: [p, floor, edge_y + sy * 0.25],
                title: false,
            });
        }
        for v in ticks(y) {
            let p = mapped(v, y, 4.);
            line([edge_x, floor, p], [edge_x + sx * 0.1, floor, p], 1);
            out.labels.push(Label {
                axis: "y",
                text: text(v, y),
                position: [edge_x + sx * 0.25, floor, p],
                title: false,
            });
        }
        for v in ticks(z) {
            let p = if flat { 0. } else { mapped(v, z, 2.5) };
            line([edge_x, p, edge_y], [edge_x + sx * 0.1, p, edge_y], 2);
            out.labels.push(Label {
                axis: "z",
                text: text(v, z),
                position: [edge_x + sx * 0.28, p, edge_y + sy * 0.08],
                title: false,
            });
        }
        for (axis, position) in [
            ("x", [0., floor, edge_y + sy * 0.65]),
            ("y", [edge_x + sx * 0.65, floor, 0.]),
            ("z", [edge_x, 1.75, edge_y]),
        ] {
            out.labels.push(Label {
                axis,
                text: axis.into(),
                position,
                title: true,
            });
        }
        out
    }
    pub fn project(&self, matrix: [[f32; 4]; 4]) -> Vec<ScreenLabel> {
        self.labels
            .iter()
            .filter_map(|l| {
                let p = [l.position[0], l.position[1], l.position[2], 1.];
                let clip: [f32; 4] =
                    std::array::from_fn(|row| (0..4).map(|col| matrix[col][row] * p[col]).sum());
                if clip.iter().any(|v| !v.is_finite()) || clip[3] <= 0. {
                    return None;
                }
                let [x, y, z] = [clip[0] / clip[3], clip[1] / clip[3], clip[2] / clip[3]];
                if !(-1.0..=1.0).contains(&x)
                    || !(-1.0..=1.0).contains(&y)
                    || !(0.0..=1.0).contains(&z)
                {
                    return None;
                }
                Some(ScreenLabel {
                    axis: l.axis,
                    text: l.text.clone(),
                    position: [(x + 1.) / 2., (1. - y) / 2.],
                    title: l.title,
                })
            })
            .collect()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ticks_preserve_asymmetric_ranges_and_avoid_negative_zero() {
        assert_eq!(ticks([-2., 2.]), vec![-2., -1., 0., 1., 2.]);
        assert_eq!(ticks([10., 30.]), vec![10., 15., 20., 25., 30.]);
        assert_eq!(text(-0., [-1., 1.]), "0");
        assert_eq!(ticks([7., 7.]), vec![7.]);
        for range in [[1e-6, 5e-6], [-1e6, 1e6], [-3., 7.]] {
            let t = ticks(range);
            assert!(t.len() <= 8);
            assert!(
                t.iter()
                    .all(|v| v.is_finite() && *v >= range[0] && *v <= range[1])
            );
        }
    }
    #[test]
    fn ruler_positions_and_values_use_independent_axis_domains() {
        let a = Axes::build([-2., 2.], [10., 30.], [-4., 4.], 0.7);
        let y = a
            .labels
            .iter()
            .find(|l| l.axis == "y" && l.text == "20")
            .unwrap();
        assert_eq!(y.position[2], 0.);
        let z = a
            .labels
            .iter()
            .find(|l| l.axis == "z" && l.text == "0")
            .unwrap();
        assert_eq!(z.position[1], 0.);
        assert!(a.lines.len() < 256);
    }
    #[test]
    fn flat_height_has_one_truthful_tick() {
        let a = Axes::build([-1., 1.], [-1., 1.], [42., 42.], 0.);
        let z: Vec<_> = a
            .labels
            .iter()
            .filter(|l| l.axis == "z" && !l.title)
            .collect();
        assert_eq!(z.len(), 1);
        assert_eq!(z[0].text, "42");
        assert_eq!(z[0].position[1], 0.);
    }
}

#[cfg(test)]
mod projection_tests {
    use super::*;
    #[test]
    fn label_anchors_match_gpu_perspective_division_and_screen_y_direction() {
        let axes = Axes {
            lines: vec![],
            labels: vec![Label {
                axis: "x",
                text: "1".into(),
                position: [0.5, 0.25, 0.5],
                title: false,
            }],
        };
        let mut m = threex_core::identity();
        m[3][3] = 2.;
        let projected = axes.project(m);
        assert_eq!(projected[0].position, [0.625, 0.4375]);
        m[3][3] = -2.;
        assert!(axes.project(m).is_empty());
    }
}

#[cfg(test)]
mod precision_tests {
    #[test]
    fn narrow_ranges_at_large_offsets_have_distinct_tick_labels() {
        let range = [999999., 1000000.];
        let labels: Vec<_> = super::ticks(range)
            .into_iter()
            .map(|v| super::text(v, range))
            .collect();
        assert_eq!(
            labels
                .iter()
                .collect::<std::collections::BTreeSet<_>>()
                .len(),
            labels.len()
        );
        let range = [1e12, 1e12 + 1.];
        assert_ne!(super::text(1e12, range), super::text(1e12 + 0.5, range));
    }
}
