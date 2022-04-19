use clap::Parser;

#[derive(Debug, Parser)]
#[clap(version, about, long_about = None)]
pub struct Opt {
    /// Input file.
    #[clap(short, long, parse(from_os_str))]
    pub input: std::path::PathBuf,

    /// Output file, defaults to PNG image output.
    #[clap(short, long, parse(from_os_str))]
    pub output: Option<std::path::PathBuf>,

    /// Number of superpixel segments to find.
    #[clap(short, short_alias = 'n', default_value_t = 1_000)]
    pub k: u32,

    /// Compactness of superpixels, `m` parameter. Range of 1 to 20.
    #[clap(short, default_value_t = 10)]
    pub m: u8,

    /// Number of iterations to run (SLIC).
    #[clap(long, default_value_t = 10)]
    pub iter: u8,

    /// Disable saving an output image with mean superpixel segment colors.
    #[clap(long)]
    pub no_mean: bool,

    /// Segmentation algorithm used, such as SNIC or SLIC.
    #[clap(short, long, default_value = "snic")]
    pub algorithm: crate::utils::Algorithm,

    /// Print the number of segments found and time taken.
    #[clap(short, long)]
    pub verbose: bool,

    /// Save as a JPG or PNG file.
    #[clap(long, default_value = "png")]
    pub format: String,

    /// Development flag for testing speeds of calculation.
    #[clap(long, hide = true)]
    pub benchmark: bool,

    /// Save an image with segmented superpixel contours.
    #[clap(long)]
    pub segments: bool,

    /// Specify the hexadecimal RGB color for segment contours.
    #[clap(long, default_value = "000")]
    pub segment_color: String,
}
