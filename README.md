# QOI Parser

A parser for the [Quite Ok Image](https://qoiformat.org) format (QOI). 

This is currently only contains a decoder implementation, with both chunked and
streaming decoders. The chunked decoder borrows it's implementation from the
reference implementation found [here](https://github.com/phoboslab/qoi). The
streaming decoder is custom but shares some arithmetic from the reference
implementation.

## Usage

The parser is not yet up on [crates.io](https://crates.io) due to it being
still in its early stages. I'd like to get both the chunked and streaming
encoders built before publishing it.

To use this package, add the git repository to your `Cargo.toml`:

```toml
[dependencies]
qoi-parser = { git = "https://github.com/zdelv/qoi-parser" }
```

### Chunked Decoder

The chunked decoder is a standard image decoder that parses the incoming bytes
into an image stored entirely in memory. It expects to be given any object that
implements `Read`. The chunked decoder is most commonly used for reading QOI
files that are entirely loaded into memory as a `Vec<u8>`/`&[u8]` or from disk
as a `File`. The output of `Decoder::decode()` is both the file header and a
`Vec<Pixel>`:

```rust
use std::fs::File;
use std::path::PathBuf;
use std::io::BufReader;

use qoiparser::Decoder;

let qoi_file = File::open(PathBuf::from("tests/dice.qoi")).unwrap();
// BufReader helps with buffering the file while parsing.
let mut qoi_file = BufReader::new(qoi_file);

// Decode the image into a Header and Vec<Pixel>
let (header, qoi_img) = Decoder::new().decode(&mut qoi_file).unwrap();

// Convert the image to a Vec<u8>
let qoi_img: Vec<u8> = qoi_img.into_iter().flat_map(|a| a.to_bytes()).collect();
```

### Streaming Decoder

The streaming decoder operates byte-by-byte, returning `Pixel`s immediately
when they are decoded. Users feed bytes individually and the decoder responds
with either an iterator of pixels (`PixelsIter`) or a request for more bytes.
The `StreamDecoder` does not store the pixels in an internal buffer and the
user is responsible for for handling the `Pixel`s as they are returned.

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
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::PathBuf;

use qoiparser::stream::{StreamDecoder, StreamDecoderOutput};
use qoiparser::Pixel;

let mut sdec = StreamDecoder::new();

let qoi_file = BufReader::new(File::open(PathBuf::from("tests/dice.qoi")).unwrap());

let mut img_size: u64 = 0;
let mut img: Vec<Pixel> = Vec::new();

// Using read_exact + a buffer is faster than file.bytes() for reasons.
let mut buf = [0u8; 1];

// Iterate over all bytes in the image.
while let Ok(_) = qoi_file.read_exact(&mut buf) {

    // We feed each byte into the decoder and recieve a
    // Result<StreamDecoderOutput, Error>. Any decoding errors are
    // propogated through the Result and all other output appears
    // through the StreamDecoderOutput object.
    match sdec.feed(buf[0]).unwrap() {
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
```

## Performance 

At its current implementation, `Decoder` is roughly 2-3x faster than
`StreamDecoder`, assuming equivalent circumstances (`Decoder` passed a
`BufReader` wrapping a `File` and `StreamDecoder` using a `BufReader` wrapping
a `File` as its byte source.).

All testing was done on `--release` with no other changes. Tested on a M2 Max
Macbook Pro with 10 runs on each decoder. File tested was the `dice.qoi` file
found in the QOI test images from [here](https://www.qoiformat.org) and in the
`tests` folder in this repo.

<table>
    <tr>
        <th>Decoder Type</th>
        <th>Time (ms)</th>
        <th>Throughput (MB/sec)</th>
    </tr>
    <tr>
        <td>Stream</td>
        <td>9.579</td>
        <td>57.44</td>
    </tr>
    <tr>
        <td>Chunked</td>
        <td>3.255</td>
        <td>173.2</td>
    </tr>
</table>


## TODO

- [x] Add streaming Decoder
- [ ] Share parts of decoder implementations (reduce code duplication).
- [ ] Add chunked encoder
- [ ] Add streaming encoder
- [ ] Minimize RAM usage (streaming only)
- [ ] `no-std` and maybe dependency free?
- [ ] Compare to reference C implementation
- [ ] CLI for conversion to/from QOI using the image crate as the converter.
      (image has support for QOI itself, but I don't think it's streaming capable)

## Licence

MIT
