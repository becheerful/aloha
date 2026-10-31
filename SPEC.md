# ALOHA Image Format Specification

**Format version:** 0 \
**Status:** Draft \
**Last updated:** 2026-10-31

## 1. Overview

ALOHA is a lossless, archive-oriented raster image format with HDR support.

Design goals:

- **Lossless** — no information is discarded on encode/decode.
- **Self-contained** — a single file holds everything needed to reconstruct
  the image.
- **Open** — this document is the authoritative reference for any
  independent implementation.

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
- [cICP](#5-cicp-section) (conditional)
- [ICC profile](#6-icc-profile-section) (conditional)
- [Palette](#7-palette-section) (conditional)
- [Metadata (JSON)](#8-metadata-section)
- [Compressed pixel data](#9-pixel-data-section)
- [Gain map metadata](#10-gain-map-metadata-section) (conditional)
- [Gain map data](#11-gain-map-section) (conditional)
- [CRC32](#12-crc32-section)

All sections are contiguous. No padding is inserted. Conditional sections are present
only if the corresponding flags or descriptros indicate so.

## 4. Header

| Offset | Size | Field                | Description                                                      |
|--------|------|----------------------|------------------------------------------------------------------|
| 0      | 5    | `magic`              | ASCII `"ALOHA"` (0x41 0x4C 0x4F 0x48 0x41)                       |
| 5      | 2    | `version`            | Format version, `u16`.                                           |
| 7      | 4    | `width`              | Image width in pixels, `u32`. MUST be $\geq$ 1.                  |
| 11     | 4    | `height`             | Image height in pixels, `u32`. MUST be $\geq$ 1.                 |
| 15     | 1    | `color_type`         | See [4.1](#41-color-types).                                      |
| 16     | 1    | `bit_depth`          | Bits per channel: `8` or `16`.                                   |
| 17     | 1    | `color_profile_desc` | See [4.2](#42-color-profile-descriptor).                         |
| 18     | 1    | `compression`        | See [4.3](#43-compression-algorithms).                           |
| 19     | 1    | `has_gain_map`       | `0` = no gain map, `1` = gain map section follow the pixel data. |

### 4.1 Color types

An 8-bit unsigned integer that represents the image's color model.

| Value | Name           | Channels | Description                                |
|-------|----------------|----------|--------------------------------------------|
| 0     | Grayscale      | 1        | Single channel, bit depth as declared.     |
| 1     | GrayscaleAlpha | 2        | Gray + alpha.                              |
| 2     | RGB            | 3        | Red, green, blue.                          |
| 3     | RGBA           | 4        | Red, green, blue, alpha.                   |
| 4     | Indexed        | 1        | Palette index; palette selection required. |

Values `5`—`255` are invalid and MUST be rejected.

### 4.2 Color Profile Descriptor

An 8-bit unsigned integer indicating the type of color profile information present.

| Value | Name | Description                                            |
|-------|------|--------------------------------------------------------|
| 0     | None | No color profile section follows.                      |
| 1     | Cicp | A cICP section immediately follows the header.         |
| 2     | Icc  | An ICC profile section immediately follows the header. |

Values `3`—`255` are invalid and MUST be rejected.

### 4.3 Compression algorithms

An 8-bit unsigned integer that represents the pixel data compression algotithm being used.

| Value | Algorithm    | Description                                         |
|-------|--------------|-----------------------------------------------------|
| 0     | None         | No compression.                                     |
| 1     | Delta + zlib | Horizontal delta coding followed by zlib (DEFLATE). |

Values `2`—`255` are invalid and MUST be rejected.

## 5. cICP section

Present only when `color_profile_desc` is `1` (`Cicp`).

| Size | Field                      | Description                                |
|------|----------------------------|--------------------------------------------|
| 1    | `color_primaries`          | Color primaries code point, `u8`.          |
| 1    | `transfer_characteristics` | Transfer characteristics code point, `u8`. |
| 1    | `matrix_coefficients`      | Matrix coefficients code point, `u8`.      |
| 1    | `full_range`               | `0` = limited range, `1` = full range.     |

These values corresponded to the coding-independent code points (cICP) as defined by
ITU-T H.273.

## 6. ICC Profile Section

Present only when `color_profile_desc` is `2` (`Icc`).

| Size | Field         | Description                              |
|------|---------------|------------------------------------------|
| 4    | `iccp_len`    | Length of ICC profile, `u32`.            |
| *    | `icc_profile` | `iccp_len` bytes of ICC profile data.    |

## 7. Palette Section

Required when `color_type` is `4` (`Indexed`). The palette section MUST be absent otherwise.

| Size | Field         | Description                              |
|------|---------------|------------------------------------------|
| 2    | `palette_len` | Length of palette bytes, `u16`.          |
| *    | `palette`     | `palette_len` bytes of raw palette data. |

The palette data is stored as raw bytes as provided by the source (e.g., from a PNG indexed image).
The number of entries is not explicitly derived here; it is determined by the consumer based on the
context (e.g., PNG palette semantics). For an ALOHA file generated from an indexed PNG, `palette_len`
equals the byte length of the PNG palette.

## 8. Metadata Section

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

## 9. Pixel Data Section

### 9.1 Representation

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

### 9.2 Delta coding

Delta coding is applied when `compression` is `1` **and** `color_type` is not `4` (`Indexed`).
For `Indexed` images the pixel buffer is stored unchanged, with no delta coding, regardless of
`compression`.

For non-`Indexed` images, delta coding is applied **per row**, with per-channel state that
resets to zero at the start of every row:

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

Because `prev` is initialized to zero, the first pixel of each row is stored with its raw
sample values as the differences (`(v - 0) mod 2^bit_depth == v`). This is not a special case
in the algorithm — it falls out of the per-row reset.

If `compression` is `0`, or if `color_type` is `4` (`Indexed`), no delta coding is applied and
`out` equals `data` for the pixel section.

### 9.3 Compression

When `compression` is `1`, the (possibly delta-coded) pixel buffer is compressed with DEFLATE
as produced by zlib, i.e. the same algorthm exposed by `flate2`'s `ZlibEncoder` / `ZlibDecoder`
with `Compression::best()`. No preset dictionary is used. The zlib header MUST be present.

When `compression` is `0`, the pixel data is stored raw.

## 10. Gain Map Metadata Section

Preset only when `has_gain_map` is `1`. The gain map metadata section precedes the gain map
pixel data.

The section stores three-element arrays for `gain_map_max`, `gain_map_min`, `gamma`, `base_offset`,
and `alternate_offset`, followed by scalar values for `base_hdr_headroom`, `alternate_hdr_headroom`,
and boolean flags `use_base_color_space` and `backward_direction`. All floating-point values are `f64`,
**litle-endian**.

| Size  | Field                  | Description                             |
|-------|------------------------|-----------------------------------------|
| 8 * 3 | `gain_map_max[3]`      | Maximum gain values per channel, `f64`. |
| 8 * 3 | `gain_map_min[3]`      | Minimum gain values per channel, `f64`. |
| 8 * 3 | `gamma[3]`             | Gamma values per channel, `f64`.        |
| 8 * 3 | `base_offset[3]`       | Base offsets per channel, `f64`.        |
| 8 * 3 | `alternate_offset[3]`  | Alternate offsets per channel, `f64`.   |
| 8     | `base_hdr_headroom`    | Base HDR headroom, `f64`.               |
| 1     | `use_base_color_space` | `0` = false, `1` = true.                |
| 1     | `backward_direction`   | `0` = false, `1` = true.                |

These fields correspond to the gain map metadata defined by the Ultra HDR specification.

## 11. Gain Map Section

Present only when `has_gain_map` is `1`. The gain map is a separate image with the following layout:

| Size | Field           | Description                                                      |
|------|-----------------|------------------------------------------------------------------|
| 4    | `width`         | Width of the gain map, `u32`. MUST be $\geq$ 1.                  |
| 4    | `height`        | Height of the gain map, `u32`. MUST be $\geq$ 1.                 |
| 1    | `channels`      | Number of channels (`1` or `4`), `u8`.                           |
| 8    | `gain_map_len`  | Length of the compressed gain map data, `u64`.                   |
| *    | `gain_map_data` | `gain_map_len` bytes of zlib-compressed, delta-coded pixel data. |

The gain map pixel data is **always** 8-bit per channel and is processed as follows:

1. Delta encoding is applied per row (same algorithm as in [9.2](#92-delta-coding)), regardless of the
main image's `compression` value.

2. The delta-coded buffer is compressed with zlib (`flate2::Compression::best()`).

During decoding, the compressed data is first decompressed with zlib, then delta-decoded using the gain
map's own `width`, `height`, and channel count (width `bit_depth` = `8`).

## 12. CRC32 Section

The final 4 bytes of the file are the CRC32 checksum of all the data in the file (without the
checksum itself), computed with the `crc32fast` crate.

The CRC32 covers every byte from the beggining of the header through the end of the last section preceding
the checksum (i.e., the entire file except the 4-byte checksum itself). Decoders MUST verify this checksum
and MUST reject the file if it does not match.
