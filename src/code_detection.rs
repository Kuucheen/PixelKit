//! Full-resolution QR code and barcode detection.

use crate::capture::CaptureFrame;
use anyhow::{Result, bail};
use rxing::{
    BarcodeFormat, Binarizer, BinaryBitmap, DecodeHints, Luma8LuminanceSource,
    MultiUseMultiFormatReader, RXingResult, Reader,
    common::{HybridBinarizer, Quadrilateral},
    multi::{
        MultipleBarcodeReader,
        qrcode::{QRCodeMultiReader, detector::MultiFinderPatternFinder},
    },
    qrcode::{QRCodeReader, detector::FinderPatternInfo},
};
use std::collections::HashSet;

const MIN_GENERIC_SCAN_DIMENSION: f32 = 100.0;
const MAX_GENERIC_SCAN_DEPTH: u32 = 4;
const FINDER_PATTERN_MARGIN_MODULES: f32 = 8.0;
const MAX_LOCAL_QR_CANDIDATES: usize = 400;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SourcePoint {
    pub x: f32,
    pub y: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SourceRect {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
}

impl SourceRect {
    pub fn width(self) -> f32 {
        self.right - self.left
    }

    pub fn height(self) -> f32 {
        self.bottom - self.top
    }

    fn intersection_area(self, other: Self) -> f32 {
        let width = self.right.min(other.right) - self.left.max(other.left);
        let height = self.bottom.min(other.bottom) - self.top.max(other.top);
        width.max(0.0) * height.max(0.0)
    }

    fn area(self) -> f32 {
        self.width().max(0.0) * self.height().max(0.0)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct DetectedCode {
    pub text: String,
    pub format: String,
    pub points: Vec<SourcePoint>,
    pub bounds: Option<SourceRect>,
}

/// Converts the retained RGBA capture to one luminance sample per source
/// pixel. Transparent pixels are composited over white, matching how codes in
/// transparent PNG fixtures are normally displayed.
pub fn capture_luma(frame: &CaptureFrame) -> Result<Vec<u8>> {
    let expected = frame.width as usize * frame.height as usize * 4;
    if frame.rgba.len() != expected {
        bail!(
            "capture contains {} RGBA bytes, expected {expected}",
            frame.rgba.len()
        );
    }

    Ok(frame
        .rgba
        .chunks_exact(4)
        .map(|pixel| {
            let alpha = u32::from(pixel[3]);
            let inverse_alpha = 255 - alpha;
            let composite =
                |channel: u8| (u32::from(channel) * alpha + 255 * inverse_alpha + 127) / 255;
            let red = composite(pixel[0]);
            let green = composite(pixel[1]);
            let blue = composite(pixel[2]);
            ((54 * red + 183 * green + 19 * blue + 128) >> 8) as u8
        })
        .collect())
}

pub fn detect_codes(frame: &CaptureFrame) -> Result<Vec<DetectedCode>> {
    detect_codes_in_luma(capture_luma(frame)?, frame.width, frame.height)
}

pub fn detect_codes_in_luma(luma: Vec<u8>, width: u32, height: u32) -> Result<Vec<DetectedCode>> {
    detect_codes_in_luma_with_updates(luma, width, height, |_| {})
}

/// Detects codes and reports a new, de-duplicated snapshot whenever another
/// result becomes available. The callback runs on the caller's thread.
pub fn detect_codes_in_luma_with_updates(
    luma: Vec<u8>,
    width: u32,
    height: u32,
    mut on_update: impl FnMut(&[DetectedCode]),
) -> Result<Vec<DetectedCode>> {
    let expected = width as usize * height as usize;
    if width == 0 || height == 0 || luma.len() != expected {
        bail!(
            "invalid luminance image: {width}×{height} requires {expected} bytes, received {}",
            luma.len()
        );
    }

    let hints = DecodeHints {
        PossibleFormats: Some(supported_formats()),
        TryHarder: Some(true),
        AlsoInverted: Some(true),
        ..DecodeHints::default()
    };
    let qr_luma = luma.clone();
    let geometry_luma = luma.clone();
    let mut progress = DetectionProgress::new(width, height, &geometry_luma, &mut on_update);

    {
        let source = Luma8LuminanceSource::new(luma, width, height)?;
        let mut bitmap = BinaryBitmap::new(HybridBinarizer::new(source));
        let mut reader = MultiUseMultiFormatReader::default();
        decode_generic_regions(&mut reader, &mut bitmap, &hints, &mut progress, 0, 0, 0);
    }

    // The dedicated multi-QR detector can separate tightly grouped QR codes
    // that a generic reader sees as one combined finder-pattern field.
    {
        let source = Luma8LuminanceSource::new(qr_luma, width, height)?;
        let mut bitmap = BinaryBitmap::new(HybridBinarizer::new(source));
        let candidate_regions = local_qr_candidate_regions(&bitmap, &hints, width, height);

        // On a dense screen the multi-reader can combine finder patterns from
        // neighboring thumbnails. Decode the smallest plausible QR regions
        // independently first, while keeping every crop at source resolution.
        let mut local_reader = QRCodeReader;
        for region in candidate_regions {
            let mut cropped = bitmap.crop(
                region.left as usize,
                region.top as usize,
                region.width as usize,
                region.height as usize,
            );
            let Ok(mut result) = local_reader.decode_with_hints(&mut cropped, &hints) else {
                continue;
            };
            offset_result_points(&mut result, region.left, region.top);
            progress.qr_results.push(result);
            progress.publish();
        }

        // Retain the whole-image multi pass for unusually large or overlapping
        // codes and for its structured-append handling.
        let mut multi_reader = QRCodeMultiReader::default();
        if let Ok(results) = multi_reader.decode_multiple_with_hints(&mut bitmap, &hints) {
            for result in results {
                progress.qr_results.push(result);
                progress.publish();
            }
        }
    }

    Ok(progress.finish())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct ScanRegion {
    left: u32,
    top: u32,
    width: u32,
    height: u32,
}

fn local_qr_candidate_regions<B: Binarizer>(
    bitmap: &BinaryBitmap<B>,
    hints: &DecodeHints,
    width: u32,
    height: u32,
) -> Vec<ScanRegion> {
    let Ok(infos) = MultiFinderPatternFinder::new(bitmap.get_black_matrix(), None).findMulti(hints)
    else {
        return Vec::new();
    };

    let mut regions = infos
        .iter()
        .filter_map(|info| finder_pattern_region(info, width, height))
        .collect::<Vec<_>>();
    regions.sort_by_key(|region| {
        (
            u64::from(region.width) * u64::from(region.height),
            region.top,
            region.left,
        )
    });
    let mut seen = HashSet::new();
    regions.retain(|region| seen.insert(*region));
    regions.truncate(MAX_LOCAL_QR_CANDIDATES);
    regions
}

fn finder_pattern_region(info: &FinderPatternInfo, width: u32, height: u32) -> Option<ScanRegion> {
    let bottom_left = rxing::Point::from(info.getBottomLeft());
    let top_left = rxing::Point::from(info.getTopLeft());
    let top_right = rxing::Point::from(info.getTopRight());
    let bottom_right = rxing::Point {
        x: bottom_left.x + top_right.x - top_left.x,
        y: bottom_left.y + top_right.y - top_left.y,
    };
    let points = [bottom_left, top_left, top_right, bottom_right];
    if points
        .iter()
        .any(|point| !point.x.is_finite() || !point.y.is_finite())
    {
        return None;
    }

    let module_size = [
        info.getBottomLeft().getEstimatedModuleSize(),
        info.getTopLeft().getEstimatedModuleSize(),
        info.getTopRight().getEstimatedModuleSize(),
    ]
    .into_iter()
    .sum::<f32>()
        / 3.0;
    if !module_size.is_finite() || module_size <= 0.0 {
        return None;
    }
    let padding = module_size * FINDER_PATTERN_MARGIN_MODULES;
    let left = (points
        .iter()
        .map(|point| point.x)
        .fold(f32::INFINITY, f32::min)
        - padding)
        .floor()
        .clamp(0.0, width as f32) as u32;
    let top = (points
        .iter()
        .map(|point| point.y)
        .fold(f32::INFINITY, f32::min)
        - padding)
        .floor()
        .clamp(0.0, height as f32) as u32;
    let right = (points
        .iter()
        .map(|point| point.x)
        .fold(f32::NEG_INFINITY, f32::max)
        + padding)
        .ceil()
        .clamp(0.0, width as f32) as u32;
    let bottom = (points
        .iter()
        .map(|point| point.y)
        .fold(f32::NEG_INFINITY, f32::max)
        + padding)
        .ceil()
        .clamp(0.0, height as f32) as u32;
    (right > left && bottom > top).then_some(ScanRegion {
        left,
        top,
        width: right - left,
        height: bottom - top,
    })
}

fn offset_result_points(result: &mut RXingResult, x_offset: u32, y_offset: u32) {
    for point in result.getPointsMut() {
        point.x += x_offset as f32;
        point.y += y_offset as f32;
    }
}

struct DetectionProgress<'a, F> {
    width: u32,
    height: u32,
    luma: &'a [u8],
    generic_results: Vec<RXingResult>,
    qr_results: Vec<RXingResult>,
    visible_codes: Vec<DetectedCode>,
    on_update: &'a mut F,
}

impl<F: FnMut(&[DetectedCode])> DetectionProgress<'_, F> {
    fn new<'a>(
        width: u32,
        height: u32,
        luma: &'a [u8],
        on_update: &'a mut F,
    ) -> DetectionProgress<'a, F> {
        DetectionProgress {
            width,
            height,
            luma,
            generic_results: Vec::new(),
            qr_results: Vec::new(),
            visible_codes: Vec::new(),
            on_update,
        }
    }

    fn publish(&mut self) {
        let codes = collect_codes(
            &self.generic_results,
            &self.qr_results,
            self.width,
            self.height,
            self.luma,
        );
        if codes != self.visible_codes {
            self.visible_codes = codes;
            (self.on_update)(&self.visible_codes);
        }
    }

    fn finish(self) -> Vec<DetectedCode> {
        self.visible_codes
    }
}

// This follows rxing/ZXing's GenericMultipleBarcodeReader region recursion,
// with the result publication point exposed between recursive decodes. See
// NOTICE and LICENSES/Apache-2.0.txt for the upstream attribution and license.
fn decode_generic_regions<B: Binarizer, F: FnMut(&[DetectedCode])>(
    reader: &mut MultiUseMultiFormatReader,
    image: &mut BinaryBitmap<B>,
    hints: &DecodeHints,
    progress: &mut DetectionProgress<'_, F>,
    x_offset: u32,
    y_offset: u32,
    depth: u32,
) {
    if depth > MAX_GENERIC_SCAN_DEPTH {
        return;
    }

    let Ok(mut result) = reader.decode_with_hints(image, hints) else {
        return;
    };
    let result_points = result.getPoints().to_vec();
    for point in result.getPointsMut() {
        point.x += x_offset as f32;
        point.y += y_offset as f32;
    }
    progress.generic_results.push(result);
    progress.publish();

    if result_points.is_empty() {
        return;
    }

    let image_width = image.get_width();
    let image_height = image.get_height();
    let (mut min_x, mut max_x, mut min_y, mut max_y) =
        (image_width as f32, 0.0_f32, image_height as f32, 0.0_f32);
    for point in result_points {
        min_x = min_x.min(point.x);
        max_x = max_x.max(point.x);
        min_y = min_y.min(point.y);
        max_y = max_y.max(point.y);
    }

    if min_x > MIN_GENERIC_SCAN_DIMENSION {
        decode_generic_regions(
            reader,
            &mut image.crop(0, 0, min_x as usize, image_height),
            hints,
            progress,
            x_offset,
            y_offset,
            depth + 1,
        );
    }
    if min_y > MIN_GENERIC_SCAN_DIMENSION {
        decode_generic_regions(
            reader,
            &mut image.crop(0, 0, image_width, min_y as usize),
            hints,
            progress,
            x_offset,
            y_offset,
            depth + 1,
        );
    }
    if max_x < image_width as f32 - MIN_GENERIC_SCAN_DIMENSION {
        decode_generic_regions(
            reader,
            &mut image.crop(
                max_x as usize,
                0,
                image_width - max_x as usize,
                image_height,
            ),
            hints,
            progress,
            x_offset + max_x as u32,
            y_offset,
            depth + 1,
        );
    }
    if max_y < image_height as f32 - MIN_GENERIC_SCAN_DIMENSION {
        decode_generic_regions(
            reader,
            &mut image.crop(
                0,
                max_y as usize,
                image_width,
                image_height - max_y as usize,
            ),
            hints,
            progress,
            x_offset,
            y_offset + max_y as u32,
            depth + 1,
        );
    }
}

fn collect_codes(
    generic_results: &[RXingResult],
    qr_results: &[RXingResult],
    width: u32,
    height: u32,
    luma: &[u8],
) -> Vec<DetectedCode> {
    let mut codes = qr_results
        .iter()
        .chain(unique_generic_results(generic_results))
        .map(|result| convert_result(result.clone(), width, height, luma))
        .collect::<Vec<_>>();
    deduplicate(&mut codes);
    codes.sort_by(|left, right| match (left.bounds, right.bounds) {
        (Some(left), Some(right)) => left
            .top
            .total_cmp(&right.top)
            .then_with(|| left.left.total_cmp(&right.left)),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => left.text.cmp(&right.text),
    });
    codes
}

fn unique_generic_results(results: &[RXingResult]) -> Vec<&RXingResult> {
    results
        .iter()
        .enumerate()
        .filter(|(index, result)| {
            !results
                .iter()
                .skip(index + 1)
                .any(|later| raw_results_overlap(result, later))
        })
        .map(|(_, result)| result)
        .collect()
}

fn raw_results_overlap(result: &RXingResult, later: &RXingResult) -> bool {
    if result.getPoints().len() >= 4 {
        let quadrilateral = Quadrilateral::new(
            result.getPoints()[0],
            result.getPoints()[1],
            result.getPoints()[2],
            result.getPoints()[3],
        );
        if later.getPoints().len() >= 4 {
            let later_quadrilateral = Quadrilateral::new(
                later.getPoints()[0],
                later.getPoints()[1],
                later.getPoints()[2],
                later.getPoints()[3],
            );
            Quadrilateral::have_intersecting_bounding_boxes(&quadrilateral, &later_quadrilateral)
        } else {
            later
                .getPoints()
                .iter()
                .any(|point| quadrilateral.is_inside(*point))
        }
    } else if later.getPoints().len() >= 4 {
        let later_quadrilateral = Quadrilateral::new(
            later.getPoints()[0],
            later.getPoints()[1],
            later.getPoints()[2],
            later.getPoints()[3],
        );
        later
            .getPoints()
            .iter()
            .any(|point| later_quadrilateral.is_inside(*point))
    } else {
        later.getText() == result.getText() && later.getBarcodeFormat() == result.getBarcodeFormat()
    }
}

fn supported_formats() -> HashSet<BarcodeFormat> {
    [
        BarcodeFormat::AZTEC,
        BarcodeFormat::CODABAR,
        BarcodeFormat::CODE_39,
        BarcodeFormat::CODE_93,
        BarcodeFormat::CODE_128,
        BarcodeFormat::DATA_MATRIX,
        BarcodeFormat::EAN_8,
        BarcodeFormat::EAN_13,
        BarcodeFormat::ITF,
        BarcodeFormat::MAXICODE,
        BarcodeFormat::MICRO_QR_CODE,
        BarcodeFormat::PDF_417,
        BarcodeFormat::QR_CODE,
        BarcodeFormat::RECTANGULAR_MICRO_QR_CODE,
        BarcodeFormat::RSS_14,
        BarcodeFormat::RSS_EXPANDED,
        BarcodeFormat::TELEPEN,
        BarcodeFormat::UPC_A,
        BarcodeFormat::UPC_E,
    ]
    .into_iter()
    .collect()
}

fn convert_result(result: RXingResult, width: u32, height: u32, luma: &[u8]) -> DetectedCode {
    let format = *result.getBarcodeFormat();
    let points = result
        .getPoints()
        .iter()
        .filter(|point| point.x.is_finite() && point.y.is_finite())
        .map(|point| SourcePoint {
            x: point.x.clamp(0.0, width as f32),
            y: point.y.clamp(0.0, height as f32),
        })
        .collect::<Vec<_>>();
    let bounds = detection_bounds(format, &points, width, height).map(|bounds| {
        if is_linear(format) {
            refine_linear_bounds(bounds, &points, luma, width, height)
        } else {
            bounds
        }
    });
    DetectedCode {
        text: result.getText().to_owned(),
        format: format_label(format).into(),
        points,
        bounds,
    }
}

fn refine_linear_bounds(
    mut bounds: SourceRect,
    points: &[SourcePoint],
    luma: &[u8],
    width: u32,
    height: u32,
) -> SourceRect {
    if points.is_empty() {
        return bounds;
    }
    let (min_x, max_x, min_y, max_y) = point_extents(points);
    if max_x - min_x >= max_y - min_y {
        let row = (points.iter().map(|point| point.y).sum::<f32>() / points.len() as f32)
            .round()
            .clamp(0.0, height.saturating_sub(1) as f32) as u32;
        let reference = row_transition_score(luma, width, row, min_x, max_x);
        if reference >= 6 {
            let threshold = (reference / 3).max(4);
            let (top, bottom) =
                scan_horizontal_bar_extent(luma, width, height, row, min_x, max_x, threshold);
            let padding = ((bottom - top + 1) as f32 * 0.06).clamp(3.0, 14.0);
            bounds.top = (top as f32 - padding).max(0.0);
            bounds.bottom = (bottom as f32 + 1.0 + padding).min(height as f32);
        }
    } else {
        let column = (points.iter().map(|point| point.x).sum::<f32>() / points.len() as f32)
            .round()
            .clamp(0.0, width.saturating_sub(1) as f32) as u32;
        let reference = column_transition_score(luma, width, column, min_y, max_y);
        if reference >= 6 {
            let threshold = (reference / 3).max(4);
            let (left, right) =
                scan_vertical_bar_extent(luma, width, column, min_y, max_y, threshold);
            let padding = ((right - left + 1) as f32 * 0.06).clamp(3.0, 14.0);
            bounds.left = (left as f32 - padding).max(0.0);
            bounds.right = (right as f32 + 1.0 + padding).min(width as f32);
        }
    }
    bounds
}

fn point_extents(points: &[SourcePoint]) -> (f32, f32, f32, f32) {
    points.iter().fold(
        (
            f32::INFINITY,
            f32::NEG_INFINITY,
            f32::INFINITY,
            f32::NEG_INFINITY,
        ),
        |(min_x, max_x, min_y, max_y), point| {
            (
                min_x.min(point.x),
                max_x.max(point.x),
                min_y.min(point.y),
                max_y.max(point.y),
            )
        },
    )
}

fn scan_horizontal_bar_extent(
    luma: &[u8],
    width: u32,
    height: u32,
    row: u32,
    left: f32,
    right: f32,
    threshold: usize,
) -> (u32, u32) {
    let mut top = row;
    let mut misses = 0;
    for candidate in (0..row).rev() {
        if row_transition_score(luma, width, candidate, left, right) >= threshold {
            top = candidate;
            misses = 0;
        } else {
            misses += 1;
            if misses >= 3 {
                break;
            }
        }
    }

    let mut bottom = row;
    misses = 0;
    for candidate in row + 1..height {
        if row_transition_score(luma, width, candidate, left, right) >= threshold {
            bottom = candidate;
            misses = 0;
        } else {
            misses += 1;
            if misses >= 3 {
                break;
            }
        }
    }
    (top, bottom)
}

fn scan_vertical_bar_extent(
    luma: &[u8],
    width: u32,
    column: u32,
    top: f32,
    bottom: f32,
    threshold: usize,
) -> (u32, u32) {
    let mut left = column;
    let mut misses = 0;
    for candidate in (0..column).rev() {
        if column_transition_score(luma, width, candidate, top, bottom) >= threshold {
            left = candidate;
            misses = 0;
        } else {
            misses += 1;
            if misses >= 3 {
                break;
            }
        }
    }

    let mut right = column;
    misses = 0;
    for candidate in column + 1..width {
        if column_transition_score(luma, width, candidate, top, bottom) >= threshold {
            right = candidate;
            misses = 0;
        } else {
            misses += 1;
            if misses >= 3 {
                break;
            }
        }
    }
    (left, right)
}

fn row_transition_score(luma: &[u8], width: u32, row: u32, left: f32, right: f32) -> usize {
    let start = left.floor().max(0.0) as u32;
    let end = right.ceil().min(width.saturating_sub(1) as f32) as u32;
    if start >= end {
        return 0;
    }
    let offset = row as usize * width as usize;
    (start..end)
        .filter(|x| luma[offset + *x as usize].abs_diff(luma[offset + *x as usize + 1]) >= 48)
        .count()
}

fn column_transition_score(luma: &[u8], width: u32, column: u32, top: f32, bottom: f32) -> usize {
    let height = luma.len() as u32 / width;
    let start = top.floor().max(0.0) as u32;
    let end = bottom.ceil().min(height.saturating_sub(1) as f32) as u32;
    if start >= end {
        return 0;
    }
    (start..end)
        .filter(|y| {
            let first = *y as usize * width as usize + column as usize;
            luma[first].abs_diff(luma[first + width as usize]) >= 48
        })
        .count()
}

fn detection_bounds(
    format: BarcodeFormat,
    points: &[SourcePoint],
    width: u32,
    height: u32,
) -> Option<SourceRect> {
    if points.is_empty() {
        return None;
    }

    let mut geometry = points.to_vec();
    if format == BarcodeFormat::QR_CODE && geometry.len() == 3 {
        // Standard QR results are bottom-left, top-left, and top-right finder
        // centers. Reconstruct the fourth corner before calculating the box.
        geometry.push(SourcePoint {
            x: geometry[0].x + geometry[2].x - geometry[1].x,
            y: geometry[0].y + geometry[2].y - geometry[1].y,
        });
    }

    let mut left = f32::INFINITY;
    let mut top = f32::INFINITY;
    let mut right = f32::NEG_INFINITY;
    let mut bottom = f32::NEG_INFINITY;
    for point in geometry {
        left = left.min(point.x);
        top = top.min(point.y);
        right = right.max(point.x);
        bottom = bottom.max(point.y);
    }

    let span_x = right - left;
    let span_y = bottom - top;
    if is_linear(format) {
        let long_span = span_x.max(span_y).max(1.0);
        let long_padding = (long_span * 0.04).clamp(5.0, 32.0);
        let short_half_span = (long_span * 0.20).clamp(22.0, 180.0);
        if span_x >= span_y {
            let center_y = (top + bottom) * 0.5;
            left -= long_padding;
            right += long_padding;
            top = center_y - short_half_span;
            bottom = center_y + short_half_span;
        } else {
            let center_x = (left + right) * 0.5;
            top -= long_padding;
            bottom += long_padding;
            left = center_x - short_half_span;
            right = center_x + short_half_span;
        }
    } else {
        let reference_span = span_x.min(span_y).max(span_x.max(span_y) * 0.5);
        let padding = (reference_span * 0.14).clamp(7.0, 56.0);
        left -= padding;
        top -= padding;
        right += padding;
        bottom += padding;
    }

    Some(SourceRect {
        left: left.clamp(0.0, width as f32),
        top: top.clamp(0.0, height as f32),
        right: right.clamp(0.0, width as f32),
        bottom: bottom.clamp(0.0, height as f32),
    })
}

fn is_linear(format: BarcodeFormat) -> bool {
    matches!(
        format,
        BarcodeFormat::CODABAR
            | BarcodeFormat::CODE_39
            | BarcodeFormat::CODE_93
            | BarcodeFormat::CODE_128
            | BarcodeFormat::EAN_8
            | BarcodeFormat::EAN_13
            | BarcodeFormat::ITF
            | BarcodeFormat::RSS_14
            | BarcodeFormat::RSS_EXPANDED
            | BarcodeFormat::TELEPEN
            | BarcodeFormat::UPC_A
            | BarcodeFormat::UPC_E
    )
}

fn format_label(format: BarcodeFormat) -> &'static str {
    match format {
        BarcodeFormat::AZTEC => "Aztec",
        BarcodeFormat::CODABAR => "Codabar",
        BarcodeFormat::CODE_39 => "Code 39",
        BarcodeFormat::CODE_93 => "Code 93",
        BarcodeFormat::CODE_128 => "Code 128",
        BarcodeFormat::DATA_MATRIX => "Data Matrix",
        BarcodeFormat::EAN_8 => "EAN-8",
        BarcodeFormat::EAN_13 => "EAN-13",
        BarcodeFormat::ITF => "ITF",
        BarcodeFormat::MAXICODE => "MaxiCode",
        BarcodeFormat::MICRO_QR_CODE => "Micro QR",
        BarcodeFormat::PDF_417 => "PDF417",
        BarcodeFormat::QR_CODE => "QR Code",
        BarcodeFormat::RECTANGULAR_MICRO_QR_CODE => "Rectangular Micro QR",
        BarcodeFormat::RSS_14 => "GS1 DataBar",
        BarcodeFormat::RSS_EXPANDED => "GS1 DataBar Expanded",
        BarcodeFormat::TELEPEN => "Telepen",
        BarcodeFormat::UPC_A => "UPC-A",
        BarcodeFormat::UPC_E => "UPC-E",
        _ => "Barcode",
    }
}

fn deduplicate(codes: &mut Vec<DetectedCode>) {
    let mut index = 0;
    while index < codes.len() {
        let duplicate = (0..index).any(|earlier| same_detection(&codes[earlier], &codes[index]));
        if duplicate {
            codes.remove(index);
        } else {
            index += 1;
        }
    }
}

fn same_detection(left: &DetectedCode, right: &DetectedCode) -> bool {
    if left.text != right.text || left.format != right.format {
        return false;
    }
    match (left.bounds, right.bounds) {
        (Some(left), Some(right)) => {
            let intersection = left.intersection_area(right);
            let smaller = left.area().min(right.area());
            smaller > 0.0 && intersection / smaller >= 0.45
        }
        (None, None) => true,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rxing::{MultiFormatWriter, Writer, common::BitMatrix};

    #[test]
    fn transparent_pixels_are_composited_over_white() {
        let frame = CaptureFrame {
            width: 3,
            height: 1,
            rgba: vec![
                0, 0, 0, 255, // opaque black
                255, 255, 255, 255, // opaque white
                0, 0, 0, 0, // transparent black
            ],
            dpi: 96.0,
            backend: crate::capture::CaptureBackend::File,
        };
        assert_eq!(capture_luma(&frame).unwrap(), vec![0, 255, 255]);
    }

    #[test]
    fn qr_bounds_include_the_reconstructed_fourth_corner() {
        let points = [
            SourcePoint { x: 20.0, y: 80.0 },
            SourcePoint { x: 20.0, y: 20.0 },
            SourcePoint { x: 80.0, y: 20.0 },
        ];
        let bounds = detection_bounds(BarcodeFormat::QR_CODE, &points, 120, 120).unwrap();
        assert!(bounds.left < 20.0);
        assert!(bounds.top < 20.0);
        assert!(bounds.right > 80.0);
        assert!(bounds.bottom > 80.0);
    }

    #[test]
    fn detects_multiple_qr_codes_and_a_linear_barcode() {
        const WIDTH: u32 = 760;
        const HEIGHT: u32 = 440;
        let writer = MultiFormatWriter;
        let qr_one = writer
            .encode(
                "https://pixelkit.example/one",
                &BarcodeFormat::QR_CODE,
                160,
                160,
            )
            .unwrap();
        let qr_two = writer
            .encode("second code", &BarcodeFormat::QR_CODE, 160, 160)
            .unwrap();
        let code_128 = writer
            .encode("PIXELKIT-128", &BarcodeFormat::CODE_128, 430, 110)
            .unwrap();

        let mut luma = vec![255; WIDTH as usize * HEIGHT as usize];
        paste_matrix(&mut luma, WIDTH, HEIGHT, &qr_one, 35, 30);
        paste_matrix(&mut luma, WIDTH, HEIGHT, &qr_two, 285, 30);
        paste_matrix(&mut luma, WIDTH, HEIGHT, &code_128, 165, 280);

        let mut updates = Vec::new();
        let codes = detect_codes_in_luma_with_updates(luma, WIDTH, HEIGHT, |codes| {
            updates.push(codes.to_vec());
        })
        .unwrap();
        assert_eq!(updates.last(), Some(&codes));
        assert!(
            updates
                .first()
                .is_some_and(|first| !first.is_empty() && first.len() < codes.len()),
            "expected a partial result before the final {codes:#?}, got {updates:#?}"
        );
        for expected in [
            "https://pixelkit.example/one",
            "second code",
            "PIXELKIT-128",
        ] {
            assert!(
                codes.iter().any(|code| code.text == expected),
                "missing {expected:?} in {codes:#?}"
            );
        }
        assert!(codes.iter().all(|code| code.bounds.is_some()));
        let barcode = codes
            .iter()
            .find(|code| code.text == "PIXELKIT-128")
            .unwrap()
            .bounds
            .unwrap();
        assert!(barcode.top <= 283.0, "{barcode:?}");
        assert!(barcode.bottom >= 389.0, "{barcode:?}");
    }

    #[test]
    fn detects_dense_qr_grid_without_cross_matching_finders() {
        const WIDTH: u32 = 800;
        const HEIGHT: u32 = 400;
        const COLUMNS: u32 = 4;
        const ROWS: u32 = 2;
        const CODE_SIZE: i32 = 150;
        const STEP_X: u32 = 195;
        const STEP_Y: u32 = 195;

        let writer = MultiFormatWriter;
        let mut luma = vec![242; WIDTH as usize * HEIGHT as usize];
        let mut expected = Vec::new();
        for row in 0..ROWS {
            for column in 0..COLUMNS {
                let index = row * COLUMNS + column;
                let text = format!(
                    "https://pixelkit.example/{index}/{}",
                    "x".repeat(index as usize * 2)
                );
                let qr = writer
                    .encode(&text, &BarcodeFormat::QR_CODE, CODE_SIZE, CODE_SIZE)
                    .unwrap();
                paste_matrix(
                    &mut luma,
                    WIDTH,
                    HEIGHT,
                    &qr,
                    15 + column * STEP_X,
                    15 + row * STEP_Y,
                );
                expected.push(text);
            }
        }

        let codes = detect_codes_in_luma(luma, WIDTH, HEIGHT).unwrap();
        for text in expected {
            assert!(
                codes.iter().any(|code| code.text == text),
                "missing {text:?} in {codes:#?}"
            );
        }
    }

    fn paste_matrix(
        target: &mut [u8],
        target_width: u32,
        target_height: u32,
        source: &BitMatrix,
        left: u32,
        top: u32,
    ) {
        assert!(left + source.width() <= target_width);
        assert!(top + source.height() <= target_height);
        for y in 0..source.height() {
            for x in 0..source.width() {
                target[((top + y) * target_width + left + x) as usize] =
                    if source.get(x, y) { 0 } else { 255 };
            }
        }
    }
}
