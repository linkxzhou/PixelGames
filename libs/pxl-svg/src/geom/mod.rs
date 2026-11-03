//! Geometry: affine transforms, path data, basic shapes, curve flattening and
//! stroke expansion. Curves are transformed to pixel space before they are
//! flattened (an affine map keeps a Bézier a Bézier), so the tolerance is in
//! output pixels whatever the viewBox.

mod affine;
mod path;
mod stroke;

pub use affine::{parse_transform, Affine, Pt};
pub use path::{ellipse, path, points, rect, PathBuilder, Poly};
pub use stroke::{stroke, Cap, Join};

#[cfg(test)]
mod tests;
