# QOI Parser

A very simple parser for the Quite Ok Image format (QOI). This is currently
only a non-streaming decoder implementation. This also borrows the
implementation from the reference C implementation found
[here](https://github.com/phoboslab/qoi).

## TODO

- [ ] Add streaming decoder
- [ ] Add chunked encoder
- [ ] Add streaming encoder
- [ ] Minimize RAM usage (streaming only)
- [ ] CLI for conversion to/from QOI using the image crate as the converter.
      (image has support for QOI itself, but I don't think it's streaming capable)
