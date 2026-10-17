#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QrSizeError {
    TooSmall,
    InvalidCode,
}

/// Encodes QR modules as half-height terminal cells with a four-module quiet zone.
/// Each character represents two vertically adjacent modules. The payload itself
/// is never returned or formatted as text.
pub fn encode_cells(payload: &str, width: u16, height: u16) -> Result<Vec<String>, QrSizeError> {
    let code = qrcode::QrCode::new(payload.as_bytes()).map_err(|_| QrSizeError::InvalidCode)?;
    let side = code.width();
    let modules = side + 8;
    let terminal_rows = modules.div_ceil(2);
    if usize::from(width) < modules || usize::from(height) < terminal_rows {
        return Err(QrSizeError::TooSmall);
    }
    let colors = code.to_colors();
    let dark_at = |x: usize, y: usize| {
        x >= 4
            && y >= 4
            && x < side + 4
            && y < side + 4
            && colors[(y - 4) * side + x - 4] == qrcode::Color::Dark
    };
    let mut rows = Vec::with_capacity(terminal_rows);
    for terminal_y in 0..terminal_rows {
        let mut row = String::with_capacity(modules);
        for x in 0..modules {
            let top = dark_at(x, terminal_y * 2);
            let bottom = terminal_y * 2 + 1 < modules && dark_at(x, terminal_y * 2 + 1);
            row.push(match (top, bottom) {
                (true, true) => '█',
                (true, false) => '▀',
                (false, true) => '▄',
                (false, false) => ' ',
            });
        }
        rows.push(row);
    }
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::{QrSizeError, encode_cells};

    #[test]
    fn qr_has_quiet_zone() {
        let rows = encode_cells("fixture payload", 100, 100).unwrap();
        assert!(rows.first().unwrap().chars().all(|cell| cell == ' '));
        assert!(rows.last().unwrap().chars().all(|cell| cell == ' '));
        assert!(
            rows.iter()
                .all(|row| row.starts_with("    ") && row.ends_with("    "))
        );
        assert!(
            rows.iter()
                .any(|row| row.chars().any(|cell| matches!(cell, '▀' | '▄' | '█')))
        );
    }

    #[test]
    fn qr_reports_a_small_terminal() {
        assert_eq!(encode_cells("fixture", 10, 10), Err(QrSizeError::TooSmall));
    }

    #[test]
    fn qr_fits_a_typical_terminal_with_half_height_cells() {
        let rows = encode_cells("fixture payload", 78, 22).unwrap();
        assert!(rows.len() <= 22);
        assert!(rows.iter().all(|row| row.chars().count() <= 78));
        assert!(
            rows.iter()
                .any(|row| row.chars().any(|cell| matches!(cell, '▀' | '▄' | '█')))
        );
        assert!(rows.first().unwrap().chars().all(|cell| cell == ' '));
    }
}
