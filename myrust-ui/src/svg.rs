//! SVG path subset parser.
//!
//! Supported commands: M/m, L/l, H/h, V/v, Q/q, C/c, A/a, and Z/z.
//! Curves and elliptical arcs are flattened into polylines. The CPU presenter
//! fills closed contours and strokes open or closed polylines.

use super::layout::Point;

/// One flattened path contour.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SvgContour {
    /// Flattened points.
    pub points: Vec<Point>,
    /// Whether the contour was explicitly closed.
    pub closed: bool,
}

/// Flattened SVG path.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SvgOutline {
    /// Path contours in command order.
    pub contours: Vec<SvgContour>,
}

impl SvgOutline {
    /// Whether the path produced no points.
    pub fn is_empty(&self) -> bool {
        self.contours
            .iter()
            .all(|contour| contour.points.is_empty())
    }
}

/// Parses an SVG path subset into flattened contours.
///
/// Malformed or unsupported command data is ignored rather than raising an
/// error; an empty path returns an empty outline.
pub fn parse_svg_path(path: &str) -> SvgOutline {
    let commands = tokenize(path);
    let mut outline = SvgOutline::default();
    let mut current = Point::new(0.0, 0.0);
    let mut subpath_start = Point::new(0.0, 0.0);
    let mut current_index: Option<usize> = None;

    for (command, arguments) in commands {
        let relative = command.is_ascii_lowercase();
        match command.to_ascii_uppercase() {
            'M' => {
                let mut index = 0;
                let mut first = true;
                while index + 1 < arguments.len() {
                    let x = arguments[index];
                    let y = arguments[index + 1];
                    index += 2;
                    let point = if relative {
                        Point::new(current.x + x, current.y + y)
                    } else {
                        Point::new(x, y)
                    };
                    if first {
                        outline.contours.push(SvgContour {
                            points: vec![point],
                            closed: false,
                        });
                        current_index = Some(outline.contours.len() - 1);
                        subpath_start = point;
                        first = false;
                    } else {
                        push_point(&mut outline, current_index, point);
                    }
                    current = point;
                }
            }
            'L' => {
                let mut index = 0;
                while index + 1 < arguments.len() {
                    let point =
                        point_from(&current, arguments[index], arguments[index + 1], relative);
                    push_point(&mut outline, current_index, point);
                    current = point;
                    index += 2;
                }
            }
            'H' => {
                for value in arguments {
                    let point = if relative {
                        Point::new(current.x + value, current.y)
                    } else {
                        Point::new(value, current.y)
                    };
                    push_point(&mut outline, current_index, point);
                    current = point;
                }
            }
            'V' => {
                for value in arguments {
                    let point = if relative {
                        Point::new(current.x, current.y + value)
                    } else {
                        Point::new(current.x, value)
                    };
                    push_point(&mut outline, current_index, point);
                    current = point;
                }
            }
            'Q' => {
                let mut index = 0;
                while index + 3 < arguments.len() {
                    let control =
                        point_from(&current, arguments[index], arguments[index + 1], relative);
                    let end = point_from(
                        &current,
                        arguments[index + 2],
                        arguments[index + 3],
                        relative,
                    );
                    flatten_quadratic(&mut outline, current_index, current, control, end);
                    current = end;
                    index += 4;
                }
            }
            'C' => {
                let mut index = 0;
                while index + 5 < arguments.len() {
                    let control1 =
                        point_from(&current, arguments[index], arguments[index + 1], relative);
                    let control2 = point_from(
                        &current,
                        arguments[index + 2],
                        arguments[index + 3],
                        relative,
                    );
                    let end = point_from(
                        &current,
                        arguments[index + 4],
                        arguments[index + 5],
                        relative,
                    );
                    flatten_cubic(
                        &mut outline,
                        current_index,
                        current,
                        control1,
                        control2,
                        end,
                    );
                    current = end;
                    index += 6;
                }
            }
            'A' => {
                let mut index = 0;
                while index + 6 < arguments.len() {
                    let rx = arguments[index].abs();
                    let ry = arguments[index + 1].abs();
                    let rotation = arguments[index + 2].to_radians();
                    let large = arguments[index + 3] != 0.0;
                    let sweep = arguments[index + 4] != 0.0;
                    let end = point_from(
                        &current,
                        arguments[index + 5],
                        arguments[index + 6],
                        relative,
                    );
                    flatten_arc(
                        &mut outline,
                        current_index,
                        current,
                        end,
                        rx,
                        ry,
                        rotation,
                        large,
                        sweep,
                    );
                    current = end;
                    index += 7;
                }
            }
            'Z' => {
                if let Some(index) = current_index {
                    outline.contours[index].closed = true;
                    if outline.contours[index].points.last().copied() != Some(subpath_start) {
                        outline.contours[index].points.push(subpath_start);
                    }
                }
                current = subpath_start;
            }
            _ => {}
        }
    }
    outline
}

fn tokenize(path: &str) -> Vec<(char, Vec<f32>)> {
    let mut commands = Vec::new();
    let mut index = 0;
    let bytes = path.as_bytes();
    while index < bytes.len() {
        let byte = bytes[index];
        if byte.is_ascii_alphabetic() {
            let command = byte as char;
            index += 1;
            let mut arguments = Vec::new();
            loop {
                while index < bytes.len()
                    && (bytes[index] == b' '
                        || bytes[index] == b','
                        || bytes[index] == b'\t'
                        || bytes[index] == b'\n'
                        || bytes[index] == b'\r')
                {
                    index += 1;
                }
                if index >= bytes.len() || bytes[index].is_ascii_alphabetic() {
                    break;
                }
                if let Some((number, next)) = parse_number(path, index) {
                    arguments.push(number);
                    index = next;
                } else {
                    index += 1;
                }
            }
            commands.push((command, arguments));
        } else {
            index += 1;
        }
    }
    commands
}

fn parse_number(path: &str, start: usize) -> Option<(f32, usize)> {
    let bytes = path.as_bytes();
    let mut index = start;
    if index >= bytes.len() {
        return None;
    }
    if matches!(bytes[index], b'+' | b'-') {
        index += 1;
    }
    let mut digits = false;
    while index < bytes.len() && bytes[index].is_ascii_digit() {
        digits = true;
        index += 1;
    }
    if index < bytes.len() && bytes[index] == b'.' {
        index += 1;
        while index < bytes.len() && bytes[index].is_ascii_digit() {
            digits = true;
            index += 1;
        }
    }
    if !digits {
        return None;
    }
    if index < bytes.len() && matches!(bytes[index], b'e' | b'E') {
        let exponent_start = index;
        index += 1;
        if index < bytes.len() && matches!(bytes[index], b'+' | b'-') {
            index += 1;
        }
        let mut exponent_digits = false;
        while index < bytes.len() && bytes[index].is_ascii_digit() {
            exponent_digits = true;
            index += 1;
        }
        if !exponent_digits {
            index = exponent_start;
        }
    }
    path[start..index]
        .parse::<f32>()
        .ok()
        .map(|number| (number, index))
}

fn point_from(current: &Point, x: f32, y: f32, relative: bool) -> Point {
    if relative {
        Point::new(current.x + x, current.y + y)
    } else {
        Point::new(x, y)
    }
}

fn push_point(outline: &mut SvgOutline, current_index: Option<usize>, point: Point) {
    if let Some(index) = current_index {
        outline.contours[index].points.push(point);
    } else {
        outline.contours.push(SvgContour {
            points: vec![point],
            closed: false,
        });
    }
}

fn flatten_quadratic(
    outline: &mut SvgOutline,
    current_index: Option<usize>,
    start: Point,
    control: Point,
    end: Point,
) {
    let steps = 12;
    for step in 1..=steps {
        let t = step as f32 / steps as f32;
        let one = 1.0 - t;
        let point = Point::new(
            one * one * start.x + 2.0 * one * t * control.x + t * t * end.x,
            one * one * start.y + 2.0 * one * t * control.y + t * t * end.y,
        );
        push_point(outline, current_index, point);
    }
}

fn flatten_cubic(
    outline: &mut SvgOutline,
    current_index: Option<usize>,
    start: Point,
    control1: Point,
    control2: Point,
    end: Point,
) {
    let steps = 16;
    for step in 1..=steps {
        let t = step as f32 / steps as f32;
        let one = 1.0 - t;
        let point = Point::new(
            one * one * one * start.x
                + 3.0 * one * one * t * control1.x
                + 3.0 * one * t * t * control2.x
                + t * t * t * end.x,
            one * one * one * start.y
                + 3.0 * one * one * t * control1.y
                + 3.0 * one * t * t * control2.y
                + t * t * t * end.y,
        );
        push_point(outline, current_index, point);
    }
}

#[allow(clippy::too_many_arguments)]
fn flatten_arc(
    outline: &mut SvgOutline,
    current_index: Option<usize>,
    start: Point,
    end: Point,
    rx: f32,
    ry: f32,
    rotation: f32,
    large_arc: bool,
    sweep: bool,
) {
    if rx <= 0.0 || ry <= 0.0 {
        push_point(outline, current_index, end);
        return;
    }
    let cos = rotation.cos();
    let sin = rotation.sin();
    let dx = (start.x - end.x) / 2.0;
    let dy = (start.y - end.y) / 2.0;
    let x1 = cos * dx + sin * dy;
    let y1 = -sin * dx + cos * dy;
    let radii_scale = x1 * x1 / (rx * rx) + y1 * y1 / (ry * ry);
    let scale = if radii_scale > 1.0 {
        radii_scale.sqrt()
    } else {
        1.0
    };
    let rx = rx * scale;
    let ry = ry * scale;
    let denominator = rx * rx * y1 * y1 + ry * ry * x1 * x1;
    let numerator = (rx * rx * ry * ry - denominator).max(0.0);
    let sign = if large_arc == sweep { -1.0 } else { 1.0 };
    let coefficient = if denominator > f32::EPSILON {
        sign * (numerator / denominator).sqrt()
    } else {
        0.0
    };
    let center_x1 = coefficient * rx * y1 / ry;
    let center_y1 = -coefficient * ry * x1 / rx;
    let center = Point::new(
        cos * center_x1 - sin * center_y1 + (start.x + end.x) / 2.0,
        sin * center_x1 + cos * center_y1 + (start.y + end.y) / 2.0,
    );
    let start_angle = ((y1 - center_y1) / ry).atan2((x1 - center_x1) / rx);
    let end_angle = ((-y1 - center_y1) / ry).atan2((-x1 - center_x1) / rx);
    let mut delta = end_angle - start_angle;
    if !sweep && delta > 0.0 {
        delta -= std::f32::consts::TAU;
    } else if sweep && delta < 0.0 {
        delta += std::f32::consts::TAU;
    }
    let steps = ((delta.abs() / std::f32::consts::TAU) * 64.0)
        .ceil()
        .clamp(1.0, 128.0) as usize;

    for step in 1..=steps {
        let angle = start_angle + delta * step as f32 / steps as f32;
        let point = Point::new(
            center.x + cos * rx * angle.cos() - sin * ry * angle.sin(),
            center.y + sin * rx * angle.cos() + cos * ry * angle.sin(),
        );
        push_point(outline, current_index, point);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn contour(path: &str) -> SvgContour {
        let outline = parse_svg_path(path);
        assert_eq!(outline.contours.len(), 1, "path {path}");
        outline.contours[0].clone()
    }

    #[test]
    fn parses_absolute_relative_move_line_quadratic_cubic_and_close() {
        let absolute = contour("M0 0L10 0L10 10Z");
        assert!(absolute.closed);
        assert_eq!(absolute.points.first(), Some(&Point::new(0.0, 0.0)));
        assert_eq!(absolute.points.get(1), Some(&Point::new(10.0, 0.0)));
        assert_eq!(absolute.points.get(2), Some(&Point::new(10.0, 10.0)));
        assert_eq!(absolute.points.last(), Some(&Point::new(0.0, 0.0)));

        let relative = contour("M10 10l5 0v5h-5z");
        assert!(relative.closed);
        assert_eq!(relative.points.first(), Some(&Point::new(10.0, 10.0)));
        assert_eq!(relative.points.get(1), Some(&Point::new(15.0, 10.0)));
        assert_eq!(relative.points.get(2), Some(&Point::new(15.0, 15.0)));
        assert_eq!(relative.points.get(3), Some(&Point::new(10.0, 15.0)));

        let quadratic = contour("M0 0Q5 10 10 0");
        assert_eq!(quadratic.points.len(), 13);
        assert_eq!(quadratic.points.last(), Some(&Point::new(10.0, 0.0)));

        let cubic = contour("M0 0C3 10 7 10 10 0");
        assert_eq!(cubic.points.len(), 17);
        assert_eq!(cubic.points.last(), Some(&Point::new(10.0, 0.0)));
    }

    #[test]
    fn parses_elliptical_arc_and_empty_paths() {
        let arc = contour("M10.5 4A6.5 6.5 0 1 0 10.5 17A6.5 6.5 0 1 0 10.5 4");
        assert!(arc.points.len() > 16);
        let end = arc.points.last().copied().expect("arc points");
        assert!((end.x - 10.5).abs() < 0.01);
        assert!((end.y - 4.0).abs() < 0.01);
        assert!(parse_svg_path("").is_empty());
        assert!(parse_svg_path("garbage").is_empty());
    }
}
