# QOI Parser

A very simple parser for the [Quite Ok Image](https://qoiformat.org) format (QOI). This is currently
only a non-streaming decoder implementation. This also borrows the
implementation from the reference C implementation found
[here](https://github.com/phoboslab/qoi).

## Usage

Available as both a library and a binary. The binary currently does nothing but
parse a file and return the total number of bytes in it.

```bash
cargo run --bin qoi-parser <path-to-qoi-file>
```

The library is available as `qoiparser` and includes the `dec::Decoder` type, which can be used to decode bytes (anything implementing `Read`) using `dec::Decoder::decode(&mut buf)`.

## TODO

- [ ] Add streaming decoder
- [ ] Add chunked encoder
- [ ] Add streaming encoder
- [ ] Minimize RAM usage (streaming only)
- [ ] CLI for conversion to/from QOI using the image crate as the converter.
      (image has support for QOI itself, but I don't think it's streaming capable)
