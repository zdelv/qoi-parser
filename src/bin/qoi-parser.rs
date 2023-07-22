use std::fs::File;
use std::io::BufReader;

use clap::Parser;

use qoiparser::{Args, Decoder};

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
    #[test]
    fn test_save_stream_decoder() {
        use image::codecs::png::PngEncoder;
        use image::ImageEncoder;
        use qoiparser::{Pixel, StreamDecoder, StreamDecoderOutput};
        use std::fs::File;
        use std::io::{BufReader, Read};

        let file = BufReader::new(File::open("tests/dice.qoi").unwrap());
        let mut iter = file.bytes();

        let mut sdec = StreamDecoder::new();

        let mut width: u32 = 0;
        let mut height: u32 = 0;

        let mut img_size: u64 = 0;
        let mut img: Vec<Pixel> = Vec::new();

        while let Some(b) = iter.next() {
            match b {
                Ok(byte) => {
                    match sdec.feed(byte).unwrap() {
                        // The StreamDecoder informs us if it needs more bytes after recieving one
                        // byte. This allows us to work on just getting those bytes and checking
                        // the state again later.
                        StreamDecoderOutput::NeedMore(_) => {}

                        // After recieving the image size, we can reserve space for the image
                        // buffer.
                        StreamDecoderOutput::ImageWidthParsed(w) => {
                            width = w;
                            img_size = w as u64;
                        }
                        StreamDecoderOutput::ImageHeightParsed(h) => {
                            height = h;
                            img_size *= h as u64;
                            img.reserve_exact(img_size as usize);
                        }

                        // When pixels are ready to be produced, the StreamDecoder returns an
                        // iterator that produces those pixels. This is a lightweight iterator,
                        // with just a Pixel and u8 count attached (5 bytes in total).
                        StreamDecoderOutput::Pixels(it) => {
                            for pix in it {
                                img.push(pix);
                            }
                        }

                        // The StreamDecoder informs us when it has returned all pixels in the
                        // image.
                        StreamDecoderOutput::Finished => break,
                        _ => {}
                    }
                }
                // If we failed to pull a byte out of the file, then throw an error.
                Err(e) => {
                    println!("{}", e);
                    assert!(false)
                }
            }
        }

        let png_enc = PngEncoder::new(File::create("tests/output_stream.png").unwrap());

        let buf: Vec<u8> = img.into_iter().flat_map(|a| a.to_bytes()).collect();

        png_enc
            .write_image(&buf, width, height, image::ColorType::Rgba8)
            .unwrap();
    }

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
