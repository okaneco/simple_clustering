//! Image segmentation based on clustering methods.
//!
//! Currently supported algorithms are the **SLIC** (*simple linear iterative
//! clustering*) and **SNIC** (*simple non-iterative clustering*) superpixel
//! algorithms. The crate also supports drawing basic contours around the image
//! segments.
//!
//! The library uses the `palette` crate for some of its color types. The
//! current version used is `palette 0.6`.
//!
//! ## Usage
//!
//! Note that the convenience methods [`slic_from_bytes`] and
//! [`snic_from_bytes`] also exist to allow for calculation of superpixel labels
//! without having to convert to `Lab`.
//!
//! ### SNIC
//!
//! ```
//! use palette::{FromColor, Lab, Pixel, Srgb};
//! use simple_clustering::snic;
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! # let (width, height) = (1, 3);
//! # let image = [0u8, 0, 0, 127, 127, 127, 255, 255, 255];
//! # let (k, m) = (1, 10);
//! let lab_buffer: Vec<Lab<_, f64>> = Srgb::from_raw_slice(&image)
//!     .iter()
//!     .map(|&c| Lab::from_color(c.into_format()))
//!     .collect();
//! let labels = snic(k, m, width, height, &lab_buffer)?;
//!
//! # Ok(())
//! # }
//! ```
//!
//! ### SLIC
//!
//! ```
//! use palette::{FromColor, Lab, Pixel, Srgb};
//! use simple_clustering::slic;
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! # let (width, height) = (1, 3);
//! # let image = [0u8, 0, 0, 127, 127, 127, 255, 255, 255];
//! # let (k, m) = (1, 10);
//! let lab_buffer: Vec<Lab<_, f64>> = Srgb::from_raw_slice(&image)
//!     .iter()
//!     .map(|&c| Lab::from_color(c.into_format()))
//!     .collect();
//! let labels = slic(k, m, width, height, None, &lab_buffer)?;
//! # Ok(())
//! # }
//! ```
//!
//! ### Mean color segments and drawing segment contours
//!
//! Using the labels from SNIC or SLIC, the mean colors can be found of each
//! segment and output as an RGB image buffer. Contours can also be drawn
//! around those segments.
//!
//! ```
//! # use palette::{FromColor, Lab, Pixel, Srgb};
//! # use simple_clustering::snic;
//! use simple_clustering::image::{mean_colors, segment_contours};
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! # let (width, height) = (1, 3);
//! # let image = [0u8, 0, 0, 127, 127, 127, 255, 255, 255];
//! # let (k, m) = (1, 10);
//! # let lab_buffer: Vec<Lab<_, f64>> = Srgb::from_raw_slice(&image)
//! #    .iter()
//! #    .map(|&c| Lab::from_color(c.into_format()))
//! #    .collect();
//! # let labels = snic(k, m, width, height, &lab_buffer)?;
//! # let mut output_buffer = [0; 9];
//! # let k = 1;
//! let _ = mean_colors(&mut output_buffer, k, &labels, &lab_buffer)?;
//! segment_contours(&mut output_buffer, width, height, &labels, [0; 3])?;
//!
//! # Ok(())
//! # }
//! ```
#![forbid(
    absolute_paths_not_starting_with_crate,
    missing_docs,
    non_ascii_idents,
    noop_method_call,
    unsafe_code,
    unused_results
)]
#![warn(
    clippy::cast_lossless,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss
)]

use num_traits::{Float, One, Unsigned, Zero};
use palette::{white_point::WhitePoint, FloatComponent, Lab};
use std::ops::{Add, Div, Rem};

pub mod error;
pub mod image;
pub mod seed;
mod slic;
mod snic;

pub use slic::{slic, slic_from_bytes};
pub use snic::{snic, snic_from_bytes};

/// Calculate the superpixel side length, `S`.
///
/// `S * S` is the approximate size of each superpixel in pixels. The formula is
/// `S = (N / K).sqrt()`, where `N` is the number of pixels and `K` is the
/// number of desired superpixels.
#[inline]
fn calculate_grid_interval(width: u32, height: u32, superpixels: u32) -> f64 {
    ((f64::from(width) * f64::from(height)) / f64::from(superpixels)).sqrt()
}

/// Calculate the distance between two `Lab` colors.
#[inline]
fn distance_lab<Wp, T>(lhs: Lab<Wp, T>, rhs: Lab<Wp, T>) -> T
where
    Wp: WhitePoint,
    T: FloatComponent,
{
    (rhs.l - lhs.l).powi(2) + (rhs.a - lhs.a).powi(2) + (rhs.b - lhs.b).powi(2)
}

/// Calculate the distance between two two-dimensional points.
#[inline]
fn distance_xy<T: Float>(lhs: (T, T), rhs: (T, T)) -> T {
    (rhs.0 - lhs.0).powi(2) + (rhs.1 - lhs.1).powi(2)
}

/// Calculate the `s` distance.
#[inline]
fn distance_s<T: Float>(m_div_s: T, d_lab: T, d_xy: T) -> T {
    d_lab + m_div_s * d_xy
}

/// Calculate the superpixel scaling factor.
///
/// `m_div_s` is `(m / s).powi(2)`.
#[inline]
fn m_div_s(m: f64, s: f64) -> f64 {
    (m / s).powi(2)
}

/// Calculates the quotient of `lhs` and `rhs`, rounding the result towards
/// positive infinity.
// FIXME: Remove when stable
#[inline]
fn div_ceil<T>(lhs: T, rhs: T) -> T
where
    T: PartialOrd + Copy + Div + Rem + Add + Unsigned + Zero + One,
{
    let d = lhs / rhs;
    let r = lhs % rhs;
    if r > T::zero() && rhs > T::zero() {
        d + T::one()
    } else {
        d
    }
}

/// Checks if the index is in bounds and returns a reference to the data at that
/// point if it exists.
#[inline]
fn get_in_bounds<T>(width: i64, _height: i64, x: i64, y: i64, image: &[T]) -> Option<&T> {
    if (0..width).contains(&x) {
        let i = u64::try_from(y)
            .ok()?
            .checked_mul(u64::try_from(width).ok()?)?
            .checked_add(u64::try_from(x).ok()?)
            .and_then(|i| usize::try_from(i).ok())?;
        image.get(i)
    } else {
        None
    }
}

/// Checks if the index is in bounds and returns a mutable referance to the data
/// at that point if it exists.
#[inline]
fn get_mut_in_bounds<T>(
    width: i64,
    _height: i64,
    x: i64,
    y: i64,
    image: &mut [T],
) -> Option<&mut T> {
    if (0..width).contains(&x) {
        let i = u64::try_from(y)
            .ok()?
            .checked_mul(u64::try_from(width).ok()?)?
            .checked_add(u64::try_from(x).ok()?)
            .and_then(|i| usize::try_from(i).ok())?;
        image.get_mut(i)
    } else {
        None
    }
}

/// Struct containing a superpixel's color, X-coordinate, and Y-coordinate in
/// an image.
#[derive(Debug, Clone, Copy)]
pub struct Superpixel<T> {
    /// Superpixel color.
    pub data: T,
    /// X-position coordinate.
    pub x: u32,
    /// Y-position coordinate.
    pub y: u32,
}

impl<T: Default> Default for Superpixel<T> {
    #[inline]
    fn default() -> Self {
        Self {
            data: Default::default(),
            x: Default::default(),
            y: Default::default(),
        }
    }
}
