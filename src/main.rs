use byteorder::{BigEndian, ReadBytesExt};
use clap::Parser;
use std::fmt::Display;
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::PathBuf;

#[derive(Debug, Clone, thiserror::Error)]
enum Error {
    #[error("Failed to parse header: {0}")]
    HeaderParseError(String),
    #[error("Failed to decode: {0}")]
    DecodingError(String),
}

#[derive(Debug, Parser)]
struct Args {
    file: PathBuf,
}

#[repr(u8)]
#[derive(Debug)]
enum Channels {
    RGB = 3,
    RGBA = 4,
}

impl TryFrom<u8> for Channels {
    type Error = Error;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            3 => Ok(Channels::RGB),
            4 => Ok(Channels::RGBA),
            _ => Err(Error::HeaderParseError(format!(
                "Unknown value for channels: {}",
                value
            ))),
        }
    }
}

impl Display for Channels {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let val = match self {
            Channels::RGB => "RGB",
            Channels::RGBA => "RGBA",
        };
        f.write_str(val)
    }
}

#[repr(u8)]
#[derive(Debug)]
enum Colorspace {
    #[allow(non_camel_case_types)]
    sRGB = 3,
    Linear = 4,
}

impl TryFrom<u8> for Colorspace {
    type Error = Error;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Colorspace::sRGB),
            1 => Ok(Colorspace::Linear),
            _ => Err(Error::HeaderParseError(format!(
                "Unknown value for colorspace: {}",
                value
            ))),
        }
    }
}

impl Display for Colorspace {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let val = match self {
            Colorspace::sRGB => "sRGB",
            Colorspace::Linear => "Linear",
        };
        f.write_str(val)
    }
}

#[derive(Debug)]
struct Header {
    magic: [u8; 4], // reads to 'qoif'
    width: u32,
    height: u32,
    channels: Channels,
    colorspace: Colorspace,
}

impl Header {
    fn from_bytes(data: &[u8; 14]) -> Result<Self, anyhow::Error> {
        let mut data = std::io::Cursor::new(data);

        let mut magic = [0; 4];
        for i in 0..4 {
            magic[i] = data.read_u8()?;
        }

        let width = data.read_u32::<BigEndian>()?;
        let height = data.read_u32::<BigEndian>()?;

        let channels = data.read_u8()?;
        let colorspace = data.read_u8()?;

        Ok(Header {
            magic,
            width,
            height,
            channels: channels.try_into()?,
            colorspace: colorspace.try_into()?,
        })
    }
}

impl Display for Header {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&format!(
            "Magic: {} ({:?})\nWidth: {}, Height: {}\nChannels: {}, Colorspace: {}",
            std::str::from_utf8(&self.magic).map_err(|_| std::fmt::Error)?,
            self.magic,
            self.width,
            self.height,
            self.channels,
            self.colorspace
        ))
    }
}

mod ops {
    pub const QOI_OP_RGB: u8 = 0b1111_1110;
    pub const QOI_OP_RGBA: u8 = 0b1111_1111;
    pub const QOI_OP_INDEX: u8 = 0b0000_0000;
    pub const QOI_OP_DIFF: u8 = 0b0100_0000;
    pub const QOI_OP_LUMA: u8 = 0b1000_0000;
    pub const QOI_OP_RUN: u8 = 0b1100_0000;
}

#[derive(Copy, Clone, Debug)]
struct Pixel {
    r: u8,
    g: u8,
    b: u8,
    a: u8,
}

impl Pixel {
    fn new(r: u8, g: u8, b: u8, a: u8) -> Self {
        Pixel { r, g, b, a }
    }

    fn from_bytes(b: &[u8; 4]) -> Self {
        Pixel {
            r: b[0],
            g: b[1],
            b: b[2],
            a: b[3],
        }
    }

    fn to_bytes(self) -> [u8; 4] {
        [self.r, self.g, self.b, self.a]
    }
}

impl Display for Pixel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&format!(
            "r:{}, g:{}, b:{}, a:{}",
            self.r, self.g, self.b, self.a
        ))
    }
}

struct Decoder {
    state: Pixel,
    buffer: [Pixel; 64],
}

impl Decoder {
    fn new() -> Self {
        Self {
            state: Pixel::new(0, 0, 0, 255),
            buffer: [Pixel::new(0, 0, 0, 255); 64],
        }
    }
    /// Assumes to start at the beginning, before the header.
    fn decode<T>(&mut self, data: &mut T) -> Result<(Header, Vec<Pixel>), anyhow::Error>
    where
        T: Read + std::io::Seek,
    {
        let mut buf = [0u8; 14];
        data.read(&mut buf)?;

        let header = Header::from_bytes(&buf)?;

        let num_pixels = (header.width * header.height) as usize;
        let mut img = vec![Pixel::new(0, 0, 0, 0); num_pixels];

        // Main buffer used for storing data.
        let mut buf = [0u8; 4];
        // let mut op_buf = [0u8; 1];

        let mut pos = 0; // Header is 14 bytes
        while pos < num_pixels {
            data.take(1).read(&mut buf)?;

            match buf[0] {
                // 8-bit tags have precedence (RGB & RGBA).
                ops::QOI_OP_RGB => {
                    // Read the RGB values
                    data.take(3).read(&mut buf)?;

                    // Set the alpha channel to the last pixel
                    buf[3] = self.state.a;

                    // Set the pixel
                    let pix = Pixel::from_bytes(&buf);
                    img[pos] = pix;

                    // Set the last pixel state
                    self.state = pix;

                    // Hash the pixel and set it in the global buffer
                    let hash = Decoder::hash_pixel(pix);
                    self.buffer[hash as usize] = pix;
                    pos += 1;
                }
                ops::QOI_OP_RGBA => {
                    // Read the RGBA values
                    data.take(4).read(&mut buf)?;

                    // Set the pixel
                    let pix = Pixel::from_bytes(&buf);
                    img[pos] = pix;

                    // Set the last pixel state
                    self.state = pix;

                    // Hash the pixel and set it in the global buffer
                    let hash = Decoder::hash_pixel(pix);
                    self.buffer[hash as usize] = pix;
                    pos += 1;
                }
                // 2-bit tags
                _ => {
                    // Match on only the top two bits.
                    match buf[0] & 0b1100_0000 {
                        ops::QOI_OP_INDEX => {
                            // Grab the index
                            let idx = buf[0] | 0b0011_1111;

                            // Grab the pixel at this index
                            let pix = self.buffer[idx as usize];

                            // Set the pixel
                            img[pos] = pix;

                            // Set the last pixel state
                            self.state = pix;
                            pos += 1;
                        }
                        ops::QOI_OP_DIFF => {
                            // Grab the three differences (r,g,b). Each are 2-bits.
                            let dr = (buf[0] >> 4) & 0x03;
                            let dg = (buf[0] >> 2) & 0x03;
                            let db = buf[0] & 0x03;

                            // Clone the last pixel state.
                            let mut pix = self.state.clone();

                            // Set each pixel value from the differences.
                            // Each is biased by 2 (e.g., 0b00 = -2, 0b11 = 1).
                            pix.r += dr - 2;
                            pix.g += dg - 2;
                            pix.b += db - 2;

                            // Set the pixel
                            img[pos] = pix;

                            // Set the last pixel state
                            self.state = pix;
                            pos += 1;
                        }
                        ops::QOI_OP_LUMA => {
                            // Grab the green difference (6-bits).
                            let dg = u8::wrapping_sub(buf[0] & 0b0011_1111, 32);

                            // Read in the second byte of data.
                            data.take(1).read(&mut buf)?;

                            // Grab the dr - dg and db - dg values (4-bits).
                            let dr_dg = (buf[0] & 0b1111_0000) >> 4;
                            let db_dg = buf[0] & 0b0000_1111;

                            // Clone the last pixel state.
                            let mut pix = self.state.clone();

                            // Set each pixel value from the differences.
                            pix.r += dg - 8 + dr_dg;
                            pix.g += dg;
                            pix.b += dg - 8 + db_dg;

                            // Set the pixel
                            img[pos] = pix;

                            // Set the last pixel state
                            self.state = pix;
                            pos += 1;
                        }
                        ops::QOI_OP_RUN => {
                            // Grab the number of pixels in the run.
                            let run_len = (buf[0] & 0b0011_1111) as usize;

                            // Grab the last pixel.
                            let pix = self.state;

                            // Duplicate the pixel through the image.
                            for j in 0..run_len {
                                img[(pos + j) as usize] = pix;
                            }
                            pos += run_len;
                        }
                        _ => {
                            Err(Error::DecodingError("Unknown tag!".to_string()))?;
                        }
                    }
                }
            }
        }

        Ok((header, img))
    }

    fn hash_pixel(p: Pixel) -> u8 {
        (p.r * 3 + p.g * 5 + p.b * 7 + p.a * 11) % 64
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    let mut file = BufReader::new(File::open(args.file)?);

    let mut dec = Decoder::new();

    let (header, img) = dec.decode(&mut file)?;

    let img_len = img.len();
    println!("{}", img.len());
    println!("{:#?}", &img[(img_len / 2)..((img_len / 2) + 100)]);

    Ok(())
}

mod tests {
    use std::io::Cursor;

    #[test]
    fn test_save() {
        use std::fs::File;
        use std::io::BufReader;

        use crate::Decoder;

        let mut file = BufReader::new(File::open("tests/dice.qoi").unwrap());

        let img_p = image::load(&mut file, image::ImageFormat::Qoi).unwrap();

        let mut dec = Decoder::new();
        let (header, img) = dec.decode(&mut file).unwrap();

        let img = image::io::Reader::new(Cursor::new(
            img.into_iter()
                .map(|a| a.to_bytes()[..])
                .collect::<Vec<u8>>(),
        ));
    }
}
