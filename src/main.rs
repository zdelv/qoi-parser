mod dec;
mod utils;

use std::fs::File;
use std::io::BufReader;

use crate::utils::Args;
use crate::dec::Decoder;
use clap::Parser;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    let mut file = BufReader::new(File::open(args.file)?);

    let mut dec = Decoder::new();

    let (_header, img) = dec.decode(&mut file)?;

    // let img_len = img.len();
    println!("{}", img.len());
    // println!("{:#?}", &img[(img_len / 2)..((img_len / 2) + 100)]);

    Ok(())
}

mod tests {
    /// Not really a test, but more of a "input" == "output" where the two must be manually
    /// checked.
    #[test]
    fn test_save() {
        use image::codecs::png::PngEncoder;
        use image::ImageEncoder;
        use std::fs::File;
        use std::io::BufReader;

        use crate::Decoder;

        let mut file = BufReader::new(File::open("tests/qoi_test_images/wikipedia_008.qoi").unwrap());
        // let img_p = image::load(&mut file, image::ImageFormat::Qoi).unwrap();

        let mut dec = Decoder::new();
        let (header, img) = dec.decode(&mut file).unwrap();

        let png_enc = PngEncoder::new(File::create("tests/output.png").unwrap());

        let buf: Vec<u8> = img.into_iter().flat_map(|a| a.to_bytes()).collect();

        png_enc
            .write_image(&buf, header.width, header.height, image::ColorType::Rgba8)
            .unwrap();
    }
}
