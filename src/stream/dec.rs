use crate::dec::{
    Channels, Colorspace, Pixel, ops, Decoder
};
use crate::utils::Error;
use std::fmt::Display;

/// The output of the StreamDecoder while decoding.
///
/// When designing a program using [StreamDecoder](crate::stream::StreamDecoder), a user may chose
/// to ignore some of these outputs. For example, if the individual pixel values are all that is
/// needed, then the `*Parsed` variants can be ignored. The `NeedsMore` variant also only exists
/// for the user to potentially pre-buffer a number of bytes ahead of time, but can also be
/// ignored.
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
///
/// `NotStarted` is the default value and Finished is the last value. `ParsingHeader` is jumped to when
/// starting from `NotStarted`. The value in `ParsingHeader` is the number of header bytes that have
/// been parsed. After the header finishes, `ParsingOp` is set to (0, -1), a sentinel that marks that
/// the previous op has finished and the next byte passed into
/// [feed][crate::stream::StreamDecoder::feed()] will be the next opcode. All other cases of
/// `ParsingOp(a, b)` have a as the currently running opcode and b as the number of bytes parsed for
/// that op so far.
#[derive(Default, Debug)]
enum StreamDecoderState {
    #[default]
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


// TODO: Allow for RGB instead of RGBA for 64 bytes of savings. Remove buffer for 4 bytes. Allow for
// 32 bit maximum (through features) to reduce num_pix and cur_pix to u32s (4 byte savings each).
/// A streaming decoder for the QOI image format.
///
/// This decoder and it's [feed][crate::stream::StreamDecoder::feed()] function are designed to
/// store no pixel values while decoding. The pixels are instead sent out to the user immediately
/// as they finish being decoded. This allows the user to handle storing or using the pixels as
/// they wish and also reduces the memory usage by not storing all bytes in an image in memory.
/// Images larger than the amount of memory in the system can be decoded using StreamDecoder.
pub struct StreamDecoder {
    // 280 bytes total
    state: StreamDecoderState, // 2 bytes
    last_pixel: Pixel,         // 4 bytes
    dec_buffer: [Pixel; 64],   // 256 bytes
    buffer: [u8; 4],           // 4 bytes
    num_pix: Option<u64>,      // 8 bytes
    cur_pix: u64,              // 8 bytes
}

impl Default for StreamDecoder {
    fn default() -> Self {
        Self::new()
    }
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

    /// The main feeding function for decoding a QOI image as a stream of bytes.
    ///
    /// The user is expected to pass in the bytes of a QOI image sequentially, starting from the
    /// first byte of the header and ending with the last byte of the image (we techically stop
    /// before the ending sentinel).
    ///
    /// The function will return a `Result<StreamDecoderOutput, Error>`, where all errors are
    /// passed through the result and all decoded values are passed through the
    /// [StreamDecoderOutput](crate::stream::StreamDecoderOutput) object. This output contains
    /// information regarding the header fields parsed (height, width, colorspace, and channel
    /// count), as well as the number of bytes needed to finish parsing the current pixel(s).
    ///
    /// The user techincally does not need to handle any of the outputs from the
    /// `StreamDecoderOutput` object other than `Pixels`. As long as the StreamDecoder does not
    /// error, it will continue to return either `NeedsMore` and `Pixels` (after the header is
    /// done) until the image is finished (marked by `Finished`). `NeedsMore` can be ignored and
    /// is purely informational.
    ///
    /// See [Decoder](crate::dec::Decoder) for a chunked decoder that stores all data in memory.
    /// `Decoder` generally has a simpler interface and is faster than `StreamDecoder`.
    ///
    /// Internally, feed is a big state machine that takes in a single byte and uses it's internal
    /// state from the previous byte(s) to properly parse a QOI opcode. See the QOI spec
    /// [here](https://qoiformat.org) for more information.
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

        out
    }
}

/// An iterator returned by the StreamDecoder whenever it has some number of pixels extracted.
///
/// This computes the pixels on the fly using information passed in by the iterator. This is
/// designed to be memory efficient as only the information needed to make a new pixel is stored.
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

#[cfg(test)]
mod tests {
    use crate::stream::dec::{Pixel, StreamDecoder, StreamDecoderOutput};
    use image::io::Reader as ImageReader;
    use std::fs::File;
    use std::io::{BufReader, Read};
    use std::path::PathBuf;

    #[test]
    fn test_stream_decoder() {

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
}
