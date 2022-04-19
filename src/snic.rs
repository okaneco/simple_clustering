use std::cmp::Reverse;

use crate::error::ScError;
use crate::seed::{init_seeds, perturb};
use crate::{
    calculate_grid_interval, distance_lab, distance_s, distance_xy, get_in_bounds,
    get_mut_in_bounds, m_div_s,
};

use num_traits::ToPrimitive;
use palette::{white_point::D65, FromColor, Lab, Pixel, Srgb};

/// Struct used for accumulating and calculating superpixel clusters in SNIC.
#[derive(Debug, Clone, Copy)]
struct SnicUpdate<T> {
    /// Accumulated color data, divide by count for the mean color.
    pub accum: T,
    /// X-coordinate.
    pub x: f64,
    /// Y-coordinate.
    pub y: f64,
    /// Total elements in the cluster.
    pub count: f64,
}

impl<T: Default> SnicUpdate<T> {
    /// Create a [`SnicUpdate`].
    #[inline]
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

impl<T: Default> Default for SnicUpdate<T> {
    #[inline]
    fn default() -> Self {
        Self {
            accum: Default::default(),
            x: Default::default(),
            y: Default::default(),
            count: Default::default(),
        }
    }
}

/// Calculate SNIC by providing a buffer of RGB component bytes as `&[u8]`.
///
/// `k` must not be `0`.
/// `m` is clamped to be between `1` and `20`.
/// `width` and `height` must not be `0`.
pub fn snic_from_bytes(
    k: u32,
    m: u8,
    width: u32,
    height: u32,
    image: &[u8],
) -> Result<Vec<usize>, ScError> {
    if usize::try_from(u64::from(width) * u64::from(height))
        .or(Err("Invalid image dimensions in SNIC from bytes"))?
        != image.len() / 3
    {
        return Err(ScError::MismatchedSnicBuffer);
    }
    let input_buffer = Srgb::from_raw_slice(image);
    let mut input_lab: Vec<Lab<_, f64>> = Vec::new();
    input_lab.try_reserve_exact(input_buffer.len())?;
    input_lab.extend(
        input_buffer
            .iter()
            .map(|&c| Lab::from_color(c.into_format())),
    );

    snic(k, m, width, height, &input_lab)
}

/// Calculate SNIC.
///
/// `k` must not be `0`.
/// `m` is clamped to be between `1` and `20`.
/// `width` and `height` must not be `0`.
///
/// ## Reference
///
/// *Achanta, R., & Süsstrunk, S. Superpixels and polygons using simple
/// non-iterative clustering. Proceedings of the IEEE Conference on Computer Vision
/// and Pattern Recognition, 2017.*
pub fn snic(
    k: u32,
    m: u8,
    width: u32,
    height: u32,
    image: &[Lab<D65, f64>],
) -> Result<Vec<usize>, ScError> {
    let width_i = i64::from(width);
    let height_i = i64::from(height);
    // Validate input parameters
    let m = m.clamp(1, 20);
    if k == 0 {
        return Err(ScError::ZeroSuperpixelCount);
    }

    if width == 0 || height == 0 {
        return Err(ScError::InvalidImageDimension);
    }

    match u64::from(k).cmp(&(u64::from(width) * u64::from(height))) {
        std::cmp::Ordering::Less => {}
        std::cmp::Ordering::Equal | std::cmp::Ordering::Greater => {
            return Err(ScError::InvalidSuperpixelCount);
        }
    }

    // Calculate S
    let s = calculate_grid_interval(width, height, k)
        .to_u32()
        .ok_or(ScError::InvalidGridInterval)?;

    if s == 0 {
        return Err(ScError::ZeroGridInterval);
    }

    let m_s_term = m_div_s(f64::from(m), f64::from(s));

    // Init seeds and shuffle them to a hopefully non-noisy pixel
    let mut clusters = Vec::new();
    init_seeds(width, height, s, k, image, &mut clusters)?;

    for seed in &mut clusters {
        perturb(seed, i64::from(width), i64::from(height), image)?;
    }

    // Output labels
    let mut labels = Vec::new();
    labels.try_reserve_exact(image.len())?;
    labels.extend((0..image.len()).map(|_| 0_usize));

    // Leave the first entry vacant since label k starts at 1
    let mut updates: Vec<SnicUpdate<Lab<D65, f64>>> = Vec::new();
    updates.try_reserve_exact(clusters.len().saturating_add(1))?;
    updates.extend((0..=clusters.len()).map(|_| SnicUpdate::new()));

    // Reuse element and copy it into pq
    let mut element = SnicElement::default();

    // Min-heap priority queue that will pop the lowest distance to a k_th
    // cluster. Push all current centers onto the queue with 0.0 distance. Label
    // values start at 1.
    let mut pq = std::collections::BinaryHeap::with_capacity(image.len() / 5);
    for (k, &cluster) in clusters.iter().skip(1).enumerate() {
        element.distance = Reverse(NonNanFloat(0.0));
        element.k = k.saturating_add(1);
        element.x = cluster.x;
        element.y = cluster.y;
        pq.push(element);
    }

    // 4 way connectivity for neighboring pixels going clockwise from west
    let neighbors = [(-1, 0), (0, -1), (1, 0), (0, 1)];

    // Cache next element if its distance is less than the pq root to avoid
    // reheaping
    let mut swap_elem = None;

    // Remember that we have to offset down by 1 for indexing k
    while !pq.is_empty() {
        let elem = if let Some(elem) = swap_elem.take().or_else(|| pq.pop()) {
            elem
        } else {
            break;
        };
        if let Some(label) = get_mut_in_bounds(
            width_i,
            height_i,
            i64::from(elem.x),
            i64::from(elem.y),
            &mut labels,
        ) {
            if *label == 0 {
                *label = elem.k;

                // Update C[k_i]
                let update = updates
                    .get_mut(elem.k)
                    .ok_or("Update index out of bounds")?;
                // Subtract 1 to index into cluster since k is 1-indexed
                let cluster = clusters
                    .get_mut(elem.k - 1)
                    .ok_or("Cluster index out of bounds")?;
                update.accum += *get_in_bounds(
                    width_i,
                    height_i,
                    i64::from(elem.x),
                    i64::from(elem.y),
                    image,
                )
                .ok_or("Element color out of bounds")?;
                update.x += f64::from(elem.x);
                update.y += f64::from(elem.y);
                update.count += 1.0;

                cluster.data = update.accum * update.count.recip();
                cluster.x = (update.x * update.count.recip())
                    .to_u32()
                    .ok_or("Invalid x update coordinate")?;
                cluster.y = (update.y * update.count.recip())
                    .to_u32()
                    .ok_or("Invalid y update coordinate")?;

                // Pushpop array to possibly skip a heap balancing operation
                let mut arr_neighbors = [None; 4];

                for (&neighbor, arr) in neighbors.iter().zip(arr_neighbors.iter_mut()) {
                    let n_x = i64::from(elem.x) + neighbor.0;
                    let n_y = i64::from(elem.y) + neighbor.1;

                    if let (Some(n_label), Some(color)) = (
                        get_in_bounds(width_i, height_i, n_x, n_y, &labels),
                        get_in_bounds(width_i, height_i, n_x, n_y, image),
                    ) {
                        if *n_label == 0 {
                            let distance = distance_s(
                                m_s_term,
                                distance_lab(*color, cluster.data),
                                distance_xy(
                                    (
                                        n_x.to_f64().ok_or("Could not convert x neighbor")?,
                                        n_y.to_f64().ok_or("Could not convert y neighbor")?,
                                    ),
                                    (f64::from(cluster.x), f64::from(cluster.y)),
                                ),
                            );

                            if distance.is_nan() {
                                return Err(ScError::NanDistance);
                            }

                            element.distance = Reverse(NonNanFloat(distance));
                            element.k = elem.k;
                            element.x = u32::try_from(n_x).or(Err("Invalid neighbor x"))?;
                            element.y = u32::try_from(n_y).or(Err("Invalid neighbor y"))?;
                            *arr = Some(element);
                        }
                    }
                }

                // Pushpop: Find the min value and if it's less than the root,
                // assign it to swap_elem. Because we're using cmp::Reverse, the
                // smallest value will be the last element of the array.
                arr_neighbors.sort_unstable();
                if let Some(min) = arr_neighbors[3] {
                    if let Some(mut peek) = pq.peek_mut() {
                        // Swap element is less than root: don't push to heap.
                        // (.distance field is wrapped in cmp::Reverse)
                        if min.distance > peek.distance {
                            swap_elem = Some(min);
                        } else {
                            // Root is less than element: swap and heapify
                            swap_elem = Some(*peek);
                            *peek = min;
                        }
                    } else {
                        swap_elem = arr_neighbors[3];
                    }
                    pq.extend(arr_neighbors[..3].iter().flatten());
                }
            }
        }
    }

    enforce_connectivity(width_i, height_i, &mut labels);

    Ok(labels)
}

// Enforce connectivity if algorithm fails to do so, iterate in WNES order.
// BSDS300-images\BSDS300\images\test\295087.jpg (desert rocks with tree)
// showed some stray white pixels at k=1000, m=10.
fn enforce_connectivity(width: i64, height: i64, labels: &mut [usize]) {
    for y in 0..height {
        for x in 0..width {
            if let Some(first) = get_in_bounds(width, height, x, y, labels) {
                let neighbors = [
                    get_in_bounds(width, height, x - 1, y, labels).copied(),
                    get_in_bounds(width, height, x, y - 1, labels).copied(),
                    get_in_bounds(width, height, x + 1, y, labels).copied(),
                    get_in_bounds(width, height, x, y + 1, labels).copied(),
                ];
                if !neighbors.iter().any(|&n| n == Some(*first)) {
                    for &n in neighbors.iter().flatten() {
                        // We know this pixel is inbounds from `if let`
                        *get_mut_in_bounds(width, height, x, y, labels).unwrap() = n;
                    }
                }
            }
        }
    }
}

/// Queue element used in SNIC computation.
#[derive(Debug, Default, PartialEq, Clone, Copy)]
struct SnicElement {
    /// Distance to nearest superpixel.
    pub distance: Reverse<NonNanFloat>,
    /// Superpixel label.
    pub k: usize,
    /// X-coordinate.
    pub x: u32,
    /// Y-coordinate.
    pub y: u32,
}

impl Eq for SnicElement {}

impl PartialOrd for SnicElement {
    #[inline]
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.distance.partial_cmp(&other.distance)
    }
}

impl Ord for SnicElement {
    #[inline]
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.partial_cmp(other).unwrap()
    }
}

/// Floating point number used for distance in SNIC as the min-heap key. This
/// number must not be a `NaN`.
#[derive(Debug, Default, PartialEq, Clone, Copy)]
struct NonNanFloat(f64);

impl Eq for NonNanFloat {}

impl PartialOrd for NonNanFloat {
    #[inline]
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.0.partial_cmp(&other.0)
    }
}

impl Ord for NonNanFloat {
    #[inline]
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.partial_cmp(other).unwrap()
    }
}
