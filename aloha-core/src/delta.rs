use crate::{
    color::ColorType,
    error::{AlohaError, AlohaResult},
};

#[repr(u8)]
#[derive(Debug, Clone, Copy)]
pub enum DeltaFilter {
    None,
    Left,
    Top,
    Average,
    Paeth,
}

impl TryFrom<u8> for DeltaFilter {
    type Error = AlohaError;

    fn try_from(value: u8) -> AlohaResult<Self> {
        Ok(match value {
            0 => Self::None,
            1 => Self::Left,
            2 => Self::Top,
            3 => Self::Average,
            4 => Self::Paeth,
            _ => Err(AlohaError::DecodingError(format!(
                "invalid delta filter {value}"
            )))?,
        })
    }
}

pub fn delta_encode(
    data: &[u8],
    width: usize,
    height: usize,
    color_type: &ColorType,
    bit_depth: u8,
) -> Vec<u8> {
    if color_type.is_indexed() {
        return data.to_vec();
    }

    let mut out = Vec::with_capacity(data.len() + height);
    let mut prev_row: &[u8] = &[];
    let bpp = (bit_depth as usize / 8) * color_type.channels();

    for y in 0..height {
        let row = &data[y * width * bpp..(y + 1) * width * bpp];
        out.extend_from_slice(&choose_best_filter(row, prev_row, bpp));
        prev_row = row;
    }

    out
}

pub fn delta_decode(
    data: &[u8],
    width: usize,
    height: usize,
    color_type: &ColorType,
    bit_depth: u8,
) -> AlohaResult<Vec<u8>> {
    if color_type.is_indexed() {
        return Ok(data.to_vec());
    }

    let mut out = Vec::with_capacity(data.len() - height);
    let bpp = (bit_depth as usize / 8) * color_type.channels();
    let mut prev = Vec::with_capacity(width * bpp);

    for y in 0..height {
        let filter = DeltaFilter::try_from(data[y * (width * bpp + 1)])?;
        let row = &data[y * (width * bpp + 1) + 1..(y + 1) * (width * bpp + 1)];
        let raw = remove_filter(&filter, row, &prev, bpp);
        out.extend_from_slice(&raw);
        prev = raw;
    }

    Ok(out)
}

fn choose_best_filter(row: &[u8], prev_row: &[u8], bpp: usize) -> Vec<u8> {
    let mut out = vec![0u8; row.len() + 1];
    let mut best_cost = u64::MAX;

    for filter in [
        DeltaFilter::None,
        DeltaFilter::Left,
        DeltaFilter::Top,
        DeltaFilter::Paeth,
    ] {
        let filtered = apply_filter(&filter, row, prev_row, bpp);
        let cost: u64 = filtered[1..]
            .iter()
            .map(|&b| (b as i8).unsigned_abs() as u64)
            .sum();
        if cost < best_cost {
            best_cost = cost;
            out.copy_from_slice(&filtered);
        }
    }

    out
}

fn calc_prediction(filter: &DeltaFilter, a: u8, b: u8, c: u8) -> u8 {
    match filter {
        DeltaFilter::None => 0,
        DeltaFilter::Left => a,
        DeltaFilter::Top => b,
        DeltaFilter::Average => (a + b) / 2,
        DeltaFilter::Paeth => {
            let p = a as i16 + b as i16 - c as i16;
            let pa = (p - a as i16).abs();
            let pb = (p - b as i16).abs();
            let pc = (p - c as i16).abs();

            if pa <= pb && pa <= pc {
                a
            } else if pb <= pc {
                b
            } else {
                c
            }
        }
    }
}

fn apply_filter(filter: &DeltaFilter, row: &[u8], prev_row: &[u8], bpp: usize) -> Vec<u8> {
    let mut out = vec![0u8; row.len() + 1];
    out[0] = *filter as u8;

    for i in 0..row.len() {
        let a = if i >= bpp { row[i - bpp] } else { 0 };
        let b = if !prev_row.is_empty() { prev_row[i] } else { 0 };
        let c = if i >= bpp && !prev_row.is_empty() {
            prev_row[i - bpp]
        } else {
            0
        };

        out[i + 1] = row[i].wrapping_sub(calc_prediction(filter, a, b, c));
    }

    out
}

fn remove_filter(filter: &DeltaFilter, row: &[u8], prev_row: &[u8], bpp: usize) -> Vec<u8> {
    let mut out = vec![0u8; row.len()];

    for i in 0..row.len() {
        let a = if i >= bpp { out[i - bpp] } else { 0 };
        let b = if !prev_row.is_empty() { prev_row[i] } else { 0 };
        let c = if i >= bpp && !prev_row.is_empty() {
            prev_row[i - bpp]
        } else {
            0
        };

        out[i] = row[i].wrapping_add(calc_prediction(filter, a, b, c));
    }

    out
}
