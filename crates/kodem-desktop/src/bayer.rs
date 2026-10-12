//! The ordered-dither threshold shared by every dithered texture and reveal in the workspace.

const BAYER: [[u8; 8]; 8] = [
    [0, 32, 8, 40, 2, 34, 10, 42],
    [48, 16, 56, 24, 50, 18, 58, 26],
    [12, 44, 4, 36, 14, 46, 6, 38],
    [60, 28, 52, 20, 62, 30, 54, 22],
    [3, 35, 11, 43, 1, 33, 9, 41],
    [51, 19, 59, 27, 49, 17, 57, 25],
    [15, 47, 7, 39, 13, 45, 5, 37],
    [63, 31, 55, 23, 61, 29, 53, 21],
];

/// Returns the threshold of a grid cell in the open interval (0, 1). A cell is lit when the
/// density at it is above this value, so a rising density lights cells in Bayer order.
pub fn threshold(x: i64, y: i64) -> f32 {
    (BAYER[y.rem_euclid(8) as usize][x.rem_euclid(8) as usize] as f32 + 0.5) / 64.
}
