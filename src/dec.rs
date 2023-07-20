use byteorder::{BigEndian, ReadBytesExt};
use std::fmt::Display;
use std::io::Read;
use std::num::Wrapping;

use crate::utils::Error;

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
pub struct Pixel {
    r: u8,
    g: u8,
    b: u8,
    a: u8,
}

impl Pixel {
    pub fn new(r: u8, g: u8, b: u8, a: u8) -> Self {
        Pixel { r, g, b, a }
    }

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

pub struct Decoder {
    state: Pixel,
    buffer: [Pixel; 64],
}

impl Decoder {
    pub fn new() -> Self {
        Self {
            state: Pixel::new(0, 0, 0, 255),
            buffer: [Pixel::new(0, 0, 0, 0); 64],
        }
    }

    #[inline]
    fn hash_pixel(p: Pixel) -> u8 {
        let r = Wrapping(p.r);
        let g = Wrapping(p.g);
        let b = Wrapping(p.b);
        let a = Wrapping(p.a);

        let res = r * Wrapping(3) + g * Wrapping(5) + b * Wrapping(7) + a * Wrapping(11);
        res.0
    }

    /// Decodes incoming readable objects with a QOI format into a Vec<Pixel>. This isn't
    /// technically streaming capable, but there might be a way to build an object that implements
    /// Read and is a streaming input. Using a File as the input here is actually technically a
    /// streaming input (the bytes aren't all read from disk at once), but our output is still one
    /// big chunk.
    ///
    /// Assumes to start at the beginning, before the header.
    ///
    /// The decoding code below was heavily based on the reference implementation found at:
    /// https://github.com/phoboslab/qoi
    pub fn decode<T>(&mut self, data: &mut T) -> Result<(Header, Vec<Pixel>), anyhow::Error>
    where
        T: Read,
    {
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
        // to fill the buffer and you must have a buffer of correct size.
        //
        // We preallocate buffers for that use here.
        let mut rgba_buf = [0; 4];
        let mut rgb_buf = [0; 3];

        // Every loop is one pixel in the image.
        for pos in 0..num_pixels {
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
                                self.state.r += dr - 2;
                                self.state.g += dg - 2;
                                self.state.b += db - 2;
                            }
                            ops::QOI_OP_LUMA => {
                                // Grab the green difference (6-bits).
                                let dg = (buf[0] & 0x3f) - 32;

                                // Read in the second byte of data.
                                data.read_exact(&mut buf)?;

                                // Grab the dr - dg and db - dg values (4-bits).
                                let dr_dg = (buf[0] >> 4) & 0x0f;
                                let db_dg = buf[0] & 0x0f;

                                // Set each pixel value from the differences.
                                self.state.r += dg - 8 + dr_dg;
                                self.state.g += dg;
                                self.state.b += dg - 8 + db_dg;
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
            img[pos] = self.state;
        }

        Ok((header, img))
    }
}