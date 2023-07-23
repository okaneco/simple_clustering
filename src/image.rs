//! Functions for interacting with image labels and manipulating images.
use crate::{error::ScError, get_in_bounds};
use fxhash::{FxHashMap, FxHashSet};
use palette::{encoding, rgb::Rgb, white_point::WhitePoint, IntoColor, Lab, Srgb};

/// Count the number of unique labels in a slice of superpixel labels.
pub fn count_colors(labels: &[usize]) -> usize {
    labels.iter().copied().collect::<FxHashSet<usize>>().len()
}

/// Modify `output` to contain an RGB image of superpixel segments filled with
/// the mean color of that region. The return value is the count of superpixels
/// in the image.
pub fn mean_colors<Wp>(
    output: &mut [u8],
    k: usize,
    labels: &[usize],
    image: &[Lab<Wp, f64>],
) -> Result<usize, ScError>
where
    Wp: WhitePoint<f64>,
    Lab<Wp, f64>: IntoColor<Rgb<encoding::Srgb, f64>>,
{
    if Some(output.len()) != image.len().checked_mul(3) {
        return Err(ScError::General(
            "Mean color buffer does not match image length",
        ));
    }

    let mut map = FxHashMap::<usize, (Lab<Wp, f64>, f64)>::default();
    map.try_reserve(k)?;

    for (&idx, &color) in labels.iter().zip(image.iter()) {
        let _ = map
            .entry(idx)
            .and_modify(|e| {
                e.0 += color;
                e.1 += 1.0;
            })
            .or_insert((color, 1.0));
    }

    let mut rgb_map = FxHashMap::<usize, Srgb<u8>>::default();
    rgb_map.try_reserve(map.len())?;

    rgb_map.extend(map.iter().map(|(&key, &(color, count))| {
        let rgb: Srgb<u8> = (color / count).into_color().into_format();
        (key, rgb)
    }));

    output
        .chunks_exact_mut(3)
        .zip(labels.iter().filter_map(|a| rgb_map.get(a)))
        .for_each(|(chunk, color)| chunk.copy_from_slice(color.into()));

    Ok(map.len())
}

/// Modify `output` to contain an RGB image with colored contours based on
/// superpixel labels.
pub fn segment_contours(
    output: &mut [u8],
    width: u32,
    height: u32,
    labels: &[usize],
    segment_color: [u8; 3],
) -> Result<(), ScError> {
    let mut segment = Vec::new();
    segment.try_reserve_exact(labels.len())?;
    segment.extend((0..labels.len()).map(|_| false));

    let width_i = i64::from(width);
    let height_i = i64::from(height);

    let mut chunks_iter = output.chunks_exact_mut(3).enumerate();
    let mut label_iter = labels.iter();
    for y in 0..height_i {
        for x in 0..width_i {
            let label = label_iter.next().ok_or("Labels exhausted")?;
            let (chunk_idx, chunk) = chunks_iter.next().ok_or("Chunks exhausted")?;
            let neighbors = [
                get_in_bounds(width_i, height_i, x - 1, y, labels),
                get_in_bounds(width_i, height_i, x - 1, y - 1, labels),
                get_in_bounds(width_i, height_i, x, y - 1, labels),
                get_in_bounds(width_i, height_i, x + 1, y - 1, labels),
                get_in_bounds(width_i, height_i, x + 1, y, labels),
                get_in_bounds(width_i, height_i, x + 1, y + 1, labels),
                get_in_bounds(width_i, height_i, x, y + 1, labels),
                get_in_bounds(width_i, height_i, x - 1, y + 1, labels),
            ];
            let neighbor_segments = [
                get_in_bounds(width_i, height_i, x - 1, y, &segment),
                get_in_bounds(width_i, height_i, x - 1, y - 1, &segment),
                get_in_bounds(width_i, height_i, x, y - 1, &segment),
                get_in_bounds(width_i, height_i, x + 1, y - 1, &segment),
                get_in_bounds(width_i, height_i, x + 1, y, &segment),
                get_in_bounds(width_i, height_i, x + 1, y + 1, &segment),
                get_in_bounds(width_i, height_i, x, y + 1, &segment),
                get_in_bounds(width_i, height_i, x - 1, y + 1, &segment),
            ];

            // Count neighboring labels that are different from current label
            // and aren't already a border segment
            if neighbors
                .iter()
                .zip(neighbor_segments.iter())
                .filter(|(&n, &ns)| ns == Some(&false) && n != Some(label))
                .count()
                >= 2
            {
                chunk.copy_from_slice(&segment_color);
                if let Some(s) = segment.get_mut(chunk_idx) {
                    *s = true;
                }
            }
        }
    }

    Ok(())
}
