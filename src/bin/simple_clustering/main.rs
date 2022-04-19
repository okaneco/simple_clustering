mod args;
mod utils;

use crate::args::Opt;
use crate::utils::{generate_filename, save_image, Algorithm};

use clap::Parser;

use palette::{FromColor, Lab, Pixel, Srgb};
use simple_clustering::image::{count_colors, mean_colors, segment_contours};
use std::fmt::Write;
use std::str::FromStr;

fn main() {
    if let Err(e) = try_main() {
        eprintln!("simple_clustering: {}", e);
        std::process::exit(1);
    }
}

fn try_main() -> Result<(), Box<dyn std::error::Error>> {
    let opt = Opt::parse();

    let output_image = if let Some(output) = opt.output {
        output
    } else {
        generate_filename(&opt)?.into()
    };

    let input_image = image::open(opt.input)?.into_rgb8();
    let (width, height) = input_image.dimensions();
    let input_buffer = Srgb::from_raw_slice(input_image.as_raw());
    let mut input_lab: Vec<Lab<_, f64>> = Vec::new();
    input_lab.try_reserve_exact(input_buffer.len())?;
    input_lab.extend(
        input_buffer
            .iter()
            .map(|&c| Lab::from_color(c.into_format())),
    );

    let mut display_string = String::new();
    let mut output_buffer = Vec::new();
    output_buffer.try_reserve_exact(input_image.as_raw().len())?;
    output_buffer.extend((0..input_image.as_raw().len()).map(|_| 0));

    if opt.benchmark {
        let t0 = std::time::Instant::now();
        let _ = simple_clustering::slic(opt.k, opt.m, width, height, Some(opt.iter), &input_lab)?;
        writeln!(&mut display_string, "SLIC: {:?}", t0.elapsed())?;

        let t0 = std::time::Instant::now();
        let _ = simple_clustering::snic(opt.k, opt.m, width, height, &input_lab)?;
        writeln!(&mut display_string, "SNIC: {:?}", t0.elapsed())?;

        print!("{display_string}");
        return Ok(());
    }

    let labels = match opt.algorithm {
        Algorithm::Snic => {
            let t0 = std::time::Instant::now();
            let labels = simple_clustering::snic(opt.k, opt.m, width, height, &input_lab)?;
            let t1 = t0.elapsed();
            if opt.verbose {
                write!(&mut display_string, "SNIC: {:?}", t1)?;
            }
            labels
        }
        Algorithm::Slic => {
            let t0 = std::time::Instant::now();
            let labels =
                simple_clustering::slic(opt.k, opt.m, width, height, Some(opt.iter), &input_lab)?;
            let t1 = t0.elapsed();
            if opt.verbose {
                write!(&mut display_string, "SLIC: {:?}", t1)?;
            }
            labels
        }
    };

    let segment_color = *Srgb::from_str(opt.segment_color.as_str())
        .or(Err("Segment color is invalid hex"))?
        .as_raw();

    if !opt.no_mean {
        let num_segments = mean_colors(
            &mut output_buffer,
            usize::try_from(opt.k)?,
            &labels,
            &input_lab,
        )?;

        // Draw segment contours over mean image
        if opt.segments {
            segment_contours(&mut output_buffer, width, height, &labels, segment_color)?;
        }

        save_image(output_image.as_ref(), &output_buffer, width, height)?;

        if opt.verbose {
            write!(&mut display_string, ", {num_segments} segments")?;
        }
    } else {
        // Save segmented original image
        if opt.segments {
            output_buffer.copy_from_slice(&input_image);
            segment_contours(&mut output_buffer, width, height, &labels, segment_color)?;
            save_image(output_image.as_ref(), &output_buffer, width, height)?;
        }

        // Otherwise, count individual labels for verbose output
        if opt.verbose {
            write!(&mut display_string, ", {} segments", count_colors(&labels))?;
        }
    }

    if opt.verbose {
        println!("{display_string}");
    }

    Ok(())
}
