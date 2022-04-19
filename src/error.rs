//! Superpixel error enums.
use std::collections::TryReserveError;

/// Error for SLIC calculations.
#[derive(Clone, Debug)]
pub enum ScError {
    /// The image `width` and/or `height` is equal to `0`.
    InvalidImageDimension,
    /// The number of superpixels to find is equal to `0`.
    ZeroSuperpixelCount,
    /// The number of superpixels to find is greater than or equal to the number
    /// of pixels in the image.
    InvalidSuperpixelCount,
    /// The calculated grid interval is equal to `0`.
    ZeroGridInterval,
    /// The calculated grid interval is too large.
    InvalidGridInterval,
    /// The SLIC image buffer length does not match the dimensions.
    MismatchedSlicBuffer,
    /// The SNIC image buffer length does not match the dimensions.
    MismatchedSnicBuffer,
    /// A distance calculated during SNIC resulted in a NaN.
    NanDistance,
    /// An error occured while initializing or perturbing superpixel seeds.
    SeedError(SeedErrorKind),
    /// Space could not be reserved for a collection required in superpixel
    /// calculation.
    Reserve(TryReserveError),
    /// A general error occurred.
    General(&'static str),
}

impl std::fmt::Display for ScError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidImageDimension => write!(f, "Image dimension cannot be 0"),
            Self::ZeroSuperpixelCount => write!(f, "Number of superpixels cannot be 0"),
            Self::InvalidSuperpixelCount => write!(
                f,
                "Number of superpixels greater than or equal to pixels in image"
            ),
            Self::ZeroGridInterval => write!(f, "Grid interval cannot be 0"),
            Self::InvalidGridInterval => write!(f, "Grid interval larger than u32"),
            Self::MismatchedSlicBuffer => {
                write!(f, "SLIC buffer length does not equal image dimensions")
            }
            Self::MismatchedSnicBuffer => {
                write!(f, "SNIC buffer length does not equal image dimensions")
            }
            Self::NanDistance => write!(f, "NaN encountered during SNIC"),
            Self::SeedError(e) => write!(f, "{e}"),
            Self::Reserve(e) => write!(f, "{e}"),
            Self::General(e) => write!(f, "{e}"),
        }
    }
}

/// Errors that can occur while initializing the superpixel seeds.
#[derive(Clone, Debug)]
pub enum SeedErrorKind {
    /// Index out of bounds for seed initialization.
    InvalidImageIndex,
    /// The total number of seeds is too large to be stored in a vector.
    InvalidTotalSeeds,
    /// An integer conversion error occurred while perturbing superpixel seeds.
    PerturbConversion,
}

impl std::fmt::Display for SeedErrorKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidImageIndex => {
                write!(f, "Invalid image index for seed initialization")
            }
            Self::InvalidTotalSeeds => write!(f, "Total number of seeds too large"),
            Self::PerturbConversion => write!(f, "Could not convert integer in seed perturbation"),
        }
    }
}

impl std::error::Error for ScError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Reserve(e) => e.source(),
            Self::InvalidImageDimension
            | Self::ZeroSuperpixelCount
            | Self::InvalidSuperpixelCount
            | Self::ZeroGridInterval
            | Self::InvalidGridInterval
            | Self::MismatchedSlicBuffer
            | Self::MismatchedSnicBuffer
            | Self::NanDistance
            | Self::SeedError(_)
            | Self::General(_) => None,
        }
    }
}

impl std::convert::From<TryReserveError> for ScError {
    fn from(error: TryReserveError) -> Self {
        Self::Reserve(error)
    }
}

impl std::convert::From<&'static str> for ScError {
    fn from(error: &'static str) -> Self {
        Self::General(error)
    }
}
