    use super::*;

    fn opaque_block(w: u32, h: u32, b: u32, colors: &[[u8; 4]]) -> Vec<u8> {
        let mut out = vec![0u8; (w * h * 4) as usize];
        for y in 0..h {
            for x in 0..w {
                let c = colors[(((x / b) + (y / b)) as usize) % colors.len()];
                let i = ((y * w + x) * 4) as usize;
                out[i..i + 4].copy_from_slice(&c);
            }
        }
        out
    }

    #[test]
    fn the_largest_uniform_block_is_found() {
        let red = [200, 0, 0, 255];
        let blue = [0, 0, 200, 255];
        assert_eq!(
            detect_block(12, 12, &opaque_block(12, 12, 3, &[red, blue])),
            3
        );
        assert_eq!(
            detect_block(12, 12, &opaque_block(12, 12, 1, &[red, blue])),
            1
        );
        assert_eq!(
            detect_block(12, 12, &opaque_block(12, 12, 6, &[red, blue])),
            6
        );
    }

    #[test]
    fn transparent_blocks_match_whatever_their_rgb() {
        let mut px = vec![0u8; 4 * 4 * 4];
        px[0] = 9;
        assert_eq!(detect_block(4, 4, &px), 4);
    }

    #[test]
    fn cleanup_names_are_validated() {
        assert_eq!(parse_cleanup("all").unwrap(), CleanOptions::ALL);
        assert_eq!(parse_cleanup("none").unwrap(), CleanOptions::NONE);
        let two = parse_cleanup("orphan,notch").unwrap();
        assert!(two.orphans && two.notches && !two.elbows);
        let modern = parse_cleanup("rhythm,symmetry").unwrap();
        assert!(modern.rhythm && modern.symmetry && !modern.orphans);
        assert!(parse_cleanup("pixel_perfect").unwrap().elbows);
        assert!(parse_cleanup("blur").is_err());
    }
