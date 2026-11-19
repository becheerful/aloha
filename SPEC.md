# ALOHA Image Format Specification

**Format version:** 0 \
**Status:** Draft \
**Last updated:** 2026-11-18

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
- [CICP](#5-cicp-section) (conditional)
- [ICC profile](#6-icc-profile-section) (conditional)
- [Palette](#7-palette-section) (conditional)
- [Transparency Palette](#8-transparency-palette-section) (conditional)
- [Metadata (JSON)](#9-metadata-section)
- [Compressed pixel data](#10-pixel-data-section)
- [Gain map metadata](#11-gain-map-metadata-section) (conditional)
- [Gain map data](#12-gain-map-section) (conditional)

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
| 20     | 4    | `crc`                | Header's checksum, computed with the `crc32fast` crate.          |

### 4.1 Color types

An 8-bit unsigned integer that represents the image's color model.

| Value | Name           | Channels | Description                                                            |
|-------|----------------|----------|------------------------------------------------------------------------|
| 0     | Grayscale      | 1        | Single channel, shades of gray.                                        |
| 1     | GrayscaleAlpha | 2        | Gray + alpha.                                                          |
| 2     | Rgb            | 3        | Red, green, blue.                                                      |
| 3     | Rgba           | 4        | Red, green, blue, alpha.                                               |
| 4     | Indexed        | 1        | Palette index; palette section required.                               |
| 5     | IndexedAlpha   | 2        | Index in palettes; palette and transparency palette sections required. |

Values `6`—`255` are invalid and MUST be rejected.

### 4.2 Color Profile Descriptor

An 8-bit unsigned integer indicating the type of color profile information present.

| Value | Name | Description                                            |
|-------|------|--------------------------------------------------------|
| 0     | None | No color profile section follows.                      |
| 1     | Cicp | A CICP section immediately follows the header.         |
| 2     | Icc  | An ICC profile section immediately follows the header. |

Values `3`—`255` are invalid and MUST be rejected.

### 4.3 Compression algorithms

An 8-bit unsigned integer that represents the pixel data compression algotithm being used.

| Value | Algorithm    | Description                                                                   |
|-------|--------------|-------------------------------------------------------------------------------|
| 0     | None         | No compression.                                                               |
| 1     | Delta + zlib | Adaptive line-by-line PNG-like delta, followed by zlib (DELFATE) compression. |
| 2     | Delta + zstd | Adaptive line-by-line PNG-like delta, followed by zstd compression.           |

Values `3`—`255` are invalid and MUST be rejected.

## 5. CICP section

Present only when `color_profile_desc` is `1` (`Cicp`). Otherwise, it MUST be absent.

| Size | Field                      | Description                                           |
|------|----------------------------|-------------------------------------------------------|
| 1    | `color_primaries`          | Color primaries code point, `u8`.                     |
| 1    | `transfer_characteristics` | Transfer characteristics code point, `u8`.            |
| 1    | `matrix_coefficients`      | Matrix coefficients code point, `u8`.                 |
| 1    | `full_range`               | `0` = limited range, `1` = full range.                |
| 4    | `crc`                      | CICP's checksum, computed with the `crc32fast` crate. |

These values corresponded to the coding-independent code points (CICP) as defined by
ITU-T H.273.

## 6. ICC Profile Section

Present only when `color_profile_desc` is `2` (`Icc`). Otherwise, it MUST be absent.

| Size  | Field         | Description                                                  |
|-------|---------------|--------------------------------------------------------------|
| 4     | `len`         | Length of ICC profile, `u32`.                                |
| `len` | `icc_profile` | `len` bytes of ICC profile data.                             |
| 4     | `crc`         | ICC profile's checksum, computed with the `crc32fast` crate. |

## 7. Palette Section

Required only when `color_type` is `4` (`Indexed`) or `5` (`IndexedAlpha`). Otherwise, it MUST be absent.

| Size  | Field     | Description                                              |
|-------|-----------|----------------------------------------------------------|
| 2     | `len`     | Length of palette bytes, `u16`.                          |
| `len` | `palette` | `len` bytes of raw palette data.                         |
| 4     | `crc`     | Palette's checksum, computed with the `crc32fast` crate. |

The palette data is stored as raw bytes as provided by the source (e.g., from a PNG indexed image).
The number of entries is not explicitly derived here; it is determined by the consumer based on the
context (e.g., PNG palette semantics). For an ALOHA file generated from an indexed PNG, `len` equals
the byte length of the PNG palette.

## 8. Transparency Palette Section

Required only when `color_type` is `5` (`IndexedAlpha`). Otherwise, it MUST be absent.

| Size  | Field                  | Description                                                           |
|-------|------------------------|-----------------------------------------------------------------------|
| 1     | `len`                  | Length of transparency palette bytes, `u8`.                           |
| `len` | `transparency_palette` | `len` bytes of raw transparency palette data.                         |
| 4     | `crc`                  | Transparency palette's checksum, computed with the `crc32fast` crate. |

The transparency palette data is stored as raw bytes as provided by the source (e.g., from a PNG
indexed image). The number of entries is not explicitly derived here; it is determined by the consumer
based on the context (e.g., PNG tRNS chunk semantics). For an ALOHA file generated from an indexed PNG,
`len` equals the byte length of the PNG tRNS chunk.

## 9. Metadata Section

| Size  | Field      | Description                                               |
|-------|------------|-----------------------------------------------------------|
| 4     | `len`      | Length of JSON bytes, `u32`.                              |
| `len` | `metadata` | `len` bytes of UTF-8 JSON.                                |
| 4     | `crc`      | Metadata's checksum, computed with the `crc32fast` crate. |

The metadata is serialized as XMP (XML) within the `.aloha` container, using the XMP Dubli Core schema
(`dc:` namespace). All fields are optional; a field is omitted from the serialized XML when it is None.

The following Dublic Core properties are supported:

| XMP Property (`dc:`) | Type                                                 | Description                                                                  |
|----------------------|------------------------------------------------------|------------------------------------------------------------------------------|
| `contributor`        | Array of strings                                     | Contributors to the resource (other than the authors).                       |
| `coverage`           | String                                               | The extent or scope of the resource.                                         |
| `creator`            | Array of strings                                     | The authors of the resource (listed in order of precedence, if significant). |
| `date`               | Array of date strings (`YYYY-MM-DD`)                 | Date(s) that something interesting happened to the resource.                 |
| `description`        | Language-alternative structure (string per language) | Textual description of the content.                                          |
| `format`             | String                                               | File format used when saving the resource.                                   |
| `identifier`         | String (UUID v7 URN)                                 | Unique identifier of the reousce, e.g. `urn:uuid:...`.                       |
| `language`           | Array of strings                                     | Languages used in the resource.                                              |
| `publisher`          | Array of strings                                     | Publishers.                                                                  |
| `relation`           | Array of strings                                     | Relationship to other documents.                                             |
| `rights`             | Language-alternative structure (string per language) | Informal rights statement.                                                   |
| `source`             | String                                               | Unique identifier of the work from which this resource was derived.          |
| `subject`            | Array of strings                                     | Descriptive phrases or keywords.                                             |
| `title`              | Language-alternative structure (string per language) | Title or name of the resource.                                               |
| `type`               | Array of strings                                     | Document type (e.g. novel, poem, working paper).                             |

If the metadata is empty (all fields are `None`), `len` is `0` and the metadata bytes are absent.
However, the `crc` MUST be present in any case.

## 10. Pixel Data Section

### 10.1 Representation

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

### 10.2 Delta coding

Delta coding is applied when `compression` is greater than `0` **and** `color_type` is not `4`
(`Indexed`) or `5` (`IndexedAlpha`). For indexed images the pixel buffer is stored unchanged,
with no delta coding, regardless of `compression`.

For non-indexed images, delta coding is applied **per row**. Each row is coded independently
using one of the following filter types (adapted from the PNG specification):

| Value | Filter  | Description                                                                                                                        |
|-------|---------|------------------------------------------------------------------------------------------------------------------------------------|
| 0     | None    | The row is stored as-is.                                                                                                           |
| 1     | Left    | Each byte is replaced by the difference from the corresponding byte of the previous pixel (to the left).                           |
| 2     | Top     | Each byte is replaced by the difference from the corresponding byte of the pixel directly above (same column in the previous row). |
| 3     | Average | Each byte is replaced by the difference from the average of the left and above pixels (rounded down).                              |
| 4     | Paeth   | Each byte is replaced by the difference from the Paeth predictor (a linear combination of left, above, and upper-left pixels).     |

For the first pixel of a row, the "left" value is treated as zero. For the first row, the "above"
and "upper-left" values are treated as zero.

The encoder MUST select the filter that yields the smallest sum of absolute values of the encoded
bytes for each row. The selected filter type is stored as a single byte preceding the encoded row
data. Therefore, the encoded output has the following layout:

```text
for each row y in 0..height:
  filter_type[y] (1 byte)
  filtered_row[y] (width * bytes per pixel)
```

Bytes per pixel are calculated as `(bit_depth / 8) * channels`.

Decoding reverses the encoding process:

```text
for each row y in 0..height:
  read filter_type
  for each byte in the filtered row:
    reconstruct the original byte using the inverse of the selected filter
    (the left, above, and upper-left values are taken from the already reconstructed current row and the fully reconstructed previous row)
```

For indexed images (`color_type` is `4` or `5`), no coding is applied and the pixel data is stored
as-is, regardless of `compression`.

If `compression` is `0`, no coding is applied and the pixel section contains the raw pixel data.

### 10.3 Compression

When `compression` is `0`, the pixel data is stored raw.

When `compression` is `1`, the (possibly delta-coded) pixel buffer is compressed with DEFLATE
as produced by zlib, i.e. the same algorthm exposed by `flate2`'s `ZlibEncoder` / `ZlibDecoder`
with `Compression::best()`. No preset dictionary is used. The zlib header MUST be present.

When `compression` is `2`, the (possibly delta-coded) pixel buffer is compressed with zstd.

## 11. Gain Map Metadata Section

Preset only when `has_gain_map` is `1`. The gain map metadata section precedes the gain map
pixel data.

The section stores three-element arrays for `gain_map_max`, `gain_map_min`, `gamma`, `base_offset`,
and `alternate_offset`, followed by scalar values for `base_hdr_headroom`, `alternate_hdr_headroom`,
and boolean flags `use_base_color_space` and `backward_direction`. All floating-point values are `f64`,
**litle-endian**.

| Offset | Size  | Field                    | Description                                                        |
|--------|-------|--------------------------|--------------------------------------------------------------------|
| 0      | 8 * 3 | `gain_map_max[3]`        | Maximum gain values per channel, `f64`.                            |
| 24     | 8 * 3 | `gain_map_min[3]`        | Minimum gain values per channel, `f64`.                            |
| 48     | 8 * 3 | `gamma[3]`               | Gamma values per channel, `f64`.                                   |
| 72     | 8 * 3 | `base_offset[3]`         | Base offsets per channel, `f64`.                                   |
| 96     | 8 * 3 | `alternate_offset[3]`    | Alternate offsets per channel, `f64`.                              |
| 120    | 8     | `base_hdr_headroom`      | Base HDR headroom, `f64`.                                          |
| 128    | 8     | `alternate_hdr_headroom` | Alternate HDR headroom, `f64`.                                     |
| 136    | 1     | `use_base_color_space`   | `0` = false, `1` = true.                                           |
| 137    | 1     | `backward_direction`     | `0` = false, `1` = true.                                           |
| 138    | 4     | `crc`                    | Gain map metadata's checksum, computed with the `crc32fast` crate. |

These fields correspond to the gain map metadata defined by the Ultra HDR specification.

## 12. Gain Map Section

Present only when `has_gain_map` is `1`. The gain map is a separate image with the following layout:

| Offset     | Size  | Field           | Description                                               |
|------------|-------|-----------------|-----------------------------------------------------------|
| 0          | 4     | `width`         | Width of the gain map, `u32`. MUST be $\geq$ 1.           |
| 4          | 4     | `height`        | Height of the gain map, `u32`. MUST be $\geq$ 1.          |
| 8          | 1     | `channels`      | Number of channels (`1` or `4`), `u8`.                    |
| 9          | 8     | `len`           | Length of the compressed gain map data, `u64`.            |
| 17         | `len` | `gain_map_data` | `len` bytes of zlib-compressed, delta-coded pixel data.   |
| 17 + `len` | 4     | `crc`           | Gain map's checksum, computed with the `crc32fast` crate. |

The gain map pixel data is **always** 8-bit per channel and is processed as follows:

1. Delta encoding is applied per row (same algorithm as in [10.2](#102-delta-coding)), regardless of the
main image's `compression` value.

2. The delta-coded buffer is compressed with zlib (`flate2::Compression::best()`), regardless of the main
image's `compression` value.

During decoding, the compressed data is first decompressed with zlib, then delta-decoded using the gain
map's own `width`, `height`, and channel count (width `bit_depth` = `8`).
