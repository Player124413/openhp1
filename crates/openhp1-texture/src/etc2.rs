//! Fast, high-quality ETC2 (Ericsson Texture Compression 2) and EAC encoder.
//!
//! ETC2 compresses 4x4 texel blocks into 64-bit RGB blocks (`Etc2Rgb8Unorm`) or
//! 128-bit RGBA blocks (`Etc2Rgba8Unorm`), delivering an 8:1 compression ratio
//! for RGB and 4:1 for RGBA. This massively reduces GPU memory footprint and
//! memory bus bandwidth on mobile GPUs (Vulkan / OpenGL ES 3.0 on Android).

/// Standard 8 modifier tables for ETC1 / ETC2 RGB compression.
/// Each table has 4 intensity offsets: [small_pos, large_pos, small_neg, large_neg].
pub const ETC1_MODIFIER_TABLES: [[i16; 4]; 8] = [
    [2, 8, -2, -8],
    [5, 17, -5, -17],
    [9, 29, -9, -29],
    [13, 42, -13, -42],
    [18, 60, -18, -60],
    [24, 80, -24, -80],
    [33, 106, -33, -106],
    [47, 183, -47, -183],
];

/// 16 modifier tables for EAC (Ericsson Alpha Compression), 8 modifiers each.
pub const EAC_MODIFIER_TABLES: [[i16; 8]; 16] = [
    [-3, -6, -9, -15, 2, 5, 8, 14],
    [-3, -7, -10, -13, 2, 6, 9, 12],
    [-2, -5, -8, -13, 1, 4, 7, 12],
    [-2, -4, -6, -13, 1, 3, 5, 12],
    [-3, -6, -8, -12, 2, 5, 7, 11],
    [-3, -7, -9, -11, 2, 6, 8, 10],
    [-4, -7, -8, -11, 3, 6, 7, 10],
    [-3, -5, -8, -11, 2, 4, 7, 10],
    [-2, -6, -8, -10, 1, 5, 7, 9],
    [-2, -5, -8, -10, 1, 4, 7, 9],
    [-2, -4, -8, -10, 1, 3, 7, 9],
    [-2, -5, -7, -10, 1, 4, 6, 9],
    [-3, -4, -7, -10, 2, 3, 6, 9],
    [-1, -2, -3, -10, 0, 1, 2, 9],
    [-4, -6, -8, -9, 3, 5, 7, 8],
    [-3, -5, -7, -9, 2, 4, 6, 8],
];

#[inline]
fn clamp_u8(value: i32) -> u8 {
    value.clamp(0, 255) as u8
}

#[inline]
fn color_dist_sq(r1: i32, g1: i32, b1: i32, r2: i32, g2: i32, b2: i32) -> u64 {
    let dr = (r1 - r2) as i64;
    let dg = (g1 - g2) as i64;
    let db = (b1 - b2) as i64;
    (dr * dr + dg * dg + db * db) as u64
}

/// Extends a 5-bit color channel to 8-bit using standard ETC bit replication:
/// `(c << 3) | (c >> 2)`.
#[inline]
fn extend_5_to_8(val: u8) -> u8 {
    (val << 3) | (val >> 2)
}

/// Extends a 4-bit color channel to 8-bit: `(c << 4) | c`.
#[inline]
fn extend_4_to_8(val: u8) -> u8 {
    (val << 4) | val
}

/// Quantizes an 8-bit color channel to 5-bit: `(val * 31 + 127) / 255`.
#[inline]
fn quantize_8_to_5(val: u8) -> u8 {
    ((val as u32 * 31 + 127) / 255) as u8
}

/// Quantizes an 8-bit color channel to 4-bit: `(val * 15 + 127) / 255`.
#[inline]
fn quantize_8_to_4(val: u8) -> u8 {
    ((val as u32 * 15 + 127) / 255) as u8
}

/// Compresses a 4x4 block of RGBA pixels into an 8-byte ETC2 RGB block.
pub fn compress_etc2_rgb_block(block: &[[u8; 4]; 16]) -> [u8; 8] {
    let (err_diff, bytes_diff) = compress_sub_blocks(block, true);
    let (err_ind, bytes_ind) = compress_sub_blocks(block, false);

    if err_diff <= err_ind {
        bytes_diff
    } else {
        bytes_ind
    }
}

fn compress_sub_blocks(block: &[[u8; 4]; 16], allow_differential: bool) -> (u64, [u8; 8]) {
    // Test both horizontal (flip = 1) and vertical (flip = 0) partitions
    let (err_v, bytes_v) = compress_partition(block, false, allow_differential);
    let (err_h, bytes_h) = compress_partition(block, true, allow_differential);

    if err_v <= err_h {
        (err_v, bytes_v)
    } else {
        (err_h, bytes_h)
    }
}

fn compress_partition(
    block: &[[u8; 4]; 16],
    flip: bool,
    allow_differential: bool,
) -> (u64, [u8; 8]) {
    // In ETC spec:
    // pixel i = x * 4 + y where x in 0..3 (column), y in 0..3 (row).
    // If flip == false (vertical split):
    //   Sub-block 0: x in 0..1 (pixels 0..7)
    //   Sub-block 1: x in 2..3 (pixels 8..15)
    // If flip == true (horizontal split):
    //   Sub-block 0: y in 0..1 (pixels where y < 2)
    //   Sub-block 1: y in 2..3 (pixels where y >= 2)
    let is_sub_block_0 = |i: usize| -> bool {
        if flip {
            (i % 4) < 2
        } else {
            i < 8
        }
    };

    let mut sum0 = [0u32; 3];
    let mut sum1 = [0u32; 3];
    for (i, px) in block.iter().enumerate() {
        if is_sub_block_0(i) {
            sum0[0] += px[0] as u32;
            sum0[1] += px[1] as u32;
            sum0[2] += px[2] as u32;
        } else {
            sum1[0] += px[0] as u32;
            sum1[1] += px[1] as u32;
            sum1[2] += px[2] as u32;
        }
    }

    let avg0 = [
        (sum0[0] / 8) as u8,
        (sum0[1] / 8) as u8,
        (sum0[2] / 8) as u8,
    ];
    let avg1 = [
        (sum1[0] / 8) as u8,
        (sum1[1] / 8) as u8,
        (sum1[2] / 8) as u8,
    ];

    if allow_differential {
        let r0_5 = quantize_8_to_5(avg0[0]);
        let g0_5 = quantize_8_to_5(avg0[1]);
        let b0_5 = quantize_8_to_5(avg0[2]);

        let r1_5 = quantize_8_to_5(avg1[0]);
        let g1_5 = quantize_8_to_5(avg1[1]);
        let b1_5 = quantize_8_to_5(avg1[2]);

        let dr = r1_5 as i32 - r0_5 as i32;
        let dg = g1_5 as i32 - g0_5 as i32;
        let db = b1_5 as i32 - b0_5 as i32;

        if (-4..=3).contains(&dr) && (-4..=3).contains(&dg) && (-4..=3).contains(&db) {
            let base0 = [
                extend_5_to_8(r0_5),
                extend_5_to_8(g0_5),
                extend_5_to_8(b0_5),
            ];
            let base1 = [
                extend_5_to_8(r1_5),
                extend_5_to_8(g1_5),
                extend_5_to_8(b1_5),
            ];

            let (err0, table0, indices0) = find_best_table_indices(block, &base0, &is_sub_block_0, true);
            let (err1, table1, indices1) = find_best_table_indices(block, &base1, &is_sub_block_0, false);

            let mut out = [0u8; 8];
            out[0] = (r0_5 << 3) | ((dr as u8) & 0x07);
            out[1] = (g0_5 << 3) | ((dg as u8) & 0x07);
            out[2] = (b0_5 << 3) | ((db as u8) & 0x07);
            out[3] = (table0 << 5) | (table1 << 2) | (1 << 1) | (u8::from(flip));

            pack_indices(&mut out, block, &is_sub_block_0, &indices0, &indices1);
            return (err0 + err1, out);
        }
    }

    // Individual mode (4-bit colors per subblock)
    let r0_4 = quantize_8_to_4(avg0[0]);
    let g0_4 = quantize_8_to_4(avg0[1]);
    let b0_4 = quantize_8_to_4(avg0[2]);

    let r1_4 = quantize_8_to_4(avg1[0]);
    let g1_4 = quantize_8_to_4(avg1[1]);
    let b1_4 = quantize_8_to_4(avg1[2]);

    let base0 = [
        extend_4_to_8(r0_4),
        extend_4_to_8(g0_4),
        extend_4_to_8(b0_4),
    ];
    let base1 = [
        extend_4_to_8(r1_4),
        extend_4_to_8(g1_4),
        extend_4_to_8(b1_4),
    ];

    let (err0, table0, indices0) = find_best_table_indices(block, &base0, &is_sub_block_0, true);
    let (err1, table1, indices1) = find_best_table_indices(block, &base1, &is_sub_block_0, false);

    let mut out = [0u8; 8];
    out[0] = (r0_4 << 4) | r1_4;
    out[1] = (g0_4 << 4) | g1_4;
    out[2] = (b0_4 << 4) | b1_4;
    out[3] = (table0 << 5) | (table1 << 2) | (u8::from(flip));

    pack_indices(&mut out, block, &is_sub_block_0, &indices0, &indices1);
    (err0 + err1, out)
}

fn find_best_table_indices(
    block: &[[u8; 4]; 16],
    base_color: &[u8; 3],
    is_sub_block_0: &impl Fn(usize) -> bool,
    target_sub_block_0: bool,
) -> (u64, u8, [u8; 16]) {
    let mut best_error = u64::MAX;
    let mut best_table = 0u8;
    let mut best_indices = [0u8; 16];

    for (table_idx, table) in ETC1_MODIFIER_TABLES.iter().enumerate() {
        let mut total_error = 0u64;
        let mut indices = [0u8; 16];

        for (i, px) in block.iter().enumerate() {
            if is_sub_block_0(i) != target_sub_block_0 {
                continue;
            }

            let mut min_px_error = u64::MAX;
            let mut best_mod_idx = 0u8;

            for (mod_idx, &mod_val) in table.iter().enumerate() {
                let cr = clamp_u8(base_color[0] as i32 + mod_val as i32);
                let cg = clamp_u8(base_color[1] as i32 + mod_val as i32);
                let cb = clamp_u8(base_color[2] as i32 + mod_val as i32);

                let err = color_dist_sq(
                    px[0] as i32,
                    px[1] as i32,
                    px[2] as i32,
                    cr as i32,
                    cg as i32,
                    cb as i32,
                );

                if err < min_px_error {
                    min_px_error = err;
                    best_mod_idx = mod_idx as u8;
                }
            }

            total_error += min_px_error;
            indices[i] = best_mod_idx;
        }

        if total_error < best_error {
            best_error = total_error;
            best_table = table_idx as u8;
            best_indices = indices;
        }
    }

    (best_error, best_table, best_indices)
}

fn pack_indices(
    out: &mut [u8; 8],
    _block: &[[u8; 4]; 16],
    is_sub_block_0: &impl Fn(usize) -> bool,
    indices0: &[u8; 16],
    indices1: &[u8; 16],
) {
    let mut msb_mask = 0u16;
    let mut lsb_mask = 0u16;

    for i in 0..16 {
        let mod_idx = if is_sub_block_0(i) {
            indices0[i]
        } else {
            indices1[i]
        };

        // mod_idx:
        // 0: [0, 0] -> msb=0, lsb=0 (+small)
        // 1: [0, 1] -> msb=0, lsb=1 (+large)
        // 2: [1, 0] -> msb=1, lsb=0 (-small)
        // 3: [1, 1] -> msb=1, lsb=1 (-large)
        let msb = (mod_idx >> 1) & 1;
        let lsb = mod_idx & 1;

        if msb != 0 {
            msb_mask |= 1 << i;
        }
        if lsb != 0 {
            lsb_mask |= 1 << i;
        }
    }

    out[4] = (msb_mask & 0xFF) as u8;
    out[5] = ((msb_mask >> 8) & 0xFF) as u8;
    out[6] = (lsb_mask & 0xFF) as u8;
    out[7] = ((lsb_mask >> 8) & 0xFF) as u8;
}

/// Compresses 16 alpha values into an 8-byte EAC alpha block.
pub fn compress_eac_alpha_block(block: &[[u8; 4]; 16]) -> [u8; 8] {
    let mut min_a = 255u8;
    let mut max_a = 0u8;
    for px in block {
        min_a = min_a.min(px[3]);
        max_a = max_a.max(px[3]);
    }

    // Fast path: solid alpha (opaque or transparent)
    if min_a == max_a {
        return [min_a, 0, 0, 0, 0, 0, 0, 0];
    }

    let mut best_error = u64::MAX;
    let mut best_base = min_a;
    let mut best_multiplier = 1u8;
    let mut best_table = 0u8;
    let mut best_indices = [0u8; 16];

    let base_candidates = [min_a, ((min_a as u32 + max_a as u32) / 2) as u8, max_a];

    for &base in &base_candidates {
        for mult in 1..=15 {
            for (table_idx, table) in EAC_MODIFIER_TABLES.iter().enumerate() {
                let mut total_error = 0u64;
                let mut indices = [0u8; 16];

                for (i, px) in block.iter().enumerate() {
                    let a = px[3] as i32;
                    let mut min_diff = u64::MAX;
                    let mut best_idx = 0u8;

                    for (mod_idx, &mod_val) in table.iter().enumerate() {
                        let candidate = clamp_u8(base as i32 + mult as i32 * mod_val as i32) as i32;
                        let diff = ((a - candidate) * (a - candidate)) as u64;
                        if diff < min_diff {
                            min_diff = diff;
                            best_idx = mod_idx as u8;
                        }
                    }

                    total_error += min_diff;
                    indices[i] = best_idx;
                }

                if total_error < best_error {
                    best_error = total_error;
                    best_base = base;
                    best_multiplier = mult;
                    best_table = table_idx as u8;
                    best_indices = indices;
                }
            }
        }
    }

    let mut out = [0u8; 8];
    out[0] = best_base;
    out[1] = (best_multiplier << 4) | (best_table & 0x0F);

    // Pack 16 x 3-bit indices into 48 bits (bytes 2..7)
    // Pixel 0 is at bits 47..45 (top bits of byte 2)
    let mut bit_buf = 0u64;
    for &idx in &best_indices {
        bit_buf = (bit_buf << 3) | ((idx & 0x07) as u64);
    }

    out[2] = ((bit_buf >> 40) & 0xFF) as u8;
    out[3] = ((bit_buf >> 32) & 0xFF) as u8;
    out[4] = ((bit_buf >> 24) & 0xFF) as u8;
    out[5] = ((bit_buf >> 16) & 0xFF) as u8;
    out[6] = ((bit_buf >> 8) & 0xFF) as u8;
    out[7] = (bit_buf & 0xFF) as u8;

    out
}

/// Compresses a 4x4 block of RGBA pixels into a 16-byte ETC2 RGBA block.
pub fn compress_etc2_rgba_block(block: &[[u8; 4]; 16]) -> [u8; 16] {
    let alpha = compress_eac_alpha_block(block);
    let rgb = compress_etc2_rgb_block(block);

    let mut out = [0u8; 16];
    out[0..8].copy_from_slice(&alpha);
    out[8..16].copy_from_slice(&rgb);
    out
}

/// Compresses full-image RGBA8 pixels into ETC2 RGB format (`Etc2Rgb8Unorm`).
/// Returns compressed block data (8 bytes per 4x4 block).
pub fn compress_etc2_rgb(width: u32, height: u32, rgba: &[u8]) -> Vec<u8> {
    let blocks_x = (width + 3) / 4;
    let blocks_y = (height + 3) / 4;
    let mut result = Vec::with_capacity((blocks_x * blocks_y * 8) as usize);

    let mut block = [[0u8; 4]; 16];

    for by in 0..blocks_y {
        for bx in 0..blocks_x {
            // Fill 4x4 block
            for col in 0..4 {
                for row in 0..4 {
                    let px = (bx * 4 + col).min(width.saturating_sub(1));
                    let py = (by * 4 + row).min(height.saturating_sub(1));
                    let offset = ((py * width + px) * 4) as usize;

                    let target_idx = (col * 4 + row) as usize;
                    if offset + 4 <= rgba.len() {
                        block[target_idx] = [
                            rgba[offset],
                            rgba[offset + 1],
                            rgba[offset + 2],
                            rgba[offset + 3],
                        ];
                    } else {
                        block[target_idx] = [0, 0, 0, 255];
                    }
                }
            }

            let compressed = compress_etc2_rgb_block(&block);
            result.extend_from_slice(&compressed);
        }
    }

    result
}

/// Compresses full-image RGBA8 pixels into ETC2 RGBA format (`Etc2Rgba8Unorm`).
/// Returns compressed block data (16 bytes per 4x4 block).
pub fn compress_etc2_rgba(width: u32, height: u32, rgba: &[u8]) -> Vec<u8> {
    let blocks_x = (width + 3) / 4;
    let blocks_y = (height + 3) / 4;
    let mut result = Vec::with_capacity((blocks_x * blocks_y * 16) as usize);

    let mut block = [[0u8; 4]; 16];

    for by in 0..blocks_y {
        for bx in 0..blocks_x {
            for col in 0..4 {
                for row in 0..4 {
                    let px = (bx * 4 + col).min(width.saturating_sub(1));
                    let py = (by * 4 + row).min(height.saturating_sub(1));
                    let offset = ((py * width + px) * 4) as usize;

                    let target_idx = (col * 4 + row) as usize;
                    if offset + 4 <= rgba.len() {
                        block[target_idx] = [
                            rgba[offset],
                            rgba[offset + 1],
                            rgba[offset + 2],
                            rgba[offset + 3],
                        ];
                    } else {
                        block[target_idx] = [0, 0, 0, 255];
                    }
                }
            }

            let compressed = compress_etc2_rgba_block(&block);
            result.extend_from_slice(&compressed);
        }
    }

    result
}

/// Decompresses an 8-byte ETC1 / ETC2 RGB block into 16 RGB pixels.
pub fn decompress_etc2_rgb_block(block: &[u8; 8]) -> [[u8; 3]; 16] {
    let diff = (block[3] & 0x02) != 0;
    let flip = (block[3] & 0x01) != 0;
    let table0 = ((block[3] >> 5) & 0x07) as usize;
    let table1 = ((block[3] >> 2) & 0x07) as usize;

    let (base0, base1) = if diff {
        let r0_5 = (block[0] >> 3) & 0x1F;
        let g0_5 = (block[1] >> 3) & 0x1F;
        let b0_5 = (block[2] >> 3) & 0x1F;

        // Signed 3-bit: 3 bits two's complement (-4..=3)
        let unpack_diff = |val: u8| -> i32 {
            let s = (val & 0x07) as i32;
            if (s & 0x04) != 0 { s - 8 } else { s }
        };

        let dr = unpack_diff(block[0]);
        let dg = unpack_diff(block[1]);
        let db = unpack_diff(block[2]);

        let r1_5 = (r0_5 as i32 + dr).clamp(0, 31) as u8;
        let g1_5 = (g0_5 as i32 + dg).clamp(0, 31) as u8;
        let b1_5 = (b0_5 as i32 + db).clamp(0, 31) as u8;

        (
            [extend_5_to_8(r0_5), extend_5_to_8(g0_5), extend_5_to_8(b0_5)],
            [extend_5_to_8(r1_5), extend_5_to_8(g1_5), extend_5_to_8(b1_5)],
        )
    } else {
        let r0_4 = (block[0] >> 4) & 0x0F;
        let r1_4 = block[0] & 0x0F;
        let g0_4 = (block[1] >> 4) & 0x0F;
        let g1_4 = block[1] & 0x0F;
        let b0_4 = (block[2] >> 4) & 0x0F;
        let b1_4 = block[2] & 0x0F;

        (
            [extend_4_to_8(r0_4), extend_4_to_8(g0_4), extend_4_to_8(b0_4)],
            [extend_4_to_8(r1_4), extend_4_to_8(g1_4), extend_4_to_8(b1_4)],
        )
    };

    let msb_mask = (block[4] as u16) | ((block[5] as u16) << 8);
    let lsb_mask = (block[6] as u16) | ((block[7] as u16) << 8);

    let mut out = [[0u8; 3]; 16];

    for i in 0..16 {
        let is_sub0 = if flip { (i % 4) < 2 } else { i < 8 };
        let (base, table_idx) = if is_sub0 {
            (&base0, table0)
        } else {
            (&base1, table1)
        };

        let msb = ((msb_mask >> i) & 1) as usize;
        let lsb = ((lsb_mask >> i) & 1) as usize;
        let mod_idx = (msb << 1) | lsb;

        let mod_val = ETC1_MODIFIER_TABLES[table_idx][mod_idx];

        out[i] = [
            clamp_u8(base[0] as i32 + mod_val as i32),
            clamp_u8(base[1] as i32 + mod_val as i32),
            clamp_u8(base[2] as i32 + mod_val as i32),
        ];
    }

    out
}

/// Decompresses an 8-byte EAC alpha block into 16 alpha values.
pub fn decompress_eac_alpha_block(block: &[u8; 8]) -> [u8; 16] {
    let base = block[0] as i32;
    let mult = ((block[1] >> 4) & 0x0F) as i32;
    let table_idx = (block[1] & 0x0F) as usize;

    let bit_buf = ((block[2] as u64) << 40)
        | ((block[3] as u64) << 32)
        | ((block[4] as u64) << 24)
        | ((block[5] as u64) << 16)
        | ((block[6] as u64) << 8)
        | (block[7] as u64);

    let mut out = [0u8; 16];
    for (i, item) in out.iter_mut().enumerate() {
        let shift = (15 - i) * 3;
        let mod_idx = ((bit_buf >> shift) & 0x07) as usize;
        let mod_val = EAC_MODIFIER_TABLES[table_idx][mod_idx] as i32;
        *item = clamp_u8(base + mult * mod_val);
    }

    out
}

/// Decompresses a 16-byte ETC2 RGBA block into 16 RGBA pixels.
pub fn decompress_etc2_rgba_block(block: &[u8; 16]) -> [[u8; 4]; 16] {
    let mut alpha_block = [0u8; 8];
    alpha_block.copy_from_slice(&block[0..8]);
    let alphas = decompress_eac_alpha_block(&alpha_block);

    let mut rgb_block = [0u8; 8];
    rgb_block.copy_from_slice(&block[8..16]);
    let rgbs = decompress_etc2_rgb_block(&rgb_block);

    let mut out = [[0u8; 4]; 16];
    for i in 0..16 {
        out[i] = [rgbs[i][0], rgbs[i][1], rgbs[i][2], alphas[i]];
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compresses_and_decompresses_solid_color() {
        let solid = [[120, 150, 180, 255]; 16];
        let compressed = compress_etc2_rgb_block(&solid);
        let decompressed = decompress_etc2_rgb_block(&compressed);

        for px in decompressed {
            assert!((px[0] as i32 - 120).abs() <= 8);
            assert!((px[1] as i32 - 150).abs() <= 8);
            assert!((px[2] as i32 - 180).abs() <= 8);
        }
    }

    #[test]
    fn compresses_solid_alpha_losslessly() {
        let solid_alpha = [[0, 0, 0, 255]; 16];
        let compressed = compress_eac_alpha_block(&solid_alpha);
        let decompressed = decompress_eac_alpha_block(&compressed);
        assert_eq!(decompressed, [255; 16]);

        let solid_semi = [[0, 0, 0, 128]; 16];
        let compressed_semi = compress_eac_alpha_block(&solid_semi);
        let decompressed_semi = decompress_eac_alpha_block(&compressed_semi);
        assert_eq!(decompressed_semi, [128; 16]);
    }

    #[test]
    fn compresses_full_texture_dimensions() {
        let width = 8;
        let height = 8;
        let rgba = vec![200u8; width * height * 4];

        let compressed_rgb = compress_etc2_rgb(width as u32, height as u32, &rgba);
        // (8/4) * (8/4) * 8 = 4 blocks * 8 bytes = 32 bytes
        assert_eq!(compressed_rgb.len(), 32);

        let compressed_rgba = compress_etc2_rgba(width as u32, height as u32, &rgba);
        // 4 blocks * 16 bytes = 64 bytes
        assert_eq!(compressed_rgba.len(), 64);
    }

    #[test]
    fn handles_odd_texture_dimensions() {
        let width = 5;
        let height = 7;
        let rgba = vec![100u8; width * height * 4];

        // blocks_x = (5 + 3) / 4 = 2; blocks_y = (7 + 3) / 4 = 2
        // total blocks = 4
        let compressed = compress_etc2_rgba(width as u32, height as u32, &rgba);
        assert_eq!(compressed.len(), 4 * 16);
    }
}
