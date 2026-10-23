use pxl_core::{IndexFrame, Palette, PxlError, PxlResult, Rgba};

use crate::raster::scale_nearest;

/// Looping GIF with one fixed delay (centiseconds) for every frame.
pub fn encode_gif(
    frames: &[IndexFrame],
    palette: &Palette,
    scale: u32,
    delay_cs: u16,
) -> PxlResult<Vec<u8>> {
    let delays = vec![delay_cs; frames.len()];
    encode_gif_timed(frames, palette, scale, &delays, true)
}

/// Per-frame delays in centiseconds that add up to the nearest centisecond of
/// `n / fps` seconds. GIF cannot store 1/fps exactly, so the rounding error is
/// spread over the frames instead of drifting by `100 / fps` truncation.
pub fn gif_delays_cs(fps: u32, n: usize) -> Vec<u16> {
    let fps = fps.max(1) as u64;
    let edge = |i: u64| (i * 100 + fps / 2) / fps;
    (0..n as u64)
        .map(|i| (edge(i + 1) - edge(i)).clamp(1, u16::MAX as u64) as u16)
        .collect()
}

/// GIF with a delay per frame. `looped = false` omits the NETSCAPE loop block,
/// which players treat as "play once".
pub fn encode_gif_timed(
    frames: &[IndexFrame],
    palette: &Palette,
    scale: u32,
    delays_cs: &[u16],
    looped: bool,
) -> PxlResult<Vec<u8>> {
    if frames.is_empty() {
        return Err(PxlError::semantic("gif has no frames"));
    }
    if delays_cs.len() != frames.len() {
        return Err(PxlError::semantic(format!(
            "gif has {} frames but {} delays",
            frames.len(),
            delays_cs.len()
        )));
    }
    let first = scale_nearest(&frames[0], scale.max(1))?;
    let w = first.w;
    let h = first.h;
    for frame in frames {
        if frame.w != frames[0].w || frame.h != frames[0].h {
            return Err(PxlError::semantic("gif frames must share a size"));
        }
    }
    let global = gif_palette(&palette.colors);
    let mut buf = Vec::new();
    {
        let mut encoder = gif::Encoder::new(&mut buf, w, h, &global)
            .map_err(|e| PxlError::io(format!("gif header: {e}")))?;
        if looped {
            encoder
                .set_repeat(gif::Repeat::Infinite)
                .map_err(|e| PxlError::io(format!("gif repeat: {e}")))?;
        }
        for (frame, delay) in frames.iter().zip(delays_cs) {
            let image = scale_nearest(frame, scale.max(1))?;
            let mut gf = gif::Frame::from_indexed_pixels(w, h, image.pixels.clone(), Some(0));
            gf.delay = (*delay).max(1);
            gf.dispose = gif::DisposalMethod::Background;
            encoder
                .write_frame(&gf)
                .map_err(|e| PxlError::io(format!("gif frame: {e}")))?;
        }
    }
    Ok(buf)
}

fn gif_palette(colors: &[Rgba]) -> Vec<u8> {
    let mut n = colors.len().max(2);
    n = n.next_power_of_two().clamp(2, 256);
    let mut out = vec![0u8; n * 3];
    for (i, c) in colors.iter().take(n).enumerate() {
        out[i * 3] = c.r;
        out[i * 3 + 1] = c.g;
        out[i * 3 + 2] = c.b;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::support::palette;

    /// Read back (repeat, per-frame delay in cs) from encoded GIF bytes.
    fn gif_timing(bytes: &[u8]) -> (gif::Repeat, Vec<u16>) {
        let mut opts = gif::DecodeOptions::new();
        opts.set_color_output(gif::ColorOutput::Indexed);
        let mut dec = opts.read_info(bytes).unwrap();
        let mut delays = Vec::new();
        while let Some(frame) = dec.read_next_frame().unwrap() {
            delays.push(frame.delay);
        }
        (dec.repeat(), delays)
    }

    fn two_frames() -> Vec<IndexFrame> {
        let mut a = IndexFrame::new(3, 3).unwrap();
        a.set(1, 1, 3);
        let mut b = IndexFrame::new(3, 3).unwrap();
        b.set(2, 2, 4);
        vec![a, b]
    }

    #[test]
    fn one_shot_gif_does_not_loop_and_looping_gif_does() {
        let pal = palette();
        let frames = two_frames();
        let once = encode_gif_timed(&frames, &pal, 2, &[10, 10], false).unwrap();
        let (repeat, _) = gif_timing(&once);
        assert_eq!(
            repeat,
            gif::Repeat::Finite(0),
            "no loop block means play once"
        );
        let forever = encode_gif_timed(&frames, &pal, 2, &[10, 10], true).unwrap();
        assert_eq!(gif_timing(&forever).0, gif::Repeat::Infinite);
        // The fixed-delay entry point still loops, as before.
        let legacy = encode_gif(&frames, &pal, 2, 7).unwrap();
        let (repeat, delays) = gif_timing(&legacy);
        assert_eq!(repeat, gif::Repeat::Infinite);
        assert_eq!(delays, vec![7, 7]);
    }

    #[test]
    fn gif_delays_follow_the_requested_fps_without_drift() {
        // 100 / 12 truncates to 8cs (12.5 fps). The spread delays must total 100cs.
        for fps in [1u32, 4, 8, 12, 24, 30, 60, 100] {
            let delays = gif_delays_cs(fps, fps as usize);
            let total: u32 = delays.iter().map(|d| *d as u32).sum();
            assert_eq!(total, 100, "fps {fps}: {delays:?}");
            assert!(delays.iter().all(|d| *d >= 1));
        }
        assert_eq!(gif_delays_cs(12, 3), vec![8, 9, 8]);
    }

    #[test]
    fn gif_timed_rejects_a_delay_count_mismatch() {
        let pal = palette();
        let err = encode_gif_timed(&two_frames(), &pal, 1, &[10], true).unwrap_err();
        assert!(err.message.contains("delays"), "{err:?}");
    }
}
