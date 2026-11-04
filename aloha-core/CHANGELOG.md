# Changelog

## 0.7.0

**Format Version:** 0 \
**Date:** 2026-11-04

### Changes

Added a method to reduce the bit depth of `AlohaImage` from 16 to 8.
Moved the read/write functions in `AlohaImage`.
`AlohaImage` is now non-exhaustive.
Fixed some bugs.

## 0.6.1

**Format Version:** 0 \
**Date:** 2026-11-03

### Changes

Added a method to discard alpha-channel.

## 0.6.0

**Format Version:** 0 \
**Date:** 2026-11-02

### Changes

Added `IndexedAlpha` color type.

## 0.5.0

**Format Version:** 0 \
**Date:** 2026-10-31

### Changes

Metadata is now non-exhaustive.

## 0.4.0

**Format Version:** 0 \
**Date:** 2026-10-31

### Changes

Added HDR support.

## 0.3.0

**Format Version:** 0 \
**Date:** 2026-10-27

### Changes

Now CRC32 covers the entire file.

## 0.2.0

**Format Version:** 0 \
**Date:** 2026-10-25

### Changes

Added support for ICC profiles.

## 0.1.3

**Format Version:** 0 \
**Date:** 2026-10-15

### Changes

Now, fields in the metadata with a value of None are not serialized.

## 0.1.2

**Format Version:** 0 \
**Date:** 2026-10-13

### Changes

Fixed the issue of ignoring a compression algorithm when reading ALOHA file.

## 0.1.1

**Format Version:** 0 \
**Date:** 2026-10-13

### Changes

Removed big endians from the methods for reading and writing samples.

## 0.1.0

**Format Version:** 0 \
**Date:** 2026-10-11

### Changes

Initial specification matching implementation.
