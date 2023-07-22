use std::fs::File;
use std::io::{BufReader, Read};
use std::time::Instant;

use clap::Parser;

use qoiparser::{Args, Decoder};
use qoiparser::stream::{StreamDecoderOutput, StreamDecoder};
use qoiparser::Pixel;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    let file = File::open(args.file)?;

    let size = file.metadata()?.len();
    let size = (size as f32) / f32::powi(1000., 2); // MB

    let mut file = BufReader::new(file);

    if args.stream {
        println!("Using stream decoder");
        let mut sdec = StreamDecoder::new();

        let mut img_size: u64 = 0;
        let mut img: Vec<Pixel> = Vec::new();

        let mut buf = [0u8; 1];

        let now = Instant::now();
        while file.read_exact(&mut buf).is_ok() {
            match sdec.feed(buf[0]).unwrap() {
                // The StreamDecoder informs us if it needs more bytes after recieving one
                // byte. This allows us to work on just getting those bytes and checking
                // the state again later.
                StreamDecoderOutput::NeedMore(_) => {}

                // After recieving the image size, we can reserve space for the image
                // buffer.
                StreamDecoderOutput::ImageWidthParsed(w) => {
                    img_size = w as u64;
                }
                StreamDecoderOutput::ImageHeightParsed(h) => {
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
        let dur = Instant::now() - now;
        let dur = (dur.as_micros() as f32) / 1000.;

        println!("File Size: {} MB", size);
        println!("Time: {} ms", dur);
        println!("Throughput: {} MB/sec", size / (dur / 1000.));
        println!("Num pixels: {}", img.len());


    } else {
        println!("Using chunked decoder");
        let mut dec = Decoder::new();

        let now = Instant::now();
        let (_, img) = dec.decode(&mut file)?;

        let dur = Instant::now() - now;
        let dur = (dur.as_micros() as f32) / 1000.;

        println!("File Size: {} MB", size);
        println!("Time: {} ms", dur);
        println!("Throughput: {} MB/sec", size / (dur / 1000.));
        println!("Num pixels: {}", img.len());
    }


    Ok(())
}

mod tests {
    #[test]
    fn test_save_stream_decoder() {
        use image::codecs::png::PngEncoder;
        use image::ImageEncoder;
        use std::fs::File;
        use std::io::{BufReader, Read};

        use qoiparser::stream::{StreamDecoder, StreamDecoderOutput};
        use qoiparser::Pixel;


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

        use qoiparser::Decoder;

        let mut file = BufReader::new(File::open("tests/dice.qoi").unwrap());
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
