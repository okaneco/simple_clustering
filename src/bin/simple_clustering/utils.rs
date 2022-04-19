use image::codecs::png::{CompressionType, FilterType, PngEncoder};
use image::{codecs::jpeg::JpegEncoder, ColorType, ImageEncoder};

#[derive(Debug)]
pub enum Algorithm {
    Snic,
    Slic,
}

impl std::str::FromStr for Algorithm {
    type Err = simple_clustering::error::ScError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            s if s.eq_ignore_ascii_case("snic") => Ok(Self::Snic),
            s if s.eq_ignore_ascii_case("slic") => Ok(Self::Slic),
            _ => Err(Self::Err::General("Invalid algorithm")),
        }
    }
}

impl std::fmt::Display for Algorithm {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Snic => write!(f, "snic"),
            Self::Slic => write!(f, "slic"),
        }
    }
}

// Create a file name displaying the algorithm, `k`, and `m` values used.
pub fn generate_filename(opt: &crate::args::Opt) -> Result<String, Box<dyn std::error::Error>> {
    let mut filename = opt
        .input
        .file_stem()
        .ok_or("No file stem")?
        .to_str()
        .ok_or("Could not convert file stem to string")?
        .to_string();

    let format =
        if opt.format.eq_ignore_ascii_case("jpg") || opt.format.eq_ignore_ascii_case("jpeg") {
            "jpg"
        } else {
            opt.format.as_str()
        };

    use std::fmt::Write;
    write!(
        &mut filename,
        "-{algo}-{k}-{m:02}",
        algo = opt.algorithm,
        k = opt.k,
        m = opt.m
    )?;

    if opt.no_mean {
        write!(&mut filename, "-orig")?;
    } else {
        write!(&mut filename, "-mean")?;
    }

    if opt.segments {
        write!(&mut filename, "-segments")?;
    }
    write!(&mut filename, ".{format}")?;

    Ok(filename)
}

// Saves image buffer to file.
pub fn save_image(
    output: &std::path::Path,
    imgbuf: &[u8],
    width: u32,
    height: u32,
) -> Result<(), Box<dyn std::error::Error>> {
    let w = std::io::BufWriter::new(std::fs::File::create(output)?);

    // Save as jpg if it matches the extension
    if let Some(ext) = output.extension() {
        if ext.eq_ignore_ascii_case("jpg") || ext.eq_ignore_ascii_case("jpeg") {
            let mut encoder = JpegEncoder::new_with_quality(w, 90);

            if let Err(err) = encoder.encode(imgbuf, width, height, ColorType::Rgb8) {
                eprintln!("simple_clustering: {}", err);
                std::fs::remove_file(output)?;
            }

            return Ok(());
        }
    }

    // Sub filter seemed to result in better filesize compared to Adaptive
    let encoder = PngEncoder::new_with_quality(w, CompressionType::Best, FilterType::Sub);

    // Clean up if file is created but there's a problem writing to it
    if let Err(err) = encoder.write_image(imgbuf, width, height, ColorType::Rgb8) {
        eprintln!("simple_clustering: {}", err);
        std::fs::remove_file(output)?;
    }

    Ok(())
}
