use byteorder::{BigEndian, ReadBytesExt};
use std::fmt::Display;
use std::io::Read;
use std::num::Wrapping;

use crate::utils::Error;

/// The number of channels in the image. This is specified in the header.
///
/// This does not necessarily mean anything for the content of the image.
#[repr(u8)]
#[derive(Debug, PartialEq, Eq)]
pub enum Channels {
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

/// The colorspace in use by the pixels in the image. This is specified in the header.
///
/// This does not necessarily mean anything for the content of the image.
#[repr(u8)]
#[derive(Debug, PartialEq, Eq)]
pub enum Colorspace {
    #[allow(non_camel_case_types)]
    sRGB = 0,
    Linear = 1,
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

/// The header that appears as the first 14 bytes of a QOI image.
///
/// This should always be read first before reading any of the rest of the file.
#[derive(Debug, PartialEq, Eq)]
pub struct Header {
    pub magic: [u8; 4], // reads to 'qoif'
    pub width: u32,
    pub height: u32,
    pub channels: Channels,
    pub colorspace: Colorspace,
}

impl Header {
    fn from_bytes(data: &[u8; 14]) -> Result<Self, anyhow::Error> {
        let mut data = std::io::Cursor::new(data);

        let mut magic = [0; 4];
        data.read_exact(&mut magic)?;

        if magic != [b'q', b'o', b'i', b'f'] {
            return Err(Error::HeaderParseError(format!(
                "Magic bytes did not translate to qoif: {:?}",
                magic
            )))?;
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

/// Submodule containing constants representing the ops available in the QOI format. This isn't an
/// enum due to a limitation in the lanaguage that makes going from Enum -> u8 in a match statement
/// (i.e., in a pattern clause) not possible. The work arounds are annoying so, this is the most
/// clean way of implementing it.
pub(crate) mod ops {
    pub const QOI_OP_RGB: u8 = 0b1111_1110;
    pub const QOI_OP_RGBA: u8 = 0b1111_1111;
    pub const QOI_OP_INDEX: u8 = 0b0000_0000;
    pub const QOI_OP_DIFF: u8 = 0b0100_0000;
    pub const QOI_OP_LUMA: u8 = 0b1000_0000;
    pub const QOI_OP_RUN: u8 = 0b1100_0000;
}

/// A pixel with RGBA values.
///
/// TODO: This only allows for RGBA pixels. RGB should be exposed somehow.
#[derive(Copy, Clone, Debug)]
pub struct Pixel {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Pixel {
    pub fn new(r: u8, g: u8, b: u8, a: u8) -> Self {
        Pixel { r, g, b, a }
    }

    #[allow(dead_code)]
    pub fn to_bytes(self) -> [u8; 4] {
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

/// This default impl is NOT for the default state of a QOI decoder. It is for a default value for
/// pixels, which is all 0s.
impl Default for Pixel {
    fn default() -> Self {
        Pixel::new(0, 0, 0, 0)
    }
}

/// A decoder for QOI images.
///
/// This is a fairly lightweight object right now. It only contains the decoder state (last pixel
/// seen/written) and the buffer containing past pixel values at a hashed position. The main
/// decoding function is [decode](crate::dec::Decoder::decode).
///
/// See [StreamDecoder](crate::stream::StreamDecoder) for the streaming implementation.
pub struct Decoder {
    state: Pixel,
    buffer: [Pixel; 64],
}

impl Default for Decoder {
    fn default() -> Self {
        Self::new()
    }
}

impl Decoder {
    /// Creates a new Decoder with its default state, ready for parsing.
    pub fn new() -> Self {
        Self {
            state: Pixel::new(0, 0, 0, 255),
            buffer: [Pixel::new(0, 0, 0, 0); 64],
        }
    }

    /// Resets a Decoder to its default state. This is used before any decoding occurs, ensuring
    /// that we start at the correct state.
    fn reset(&mut self) {
        self.state = Pixel::new(0, 0, 0, 255);
        self.buffer = [Pixel::default(); 64]
    }

    /// Hashes a pixel given the format from the documentation.
    #[inline]
    pub(crate) fn hash_pixel(p: Pixel) -> u8 {
        let r = Wrapping(p.r);
        let g = Wrapping(p.g);
        let b = Wrapping(p.b);
        let a = Wrapping(p.a);

        let res = r * Wrapping(3) + g * Wrapping(5) + b * Wrapping(7) + a * Wrapping(11);
        res.0
    }

    /// Decodes incoming readable objects with a QOI format into a Vec<Pixel>. This assumes that
    /// the `impl Read` object starts at the very first byte, before the header.
    ///
    /// This is not streaming output capable. The image is saved as a Vec<Pixel> as it is being
    /// decoded. This means that the total size of the image, uncompressed, is stored in memory
    /// while decoding. If the uncompressed file is larger than memory, this function will either
    /// cause memory errors or begin forcing the host OS to page to disk.
    ///
    /// The decoding code below was heavily based on the reference implementation found at:
    /// https://github.com/phoboslab/qoi
    ///
    /// TODO: This only works with RGBA pixels, when it should work with RGB as well.
    pub fn decode(&mut self, data: &mut impl Read) -> Result<(Header, Vec<Pixel>), anyhow::Error>
    {
        // Reset the decoder's state, just in case this object is used more than once.
        self.reset();

        let mut buf = [0u8; 14];
        data.read_exact(&mut buf)?;

        let header = Header::from_bytes(&buf)?;

        let num_pixels = (header.width * header.height) as usize;
        let mut img = vec![Pixel::new(0, 0, 0, 0); num_pixels];

        // Main buffer used for storing data.
        let mut buf = [0u8; 1];
        // let mut op_buf = [0u8; 1];

        let mut run = 0;

        // Read does not guarantee that .read() will return enough bytes to fill the buffer it is
        // given. You must either check that you were given fewer bytes and recall .read(), or use
        // the alternative .read_exact(), which does that for you. Caveat here is that it attempts
        // to fill the buffer and you must have a buffer of the correct size.
        //
        // We preallocate buffers for that use here.
        let mut rgba_buf = [0; 4];
        let mut rgb_buf = [0; 3];

        // Modify every pixel in the image
        for pix in img.iter_mut().take(num_pixels) {
            // Run gets set to some number if QOI_OP_RUN is found. Each loop skips reading more ops
            // and instead just uses the previous pixel state.
            if run > 0 {
                run -= 1;
            } else {
                data.read_exact(&mut buf)?;

                match buf[0] {
                    // 8-bit tags have precedence (RGB & RGBA).
                    ops::QOI_OP_RGB => {
                        // Read the RGB values
                        data.read_exact(&mut rgb_buf)?;

                        // Set the pixel
                        self.state = Pixel::new(rgb_buf[0], rgb_buf[1], rgb_buf[2], self.state.a);
                    }
                    ops::QOI_OP_RGBA => {
                        // Read the RGBA values
                        data.read_exact(&mut rgba_buf)?;

                        // Set the pixel
                        self.state = Pixel::new(rgba_buf[0], rgba_buf[1], rgba_buf[2], rgba_buf[3]);
                    }
                    // 2-bit tags
                    _ => {
                        // Match on only the top two bits.
                        match buf[0] & 0xc0 {
                            ops::QOI_OP_INDEX => {
                                // Grab the pixel at this index
                                self.state = self.buffer[buf[0] as usize];
                            }
                            ops::QOI_OP_DIFF => {
                                // Grab the three differences (r,g,b). Each are 2-bits.
                                let dr = (buf[0] >> 4) & 0x03;
                                let dg = (buf[0] >> 2) & 0x03;
                                let db = buf[0] & 0x03;

                                // Set each pixel value from the differences.
                                // Each is biased by 2 (e.g., 0b00 = -2, 0b11 = 1).
                                self.state.r =
                                    u8::wrapping_add(self.state.r, u8::wrapping_sub(dr, 2));
                                self.state.g =
                                    u8::wrapping_add(self.state.g, u8::wrapping_sub(dg, 2));
                                self.state.b =
                                    u8::wrapping_add(self.state.b, u8::wrapping_sub(db, 2));
                            }
                            ops::QOI_OP_LUMA => {
                                // Grab the green difference (6-bits).
                                let dg = u8::wrapping_sub(buf[0] & 0x3f, 32);

                                // Read in the second byte of data.
                                data.read_exact(&mut buf)?;

                                // Grab the dr - dg and db - dg values (4-bits).
                                let dr_dg = (buf[0] >> 4) & 0x0f;
                                let db_dg = buf[0] & 0x0f;

                                let mid = u8::wrapping_sub(dg, 8);
                                // Set each pixel value from the differences.
                                self.state.r =
                                    u8::wrapping_add(self.state.r, u8::wrapping_add(mid, dr_dg));
                                self.state.g = u8::wrapping_add(self.state.g, dg);
                                self.state.b =
                                    u8::wrapping_add(self.state.b, u8::wrapping_add(mid, db_dg));
                            }
                            ops::QOI_OP_RUN => {
                                // Grab the number of pixels in the run.
                                run = buf[0] & 0x3f;
                            }
                            _ => {
                                Err(Error::DecodingError("Unknown tag!".to_string()))?;
                            }
                        }
                    }
                }
                // Hash the pixel and set it in the global buffer
                let hash = Decoder::hash_pixel(self.state);
                self.buffer[hash as usize % 64] = self.state;
            }
            *pix = self.state;
        }

        Ok((header, img))
    }
}

#[cfg(test)]
mod tests {
    use crate::dec::Decoder;
    use crate::dec::{Channels, Colorspace, Header};
    use image::io::Reader as ImageReader;
    use std::fs::File;
    use std::path::PathBuf;

    #[test]
    fn test_decoder() {

        // Using image's QOI reader as a known-good reader. We should parse to the same bytes.
        let img_qoi_img = ImageReader::open("tests/dice.qoi")
            .unwrap()
            .decode()
            .unwrap();
        let img_qoi_img = img_qoi_img.into_bytes();

        let mut qoi_file = File::open(PathBuf::from("tests/dice.qoi")).unwrap();
        let (_, qoi_img) = Decoder::new().decode(&mut qoi_file).unwrap();
        let qoi_img: Vec<u8> = qoi_img.into_iter().flat_map(|a| a.to_bytes()).collect();

        // Not doing an assert_eq on qoi_img and img_qoi_img because it blows up the terminal log.
        for (i, (p1, p2)) in img_qoi_img.iter().zip(qoi_img.iter()).enumerate() {
            if p1 != p2 {
                println!("{}", i);
            }
            assert_eq!(p1, p2)
        }
    }

    #[test]
    fn test_header() {
        let width = u32::to_be_bytes(100);
        let height = u32::to_be_bytes(200);

        let data: [u8; 14] = [
            b'q',
            b'o',
            b'i',
            b'f',
            width[0],
            width[1],
            width[2],
            width[3],
            height[0],
            height[1],
            height[2],
            height[3],
            Channels::RGB as u8,
            Colorspace::Linear as u8,
        ];

        let good = Header {
            magic: [b'q', b'o', b'i', b'f'],
            width: 100,
            height: 200,
            channels: Channels::RGB,
            colorspace: Colorspace::Linear,
        };

        assert_eq!(good, Header::from_bytes(&data).unwrap());
    }
}
