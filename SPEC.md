# ALOHA Image Format Specification

**Format version:** 0 \
**Status:** Draft \
**Last updated:** 2026-10-25

## 1. Overview

ALOHA is a lossless, archive-oriented raster image format. It combines
horizontal delta coding with DEFLATE compression and stores arbitrary metadata
alongside pixel data.

Design goals:

- **Lossless** — no information is discarded on encode/decode.
- **Self-contained** — a single file holds everything needed to reconstruct
  the image.
- **Open** — this document is the authoritative reference for any
  independent implementation.
- **Simple** — the format is intentionally minimal to keep decoders
  small and auditable.

ALOHA is not intended for streaming or progressive decoding.
It is designed for storage and interchange.

## 2. Conventions

- **Byte order:** All multi-byte numbers are **little-endian**.
- **Strings:** UTF-8 unless stated otherwise.
- **Undefined behavior:** Any file that does not conform to this
  specification is invalid and MUST be rejected by conforming decoders.

## 3. File Layout

An ALOHA file consists of the following sections, in order:

- [Header](#4-header)
- [ICC profile](#5-icc-profile-section)
- [Palette (optional)](#6-palette-section)
- [Metadata (JSON)](#7-metadata-section)
- [Compressed pixel data](#8-pixel-data)
- [CRC32](#9-crc32)

All sections are contiguous. No padding is inserted.

## 4. Header

| Offset | Size | Field         | Description                                      |
|--------|------|---------------|--------------------------------------------------|
| 0      | 6    | `magic`       | ASCII `"ALOHA1"` (0x41 0x4C 0x4F 0x48 0x41 0x31) |
| 6      | 2    | `version`     | Format version, `u16`.                           |
| 8      | 4    | `width`       | Image width in pixels, `u32`. MUST be $\geq$ 1.  |
| 12     | 4    | `height`      | Image height in pixels, `u32`. MUST be $\geq$ 1. |
| 16     | 1    | `color_type`  | See [4.1](#41-color-types).                      |
| 17     | 1    | `bit_depth`   | Bits per channel: `8` or `16`.                   |
| 18     | 1    | `compression` | See [4.2](#42-compression-algorithms).           |

Immediately after the header, the ICC profile section follows. If `color_type` is `Indexed`,
then the palette section follows. Then the metadata section, then the compressed pixel data,
then the CRC32.

### 4.1 Color types

Color type is an 8-bit unsigned integer that represents the image's color model.

| Value | Name           | Channels | Description                                |
|-------|----------------|----------|--------------------------------------------|
| 0     | Grayscale      | 1        | Single channel, bit depth as declared.     |
| 1     | GrayscaleAlpha | 2        | Gray + alpha.                              |
| 2     | RGB            | 3        | Red, green, blue.                          |
| 3     | RGBA           | 4        | Red, green, blue, alpha.                   |
| 4     | Indexed        | 1        | Palette index; palette selection required. |

Values `5`—`255` are invalid and MUST be rejected.

### 4.2 Compression algorithms

Compression algorithm is an 8-bit unsigned integer that represents the pixel data
compression algotithm being used.

| Value | Algorithm    | Description                                         |
|-------|--------------|-----------------------------------------------------|
| 0     | None         | No compression.                                     |
| 1     | Delta + zlib | Horizontal delta coding followed by zlib (DEFLATE). |

Values `2`—`255` are invalid and MUST be rejected.

## 5. ICC Profile Section

The ICC profile section consists of:

| Size | Field         | Description                              |
|------|---------------|------------------------------------------|
| 4    | `iccp_len`    | Length of ICC profile, `u32`.            |
| *    | `icc_profile` | `iccp_len` bytes of ICC profile data.    |

## 6. Palette Section

Required when `color_type` is `Indexed`. The palette section consists of:

| Size | Field         | Description                              |
|------|---------------|------------------------------------------|
| 2    | `palette_len` | Length of palette bytes, `u16`.          |
| *    | `palette`     | `palette_len` bytes of raw palette data. |

The palette data is stored as raw bytes as provided by the source
(e.g., from a PNG indexed image). The number of entries is not explicitly derived here;
it is determined by the consumer based on the context (e.g., PNG palette semantics).
For an ALOHA file generated from an indexed PNG, `palette_len` equals the byte length
of the PNG palette.

If `color_type` is not `Indexed`, this section MUST be absent.

## 7. Metadata Section

The metadata section consists of:

| Size | Field      | Description                     |
|------|------------|---------------------------------|
| 4    | `meta_len` | Length of JSON bytes, `u32`.    |
| *    | `metadata` | `meta_len` bytes of UTF-8 JSON. |

The JSON object has the following fields:

| Key      | Type           | Description                    |
|----------|----------------|--------------------------------|
| `author` | string or null | Optional. Author or creator.   |
| `source` | string or null | Optional. Origin of the image. |

Additional keys MAY be present. Decoders MUST ignore unknown keys rather than fail.

Fields with a value of `None` are not written. If metadata is empty, `meta_len` is `0`
and the metadata bytes are absent.

## 8. Pixel Data

### 8.1 Representation

Samples are stored in **pixel-interleaved** order: for a pixel at `(x, y)` and channel `c`,
the sample index is

```text
s = (y * width + x) * channels + c
```

where `channels` is the channel count for the color type (see [4.1](#41-color-types)).
Rows are laid out top to bottom, pixels within a row left to right, channels within a
pixel in the order defined by the color type (e.g. R, G, B, A).

Each sample is stored as an unsigned integer of `bit_depth` bits (`8` or `16`):

- `bit_depth == 8`: one byte per sample.
- `bit_depth == 16`: two bytes per sample, **little-endian**.

A sample's value is in the range `[0, 2^bit_depth - 1]`; the mask `2^bit_depth - 1` is
reffered to below as `sample_mask`.

### 8.2 Delta coding

Delta coding is applied when `compression = 1` **and** `color_type` is not `Indexed`.
For `Indexed` images the pixel buffer is stored unchanged, with no delta coding,
regardless of `compression`.

For non-`Indexed` images, delta coding is applied **per row**, with per-channel state
that resets to zero at the start of every row:

```text
# pseudocode

out = [0; data.len()]
for y in 0..height:
    prev[c] = 0
    for x in 0..width:
        for c in 0..channels:
            v = read_sample(data, (y * width + x) * channels + c)
            d = (v - prev[c]) mod 2^bit_depth
            write_sample(out, (y * width + x) * channels + c, d)
            prev[c] = v
```

The substraction is performed modulo `2^bit_depth`. In the reference implementation this is
`v.wrapping_sub(prev[c]) & sample_mask`, whice yields the same result because both `v` and
`prev[c]` are already in `[0, 2^bit_depth - 1]`.

Decoding reverses this with a cumulative sum per channel, again resetting `prev` to zero
at the start of each row:

```text
# pseudocode

out = [0; data.len()]
for y in 0..height:
    prev[c] = 0
    for x in 0..width:
        for c in 0..channels:
            d = read_sample(data, (y * width + x) * channels + c)
            v = (prev[c] + d) mod 2^bit_depth
            write_sample(out, (y * width + x) * channels + c, v)
            prev[c] = v
```

The addition is performed modulo `2^bit_depth`; in the reference implementation this is
`(prev[c].wrapping_add(d)) & sample_mask`.

Because `prev` is initialized to zero, the first pixel of each row is stored with its
raw sample values as the differences (`(v - 0) mod 2^bit_depth == v`). This is not a
special case in the algorithm — it falls out of the per-row reset.

If `compression == 0`, or if `color_type` is `Indexed`, no delta coding is applied and
`out` equals `data` for the pixel section.

### 8.3 Compression

When `compression == 1`, the (possibly delta-coded) pixel buffer is compressed with DEFLATE
as produced by zlib, i.e. the same algorthm exposed by `flate2`'s `ZlibEncoder` / `ZlibDecoder`
with `Compression::best()`.

No preset dictionary is used. The zlib header MUST be present.

When `compression == 0`, the pixel data is stored raw.

## 9. CRC32

The final 4 bytes of the file are the CRC32 checksum of the **compressed (or raw) pixel data**,
computed with the `crc32fast` crate. The header, palette, and metadata are **not** covered
by this checksum in version $\leq$ 0.
