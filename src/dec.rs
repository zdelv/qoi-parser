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
            return Err(Error::HeaderParseError(
                "Magic bytes did not translate to qoif".to_string(),
            ))?;
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
mod ops {
    pub const QOI_OP_RGB: u8 = 0b1111_1110;
    pub const QOI_OP_RGBA: u8 = 0b1111_1111;
    pub const QOI_OP_INDEX: u8 = 0b0000_0000;
    pub const QOI_OP_DIFF: u8 = 0b0100_0000;
    pub const QOI_OP_LUMA: u8 = 0b1000_0000;
    pub const QOI_OP_RUN: u8 = 0b1100_0000;
}

/// A pixel with RGBA values.
/// TODO: This only allows for RGBA pixels. RGB should be exposed somehow.
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
/// seen/written) and the buffer containing past pixel values at a hashed position.
///
/// Later plans include adding streaming decoder support, where the state of this decoder would
/// become much more complicated (most likely some form of a state machine). Streaming in this case
/// would mean byte-by-byte parsing of some form, with minimal memory usage. Users could pass in
/// bytes as they recieve them, and the Decoder would produce bytes as it needs.
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
    ///
    /// TODO: This only works with RGBA pixels, when it should work with RGB as well.
    pub fn decode<T>(&mut self, data: &mut T) -> Result<(Header, Vec<Pixel>), anyhow::Error>
    where
        T: Read,
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

/// The output of the StreamDecoder while decoding.
///
/// TODO: This isn't very memory efficient. Storing all of this in one struct eats up a lot of
/// space. Maybe just have this be a series of flags and output a tuple containing the actual
/// value.
pub enum StreamDecoderOutput {
    Finished,                          // All pixels have been parsed.
    NeedMore(u8),                      // Number of bytes needed. Between 1 and 4.
    Pixels(PixelsIter), // An iterator that retuns the number of pixels ready for paring.
    ImageWidthParsed(u32), // The image width has been read from the header.
    ImageHeightParsed(u32), // The image height has been read from the header.
    ImageChannelParsed(Channels), // The image height has been read from the header.
    ImageColorspaceParsed(Colorspace), // The image height has been read from the header.
}

impl Display for StreamDecoderOutput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use StreamDecoderOutput::*;

        let val = match self {
            Finished => "Finished".to_string(),
            NeedMore(c) => format!("NeedMore: {}", c),
            Pixels(_) => "Pixels".to_string(),
            ImageWidthParsed(w) => format!("ImageWidthParsed: {}", w),
            ImageHeightParsed(h) => format!("ImageHeightParsed: {}", h),
            ImageChannelParsed(c) => format!("ImageChannelParsed: {}", c),
            ImageColorspaceParsed(c) => format!("ImageColorspaceParsed: {}", c),
        };
        f.write_str(&val)
    }
}

/// The internal state of a StreamDecoder.
enum StreamDecoderState {
    NotStarted,        // No bytes have been passed in.
    Finished,          // All bytes in image have been parsed.
    ParsingHeader(u8), // Currently parsing the header. Contains number of bytes currently parsed.
    ParsingOp(u8, i8), // Contains the opcode of the op being parsed and the number of bytes parsed.
}

impl Display for StreamDecoderState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use StreamDecoderState::*;

        let val = match self {
            NotStarted => "NotStarted".to_string(),
            Finished => "Finished".to_string(),
            ParsingHeader(header) => format!("ParsingHeader: {}", header),
            ParsingOp(op, c) => format!("ParsingOp: {}, {}", op, c),
        };
        f.write_str(&val)
    }
}

impl Default for StreamDecoderState {
    fn default() -> Self {
        StreamDecoderState::NotStarted
    }
}

// TODO: Allow for RGB instead of RGBA for 64 bytes of savings. Remove buffer for 4 bytes. Allow for
// 32 bit maximum (through features) to reduce num_pix and cur_pix to u32s (4 byte savings each).
pub struct StreamDecoder {
    // 280 bytes total
    state: StreamDecoderState, // 2 bytes
    last_pixel: Pixel,         // 4 bytes
    dec_buffer: [Pixel; 64],   // 256 bytes
    buffer: [u8; 4],           // 4 bytes
    num_pix: Option<u64>,      // 8 bytes
    cur_pix: u64,              // 8 bytes
}

impl StreamDecoder {
    pub fn new() -> Self {
        StreamDecoder {
            state: StreamDecoderState::default(),
            last_pixel: Pixel::default(),
            dec_buffer: [Pixel::default(); 64],
            buffer: [0; 4],
            num_pix: None,
            cur_pix: 0,
        }
    }

    /// Resets the state of a StreamDecoder. This must be explicitly called after finishing an
    /// image or after an image parse failure.
    ///
    /// We treat the state as
    pub fn reset(&mut self) {
        self.state = StreamDecoderState::NotStarted;
        self.last_pixel = Pixel::new(0, 0, 0, 255);
        self.dec_buffer = [Pixel::default(); 64];
        self.buffer = [0; 4];
        self.num_pix = None;
        self.cur_pix = 0;
    }

    /// Feed is a big state machine that takes in a single byte and uses it's internal state to
    /// properly parse a QOI file.
    pub fn feed(&mut self, byte: u8) -> Result<StreamDecoderOutput, Error> {
        use StreamDecoderOutput as Output;
        use StreamDecoderState as State;

        if let State::NotStarted = self.state {
            self.state = State::ParsingHeader(0);
        }

        // The number of pixels added to the image due to this op.
        let mut count: u8 = 0;

        // Very big state machine below.
        let out: Result<Output, Error> = match self.state {
            State::NotStarted => Err(Error::DecodingError(
                "Not started should not be parsed!".to_string(),
            )),
            State::ParsingHeader(c) => {
                match c {
                    // If we're still parsing the first 4 bytes, check the magic bytes
                    0..=3 => {
                        let res = match c {
                            0 => byte == b'q',
                            1 => byte == b'o',
                            2 => byte == b'i',
                            3 => byte == b'f',
                            _ => false,
                        };

                        if !res {
                            return Err(Error::HeaderParseError(format!(
                                "Failed to parse header: idx={}",
                                c
                            )));
                        }

                        self.state = State::ParsingHeader(c + 1);
                        if c == 3 {
                            // Start parsing the width (4 bytes)
                            Ok(Output::NeedMore(4))
                        } else {
                            Ok(Output::NeedMore(3 - c))
                        }
                    }
                    // Next eight bytes are the width and height (two 32s).
                    4..=11 => {
                        // Byte 7 and 11 are the ends of width and height.
                        if c == 7 || c == 11 {
                            let b0 = self.buffer[0] as u32;
                            let b1 = self.buffer[1] as u32;
                            let b2 = self.buffer[2] as u32;
                            let b3 = byte as u32;

                            let v: u32 = b0 << 24 | b1 << 16 | b2 << 8 | b3;
                            self.state = State::ParsingHeader(c + 1);

                            if c == 7 {
                                self.num_pix = Some(v as u64);
                                Ok(Output::ImageWidthParsed(v))
                            } else {
                                self.num_pix = Some(self.num_pix.unwrap() * v as u64);
                                Ok(Output::ImageHeightParsed(v))
                            }
                        } else {
                            self.buffer[(c % 4) as usize] = byte;

                            self.state = State::ParsingHeader(c + 1);
                            Ok(Output::NeedMore((11 - c) % 4))
                        }
                    }
                    // TODO: Collapse 12 and 13 into one match statement.
                    12 => {
                        let ch = byte.try_into()?;

                        self.state = State::ParsingHeader(c + 1);
                        Ok(Output::ImageChannelParsed(ch))
                    }
                    13 => {
                        let cs = byte.try_into()?;

                        // We finish the header after colorspace
                        self.state = State::ParsingOp(0, -1);
                        Ok(Output::ImageColorspaceParsed(cs))
                    }
                    _ => Err(Error::HeaderParseError(
                        "Invalid index into header.".to_string(),
                    )),
                }
            }
            // Main section where op parsing occurs.
            // This section starts right after we finish parsing the header.
            //
            // ParsingOp contains the previous op (o) and the number of bytes we have parsed for
            // that op so far (c). The sentinel (0, -1) is used to mark that the previous op has
            // finished and the next op is available under byte. Otherwise, how many bytes needed
            // (the maximum of c) varies based on the op at hand.
            State::ParsingOp(o, c) => {
                // Previous op finished or we started parsing ops after header
                let op = if o == 0 && c == -1 { byte } else { o };

                match op {
                    // Requires 4 bytes
                    ops::QOI_OP_RGB => {
                        match c {
                            // We just started parsing this op.
                            // All we have recieved so far is the op code.
                            -1 => {
                                self.state = State::ParsingOp(op, 0);
                                Ok(Output::NeedMore(3))
                            }
                            0 => {
                                self.last_pixel.r = byte;
                                self.state = State::ParsingOp(op, 1);
                                Ok(Output::NeedMore(2))
                            }
                            1 => {
                                self.last_pixel.g = byte;
                                self.state = State::ParsingOp(op, 2);
                                Ok(Output::NeedMore(1))
                            }
                            2 => {
                                self.last_pixel.b = byte;
                                let hash = Decoder::hash_pixel(self.last_pixel);
                                self.dec_buffer[(hash % 64) as usize] = self.last_pixel;

                                count = 1;
                                self.state = State::ParsingOp(0, -1);
                                Ok(Output::Pixels(PixelsIter::new(1, self.last_pixel)))
                            }
                            _ => Err(Error::DecodingError(
                                "RGB parsed too many bytes".to_string(),
                            )),
                        }
                    }
                    // Requires 5 bytes
                    ops::QOI_OP_RGBA => {
                        match c {
                            // We just started parsing this op.
                            // All we have recieved so far is the op code.
                            -1 => {
                                self.state = State::ParsingOp(op, 0);
                                Ok(Output::NeedMore(4))
                            }
                            0 => {
                                self.last_pixel.r = byte;
                                self.state = State::ParsingOp(op, 1);
                                Ok(Output::NeedMore(3))
                            }
                            1 => {
                                self.last_pixel.g = byte;
                                self.state = State::ParsingOp(op, 2);
                                Ok(Output::NeedMore(2))
                            }
                            2 => {
                                self.last_pixel.b = byte;
                                self.state = State::ParsingOp(op, 3);
                                Ok(Output::NeedMore(1))
                            }
                            3 => {
                                self.last_pixel.a = byte;
                                let hash = Decoder::hash_pixel(self.last_pixel);
                                self.dec_buffer[(hash % 64) as usize] = self.last_pixel;

                                count = 1;
                                self.state = State::ParsingOp(0, -1);
                                Ok(Output::Pixels(PixelsIter::new(1, self.last_pixel)))
                            }
                            _ => Err(Error::DecodingError(
                                "RGBA parsed too many bytes".to_string(),
                            )),
                        }
                    }
                    _ => match op & 0xc0 {
                        // Requires 1 bytes
                        ops::QOI_OP_INDEX => {
                            self.last_pixel = self.dec_buffer[op as usize];

                            count = 1;
                            self.state = State::ParsingOp(0, -1);
                            Ok(Output::Pixels(PixelsIter::new(1, self.last_pixel)))
                        }
                        // Requires 1 byte
                        ops::QOI_OP_DIFF => {
                            // Grab the three differences (r,g,b). Each are 2-bits.
                            let dr = (op >> 4) & 0x03;
                            let dg = (op >> 2) & 0x03;
                            let db = op & 0x03;

                            // Set each pixel value from the differences.
                            // Each is biased by 2 (e.g., 0b00 = -2, 0b11 = 1).
                            self.last_pixel.r =
                                u8::wrapping_add(self.last_pixel.r, u8::wrapping_sub(dr, 2));
                            self.last_pixel.g =
                                u8::wrapping_add(self.last_pixel.g, u8::wrapping_sub(dg, 2));
                            self.last_pixel.b =
                                u8::wrapping_add(self.last_pixel.b, u8::wrapping_sub(db, 2));

                            let hash = Decoder::hash_pixel(self.last_pixel);
                            self.dec_buffer[(hash % 64) as usize] = self.last_pixel;

                            count = 1;
                            self.state = State::ParsingOp(0, -1);
                            Ok(Output::Pixels(PixelsIter::new(1, self.last_pixel)))
                        }
                        // Requires 2 bytes
                        // TODO: This might be do-able without the buffer. Do the calculation with
                        // the first byte on last_pixel, then finish it with the second byte.
                        ops::QOI_OP_LUMA => match c {
                            -1 => {
                                self.buffer[0] = u8::wrapping_sub(op & 0x3f, 32);
                                self.state = State::ParsingOp(op, 1);
                                Ok(Output::NeedMore(1))
                            }
                            1 => {
                                let dg = self.buffer[0];
                                let dr_dg = (byte >> 4) & 0x0f;
                                let db_dg = byte & 0x0f;

                                let mid = u8::wrapping_sub(dg, 8);
                                self.last_pixel.r = u8::wrapping_add(
                                    self.last_pixel.r,
                                    u8::wrapping_add(mid, dr_dg),
                                );
                                self.last_pixel.g = u8::wrapping_add(self.last_pixel.g, dg);
                                self.last_pixel.b = u8::wrapping_add(
                                    self.last_pixel.b,
                                    u8::wrapping_add(mid, db_dg),
                                );

                                let hash = Decoder::hash_pixel(self.last_pixel);
                                self.dec_buffer[(hash % 64) as usize] = self.last_pixel;

                                count = 1;
                                self.state = State::ParsingOp(0, -1);
                                Ok(Output::Pixels(PixelsIter::new(1, self.last_pixel)))
                            }
                            _ => Err(Error::DecodingError(
                                "Luma parsed too many bytes".to_string(),
                            )),
                        },
                        // Requires 1 byte
                        ops::QOI_OP_RUN => {
                            // Grab the number of pixels in the run.
                            // Run is biased by one, meaning we add one to the value.
                            let run = (op & 0x3f) + 1;

                            count = run;
                            self.state = State::ParsingOp(0, -1);
                            Ok(Output::Pixels(PixelsIter::new(run, self.last_pixel)))
                        }
                        _ => Err(Error::DecodingError("Invalid op found".to_string())),
                    },
                }
            }
            State::Finished => Ok(Output::Finished),
        };

        self.cur_pix += count as u64;
        //println!("{}", self.cur_pix);
        if self.num_pix.is_some() && self.cur_pix == self.num_pix.unwrap() {
            self.state = State::Finished;
        }

        return out;
    }
}

/// An iterator returned by the StreamDecoder whenever it has some number of pixels extracted. This
/// computes the pixels on the fly using information passed in by the iterator. This is designed to
/// be memory efficient (only the information needed to make a new pixel is stored).
pub struct PixelsIter {
    count: u8,
    pixel: Pixel,
}

impl PixelsIter {
    fn new(count: u8, pixel: Pixel) -> Self {
        PixelsIter { count, pixel }
    }
}

impl Iterator for PixelsIter {
    type Item = Pixel;

    fn next(&mut self) -> Option<Self::Item> {
        if self.count > 0 {
            self.count -= 1;
            Some(self.pixel)
        } else {
            None
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let c = self.count as usize;
        (c, Some(c))
    }
}

mod tests {

    #[test]
    fn test_stream_decoder() {
        use crate::dec::{Pixel, StreamDecoder, StreamDecoderOutput};
        use image::io::Reader as ImageReader;
        use std::fs::File;
        use std::io::{BufReader, Read};
        use std::path::PathBuf;

        let mut sdec = StreamDecoder::new();

        let qoi_file = BufReader::new(File::open(PathBuf::from("tests/dice.qoi")).unwrap());

        let mut iter = qoi_file.bytes();

        let mut img_size: u64 = 0;
        let mut img: Vec<Pixel> = Vec::new();

        while let Some(b) = iter.next() {
            match b {
                Ok(byte) => {
                    match sdec.feed(byte).unwrap() {
                        // The StreamDecoder informs us if it needs more bytes after recieving one
                        // byte. This allows us to work on just getting those bytes and checking
                        // the state again later.
                        StreamDecoderOutput::NeedMore(_) => {
                            // println!("needs more");
                        }

                        // After recieving the image size, we can reserve space for the image
                        // buffer.
                        StreamDecoderOutput::ImageWidthParsed(w) => {
                            println!("width: {}", w);
                            img_size = w as u64;
                        }
                        StreamDecoderOutput::ImageHeightParsed(h) => {
                            println!("height: {}", h);
                            img_size *= h as u64;
                            img.reserve_exact(img_size as usize);
                        }

                        // When pixels are ready to be produced, the StreamDecoder returns an
                        // iterator that produces those pixels. This is a lightweight iterator,
                        // with just a Pixel and u8 count attached (5 bytes in total).
                        StreamDecoderOutput::Pixels(it) => {
                            for pix in it {
                                //if img.len() == (img_size as usize) {
                                //    assert!(false)
                                //}
                                img.push(pix);
                            }
                        }

                        StreamDecoderOutput::ImageChannelParsed(c) => {
                            println!("channel: {}", c);
                        }
                        StreamDecoderOutput::ImageColorspaceParsed(c) => {
                            println!("colorspace: {}", c);
                        }

                        // The StreamDecoder informs us when it has returned all pixels in the
                        // image.
                        StreamDecoderOutput::Finished => {
                            println!("Finished");
                            break;
                        }
                    }
                }
                // If we failed to pull a byte out of the file, then throw an error.
                Err(e) => {
                    println!("{}", e);
                    assert!(false)
                }
            }
        }

        // Using image's QOI reader as a known-good reader. We should parse to the same bytes.
        let img_qoi_img = ImageReader::open("tests/dice.qoi")
            .unwrap()
            .decode()
            .unwrap();
        let img_qoi_img = img_qoi_img.into_bytes();

        let img: Vec<u8> = img.into_iter().flat_map(|a| a.to_bytes()).collect();

        assert_eq!(img.len(), img_qoi_img.len());

        // Not doing an assert_eq on qoi_img and img_qoi_img because it blows up the terminal log.
        for (i, (p1, p2)) in img_qoi_img.iter().zip(img.iter()).enumerate() {
            if p1 != p2 {
                println!("{}", i);
            }
            assert_eq!(p1, p2)
        }
    }

    #[test]
    fn test_decoder() {
        use crate::dec::Decoder;
        use image::io::Reader as ImageReader;
        use std::fs::File;
        use std::path::PathBuf;

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
        use crate::dec::{Channels, Colorspace, Header};

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
