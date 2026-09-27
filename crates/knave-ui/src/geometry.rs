/// A rectangle in logical UI coordinates, with its origin at the top-left.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Rect {
    pub const fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    pub fn is_finite_positive(self) -> bool {
        self.x.is_finite()
            && self.y.is_finite()
            && self.width.is_finite()
            && self.height.is_finite()
            && (self.x + self.width).is_finite()
            && (self.y + self.height).is_finite()
            && self.width > 0.0
            && self.height > 0.0
    }

    pub fn intersection(self, other: Self) -> Option<Self> {
        if !self.is_finite_positive() || !other.is_finite_positive() {
            return None;
        }
        let left = self.x.max(other.x);
        let top = self.y.max(other.y);
        let right = (self.x + self.width).min(other.x + other.width);
        let bottom = (self.y + self.height).min(other.y + other.height);
        (right > left && bottom > top).then(|| Self::new(left, top, right - left, bottom - top))
    }
}

/// A 2D affine transform: x'=a*x+c*y+tx, y'=b*x+d*y+ty.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Transform2D {
    pub a: f32,
    pub b: f32,
    pub c: f32,
    pub d: f32,
    pub tx: f32,
    pub ty: f32,
}

impl Transform2D {
    pub const IDENTITY: Self = Self {
        a: 1.0,
        b: 0.0,
        c: 0.0,
        d: 1.0,
        tx: 0.0,
        ty: 0.0,
    };

    pub const fn translation(x: f32, y: f32) -> Self {
        Self {
            tx: x,
            ty: y,
            ..Self::IDENTITY
        }
    }

    pub const fn scale(x: f32, y: f32) -> Self {
        Self {
            a: x,
            d: y,
            ..Self::IDENTITY
        }
    }

    pub fn rotation(radians: f32) -> Self {
        let (sin, cos) = radians.sin_cos();
        Self {
            a: cos,
            b: sin,
            c: -sin,
            d: cos,
            tx: 0.0,
            ty: 0.0,
        }
    }

    /// Returns `self * local`, so the local transform is applied first.
    pub fn compose(self, local: Self) -> Self {
        Self {
            a: self.a * local.a + self.c * local.b,
            b: self.b * local.a + self.d * local.b,
            c: self.a * local.c + self.c * local.d,
            d: self.b * local.c + self.d * local.d,
            tx: self.a * local.tx + self.c * local.ty + self.tx,
            ty: self.b * local.tx + self.d * local.ty + self.ty,
        }
    }

    pub fn transform_point(self, point: [f32; 2]) -> [f32; 2] {
        [
            self.a * point[0] + self.c * point[1] + self.tx,
            self.b * point[0] + self.d * point[1] + self.ty,
        ]
    }

    pub fn transform_rect_bounds(self, rect: Rect) -> Rect {
        let corners = [
            self.transform_point([rect.x, rect.y]),
            self.transform_point([rect.x + rect.width, rect.y]),
            self.transform_point([rect.x, rect.y + rect.height]),
            self.transform_point([rect.x + rect.width, rect.y + rect.height]),
        ];
        let left = corners
            .iter()
            .map(|point| point[0])
            .fold(f32::INFINITY, f32::min);
        let top = corners
            .iter()
            .map(|point| point[1])
            .fold(f32::INFINITY, f32::min);
        let right = corners
            .iter()
            .map(|point| point[0])
            .fold(f32::NEG_INFINITY, f32::max);
        let bottom = corners
            .iter()
            .map(|point| point[1])
            .fold(f32::NEG_INFINITY, f32::max);
        Rect::new(left, top, right - left, bottom - top)
    }
}

impl Default for Transform2D {
    fn default() -> Self {
        Self::IDENTITY
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn composition_applies_local_then_parent() {
        let transform = Transform2D::translation(10.0, 5.0).compose(Transform2D::scale(2.0, 3.0));
        assert_eq!(transform.transform_point([2.0, 4.0]), [14.0, 17.0]);
    }

    #[test]
    fn transformed_bounds_include_all_corners() {
        let transform = Transform2D::rotation(std::f32::consts::FRAC_PI_2);
        let bounds = transform.transform_rect_bounds(Rect::new(0.0, 0.0, 2.0, 1.0));
        assert!((bounds.x + 1.0).abs() < 0.0001);
        assert!(bounds.y.abs() < 0.0001);
        assert!((bounds.width - 1.0).abs() < 0.0001);
        assert!((bounds.height - 2.0).abs() < 0.0001);
    }

    #[test]
    fn rectangle_intersection_clips_and_rejects_empty_results() {
        assert_eq!(
            Rect::new(0.0, 0.0, 10.0, 10.0).intersection(Rect::new(6.0, 4.0, 8.0, 8.0)),
            Some(Rect::new(6.0, 4.0, 4.0, 6.0))
        );
        assert_eq!(
            Rect::new(0.0, 0.0, 2.0, 2.0).intersection(Rect::new(3.0, 3.0, 2.0, 2.0)),
            None
        );
    }
}
