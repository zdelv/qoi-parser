# QOI Parser

A very simple parser for the [Quite Ok Image](https://qoiformat.org) format
(QOI). This is currently only a decoder implementation, with both chunked and
streaming decoders. This also borrows the implementation from the reference C
implementation found [here](https://github.com/phoboslab/qoi) for the chunked
decoder.

## Usage

### Chunked Decoder

The chunked decoder is a standard image decoder that expects to be given any
object that implements `Read`. This most commonly used for reading images that
are entirely loaded into memory as a `Vec<u8>` or from a file as a `File`. The
chunked decoder stores the entire image in memory while decoding. The output is
both the `Header` and a `Vec<Pixel>`:

```rust
use qoiparser::dec::Decoder;
use std::fs::File;
use std::path::PathBuf;

let mut qoi_file = File::open(PathBuf::from("tests/dice.qoi")).unwrap();

// Decode the image into a Header and Vec<Pixel>
let (header, qoi_img) = Decoder::new().decode(&mut qoi_file).unwrap();

// Convert the image to a Vec<u8>
let qoi_img: Vec<u8> = qoi_img.into_iter().flat_map(|a| a.to_bytes()).collect();
```

### Streaming Decoder

The streaming decoder operates byte-by-byte, returning `Pixel`s immediately
when they are decoded. `Pixel`s are returned using a `PixelsIter`, which
iterates some number of times, returning the same `Pixel` each time.  The
`StreamDecoder` does not store the pixels in an internal buffer and the user is
responsible for for handling the `Pixel`s returned.

This streaming decoder is much more memory efficient than the chunked decoder.
The `StreamDecoder` struct takes up approximately 280 bytes and the
`StreamDecoder::feed()` operation attempts to do inplace operations rather than
creating new variables. The streaming decoder is designed for usecases where
loading the entire image into memory from disk is impossible or prohibitive,
which may occur in embedded microcontrollers and with QOI files that are larger
than the memory available.

The interface for `StreamDecoder` is much more complicated due to the fact that
it pushes some of the work onto the user. This allows the user to decide how
they wish to iterate over bytes or store pixels, which is useful for some
usecases.

```rust
use qoiparser::dec::{Pixel, StreamDecoder, StreamDecoderOutput};
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::PathBuf;

let mut sdec = StreamDecoder::new();

let qoi_file = BufReader::new(File::open(PathBuf::from("tests/dice.qoi")).unwrap());

let mut iter = qoi_file.bytes();

let mut img_size: u64 = 0;
let mut img: Vec<Pixel> = Vec::new();

// Iterate over all bytes in the image.
while let Some(b) = iter.next() {
    match b {
        Ok(byte) => {
            // We feed each byte into the decoder and recieve a
            // Result<StreamDecoderOutput, Error>. Any decoding errors are
            // propogated through the Result and all other output appears
            // through the StreamDecoderOutput object.
            match sdec.feed(byte).unwrap() {
                // If we feed the StreamDecoder a byte and this byte is only
                // part of a op code, then the StreamDecoder will inform us of how
                // many more bytes are needed before the next pixel is ready. You
                // may choose to ignore the number or use it to pre-buffer the
                // next number of bytes.
                StreamDecoderOutput::NeedMore(_) => {
                    // println!("needs more");
                }

                // The StreamDecoder returns whenever it parses a new field from the header.
                StreamDecoderOutput::ImageWidthParsed(w) => {
                    println!("width: {}", w);
                    img_size = w as u64;
                }
                StreamDecoderOutput::ImageHeightParsed(h) => {
                    println!("height: {}", h);
                    img_size *= h as u64;
                    img.reserve_exact(img_size as usize);
                }
                StreamDecoderOutput::ImageChannelParsed(c) => {
                    println!("channel: {}", c);
                }
                StreamDecoderOutput::ImageColorspaceParsed(c) => {
                    println!("colorspace: {}", c);
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
```

## TODO

- [x] Add streaming Decoder
- [ ] Share parts of decoder implementations (reduce code duplication).
- [ ] Add chunked encoder
- [ ] Add streaming encoder
- [ ] Minimize RAM usage (streaming only)
- [ ] `no-std` and maybe dependency free?
- [ ] CLI for conversion to/from QOI using the image crate as the converter.
      (image has support for QOI itself, but I don't think it's streaming capable)

## Licence

MIT
