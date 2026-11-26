# Changelog

## 0.13.0

**Format Version:** 0 \
**Date:** 2026-11-26

Now CICP and ICC do not exclude each other.
Changed header's fields.

## 0.12.0

**Format Version:** 0 \
**Date:** 2026-11-23

Added CMYK color type.

## 0.11.1

**Format Version:** 0 \
**Date:** 2026-11-23

Gain map is now compressed with zstd.

## 0.11.0

**Format Version:** 0 \
**Date:** 2026-11-22

Now pixel data is stored in the planar format.

## 0.10.1

**Format Version:** 0 \
**Date:** 2026-11-20

I forgot to convert the data to little-endian. Now everything is correct.

## 0.10.0

**Format Version:** 0 \
**Date:** 2026-11-18

Now the CRC is computed for each section.

## 0.9.0

**Format Version:** 0 \
**Date:** 2026-11-12

Added adaptive delta filtering.
Added zstd compression variant.
Removed some error variants.

## 0.8.0

**Format Version:** 0 \
**Date:** 2026-11-08

Metadata is now serialized in XML and contains fields that comply with the XMP Dublin Core.

## 0.7.0

**Format Version:** 0 \
**Date:** 2026-11-04

Added a method to reduce the bit depth of `AlohaImage` from 16 to 8.
Moved the read/write functions in `AlohaImage`.
`AlohaImage` is now non-exhaustive.
Fixed some bugs.

## 0.6.1

**Format Version:** 0 \
**Date:** 2026-11-03

Added a method to discard alpha-channel.

## 0.6.0

**Format Version:** 0 \
**Date:** 2026-11-02

Added `IndexedAlpha` color type.

## 0.5.0

**Format Version:** 0 \
**Date:** 2026-10-31

Metadata is now non-exhaustive.

## 0.4.0

**Format Version:** 0 \
**Date:** 2026-10-31

Added HDR support.

## 0.3.0

**Format Version:** 0 \
**Date:** 2026-10-27

Now CRC32 covers the entire file.

## 0.2.0

**Format Version:** 0 \
**Date:** 2026-10-25

Added support for ICC profiles.

## 0.1.3

**Format Version:** 0 \
**Date:** 2026-10-15

Now, fields in the metadata with a value of None are not serialized.

## 0.1.2

**Format Version:** 0 \
**Date:** 2026-10-13

Fixed the issue of ignoring a compression algorithm when reading ALOHA file.

## 0.1.1

**Format Version:** 0 \
**Date:** 2026-10-13

Removed big endians from the methods for reading and writing samples.

## 0.1.0

**Format Version:** 0 \
**Date:** 2026-10-11

Initial specification matching implementation.
