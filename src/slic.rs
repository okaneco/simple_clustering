use crate::error::ScError;
use crate::seed::{init_seeds, perturb};
use crate::{
    calculate_grid_interval, distance_lab, distance_s, distance_xy, get_in_bounds,
    get_mut_in_bounds, m_div_s,
};

use num_traits::ToPrimitive;
use palette::{cast, FromColor, Lab, Srgb};

/// Information for tracking image pixels' nearest superpixel cluster and
/// distance to that cluster during SLIC.
#[derive(Debug, Clone)]
struct SlicInfo<T, U> {
    /// Vector of nearest superpixel distances.
    pub distances: Vec<T>,
    /// Vector of nearest superpixel labels.
    pub labels: Vec<U>,
}

impl<T, U> SlicInfo<T, U> {
    /// Create a [`SlicInfo`].
    #[must_use]
    #[inline]
    pub fn new() -> Self {
        Self::default()
    }
}

impl<T, U> Default for SlicInfo<T, U> {
    #[inline]
    fn default() -> Self {
        Self {
            distances: Vec::new(),
            labels: Vec::new(),
        }
    }
}

/// Struct used for accumulating and calculating superpixel clusters in SLIC.
#[derive(Debug, Clone, Copy)]
struct SlicUpdate<T> {
    /// Color data.
    pub data: T,
    /// X-coordinate.
    pub x: f64,
    /// Y-coordinate.
    pub y: f64,
    /// Total elements in the cluster.
    pub count: f64,
}

impl<T: Default> SlicUpdate<T> {
    /// Create a [`SlicUpdate`].
    #[inline]
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

impl<T: Default> Default for SlicUpdate<T> {
    #[inline]
    fn default() -> Self {
        Self {
            data: Default::default(),
            x: Default::default(),
            y: Default::default(),
            count: Default::default(),
        }
    }
}

/// Calculate SLIC by providing a buffer of RGB component bytes as `&[u8]`.
///
/// `iter` will default to `10` if `None` is supplied.
///
/// `k` must not be `0`.
/// `m` is clamped to be between `1` and `20`.
/// `width` and `height` must not be `0`.
pub fn slic_from_bytes(
    k: u32,
    m: u8,
    width: u32,
    height: u32,
    iter: Option<u8>,
    image: &[u8],
) -> Result<Vec<usize>, ScError> {
    if usize::try_from(u64::from(width) * u64::from(height))
        .or(Err("Invalid image dimensions in SLIC from bytes"))?
        != image.len() / 3
    {
        return Err(ScError::MismatchedSlicBuffer);
    }
    let input_buffer = cast::from_component_slice::<Srgb<u8>>(image);
    let mut input_lab: Vec<Lab<_, f64>> = Vec::new();
    input_lab.try_reserve_exact(input_buffer.len())?;
    input_lab.extend(
        input_buffer
            .iter()
            .map(|&c| Lab::from_color(c.into_format())),
    );

    slic(k, m, width, height, iter, &input_lab)
}

/// Calculate SLIC.
///
/// `iter` will default to `10` if `None` is supplied.
///
/// `k` must not be `0`.
/// `m` is clamped to be between `1` and `20`.
/// `width` and `height` must not be `0`.
///
/// ## Reference
///
/// *Achanta, R., Shaji, A., Smith, K., Lucchi, A., Fua, P., & Süsstrunk, S. SLIC
/// Superpixels. EPFL Technical Report no. 149300, June 2010.*
///
/// *Achanta, R., Shaji, A., Smith, K., Lucchi, A., Fua, P., & Süsstrunk, S. SLIC
/// Superpixels Compared to State-of-the-art Superpixel Methods. IEEE Transactions
/// on Pattern Analysis and Machine Intelligence, vol. 34, num. 11, p. 2274 – 2282,
/// May 2012.*
pub fn slic<Wp>(
    k: u32,
    m: u8,
    width: u32,
    height: u32,
    iter: Option<u8>,
    image: &[Lab<Wp, f64>],
) -> Result<Vec<usize>, ScError> {
    // Validate input parameters
    let m = m.clamp(1, 20);
    let iter = iter.unwrap_or(10);
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

    // Bookkeeping for tracking pixel clusters and updating cluster centers
    let mut info = SlicInfo::<f64, usize>::new();
    info.distances.try_reserve_exact(image.len())?;
    info.labels.try_reserve_exact(image.len())?;
    info.distances
        .extend((0..image.len()).map(|_| f64::INFINITY));
    info.labels.extend((0..image.len()).map(|_| 0));

    let mut updates: Vec<SlicUpdate<Lab<Wp, f64>>> = Vec::new();
    updates.try_reserve_exact(clusters.len())?;
    updates.extend((0..clusters.len()).map(|_| SlicUpdate::new()));

    for _ in 0..iter {
        // Search a pixel area of 2S x 2S size and match cluster centers to
        // pixels with the lowest distance measure
        for (center_index, center) in clusters.iter().enumerate() {
            for y in center.y.saturating_sub(s)..center.y.saturating_add(s).min(height) {
                let x_start = center.x.saturating_sub(s);
                let x_end = center.x.saturating_add(s).min(width);
                let row_start = u64::from(y).saturating_mul(u64::from(width));

                // (2023/01)WOULDBENICE: Try chunks_exact, attempted it here but clusters
                // had worse results compared to current version indicating probable errors
                // in implementation
                for (x, idx) in (x_start..x_end).zip(
                    row_start.saturating_add(u64::from(center.x.saturating_sub(s)))
                        ..row_start.saturating_add(u64::from(x_end)),
                ) {
                    let idx = usize::try_from(idx)
                        .or(Err("Index out of bounds for finding new neighbors"))?;
                    if idx < image.len() && idx < info.distances.len() && idx < info.labels.len() {
                        let color = image[idx];
                        let distance = distance_s(
                            m_s_term,
                            distance_lab(color, center.data),
                            distance_xy(
                                (f64::from(x), f64::from(y)),
                                (f64::from(center.x), f64::from(center.y)),
                            ),
                        );

                        if distance < info.distances[idx] {
                            info.distances[idx] = distance;
                            info.labels[idx] = center_index;
                        }
                    }
                }
            }
        }

        // Compute new centers and update
        let width_usize = usize::try_from(width).or(Err("Could not convert width to usize"))?;
        for (y, (row, info_labels)) in image
            .chunks_exact(width_usize)
            .zip(info.labels.chunks_exact(width_usize))
            .enumerate()
        {
            #[allow(clippy::cast_precision_loss)]
            for (x, (&color, &info_label)) in row.iter().zip(info_labels).enumerate() {
                if let Some(update) = updates.get_mut(info_label) {
                    update.data += color;
                    update.x += x as f64;
                    update.y += y as f64;
                    update.count += 1.0;
                }
            }
        }

        for (update, center) in updates.iter_mut().zip(&mut clusters) {
            if update.count == 0.0 {
                continue;
            }
            center.data = update.data / update.count;
            center.x = (update.x / update.count)
                .to_u32()
                .ok_or("Update X out of bounds")?;
            center.y = (update.y / update.count)
                .to_u32()
                .ok_or("Update Y out of bounds")?;
            *update = SlicUpdate::new();
        }
    }

    enforce_connectivity(width, height, s, &mut info.labels)?;

    Ok(info.labels)
}

// Relabel disjoint labels to the largest, nearest neighbor cluster.
fn enforce_connectivity(
    width: u32,
    height: u32,
    s: u32,
    labels: &mut [usize],
) -> Result<(), ScError> {
    let width_i = i64::from(width);
    let height_i = i64::from(height);
    let cluster_threshold =
        usize::try_from(u64::from(s).pow(2) / 4).or(Err("Could not convert cluster threshold"))?;
    let mut new_labels = Vec::new();
    new_labels.try_reserve_exact(labels.len())?;
    new_labels.extend((0..labels.len()).map(|_| usize::MAX));
    let new_labels = new_labels.as_mut_slice();

    // This will be reused for searching each superpixel cluster.
    // For now, the size of the queue is 8 superpixels to start.
    let mut label_queue = Vec::new();
    label_queue.try_reserve(
        usize::try_from(u64::from(s).pow(2).saturating_mul(8))
            .or(Err("Could not calculate label set size"))?,
    )?;

    // Adjacent pixels, clockwise order West-North-East-South
    let neighbors = [(-1, 0), (0, -1), (1, 0), (0, 1)];

    // Assign new labels to pixels by finding connected pixel clusters
    let mut neighbor_label = 0;
    let mut new_label = 0_usize;

    let width_usize = usize::try_from(width).or(Err(
        "Could not convert width to usize in enforce_connectivity",
    ))?;
    for (y, label_row) in labels.chunks_exact(width_usize).enumerate() {
        for (x, &old_label) in label_row.iter().enumerate() {
            let idx_usize = y.saturating_mul(width_usize).saturating_add(x);

            // If no assigned label, assign current_label
            if new_labels.get(idx_usize) == Some(&usize::MAX) {
                *new_labels
                    .get_mut(idx_usize)
                    .ok_or("Label index out of bounds")? = new_label;

                // Find neighbor label that borders current pixel if it exists.
                // Ending on South seems to have best results. This label will
                // be used to label the cluster if the current label is too
                // small.
                for &neighbor in &neighbors {
                    // `x` and `y` went from u32->usize->i64
                    let neighbor_x = (x as i64) + neighbor.0;
                    let neighbor_y = (y as i64) + neighbor.1;
                    if let Some(l) =
                        get_in_bounds(width_i, height_i, neighbor_x, neighbor_y, new_labels)
                    {
                        if *l != usize::MAX {
                            neighbor_label = *l;
                        }
                    }
                }

                // "One component at a time" search for pixels that share the
                // same label. The members go into a queue so they can be
                // reassigned a neighboring label if it's a disjoint cluster.
                label_queue.clear();
                label_queue.push(((x as i64), (y as i64)));
                let mut label_queue_idx = 0;
                let mut label_count = 1_usize;

                while label_queue_idx < label_count {
                    for &neighbor in &neighbors {
                        let entry = label_queue
                            .get(label_queue_idx)
                            .ok_or("Could not get label")?;
                        let new_vx = entry.0 + neighbor.0;
                        let new_vy = entry.1 + neighbor.1;

                        if let (Some(old_visit_label), Some(new_visit_label)) = (
                            get_in_bounds(width_i, height_i, new_vx, new_vy, labels),
                            get_mut_in_bounds(width_i, height_i, new_vx, new_vy, new_labels),
                        ) {
                            // If new label is unassigned and matches old_label, assign it the current cluster
                            if *old_visit_label == old_label && *new_visit_label == usize::MAX {
                                if label_queue.capacity() == label_queue.len() {
                                    label_queue.try_reserve(1)?;
                                }
                                label_queue.push((new_vx, new_vy));
                                *new_visit_label = new_label;
                                label_count = label_count.saturating_add(1);
                            }
                        }
                    }
                    label_queue_idx = label_queue_idx.saturating_add(1);
                }

                // If a label set is smaller than some threshold, relabel that
                // set as the nearest neighboring label. Don't increment label
                // if too small of a set. Currently set to a quarter of a
                // superpixel size.
                if label_count <= cluster_threshold {
                    for &(l_x, l_y) in &label_queue {
                        *get_mut_in_bounds(width_i, height_i, l_x, l_y, new_labels)
                            .ok_or("New label index out of bounds")? = neighbor_label;
                    }
                    continue;
                }
                new_label = new_label.saturating_add(1);
            }
        }
    }

    labels.copy_from_slice(new_labels);

    Ok(())
}
