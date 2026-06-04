use unicode_width::UnicodeWidthChar;

#[derive(Debug, Clone)]
pub struct WrapMapping {
    rows: Vec<RowMap>,
    pub total_visual_rows: usize,
}

#[derive(Debug, Clone)]
struct RowMap {
    base_vrow: usize,
    /// Buffer-column start of each visual segment for this buffer row.
    /// Always begins with 0; length equals the number of visual rows this
    /// buffer row occupies (>= 1 -- empty lines still take one visual row).
    segment_starts: Vec<usize>,
}

pub fn wrap_lines(
    lines: &[Vec<char>],
    width: usize,
) -> (Vec<Vec<char>>, WrapMapping) {
    let width = width.max(1);
    let mut visual: Vec<Vec<char>> = Vec::new();
    let mut rows: Vec<RowMap> = Vec::with_capacity(lines.len());
    let mut base_vrow = 0;

    for line in lines {
        let mut segment_starts: Vec<usize> = vec![0];
        let mut segment_start: usize = 0;
        let mut current: Vec<char> = Vec::new();
        let mut current_width: usize = 0;
        // Buffer index just after the most recent whitespace seen within the
        // current segment -- the preferred wrap point when overflow hits.
        let mut last_break_after: Option<usize> = None;

        for (i, &ch) in line.iter().enumerate() {
            let w = char_width(ch);
            if current_width + w > width && !current.is_empty() {
                let break_at = match last_break_after {
                    Some(k) if k > segment_start => k,
                    _ => i,
                };
                visual.push(line[segment_start..break_at].to_vec());
                segment_starts.push(break_at);
                segment_start = break_at;
                current = line[break_at..i].to_vec();
                current_width = current.iter().map(|&c| char_width(c)).sum();
                last_break_after = None;
            }
            current.push(ch);
            current_width += w;
            if ch.is_whitespace() {
                last_break_after = Some(i + 1);
            }
        }
        visual.push(std::mem::take(&mut current));

        let visual_rows = segment_starts.len();
        rows.push(RowMap {
            base_vrow,
            segment_starts,
        });
        base_vrow += visual_rows;
    }

    let mapping = WrapMapping {
        rows,
        total_visual_rows: base_vrow,
    };
    (visual, mapping)
}

pub fn buffer_to_visual(
    mapping: &WrapMapping,
    lines: &[Vec<char>],
    row: usize,
    col: usize,
) -> (usize, usize) {
    let row = row.min(mapping.rows.len().saturating_sub(1));
    let rm = &mapping.rows[row];
    let line = &lines[row];
    let clamped_col = col.min(line.len());

    let mut seg = 0;
    for (i, &start) in rm.segment_starts.iter().enumerate() {
        if start <= clamped_col {
            seg = i;
        } else {
            break;
        }
    }
    let seg_start = rm.segment_starts[seg];
    let vcol: usize = line[seg_start..clamped_col]
        .iter()
        .map(|&c| char_width(c))
        .sum();
    (rm.base_vrow + seg, vcol)
}

pub fn visual_to_buffer(
    mapping: &WrapMapping,
    lines: &[Vec<char>],
    vrow: usize,
    vcol: usize,
) -> (usize, usize) {
    if mapping.rows.is_empty() {
        return (0, 0);
    }
    let last_row = mapping.rows.len() - 1;
    let mut row = last_row;
    for (i, rm) in mapping.rows.iter().enumerate() {
        let end = rm.base_vrow + rm.segment_starts.len();
        if vrow < end {
            row = i;
            break;
        }
    }
    let rm = &mapping.rows[row];
    let line = &lines[row];
    let seg_count = rm.segment_starts.len();
    let row_end_vrow = rm.base_vrow + seg_count;
    let clamped_vrow = vrow.min(row_end_vrow - 1);
    let seg = clamped_vrow - rm.base_vrow;
    let seg_start = rm.segment_starts[seg];
    let seg_end = rm
        .segment_starts
        .get(seg + 1)
        .copied()
        .unwrap_or(line.len());
    let has_next_seg = seg + 1 < seg_count;
    // If this segment is followed by another, the cursor must not slip past
    // its last char (that position is owned by the next visual segment).
    let walk_end = if has_next_seg && seg_end > seg_start {
        seg_end - 1
    } else {
        seg_end
    };

    let mut col = seg_start;
    let mut acc: usize = 0;
    while col < walk_end {
        let w = char_width(line[col]);
        if acc + w > vcol {
            break;
        }
        acc += w;
        col += 1;
    }
    (row, col)
}

fn char_width(ch: char) -> usize {
    UnicodeWidthChar::width(ch).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn buf(s: &str) -> Vec<Vec<char>> {
        if s.is_empty() {
            return vec![Vec::new()];
        }
        s.split('\n').map(|l| l.chars().collect()).collect()
    }

    fn visual_strings(v: &[Vec<char>]) -> Vec<String> {
        v.iter().map(|c| c.iter().collect()).collect()
    }

    #[test]
    fn no_wrap_when_line_fits() {
        let lines = buf("hello");
        let (visual, m) = wrap_lines(&lines, 10);
        assert_eq!(visual_strings(&visual), vec!["hello"]);
        assert_eq!(m.total_visual_rows, 1);
    }

    #[test]
    fn wraps_at_width() {
        let lines = buf("0123456789ABCDE");
        let (visual, m) = wrap_lines(&lines, 10);
        assert_eq!(visual_strings(&visual), vec!["0123456789", "ABCDE"]);
        assert_eq!(m.total_visual_rows, 2);
    }

    #[test]
    fn exact_width_no_extra_wrap() {
        let lines = buf("0123456789");
        let (visual, m) = wrap_lines(&lines, 10);
        assert_eq!(visual_strings(&visual), vec!["0123456789"]);
        assert_eq!(m.total_visual_rows, 1);
    }

    #[test]
    fn empty_line_takes_one_visual_row() {
        let lines = buf("");
        let (visual, m) = wrap_lines(&lines, 10);
        assert_eq!(visual.len(), 1);
        assert!(visual[0].is_empty());
        assert_eq!(m.total_visual_rows, 1);
    }

    #[test]
    fn multiple_buffer_lines_with_mixed_wrap() {
        let lines = buf("short\n0123456789ABCDE\n");
        let (visual, m) = wrap_lines(&lines, 10);
        assert_eq!(
            visual_strings(&visual),
            vec!["short", "0123456789", "ABCDE", ""]
        );
        assert_eq!(m.total_visual_rows, 4);
    }

    #[test]
    fn width_one_degenerate() {
        let lines = buf("abc");
        let (visual, m) = wrap_lines(&lines, 1);
        assert_eq!(visual_strings(&visual), vec!["a", "b", "c"]);
        assert_eq!(m.total_visual_rows, 3);
    }

    #[test]
    fn width_zero_treated_as_one() {
        let lines = buf("ab");
        let (visual, m) = wrap_lines(&lines, 0);
        assert_eq!(visual_strings(&visual), vec!["a", "b"]);
        assert_eq!(m.total_visual_rows, 2);
    }

    #[test]
    fn wide_char_wraps_per_segment_at_width_one() {
        // CJK chars are width 2; can't fit in width 1, so each occupies its
        // own segment (single char per visual row).
        let lines = buf("漢字");
        let (visual, m) = wrap_lines(&lines, 1);
        assert_eq!(visual.len(), 2);
        assert_eq!(m.total_visual_rows, 2);
    }

    #[test]
    fn wide_chars_at_width_three() {
        // Width 3 fits one CJK char (w=2) plus nothing else (2+2=4 > 3).
        let lines = buf("漢字漢");
        let (visual, m) = wrap_lines(&lines, 3);
        assert_eq!(visual.len(), 3);
        assert_eq!(m.total_visual_rows, 3);
    }

    #[test]
    fn cursor_at_start() {
        let lines = buf("hello world");
        let (_v, m) = wrap_lines(&lines, 5);
        assert_eq!(buffer_to_visual(&m, &lines, 0, 0), (0, 0));
    }

    #[test]
    fn cursor_at_wrap_boundary_belongs_to_next_segment() {
        let lines = buf("0123456789ABCDE");
        let (_v, m) = wrap_lines(&lines, 10);
        // col 10 is start of second segment, not end of first.
        assert_eq!(buffer_to_visual(&m, &lines, 0, 10), (1, 0));
    }

    #[test]
    fn cursor_just_before_wrap_boundary() {
        let lines = buf("0123456789ABCDE");
        let (_v, m) = wrap_lines(&lines, 10);
        assert_eq!(buffer_to_visual(&m, &lines, 0, 9), (0, 9));
    }

    #[test]
    fn cursor_at_end_of_last_segment() {
        let lines = buf("0123456789ABCDE");
        let (_v, m) = wrap_lines(&lines, 10);
        // col 15 is end of line, last segment -- vcol 5 (past 'E').
        assert_eq!(buffer_to_visual(&m, &lines, 0, 15), (1, 5));
    }

    #[test]
    fn cursor_on_second_buffer_line() {
        let lines = buf("ab\ncd");
        let (_v, m) = wrap_lines(&lines, 10);
        assert_eq!(buffer_to_visual(&m, &lines, 1, 1), (1, 1));
    }

    #[test]
    fn visual_to_buffer_basic() {
        let lines = buf("0123456789ABCDE");
        let (_v, m) = wrap_lines(&lines, 10);
        assert_eq!(visual_to_buffer(&m, &lines, 0, 5), (0, 5));
        assert_eq!(visual_to_buffer(&m, &lines, 1, 0), (0, 10));
        assert_eq!(visual_to_buffer(&m, &lines, 1, 5), (0, 15));
    }

    #[test]
    fn visual_to_buffer_clamps_past_last_visual_row() {
        let lines = buf("abc");
        let (_v, m) = wrap_lines(&lines, 10);
        // Off the bottom: clamp to last row.
        assert_eq!(visual_to_buffer(&m, &lines, 99, 1), (0, 1));
    }

    #[test]
    fn visual_to_buffer_vcol_overflow_on_wrapped_segment_stays_in_segment() {
        // First visual segment is 10 cols wide and has a next segment.
        // vcol=99 should NOT bleed into the next segment (which would mean
        // col=10); it should land just before the wrap boundary (col=9).
        let lines = buf("0123456789ABCDE");
        let (_v, m) = wrap_lines(&lines, 10);
        assert_eq!(visual_to_buffer(&m, &lines, 0, 99), (0, 9));
    }

    #[test]
    fn visual_to_buffer_vcol_overflow_on_final_segment_lands_at_end() {
        let lines = buf("0123456789ABCDE");
        let (_v, m) = wrap_lines(&lines, 10);
        // Final segment is "ABCDE" (5 wide). vcol past end goes to end-of-line.
        assert_eq!(visual_to_buffer(&m, &lines, 1, 99), (0, 15));
    }

    #[test]
    fn visual_to_buffer_on_empty_line() {
        let lines = buf("a\n\nb");
        let (_v, m) = wrap_lines(&lines, 10);
        // The middle empty buffer line is visual row 1.
        assert_eq!(visual_to_buffer(&m, &lines, 1, 0), (1, 0));
        assert_eq!(visual_to_buffer(&m, &lines, 1, 5), (1, 0));
    }

    #[test]
    fn round_trip_ascii_wrapped_line() {
        let lines = buf("0123456789ABCDEFGHIJ");
        let (_v, m) = wrap_lines(&lines, 7);
        for col in 0..=lines[0].len() {
            let v = buffer_to_visual(&m, &lines, 0, col);
            let back = visual_to_buffer(&m, &lines, v.0, v.1);
            let v2 = buffer_to_visual(&m, &lines, back.0, back.1);
            assert_eq!(v, v2, "col {col} round-trip diverged: {v:?} vs {v2:?}");
        }
    }

    #[test]
    fn round_trip_visual_positions() {
        let lines = buf("0123456789ABCDEFGHIJ");
        let (_v, m) = wrap_lines(&lines, 7);
        for vrow in 0..m.total_visual_rows {
            // walk reachable vcols up to a generous bound.
            for vcol in 0..=10 {
                let (r, c) = visual_to_buffer(&m, &lines, vrow, vcol);
                let v_back = buffer_to_visual(&m, &lines, r, c);
                // The forward map produces a canonical visual position; it
                // must round-trip through visual_to_buffer.
                let (r2, c2) = visual_to_buffer(&m, &lines, v_back.0, v_back.1);
                assert_eq!(
                    (r, c),
                    (r2, c2),
                    "buffer position {:?} not stable through round-trip via \
                     ({vrow},{vcol})",
                    (r, c)
                );
            }
        }
    }

    #[test]
    fn round_trip_with_wide_chars() {
        // Mix CJK (w=2) with ASCII.
        let lines = buf("a漢b字c");
        let (_v, m) = wrap_lines(&lines, 4);
        for col in 0..=lines[0].len() {
            let v = buffer_to_visual(&m, &lines, 0, col);
            let back = visual_to_buffer(&m, &lines, v.0, v.1);
            let v2 = buffer_to_visual(&m, &lines, back.0, back.1);
            assert_eq!(v, v2, "col {col} diverged: {v:?} vs {v2:?}");
        }
    }

    #[test]
    fn word_wrap_breaks_at_space_not_mid_word() {
        let lines = buf("hello world foo");
        let (visual, m) = wrap_lines(&lines, 10);
        // Char-break would give ["hello worl", "d foo"]; word-break keeps
        // the trailing space on the first segment and starts the next at
        // the next word.
        assert_eq!(visual_strings(&visual), vec!["hello ", "world foo"]);
        assert_eq!(m.total_visual_rows, 2);
    }

    #[test]
    fn word_wrap_multiple_breaks() {
        // "the quick brown fox" at width 10 wraps cleanly between words.
        let lines = buf("the quick brown fox jumps");
        let (visual, _) = wrap_lines(&lines, 10);
        assert_eq!(
            visual_strings(&visual),
            vec!["the quick ", "brown fox ", "jumps"]
        );
    }

    #[test]
    fn word_wrap_long_word_falls_back_to_char_break() {
        // First "word" is longer than the wrap width, so we have to char-break
        // somewhere inside it.
        let lines = buf("abcdefghij klm");
        let (visual, _) = wrap_lines(&lines, 5);
        assert_eq!(visual_strings(&visual), vec!["abcde", "fghij", " klm"]);
    }

    #[test]
    fn word_wrap_round_trip_through_buffer_positions() {
        let lines = buf("the quick brown fox jumps over the lazy dog");
        let (_v, m) = wrap_lines(&lines, 12);
        for col in 0..=lines[0].len() {
            let v = buffer_to_visual(&m, &lines, 0, col);
            let back = visual_to_buffer(&m, &lines, v.0, v.1);
            let v2 = buffer_to_visual(&m, &lines, back.0, back.1);
            assert_eq!(v, v2, "col {col} diverged: {v:?} vs {v2:?}");
        }
    }

    #[test]
    fn buffer_to_visual_clamps_col_past_end() {
        let lines = buf("abc");
        let (_v, m) = wrap_lines(&lines, 10);
        assert_eq!(buffer_to_visual(&m, &lines, 0, 99), (0, 3));
    }
}
